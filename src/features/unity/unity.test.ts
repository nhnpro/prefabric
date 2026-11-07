import { describe, expect, it, vi } from "vitest";
import { CompletionContext, type Completion } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { indentUnit } from "@codemirror/language";
import type { HierarchyItem } from "@/platform/ipc";
import { classAtMemberLevel, classes, codeOnly, declaredMethods, unityBaseInFile } from "./csharp";
import { dataVersion, declaration, loadMessageData, messagesFor, unityTypeNames } from "./messages";
import { clearUnityBases, declarationPrefix, unityMessageSource, type UnityCompletionDeps } from "./completion";

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

  async function complete(view: EditorView, deps: Partial<UnityCompletionDeps> = {}) {
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
