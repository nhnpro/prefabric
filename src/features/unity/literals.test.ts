import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { CompletionContext } from "@codemirror/autocomplete";
import type { UnityProjectSymbols } from "@/platform/ipc";
import { unityLiteralContext, unityLiteralSource } from "./literals";

const SYMBOLS: UnityProjectSymbols = {
  tags: ["Player", "Enemy", "Untagged"],
  layers: ["Default", "Ground", "UI"],
  sortingLayers: ["Default", "Foreground"],
  scenes: ["MainMenu", "Level1"],
  inputAxes: ["Horizontal", "Jump"],
  animatorParams: ["Speed", "IsGrounded"],
  shaderProperties: ["_BaseColor", "_Dissolve"],
};

describe("Unity string-literal completion", () => {
  it("classifies Unity API string literal contexts accurately", () => {
    const check = (code: string, needle: string) => {
      const pos = code.indexOf(needle) + needle.length;
      return unityLiteralContext(code, 0, pos)?.kind ?? null;
    };
    expect(check('if (other.CompareTag("En"))', "En")).toBe("tag");
    expect(check('if (gameObject.tag == "Pl")', "Pl")).toBe("tag");
    expect(check('int mask = LayerMask.GetMask("Default", "Gr");', "Gr")).toBe("layer");
    expect(check('SortingLayer.NameToID("Fore");', "Fore")).toBe("sortingLayer");
    expect(check('SceneManager.LoadScene("Lev");', "Lev")).toBe("scene");
    expect(check('Input.GetAxisRaw("Hor");', "Hor")).toBe("inputAxis");
    expect(check('animator.SetTrigger("Sp");', "Sp")).toBe("animatorParam");
    expect(check('Shader.PropertyToID("_Base");', "_Base")).toBe("shaderProperty");
    expect(check('material.SetFloat("_Dis", 0.5f);', "_Dis")).toBe("shaderProperty");
    expect(check('Invoke("Explode", 2f);', "Explode")).toBe("method");
    expect(check('StartCoroutine("FadeOut");', "FadeOut")).toBe("method");
    expect(check('Debug.Log("hello");', "hello")).toBeNull();
  });

  it("offers project symbols for tags, scenes, and shader properties", async () => {
    const source = unityLiteralSource(async () => SYMBOLS);
    const doc = 'SceneManager.LoadScene("Lev")';
    const state = EditorState.create({ doc });
    const res = await source(new CompletionContext(state, doc.indexOf("Lev") + 3, false));
    expect(res?.options.map((o) => o.label)).toEqual(["MainMenu", "Level1"]);
  });

  it("completes enclosing class method names for Invoke and StartCoroutine", async () => {
    const source = unityLiteralSource(async () => SYMBOLS);
    const doc = [
      "public class Enemy : MonoBehaviour {",
      "  void Start() {",
      '    Invoke("Re", 1f);',
      "  }",
      "  void Respawn() {}",
      "  IEnumerator FadeOut() { yield break; }",
      "}",
    ].join("\n");
    const state = EditorState.create({ doc });
    const pos = doc.indexOf('"Re') + 3;
    const res = await source(new CompletionContext(state, pos, false));
    expect(res?.options.map((o) => o.label)).toEqual(["FadeOut", "Respawn", "Start"]);
  });
});
