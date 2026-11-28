import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { CompletionContext } from "@codemirror/autocomplete";
import { includeAt, includeCandidates, shaderCompletionSource } from "./shader";

describe("Unity shader support", () => {
  it("extracts #include and #include_with_pragmas paths at cursor", () => {
    const src = [
      "HLSLPROGRAM",
      '#include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"',
      '#include_with_pragmas "Lighting.hlsl"',
      "ENDHLSL",
    ].join("\n");
    const firstPos = src.indexOf("Core.hlsl");
    expect(includeAt(src, firstPos)).toEqual({
      path: "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl",
      from: src.indexOf("Packages/"),
      to: src.indexOf("Core.hlsl") + "Core.hlsl".length,
    });
    const secondPos = src.indexOf("Lighting.hlsl");
    expect(includeAt(src, secondPos)?.path).toBe("Lighting.hlsl");
    expect(includeAt(src, 2)).toBeNull();
  });

  it("builds candidate paths for relative, Assets/, and Packages/ includes", () => {
    expect(includeCandidates("/proj/Assets/Shaders/Water.shader", "/proj", "../Common/Noise.hlsl")).toEqual([
      "/proj/Assets/Common/Noise.hlsl",
    ]);
    expect(includeCandidates("/proj/Assets/Shaders/Water.shader", "/proj", "Noise.hlsl")).toEqual([
      "/proj/Assets/Shaders/Noise.hlsl",
      "/proj/Assets/Noise.hlsl",
    ]);
    expect(
      includeCandidates(
        "/proj/Assets/Shaders/Water.shader",
        "/proj",
        "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl",
      ),
    ).toEqual(["/proj/Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"]);
  });

  it("offers #pragma directives and ShaderLab / Unity HLSL completions", () => {
    const src = "#pragma multi";
    const state = EditorState.create({ doc: src });
    const ctx = new CompletionContext(state, src.length, false);
    const res = shaderCompletionSource("Assets/Test.shader")(ctx);
    expect(res?.options.some((o) => o.label === "multi_compile")).toBe(true);
    expect(res?.options.some((o) => o.label === "shader_feature")).toBe(true);

    const hlslDoc = "TransformObj";
    const hlslState = EditorState.create({ doc: hlslDoc });
    const hlslRes = shaderCompletionSource("Assets/Helper.hlsl")(
      new CompletionContext(hlslState, hlslDoc.length, false),
    );
    expect(hlslRes?.options.some((o) => o.label === "TransformObjectToHClip")).toBe(true);
    expect(hlslRes?.options.some((o) => o.label === "SubShader")).toBe(false);
  });
});
