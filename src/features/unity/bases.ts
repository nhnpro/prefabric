/**
 * The Unity type a C# class derives from: through the bases declared in its file, then through
 * the language server's type hierarchy for bases declared in other files.
 */
import { lspRequest } from "@/editor/lsp/lsp";
import { readFileLines, withDocument } from "@/editor/lsp/files";
import { commands, type HierarchyItem, type Pos } from "@/platform/ipc";
import { classes, unityBaseInFile, type ClassInfo } from "./csharp";
import type { MessageData } from "./messages";

export interface UnityDeps {
  /** The message data for the open project, null outside Unity projects. */
  data: () => Promise<MessageData | null>;
  /** Direct supertypes of the type declared at `pos` of the file (or virtual document) `path`. */
  supertypes: (path: string, pos: Pos) => Promise<HierarchyItem[]>;
  /** Text of a supertype's file. */
  text: (item: HierarchyItem) => Promise<string>;
}

/** Bases followed through the language server; Unity types sit a few levels down at most. */
const MAX_DEPTH = 6;

function posOf(text: string, offset: number): Pos {
  const before = text.slice(0, offset);
  const line = before.split("\n").length - 1;
  return { line, col: offset - (before.lastIndexOf("\n") + 1) };
}

/** `path#Class(bases)` → the answer (a Unity type, or null) and when it was found. */
const resolvedBases = new Map<string, { base: string | null; at: number }>();
/** A "no" is asked again soon (the server may still be loading); a "yes" after a while (a base in another file may change). */
const RETRY_NO_AFTER_MS = 30_000;
const RETRY_YES_AFTER_MS = 5 * 60_000;

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
  deps: UnityDeps,
): Promise<string | null> {
  const key = `${path}#${cls.name}(${cls.bases.join(",")})`;
  const known = resolvedBases.get(key);
  if (known && Date.now() - known.at < (known.base ? RETRY_YES_AFTER_MS : RETRY_NO_AFTER_MS)) return known.base;
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
  resolvedBases.set(key, { base: found, at: Date.now() });
  return found;
}

/** The Unity type `cls` (declared in `text`, the content of `path`) derives from, null when none. */
export async function unityBaseOf(
  path: string,
  text: string,
  cls: ClassInfo,
  unityTypes: ReadonlySet<string>,
  deps: UnityDeps,
): Promise<string | null> {
  const inFile = unityBaseInFile(text, cls, unityTypes);
  if (inFile !== undefined) return inFile;
  try {
    return await unityBaseFromServer(path, text, cls, unityTypes, deps);
  } catch {
    return null;
  }
}

export const unityDeps = (data: () => Promise<MessageData | null>): UnityDeps => ({
  data,
  supertypes: (path, pos) =>
    withDocument(path, (doc) => lspRequest(() => commands.lspTypeHierarchy(doc, pos, "incoming"))),
  text: async (item) => (await readFileLines(item.uri ?? item.path))?.join("\n") ?? "",
});
