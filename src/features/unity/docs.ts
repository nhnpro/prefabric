/**
 * "Open Unity Documentation": opens the Unity ScriptReference page for the Unity event function,
 * class, or `Type.Member` symbol under the editor caret, matched to the project's Unity version.
 */
import type { Action } from "@/platform/actions";
import { activeEditor } from "@/editor/api";
import { openExternalUrl } from "@/platform/openUrl";
import { useProject } from "@/platform/project";
import { registerMenuGroup, type MenuGroupContext } from "@/workbench/contributions";
import { unityBaseOf, type UnityDeps } from "./bases";
import { classes, methodDeclarations } from "./csharp";
import { messagesFor, unityTypeNames, type MessageData } from "./messages";

export const OPEN_UNITY_DOCS = "unity.openDocs";

const IDENT_CHAR = /[A-Za-z0-9_]/;

/** Extracts the identifier (or `Type.Member` chain) under `offset` in `lineText`. */
export function symbolAtOffset(lineText: string, col: number): { ident: string; qualified: string } | null {
  if (col < 0 || col > lineText.length) return null;
  let pos = col;
  if (pos === lineText.length || !IDENT_CHAR.test(lineText[pos]!)) {
    if (pos > 0 && IDENT_CHAR.test(lineText[pos - 1]!)) pos--;
    else return null;
  }
  let start = pos;
  while (start > 0 && IDENT_CHAR.test(lineText[start - 1]!)) start--;
  let end = pos + 1;
  while (end < lineText.length && IDENT_CHAR.test(lineText[end]!)) end++;
  const ident = lineText.slice(start, end);
  if (!ident || /^\d/.test(ident)) return null;

  // Include an immediately preceding `Receiver.` when it looks like a type name (PascalCase).
  let qStart = start;
  const before = lineText.slice(0, start);
  const qualifier = /([A-Z][A-Za-z0-9_]*)\s*\.\s*$/.exec(before);
  if (qualifier) qStart = start - qualifier[0].length;
  const qualified = lineText.slice(qStart, end).replace(/\s+/g, "");
  return { ident, qualified };
}

/** Extracts `major.minor` from a Unity version string like `6000.0.23f1` or `2022.3.62f1`. */
export function unityDocsVersion(editorVersion: string | null | undefined): string | null {
  const m = /^(\d+\.\d+)/.exec((editorVersion ?? "").trim());
  return m ? m[1]! : null;
}

/** Builds a Unity ScriptReference URL for a symbol query and optional Unity editor version. */
export function unitySearchDocsUrl(query: string, editorVersion?: string | null): string {
  const version = unityDocsVersion(editorVersion);
  const prefix = version
    ? `https://docs.unity3d.com/${version}/Documentation/ScriptReference`
    : "https://docs.unity3d.com/ScriptReference";
  return `${prefix}/30_search.html?q=${encodeURIComponent(query)}`;
}

/** Resolves the best Unity ScriptReference URL for the symbol at `pos` in `text`. */
export async function resolveUnityDocsUrl(
  path: string,
  text: string,
  pos: number,
  editorVersion: string | null | undefined,
  deps: UnityDeps,
): Promise<string | null> {
  const lineStart = pos <= 0 ? 0 : text.lastIndexOf("\n", pos - 1) + 1;
  const lineEndIdx = text.indexOf("\n", pos);
  const lineEnd = lineEndIdx < 0 ? text.length : lineEndIdx;
  const lineText = text.slice(lineStart, lineEnd);
  const sym = symbolAtOffset(lineText, pos - lineStart);
  if (!sym) return null;

  const data: MessageData | null = await deps.data().catch(() => null);
  if (data) {
    // 1. Caret on a declared Unity event function inside a Unity-derived class.
    const unityTypes = unityTypeNames(data);
    for (const cls of classes(text)) {
      if (pos < cls.bodyStart || pos > cls.bodyEnd) continue;
      const methods = methodDeclarations(text, cls);
      const hit = methods.find((m) => m.name === sym.ident && pos >= m.at && pos <= m.at + m.name.length);
      if (!hit) continue;
      const base = await unityBaseOf(path, text, cls, unityTypes, deps);
      if (!base) continue;
      const msg = messagesFor(data, base).find((m) => m.name === hit.name);
      if (msg?.docs) return msg.docs;
    }
    // 2. Known Unity base type (e.g. `MonoBehaviour`, `ScriptableObject`, `EditorWindow`).
    const typeEntry = data.types[sym.ident];
    if (typeEntry?.docs) return typeEntry.docs;
  }

  // 3. Fallback to version-matched Unity ScriptReference search (`Physics.Raycast`, `Vector3`, etc.).
  return unitySearchDocsUrl(sym.qualified, editorVersion);
}

export function openUnityDocsAction(deps: UnityDeps): Action {
  return {
    id: OPEN_UNITY_DOCS,
    title: "Open Unity Documentation",
    category: "Unity",
    icon: "external-link",
    keywords: ["unity", "docs", "documentation", "scriptreference", "help", "manual"],
    when: "editorFocus",
    run: async () => {
      const unity = useProject.getState().project?.unity;
      const ed = activeEditor();
      if (!unity || !ed || ed.ctx.language !== "csharp") return;
      const pos = ed.view.state.selection.main.head;
      const url = await resolveUnityDocsUrl(ed.ctx.path, ed.view.state.doc.toString(), pos, unity.editorVersion, deps);
      if (url) void openExternalUrl(url).catch(() => undefined);
    },
  };
}

/** Shows "Open Unity Documentation" in the C# editor context menu for Unity projects. */
export function openUnityDocsMenus(): () => void {
  return registerMenuGroup({
    id: "unity.openDocs",
    menus: ["editor"],
    order: 46,
    items: (_menu, ctx: MenuGroupContext) => {
      const [path] = ctx.paths;
      if (!path || !useProject.getState().project?.unity || !path.toLowerCase().endsWith(".cs")) return [];
      return [{ kind: "action", action: OPEN_UNITY_DOCS }];
    },
  });
}
