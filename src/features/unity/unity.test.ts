import { describe, expect, it, vi } from "vitest";
import { CompletionContext, type Completion } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { indentUnit } from "@codemirror/language";
import type { AssemblyDefinition, HierarchyItem, TextEdit, WorkspaceEdit } from "@/platform/ipc";
import { referenceAt, resolveReference } from "./asmdef";
import { ussSource } from "./uss";
import { isUnityYaml, unityYamlLock, unlockUnityYaml, useUnityYaml } from "./unityYaml";
import { serializedFieldAt, unityRename } from "./rename";
import { readFileSync } from "node:fs";
import { Text } from "@codemirror/state";
import { classAtMemberLevel, classes, codeOnly, declaredMethods, methodDeclarations, unityBaseInFile } from "./csharp";
import { eventFunctions, eventFunctionTooltip, placeEventFunctions } from "./gutter";
import { dataVersion, declaration, loadMessageData, messagesFor, unityTypeNames } from "./messages";
import { clearUnityBases, type UnityDeps } from "./bases";
import { declarationPrefix, unityMessageSource } from "./completion";

const PLAYER = `using UnityEngine;
// class Fake : MonoBehaviour {
public class Player : MonoBehaviour
{
    private string label = "}{";
    void Update() { if (true) { } }

}
`;

describe("csharp scanner", () => {
  it("blanks comments and strings without moving offsets", () => {
    const code = codeOnly(PLAYER);
    expect(code).toHaveLength(PLAYER.length);
    expect(code).not.toContain("Fake");
    expect(code).not.toContain('"}{"');
  });

  it("finds classes, bases and the member level", () => {
    const [player] = classes(PLAYER);
    expect(player).toMatchObject({ name: "Player", bases: ["MonoBehaviour"] });
    expect(PLAYER.slice(player!.nameAt, player!.nameAt + 6)).toBe("Player");
    const blank = PLAYER.indexOf("\n\n}") + 1;
    expect(classAtMemberLevel(PLAYER, blank)?.name).toBe("Player");
    expect(classAtMemberLevel(PLAYER, PLAYER.indexOf("if (true)"))).toBeNull();
    expect([...declaredMethods(PLAYER, player!)]).toEqual(["Update"]);
  });

  it("follows bases declared in the file", () => {
    const text =
      "class Base : UnityEngine.ScriptableObject {}\nclass Item : Base, IComparable {}\nclass Other : Remote {}";
    const types = new Set(["MonoBehaviour", "ScriptableObject"]);
    const [, item, other] = classes(text);
    expect(unityBaseInFile(text, item!, types)).toBe("ScriptableObject");
    expect(unityBaseInFile(text, other!, types)).toBeUndefined();
    expect(unityBaseInFile("class A {}", classes("class A {}")[0]!, types)).toBeNull();
  });
});

describe("message data", async () => {
  const data = await loadMessageData("6000.6.8f1");

  it("picks the data for the editor version", () => {
    expect(dataVersion("2022.3.62f1")).toBe("2022.3");
    expect(dataVersion("6000.3.1f1")).toBe("6000.3");
    expect(dataVersion(null)).toBe("6000.3");
  });

  it("walks the Unity base chain", () => {
    const editor = messagesFor(data, "Editor");
    expect(editor.find((m) => m.name === "OnSceneGUI")?.owner).toBe("Editor");
    expect(editor.find((m) => m.name === "OnEnable")).toBeDefined();
    expect(messagesFor(data, "UIBehaviour").some((m) => m.name === "OnTriggerEnter")).toBe(true);
    expect(unityTypeNames(data).has("NetworkBehaviour")).toBe(true);
  });

  it("writes declarations", () => {
    const enter = messagesFor(data, "MonoBehaviour").find((m) => m.name === "OnTriggerEnter")!;
    const start = messagesFor(data, "MonoBehaviour").find((m) => m.name === "Start")!;
    expect(declaration(enter, { modifiers: [] })).toBe("private void OnTriggerEnter(Collider other)");
    expect(declaration(start, { modifiers: ["public"], returns: "IEnumerator" })).toBe("public IEnumerator Start()");
  });
});

describe("message completion", () => {
  const data = loadMessageData("6000.3.1f1");

  function editor(text: string) {
    const pos = text.indexOf("|");
    return new EditorView({
      state: EditorState.create({
        doc: text.replace("|", ""),
        selection: { anchor: pos },
        extensions: indentUnit.of("    "),
      }),
    });
  }

  async function complete(view: EditorView, deps: Partial<UnityDeps> = {}) {
    const source = unityMessageSource("/p/Assets/Player.cs", {
      data: () => data,
      supertypes: vi.fn().mockResolvedValue([]),
      text: vi.fn().mockResolvedValue(""),
      ...deps,
    });
    return source(new CompletionContext(view.state, view.state.selection.main.head, false, view));
  }

  it("reads the modifiers before the name", () => {
    expect(declarationPrefix("    public override OnTr")).toMatchObject({
      modifiers: ["public", "override"],
      start: 4,
      word: "OnTr",
    });
    expect(declarationPrefix("  IEnumerator St")).toMatchObject({ returns: "IEnumerator", word: "St" });
    expect(declarationPrefix("  int x = OnTr")).toBeNull();
    expect(declarationPrefix("  Rigidbody OnTr")).toBeNull();
  });

  it("offers messages not implemented yet and inserts the method", async () => {
    const view = editor(
      PLAYER.replace("\n\n}", "\n    OnTrig|\n}").replace(
        "void Update",
        "void OnTriggerExit(Collider c) {}\n    void Update",
      ),
    );
    const result = await complete(view);
    const labels = result!.options.map((o) => o.label);
    expect(labels).toContain("OnTriggerEnter");
    expect(labels).not.toContain("OnTriggerExit");
    expect(labels).not.toContain("Update");
    const enter = result!.options.find((o) => o.label === "OnTriggerEnter")! as Completion & { typeText: string };
    expect(enter.typeText).toBe("MonoBehaviour");
    (enter.apply as (v: EditorView, c: Completion, from: number, to: number) => void)(
      view,
      enter,
      result!.from,
      view.state.selection.main.head,
    );
    expect(view.state.doc.toString()).toContain(
      "    private void OnTriggerEnter(Collider other)\n    {\n        \n    }\n}",
    );
  });

  it("asks the language server for bases declared elsewhere", async () => {
    clearUnityBases();
    const item = (name: string, path: string): HierarchyItem => {
      const zero = { start: { line: 0, col: 0 }, end: { line: 0, col: 0 } };
      return { name, kind: "class", detail: null, path, uri: null, range: zero, selectionRange: zero, fromRanges: [] };
    };
    // Roslyn lists supertypes with source only: Character, then Actor; Actor's file names MonoBehaviour.
    const supertypes = vi
      .fn()
      .mockResolvedValueOnce([item("Character", "/p/Assets/Character.cs")])
      .mockResolvedValueOnce([item("Actor", "/p/Assets/Actor.cs")]);
    const files: Record<string, string> = {
      "/p/Assets/Character.cs": "namespace Game { public abstract class Character : Actor {} }",
      "/p/Assets/Actor.cs": "using UnityEngine;\npublic class Actor : MonoBehaviour, IDamageable {}",
    };
    const text = vi.fn(async (i: HierarchyItem) => files[i.path]!);
    const view = editor("class Hero : Character\n{\n    Awa|\n}");
    const result = await complete(view, { supertypes, text });
    expect(result!.options.map((o) => o.label)).toContain("Awake");
    expect(supertypes).toHaveBeenNthCalledWith(1, "/p/Assets/Player.cs", { line: 0, col: 6 });
    expect(supertypes).toHaveBeenNthCalledWith(2, "/p/Assets/Character.cs", { line: 0, col: 0 });
    // Remembered: typing on doesn't ask again.
    await complete(view, { supertypes, text });
    expect(supertypes).toHaveBeenCalledTimes(2);
  });

  it("asks again later after the server found no Unity base", async () => {
    clearUnityBases();
    const supertypes = vi.fn().mockResolvedValue([]);
    const view = editor("class Hero : Character\n{\n    Awa|\n}");
    expect(await complete(view, { supertypes })).toBeNull();
    expect(await complete(view, { supertypes })).toBeNull();
    expect(supertypes).toHaveBeenCalledTimes(1);
    vi.spyOn(Date, "now").mockReturnValue(Date.now() + 31_000);
    await complete(view, { supertypes });
    expect(supertypes).toHaveBeenCalledTimes(2);
    vi.restoreAllMocks();
  });

  it("stays out of non-Unity classes, method bodies and other projects", async () => {
    expect(await complete(editor("class Plain\n{\n    OnTr|\n}"))).toBeNull();
    expect(await complete(editor(PLAYER.replace("if (true) { }", "if (true) { OnTr| }")))).toBeNull();
    expect(await complete(editor(PLAYER.replace("\n\n}", "\n    OnTr|\n}")), { data: async () => null })).toBeNull();
  });
});

describe("event function gutter", () => {
  const data = loadMessageData("6000.3.1f1");
  const deps: UnityDeps = { data: () => data, supertypes: async () => [], text: async () => "" };
  const fixture = readFileSync("fixtures/unity-sample/Assets/Scripts/Player.cs", "utf8");

  it("finds method names with their offsets", () => {
    const methods = methodDeclarations(fixture, classes(fixture)[0]!);
    expect(methods.map((m) => m.name)).toEqual([
      "Update",
      "OnTriggerEnter",
      "AddCoin",
      "OnTriggerZoneEntered",
      "OnFootstep",
    ]);
    for (const m of methods) expect(fixture.slice(m.at, m.at + m.name.length)).toBe(m.name);
  });

  it("marks Update and OnTriggerEnter of the fixture's player, and nothing else", async () => {
    const found = await eventFunctions("/p/Assets/Scripts/Player.cs", fixture, deps);
    expect(found.map((f) => f.message.name)).toEqual(["Update", "OnTriggerEnter"]);
    const doc = Text.of(fixture.split("\n"));
    expect(placeEventFunctions(doc, found).map((p) => doc.lineAt(p.pos).text.trim())).toEqual([
      "private void Update()",
      "private void OnTriggerEnter(Collider other)",
    ]);
    expect(eventFunctionTooltip(found[0]!.message)).toMatch(/^Unity event function Update of MonoBehaviour\n\S/);
  });

  it("marks nothing outside Unity classes or projects", async () => {
    expect(await eventFunctions("/p/A.cs", "class A { void Update() {} }", deps)).toEqual([]);
    expect(await eventFunctions("/p/Assets/Scripts/Player.cs", fixture, { ...deps, data: async () => null })).toEqual(
      [],
    );
  });
});

describe("Unity-aware rename", () => {
  const data = loadMessageData("6000.3.1f1");
  const deps: UnityDeps = { data: () => data, supertypes: async () => [], text: async () => "" };
  const fixture = readFileSync("fixtures/unity-sample/Assets/Scripts/Player.cs", "utf8");
  const path = "/p/Assets/Scripts/Player.cs";
  const empty: WorkspaceEdit = { files: [] };

  function contribute(text: string, at: string, oldName: string, newName: string, file = path) {
    const state = EditorState.create({ doc: text });
    return unityRename(deps)({ state, path: file, pos: text.indexOf(at) + 2, oldName, newName, edit: empty });
  }

  it("renames a MonoBehaviour's file with the class", async () => {
    const [ext] = await contribute(fixture, "Player : MonoBehaviour", "Player", "Hero");
    expect(ext).toMatchObject({ label: "Rename file to Hero.cs", checked: true });
    expect(ext!.extend(empty).files).toEqual([{ kind: "rename", from: path, to: "/p/Assets/Scripts/Hero.cs" }]);
    // Not when the file is named otherwise, or for plain classes.
    expect(await contribute(fixture, "Player : MonoBehaviour", "Player", "Hero", "/p/Assets/Scripts/Other.cs")).toEqual(
      [],
    );
    expect(await contribute("class Player {}", "Player", "Player", "Hero")).toEqual([]);
  });

  it("offers FormerlySerializedAs for serialized fields only", async () => {
    const [ext] = await contribute(fixture, "speed = 2f", "speed", "velocity");
    expect(ext!.label).toBe('Add [FormerlySerializedAs("speed")] to keep saved values');
    const doc = Text.of(fixture.split("\n"));
    const edits = (ext!.extend(empty).files[0] as { edits: TextEdit[] }).edits;
    const changes = edits.map((e) => ({
      from: doc.line(e.range.start.line + 1).from + e.range.start.col,
      insert: e.new_text,
    }));
    const result = EditorState.create({ doc: fixture }).update({ changes }).state.doc.toString();
    expect(result).toContain("using UnityEngine;\nusing UnityEngine.Serialization;\n");
    expect(result).toContain(
      '        [FormerlySerializedAs("speed")]\n        [SerializeField] private float speed = 2f;',
    );
    // A property, a method and a non-serialized private field get nothing.
    expect(await contribute(fixture, "Speed => speed", "Speed", "Velocity")).toEqual([]);
    expect(await contribute(fixture, "AddCoin() =>", "AddCoin", "Add")).toEqual([]);
    const plain = fixture.replace("[SerializeField] private int coins;", "private int coins;");
    expect(await contribute(plain, "coins;", "coins", "count")).toEqual([]);
  });

  it("recognises public fields and skips static ones", () => {
    const text =
      "class A : MonoBehaviour {\n  /// <summary>Doc</summary>\n  public int hp = 3;\n  public static int count;\n}";
    expect(serializedFieldAt(text, text.indexOf("hp"))?.declarationStart).toBe(text.indexOf("public int hp"));
    expect(serializedFieldAt(text, text.indexOf("count"))).toBeNull();
  });
});

describe("assembly definition references", () => {
  const tests = readFileSync("fixtures/unity-sample/Assets/Tests/EditMode/Game.Tests.EditMode.asmdef", "utf8");
  const asmdef = "/p/Assets/Tests/Game.Tests.asmdef";
  const definitions: AssemblyDefinition[] = [
    { name: "Game.Runtime", path: "/p/Assets/Scripts/Game.Runtime.asmdef", guid: "f3a9c0d1e2b34567890abcdef1234567" },
    { name: "UnityEngine.TestRunner", path: "/p/Library/PackageCache/t/UnityEngine.TestRunner.asmdef", guid: null },
  ];

  it("finds the reference under the caret", () => {
    const at = tests.indexOf("Game.Runtime") + 3;
    expect(referenceAt(asmdef, tests, at)).toEqual({ value: "Game.Runtime", from: at - 3, to: at - 3 + 12 });
    // The assembly's own name, other arrays and keys are not references.
    expect(referenceAt(asmdef, tests, tests.indexOf("Game.Tests.EditMode") + 2)).toBeNull();
    expect(referenceAt(asmdef, tests, tests.indexOf("nunit.framework") + 2)).toBeNull();
    expect(referenceAt(asmdef, tests, tests.indexOf("references") + 2)).toBeNull();
    expect(referenceAt("/p/a.json", tests, at)).toBeNull();
    const asmref = '{\n  "reference": "GUID:F3A9C0D1E2B34567890ABCDEF1234567"\n}';
    expect(referenceAt("/p/Assets/X/X.asmref", asmref, asmref.indexOf("GUID") + 1)?.value).toBe(
      "GUID:F3A9C0D1E2B34567890ABCDEF1234567",
    );
  });

  it("resolves names and GUIDs", () => {
    expect(resolveReference("Game.Runtime", definitions)?.path).toBe("/p/Assets/Scripts/Game.Runtime.asmdef");
    expect(resolveReference("GUID:F3A9C0D1E2B34567890ABCDEF1234567", definitions)?.name).toBe("Game.Runtime");
    expect(resolveReference("UnityEditor.TestRunner", definitions)).toBeNull();
    expect(resolveReference("GUID:0000", definitions)).toBeNull();
  });
});

describe("USS completion", () => {
  async function complete(text: string, explicit = false) {
    const pos = text.indexOf("|");
    const state = EditorState.create({ doc: text.replace("|", ""), selection: { anchor: pos } });
    return ussSource(new CompletionContext(state, pos, explicit));
  }

  it("offers Unity properties where a declaration starts", async () => {
    const result = await complete(".title {\n  color: red;\n  -unity-te|\n}");
    expect(result!.options.map((o) => o.label)).toContain("-unity-text-align");
    expect(result!.from).toBe(".title {\n  color: red;\n  ".length);
    expect(result!.options.find((o) => o.label === "-unity-text-align")!.apply).toBe("-unity-text-align: ");
  });

  it("offers a Unity property's keyword values", async () => {
    const result = await complete(".title { -unity-font-style: bo| }");
    expect(result!.options.map((o) => o.label)).toEqual(["normal", "italic", "bold", "bold-and-italic"]);
    expect(await complete(".title { color: re| }")).toBeNull();
  });

  it("stays out of selectors", async () => {
    expect(await complete(".tit|le { }")).toBeNull();
    expect(await complete("Button > .ti|")).toBeNull();
  });
});

describe("Unity YAML", () => {
  const prefab = readFileSync("fixtures/unity-sample/Assets/Prefabs/Player.prefab", "utf8");

  it("recognises Unity's YAML dialect", () => {
    expect(isUnityYaml(Text.of(prefab.split("\n")))).toBe(true);
    expect(isUnityYaml(Text.of(["%YAML 1.2", "---", "a: 1"]))).toBe(false);
    expect(isUnityYaml(Text.of(["a: 1"]))).toBe(false);
  });

  it("opens read-only until Edit Anyway, and only Unity's files", async () => {
    const path = "/p/Assets/Prefabs/Player.prefab";
    const view = new EditorView({ state: EditorState.create({ doc: prefab, extensions: unityYamlLock(path) }) });
    expect(view.state.readOnly).toBe(true);
    expect(useUnityYaml.getState().locked[path]).toBe(true);
    unlockUnityYaml(path);
    await Promise.resolve();
    expect(view.state.readOnly).toBe(false);
    expect(useUnityYaml.getState().locked[path]).toBe(false);
    // Unlocked for the session: reopening stays editable.
    const again = EditorState.create({ doc: prefab, extensions: unityYamlLock(path) });
    expect(again.readOnly).toBe(false);
    const plain = EditorState.create({ doc: "a: 1\n", extensions: unityYamlLock("/p/config.yaml") });
    expect(plain.readOnly).toBe(false);
    view.destroy();
  });
});
