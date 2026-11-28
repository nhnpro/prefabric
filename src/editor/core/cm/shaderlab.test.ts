import { describe, expect, it } from "vitest";
import { highlightTree, tagHighlighter, tags as t } from "@lezer/highlight";
import { hlslLanguage, shaderlabLanguage } from "./shaderlab";

const highlighter = tagHighlighter([
  { tag: t.keyword, class: "kw" },
  { tag: t.typeName, class: "ty" },
  { tag: t.meta, class: "meta" },
  { tag: t.propertyName, class: "prop" },
  { tag: t.attributeName, class: "sem" },
  { tag: t.macroName, class: "macro" },
  { tag: t.string, class: "str" },
  { tag: t.comment, class: "cmt" },
  { tag: t.number, class: "num" },
  { tag: t.atom, class: "atom" },
]);

function tokens(lang: typeof shaderlabLanguage, src: string): Array<[string, string]> {
  const out: Array<[string, string]> = [];
  highlightTree(lang.parser.parse(src), highlighter, (from, to, cls) => {
    out.push([src.slice(from, to), cls]);
  });
  return out;
}

describe("shaderlab & hlsl CodeMirror highlighters", () => {
  it("highlights ShaderLab blocks and switches into HLSL inside HLSLPROGRAM..ENDHLSL", () => {
    const src = [
      'Shader "Custom/Unlit" {',
      '  Properties { _BaseColor ("Color", Color) = (1,1,1,1) }',
      "  SubShader {",
      "    Cull Off",
      "    Pass {",
      "      HLSLPROGRAM",
      "      #pragma vertex vert",
      "      float4 vert(float3 pos : POSITION) : SV_Position {",
      "        return TransformObjectToHClip(pos);",
      "      }",
      "      ENDHLSL",
      "    }",
      "  }",
      "}",
    ].join("\n");
    const toks = tokens(shaderlabLanguage, src);
    expect(toks).toContainEqual(["Shader", "kw"]);
    expect(toks).toContainEqual(['"Custom/Unlit"', "str"]);
    expect(toks).toContainEqual(["_BaseColor", "prop"]);
    expect(toks).toContainEqual(["Cull", "kw"]);
    expect(toks).toContainEqual(["Off", "atom"]);
    expect(toks).toContainEqual(["HLSLPROGRAM", "meta"]);
    expect(toks).toContainEqual(["#pragma", "meta"]);
    expect(toks).toContainEqual(["float4", "ty"]);
    expect(toks).toContainEqual(["POSITION", "sem"]);
    expect(toks).toContainEqual(["SV_Position", "sem"]);
    expect(toks).toContainEqual(["ENDHLSL", "meta"]);
  });

  it("highlights standalone HLSL files including macros, types, and block comments", () => {
    const src = [
      "/* URP helper */",
      '#include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"',
      "TEXTURE2D(_MainTex);",
      "half4 frag(float2 uv : TEXCOORD0) : SV_Target {",
      "  return SAMPLE_TEXTURE2D(_MainTex, sampler_MainTex, uv);",
      "}",
    ].join("\n");
    const toks = tokens(hlslLanguage, src);
    expect(toks).toContainEqual(["/* URP helper */", "cmt"]);
    expect(toks).toContainEqual(["#include", "meta"]);
    expect(toks).toContainEqual(["TEXTURE2D", "macro"]);
    expect(toks).toContainEqual(["half4", "ty"]);
    expect(toks).toContainEqual(["TEXCOORD0", "sem"]);
    expect(toks).toContainEqual(["SV_Target", "sem"]);
    expect(toks).toContainEqual(["SAMPLE_TEXTURE2D", "macro"]);
  });
});
