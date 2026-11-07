/**
 * Unity-aware rename. Unity finds a MonoBehaviour or ScriptableObject by its file name, so
 * renaming such a class offers to rename its file (the `.meta` moves with it, keeping the GUID
 * that scenes and prefabs reference). Renaming a serialized field offers
 * `[FormerlySerializedAs("old")]`, so the values saved in scenes and prefabs are kept.
 */
import type { Text } from "@codemirror/state";
import type { ContributedRename, RenameExtension } from "@/editor/api";
import { tx } from "@/platform/i18n";
import type { FileEdit, Pos, TextEdit, WorkspaceEdit } from "@/platform/ipc";
import { unityBaseOf, type UnityDeps } from "./bases";
import { classAtMemberLevel, classes, codeOnly } from "./csharp";
import { unityTypeNames } from "./messages";

/** Unity types whose scripts must be named like the class. */
const FILE_NAMED = new Set([
  "MonoBehaviour",
  "ScriptableObject",
  "UIBehaviour",
  "NetworkBehaviour",
  "Editor",
  "EditorWindow",
]);
/** Unity types whose fields Unity serializes. */
const SERIALIZING = new Set(["MonoBehaviour", "ScriptableObject", "UIBehaviour", "NetworkBehaviour", "EditorWindow"]);
const IDENTIFIER = /^[A-Za-z_]\w*$/;

function split(path: string): { dir: string; name: string } {
  const at = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return { dir: path.slice(0, at + 1), name: path.slice(at + 1) };
}

function posAt(doc: Text, offset: number): Pos {
  const line = doc.lineAt(offset);
  return { line: line.number - 1, col: offset - line.from };
}

/** `edit` with `edits` added to the file's text edits. */
export function withTextEdits(edit: WorkspaceEdit, path: string, edits: TextEdit[]): WorkspaceEdit {
  let added = false;
  const files = edit.files.map((f): FileEdit => {
    if (added || f.kind !== "edit" || f.path !== path) return f;
    added = true;
    return { ...f, edits: [...f.edits, ...edits] };
  });
  if (!added) files.unshift({ kind: "edit", path, edits });
  return { ...edit, files };
}

/**
 * Whether the field declared at `pos` (its name) is serialized: `[SerializeField]` or
 * `[SerializeReference]`, or public; not static, const or readonly, and not a property.
 */
export function serializedFieldAt(text: string, pos: number): { declarationStart: number } | null {
  const code = codeOnly(text);
  // The declaration: back to the previous `;`, `{` or `}` at this level, forward past the name.
  let start = pos;
  while (start > 0 && !";{}".includes(code[start - 1]!)) start--;
  const after = code.slice(pos).match(/^\w+\s*(=>|[=;,[{(])/);
  if (!after || after[1] === "{" || after[1] === "(" || after[1] === "=>") return null;
  const head = code.slice(start, pos);
  const attributes = [...head.matchAll(/\[([^\]]*)\]/g)].map((m) => m[1]!).join(",");
  const words = head
    .replace(/\[[^\]]*\]/g, " ")
    .split(/[^\w.<>[\],?]+/)
    .filter(Boolean);
  if (words.some((w) => w === "static" || w === "const" || w === "readonly" || w === "event")) return null;
  const marked = /\b(?:UnityEngine\.)?Serialize(?:Field|Reference)(?:Attribute)?\b/.test(attributes);
  if (!marked && !words.includes("public")) return null;
  if (/\bNonSerialized\b/.test(attributes)) return null;
  const firstCode = start + (head.length - head.trimStart().length);
  return { declarationStart: firstCode };
}

/** Where `using UnityEngine.Serialization;` goes: after the last top-level `using`, else the top. */
function usingInsertion(text: string): { offset: number; prefix: string } {
  const code = codeOnly(text);
  let offset = 0;
  for (const m of code.matchAll(/^[ \t]*using\s+(?!\()[\w.=\s]+;[ \t]*$/gm)) offset = m.index + m[0].length;
  return offset ? { offset, prefix: "\n" } : { offset: 0, prefix: "" };
}

export function formerlySerializedEdits(
  doc: Text,
  text: string,
  declarationStart: number,
  oldName: string,
): TextEdit[] {
  const line = doc.lineAt(declarationStart);
  const indent = /^\s*/.exec(line.text)![0];
  const at = posAt(doc, line.from);
  const edits: TextEdit[] = [
    { range: { start: at, end: at }, new_text: `${indent}[FormerlySerializedAs("${oldName}")]\n` },
  ];
  if (!/^\s*using\s+UnityEngine\.Serialization\s*;/m.test(codeOnly(text))) {
    const { offset, prefix } = usingInsertion(text);
    const p = posAt(doc, offset);
    edits.push({
      range: { start: p, end: p },
      new_text: prefix ? `${prefix}using UnityEngine.Serialization;` : "using UnityEngine.Serialization;\n",
    });
  }
  return edits;
}

export function unityRename(deps: UnityDeps): ContributedRename {
  return async ({ state, path, pos: caret, oldName, newName }) => {
    if (!IDENTIFIER.test(newName)) return [];
    // The caret may be anywhere in the name.
    const line = state.doc.lineAt(caret);
    let pos = caret;
    while (pos > line.from && /\w/.test(line.text[pos - line.from - 1]!)) pos--;
    const data = await deps.data();
    if (!data) return [];
    const unityTypes = unityTypeNames(data);
    const text = state.doc.toString();
    const out: RenameExtension[] = [];

    const cls = classes(text).find((c) => c.nameAt === pos && c.name === oldName);
    if (cls) {
      const { dir, name } = split(path);
      const base = name === `${oldName}.cs` ? await unityBaseOf(path, text, cls, unityTypes, deps) : null;
      if (base && FILE_NAMED.has(base)) {
        const to = `${dir}${newName}.cs`;
        out.push({
          label: tx`Rename file to ${`${newName}.cs`}`,
          checked: true,
          extend: (e) => ({ ...e, files: [...e.files, { kind: "rename", from: path, to }] }),
        });
      }
      return out;
    }

    const owner = classAtMemberLevel(text, pos);
    const field = owner && serializedFieldAt(text, pos);
    if (owner && field) {
      const base = await unityBaseOf(path, text, owner, unityTypes, deps);
      if (base && SERIALIZING.has(base)) {
        const edits = formerlySerializedEdits(state.doc, text, field.declarationStart, oldName);
        out.push({
          label: tx`Add [FormerlySerializedAs("${oldName}")] to keep saved values`,
          checked: true,
          extend: (e) => withTextEdits(e, path, edits),
        });
      }
    }
    return out;
  };
}
