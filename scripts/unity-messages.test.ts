import { describe, expect, it } from "vitest";
import {
  backfill,
  exampleSignature,
  mergeClassic,
  messageRows,
  tableParameters,
  type MessageData,
} from "./unity-messages.ts";

const classicType = `<h3>Public Methods</h3><table class="list">
<tr><td class="lbl"><a href="MonoBehaviour.Invoke.html">Invoke</a></td><td class="desc">Invokes a method.</td></tr></table>
<h3>Messages</h3><table class="list">
<tr><td class="lbl"><a href="MonoBehaviour.Start.html">Start</a></td><td class="desc">Called before the first frame.</td></tr>
<tr><td class="lbl"><a href="EditorWindow-hasUnsavedChanges.html">hasUnsavedChanges</a></td><td class="desc">A property.</td></tr>
</table>`;

const newType = `<h2 class="x">Methods</h2><table><tr><td class="c"><a class="l" href="/en-us/engine/6000.3/x/invoke">Invoke</a></td><td class="c">Invokes a method.</td></tr>
<tr><td class="c"><a class="l" href="/en-us/engine/6000.3/x/start">Start</a></td><td class="c">Called before the first frame.</td></tr>
<tr><td class="c"><a class="l" href="/en-us/engine/6000.3/x/onnew">OnNewThing</a></td><td class="c">Added in 6.</td></tr></table>`;

describe("unity messages", () => {
  it("reads the classic Messages table without properties", () => {
    expect(messageRows(classicType)).toEqual([
      { name: "Start", href: "MonoBehaviour.Start.html", description: "Called before the first frame." },
    ]);
  });

  it("tells messages from methods on docs.unity.com with the classic lists", () => {
    const classic = { messages: new Set(["Start"]), methods: new Set(["Invoke"]) };
    expect(messageRows(newType, classic).map((r) => r.name)).toEqual(["Start", "OnNewThing"]);
  });

  it("takes signatures from examples and signature spans, coroutines and static included", () => {
    const page = `<pre>// Unity calls the Start() method once
IEnumerator Start()
{
    yield return null;
}
void Start() { }</pre>
<span class="MuiTypography-root MuiTypography-code x">public void OnTriggerEnter(Collider collider)</span>
<pre>static void OnPostprocessAllAssets(string[] importedAssets, string[] deletedAssets)</pre>`;
    expect(exampleSignature(page, "Start")).toEqual({
      name: "Start",
      returns: "void",
      parameters: [],
      coroutine: true,
      static: false,
    });
    expect(exampleSignature(page, "OnTriggerEnter")?.parameters).toEqual([{ name: "collider", type: "Collider" }]);
    expect(exampleSignature(page, "OnPostprocessAllAssets")?.static).toBe(true);
    expect(exampleSignature(page, "OnGUI")).toBeNull();
  });

  it("falls back to the parameter table's linked types", () => {
    const page = `<h3>Parameters</h3><table><tr><td class="name lbl">other</td><td class="desc">A <a href="Collider.html">Collider</a>.</td></tr>
<tr><td class="name lbl">hasFocus</td><td class="desc">True if focused.</td></tr></table>`;
    expect(tableParameters(page)).toEqual([
      { name: "other", type: "Collider" },
      { name: "hasFocus", type: "bool" },
    ]);
  });

  it("keeps classic parameter names and static, and fills guessed classic signatures", () => {
    const message = (name: string, parameters: Array<[string, string]>, isStatic = false) => ({
      name,
      returns: "void",
      parameters: parameters.map(([type, n]) => ({ type, name: n })),
      coroutine: false,
      static: isStatic,
      description: "",
      docs: "",
    });
    const classic: MessageData = {
      unity: "2022.3",
      types: {
        T: {
          base: null,
          docs: "",
          messages: [message("OnFocus", [["bool", "hasFocus"]], true), message("OnAssign", [])],
        },
      },
    };
    const newer: MessageData = {
      unity: "6000.3",
      types: {
        T: {
          base: null,
          docs: "",
          messages: [message("OnFocus", [["bool", "arg0"]]), message("OnAssign", [["Material", "material"]])],
        },
      },
    };
    mergeClassic(newer, classic);
    expect(newer.types.T!.messages[0]).toMatchObject({
      static: true,
      parameters: [{ type: "bool", name: "hasFocus" }],
    });
    backfill(classic, newer, new Set(["T.OnAssign"]));
    expect(classic.types.T!.messages[1]!.parameters).toEqual([{ type: "Material", name: "material" }]);
  });
});
