/**
 * Unity message completion: in the body of a class deriving from a Unity type, typing `OnTrig…`
 * offers `OnTriggerEnter` and inserts the whole method.
 */
import { snippet, type Completion, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import type { EditorView } from "@codemirror/view";
import { tx } from "@/platform/i18n";
import { openExternalUrl } from "@/platform/openUrl";
import { unityBaseOf, type UnityDeps } from "./bases";
import { classAtMemberLevel, declaredMethods } from "./csharp";
import { declaration, messagesFor, unityTypeNames, type UnityMessage } from "./messages";

const MODIFIERS = new Set(["private", "protected", "public", "internal", "static", "override", "new", "virtual"]);
const RETURNS = new Set(["void", "IEnumerator", "System.Collections.IEnumerator"]);

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

function applyMessage(m: UnityMessage, prefix: NonNullable<ReturnType<typeof declarationPrefix>>, lineFrom: number) {
  return (view: EditorView, completion: Completion, _from: number, to: number) => {
    const template = `${declaration(m, prefix)}\n{\n\t\${}\n}`;
    snippet(template)(view, completion, lineFrom + prefix.start, to);
  };
}

export function unityMessageSource(path: string, deps: UnityDeps) {
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
    const base = await unityBaseOf(path, text, cls, unityTypes, deps);
    if (!base || ctx.aborted) return null;
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
            link.addEventListener("click", (e) => {
              e.preventDefault();
              void openExternalUrl(m.docs).catch(() => undefined);
            });
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
