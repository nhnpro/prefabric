/**
 * Unity message completion: in the body of a class deriving from a Unity type, typing `OnTrig…`
 * offers `OnTriggerEnter` and inserts the whole method. The base chain comes from the file itself,
 * then from the language server's type hierarchy for bases declared in other files.
 */
import { snippet, type Completion, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import type { EditorView } from "@codemirror/view";
import { lspRequest } from "@/editor/lsp/lsp";
import { readFileLines, withDocument } from "@/editor/lsp/files";
import { commands, type HierarchyItem, type Pos } from "@/platform/ipc";
import { tx } from "@/platform/i18n";
import { classAtMemberLevel, classes, declaredMethods, unityBaseInFile, type ClassInfo } from "./csharp";
import { declaration, messagesFor, unityTypeNames, type MessageData, type UnityMessage } from "./messages";

export interface UnityCompletionDeps {
  /** The message data for the open project, null outside Unity projects. */
  data: () => Promise<MessageData | null>;
  /** Direct supertypes of the type declared at `pos` of the file (or virtual document) `path`. */
  supertypes: (path: string, pos: Pos) => Promise<HierarchyItem[]>;
  /** Text of a supertype's file. */
  text: (item: HierarchyItem) => Promise<string>;
}

const MODIFIERS = new Set(["private", "protected", "public", "internal", "static", "override", "new", "virtual"]);
const RETURNS = new Set(["void", "IEnumerator", "System.Collections.IEnumerator"]);
/** Bases followed through the language server; Unity types sit a few levels down at most. */
const MAX_DEPTH = 6;

/** What precedes the name being typed on its line: only modifiers and a return type. */
export function declarationPrefix(
  line: string,
): { modifiers: string[]; returns?: string; start: number; word: string } | null {
  const m = /^(\s*)((?:[\w.]+\s+)*)(\w*)$/.exec(line);
  if (!m) return null;
  const words = m[2]!.split(/\s+/).filter(Boolean);
  const modifiers: string[] = [];
  let returns: string | undefined;
  for (const w of words) {
    if (returns === undefined && MODIFIERS.has(w)) modifiers.push(w);
    else if (returns === undefined && RETURNS.has(w)) returns = w.replace(/^System\.Collections\./, "");
    else return null;
  }
  return { modifiers, returns, start: m[1]!.length, word: m[3]! };
}

function posOf(text: string, offset: number): Pos {
  const before = text.slice(0, offset);
  const line = before.split("\n").length - 1;
  return { line, col: offset - (before.lastIndexOf("\n") + 1) };
}

/** `path#Class(bases)` → the Unity type it derives from, or when a "no" was answered (the server may be loading). */
const resolvedBases = new Map<string, string | number>();
const RETRY_NO_AFTER_MS = 30_000;

export function clearUnityBases() {
  resolvedBases.clear();
}

/**
 * The Unity type the class derives from through bases declared in other files. Roslyn's type
 * hierarchy lists only supertypes with source (not `MonoBehaviour` itself), so each source base
 * found is read for the bases it declares, and walked further while those lead elsewhere.
 */
async function unityBaseFromServer(
  path: string,
  text: string,
  cls: ClassInfo,
  unityTypes: ReadonlySet<string>,
  deps: UnityCompletionDeps,
): Promise<string | null> {
  const key = `${path}#${cls.name}(${cls.bases.join(",")})`;
  const known = resolvedBases.get(key);
  if (typeof known === "string") return known;
  if (known !== undefined && Date.now() - known < RETRY_NO_AFTER_MS) return null;
  const seen = new Set<string>();
  const walk = async (itemPath: string, pos: Pos, depth: number): Promise<string | null> => {
    if (depth > MAX_DEPTH) return null;
    for (const s of await deps.supertypes(itemPath, pos)) {
      const name = s.name.replace(/<.*$/, "").split(".").pop()!;
      if (unityTypes.has(name)) return name;
      const id = `${s.uri ?? s.path}#${name}`;
      if (s.kind === "interface" || seen.has(id)) continue;
      seen.add(id);
      const source = await deps.text(s).catch(() => "");
      const declared = classes(source).find((c) => c.name === name);
      const inFile = declared ? unityBaseInFile(source, declared, unityTypes) : undefined;
      if (inFile) return inFile;
      if (inFile === null) continue;
      const found = await walk(s.uri ?? s.path, s.selectionRange.start, depth + 1);
      if (found) return found;
    }
    return null;
  };
  const found = await walk(path, posOf(text, cls.nameAt), 0);
  resolvedBases.set(key, found ?? Date.now());
  return found;
}

function applyMessage(m: UnityMessage, prefix: NonNullable<ReturnType<typeof declarationPrefix>>, lineFrom: number) {
  return (view: EditorView, completion: Completion, _from: number, to: number) => {
    const template = `${declaration(m, prefix)}\n{\n\t\${}\n}`;
    snippet(template)(view, completion, lineFrom + prefix.start, to);
  };
}

export function unityMessageSource(path: string, deps: UnityCompletionDeps) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const prefix = declarationPrefix(line.text.slice(0, ctx.pos - line.from));
    if (!prefix || (!prefix.word && !ctx.explicit)) return null;
    const text = ctx.state.doc.toString();
    const cls = classAtMemberLevel(text, ctx.pos);
    if (!cls) return null;
    const data = await deps.data();
    if (!data || ctx.aborted) return null;
    const unityTypes = unityTypeNames(data);
    let base = unityBaseInFile(text, cls, unityTypes);
    if (base === undefined) {
      try {
        base = await unityBaseFromServer(path, text, cls, unityTypes, deps);
      } catch {
        return null;
      }
      if (ctx.aborted) return null;
    }
    if (!base) return null;
    const declared = declaredMethods(text, cls);
    const options = messagesFor(data, base)
      .filter((m) => !declared.has(m.name) && (prefix.returns !== "IEnumerator" || m.coroutine))
      .map((m): Completion & { icon: string; typeText: string } => ({
        label: m.name,
        detail: `(${m.parameters.map((p) => p.type).join(", ")})`,
        type: "method",
        icon: "symbol-method",
        typeText: m.owner,
        info: () => {
          const dom = document.createElement("div");
          dom.textContent = m.description;
          if (m.docs) {
            const link = document.createElement("a");
            link.href = m.docs;
            link.textContent = tx("Unity documentation");
            link.target = "_blank";
            link.rel = "noreferrer";
            dom.append(document.createElement("br"), link);
          }
          return dom;
        },
        apply: applyMessage(m, prefix, line.from),
        boost: -1,
      }));
    return options.length ? { from: ctx.pos - prefix.word.length, options, validFor: /^\w*$/ } : null;
  };
}

export const unityCompletionDeps = (data: () => Promise<MessageData | null>): UnityCompletionDeps => ({
  data,
  supertypes: (path, pos) =>
    withDocument(path, (doc) => lspRequest(() => commands.lspTypeHierarchy(doc, pos, "incoming"))),
  text: async (item) => (await readFileLines(item.uri ?? item.path))?.join("\n") ?? "",
});
