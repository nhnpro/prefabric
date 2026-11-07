/**
 * Go to Definition from an assembly definition's references (`"references": ["Game.Runtime",
 * "GUID:…"]` in `.asmdef`, `"reference"` in `.asmref`) to the referenced `.asmdef`.
 */
import { EditorState, type Extension } from "@codemirror/state";
import { CONTRIBUTED_DEFINITION, type ContributedDefinition } from "@/editor/api";
import { readFileLines } from "@/editor/lsp/files";
import { tx } from "@/platform/i18n";
import { call, commands, type AssemblyDefinition } from "@/platform/ipc";
import { editorService, notifications } from "@/workbench/contributions";

export interface AssemblyReference {
  /** The string's content: a name, or `GUID:<guid>`. */
  value: string;
  from: number;
  to: number;
}

const isDefinitionFile = (path: string) => /\.asm(def|ref)$/i.test(path);

/** The reference string at `pos`, if `pos` is in one. */
export function referenceAt(path: string, text: string, pos: number): AssemblyReference | null {
  if (!isDefinitionFile(path)) return null;
  let start = -1;
  // Find the string around `pos`: scan from the start so escaped quotes are skipped.
  for (let i = 0; i < text.length && i <= pos; i++) {
    if (text[i] !== '"') continue;
    let j = i + 1;
    while (j < text.length && text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
    if (i < pos && pos <= j) {
      start = i;
      break;
    }
    i = j;
  }
  if (start < 0) return null;
  const end = text.indexOf('"', start + 1);
  const before = text.slice(0, start);
  const inReferences = /"references"\s*:\s*\[[^\]]*$/.test(before);
  const isReference = /"reference"\s*:\s*$/.test(before);
  if (!(path.toLowerCase().endsWith(".asmdef") ? inReferences : isReference)) return null;
  const value = text.slice(start + 1, end);
  return value ? { value, from: start + 1, to: end } : null;
}

/** The definition a reference names: `GUID:` by the `.meta` GUID, else by assembly name. */
export function resolveReference(value: string, definitions: readonly AssemblyDefinition[]): AssemblyDefinition | null {
  const guid = /^GUID:\s*([0-9a-f]+)$/i.exec(value)?.[1]?.toLowerCase();
  return definitions.find((d) => (guid ? d.guid?.toLowerCase() === guid : d.name === value)) ?? null;
}

async function openReference(value: string) {
  const definitions = await call(commands.unityAssemblyDefinitions());
  const target = resolveReference(value, definitions);
  if (!target) {
    notifications().notify({
      level: "warning",
      title: tx`No assembly definition named '${value}' in this project`,
      key: "unity.asmdef",
    });
    return;
  }
  const lines = await readFileLines(target.path).catch(() => null);
  const line = lines ? lines.findIndex((l) => /"name"\s*:/.test(l)) : -1;
  await editorService().openFile(target.path, {
    focus: true,
    ...(line >= 0 ? { selection: { line, col: 0 } } : {}),
  });
}

export function assemblyDefinitionSupport(path: string): Extension {
  if (!isDefinitionFile(path)) return [];
  const definition: ContributedDefinition = (view, pos) => {
    const hit = referenceAt(path, view.state.doc.toString(), pos);
    if (!hit) return false;
    void openReference(hit.value).catch((e) => console.debug("[asmdef]", e));
    return true;
  };
  return EditorState.languageData.of((state, pos) =>
    referenceAt(path, state.doc.toString(), pos) ? [{ [CONTRIBUTED_DEFINITION]: definition }] : [],
  );
}
