/**
 * CodeMirror 6 StreamLanguage highlighters for Unity ShaderLab (`.shader`) and HLSL/Cg
 * (`.hlsl`, `.cginc`, `.compute`, `.raytrace`, `.shaderinc`).
 *
 * In ShaderLab mode, tracks `CGPROGRAM`/`HLSLPROGRAM`/`GLSLPROGRAM`/`CGINCLUDE`/`HLSLINCLUDE`
 * blocks so ShaderLab state keywords colour outside blocks and HLSL/Cg types, intrinsics,
 * semantics, and preprocessor directives colour inside them.
 */
import { StreamLanguage, type StreamParser, type StringStream } from "@codemirror/language";

interface ShaderState {
  inBlockComment: boolean;
  /** Inside a CGPROGRAM / HLSLPROGRAM / CGINCLUDE / HLSLINCLUDE / GLSLPROGRAM block (always true for pure HLSL). */
  inCode: boolean;
  /** True after `:` on an HLSL declaration/parameter so `SV_Position`, `TEXCOORD0` colour as semantics. */
  expectSemantic: boolean;
}

const SHADERLAB_KEYWORDS = new Set([
  "Shader",
  "Properties",
  "SubShader",
  "Pass",
  "Category",
  "Tags",
  "LOD",
  "Fallback",
  "FallBack",
  "CustomEditor",
  "CustomEditorForRenderPipeline",
  "Dependency",
  "GrabPass",
  "UsePass",
  "PackageRequirements",
  "Cull",
  "ZWrite",
  "ZTest",
  "Offset",
  "Blend",
  "BlendOp",
  "AlphaToMask",
  "ColorMask",
  "Conservative",
  "Stencil",
  "Ref",
  "ReadMask",
  "WriteMask",
  "Comp",
  "CompFront",
  "CompBack",
  "Fail",
  "ZFail",
  "Lighting",
  "Material",
  "SeparateSpecular",
  "Color",
  "Fog",
  "SetTexture",
  "Name",
  "BindChannels",
  "Bind",
]);

const SHADERLAB_VALUES = new Set([
  "On",
  "Off",
  "Front",
  "Back",
  "Always",
  "Never",
  "Less",
  "LEqual",
  "Equal",
  "GEqual",
  "Greater",
  "NotEqual",
  "Keep",
  "Zero",
  "Replace",
  "IncrSat",
  "DecrSat",
  "Invert",
  "IncrWrap",
  "DecrWrap",
  "One",
  "SrcColor",
  "SrcAlpha",
  "DstColor",
  "DstAlpha",
  "OneMinusSrcColor",
  "OneMinusSrcAlpha",
  "OneMinusDstColor",
  "OneMinusDstAlpha",
  "Add",
  "Sub",
  "RevSub",
  "Min",
  "Max",
  "Float",
  "Int",
  "Integer",
  "Range",
  "Vector",
  "2D",
  "2DArray",
  "3D",
  "Cube",
  "CubeArray",
]);

const CODE_START = new Set(["CGPROGRAM", "HLSLPROGRAM", "GLSLPROGRAM", "CGINCLUDE", "HLSLINCLUDE", "GLSLINCLUDE"]);
const CODE_END = new Set(["ENDCG", "ENDHLSL", "ENDGLSL"]);

const HLSL_KEYWORDS = new Set([
  "if",
  "else",
  "for",
  "while",
  "do",
  "switch",
  "case",
  "default",
  "break",
  "continue",
  "return",
  "discard",
  "struct",
  "cbuffer",
  "tbuffer",
  "class",
  "interface",
  "namespace",
  "typedef",
  "const",
  "static",
  "extern",
  "uniform",
  "varying",
  "volatile",
  "inline",
  "in",
  "out",
  "inout",
  "groupshared",
  "shared",
  "precise",
  "globallycoherent",
  "nointerpolation",
  "linear",
  "centroid",
  "noperspective",
  "sample",
  "row_major",
  "column_major",
  "packoffset",
  "register",
  "true",
  "false",
  "NULL",
  "point",
  "line",
  "triangle",
  "lineadj",
  "triangleadj",
]);

const HLSL_SCALARS = [
  "void",
  "bool",
  "int",
  "uint",
  "dword",
  "half",
  "float",
  "double",
  "fixed",
  "min16float",
  "min16int",
  "min16uint",
  "min10float",
  "min12int",
];

const HLSL_TYPES = new Set<string>([
  ...HLSL_SCALARS,
  "string",
  "matrix",
  "vector",
  "sampler",
  "sampler1D",
  "sampler2D",
  "sampler3D",
  "samplerCUBE",
  "sampler2DShadow",
  "SamplerState",
  "SamplerComparisonState",
  "Texture1D",
  "Texture1DArray",
  "Texture2D",
  "Texture2DArray",
  "Texture2DMS",
  "Texture2DMSArray",
  "Texture3D",
  "TextureCube",
  "TextureCubeArray",
  "RWTexture1D",
  "RWTexture1DArray",
  "RWTexture2D",
  "RWTexture2DArray",
  "RWTexture3D",
  "Buffer",
  "RWBuffer",
  "ByteAddressBuffer",
  "RWByteAddressBuffer",
  "StructuredBuffer",
  "RWStructuredBuffer",
  "AppendStructuredBuffer",
  "ConsumeStructuredBuffer",
  "ConstantBuffer",
  "RaytracingAccelerationStructure",
  "BuiltInTriangleIntersectionAttributes",
  "RayDesc",
  "PointStream",
  "LineStream",
  "TriangleStream",
]);

// Add scalar vector/matrix combinations (float2..float4, float2x2..float4x4, half*, fixed*, int*, uint*, bool*, double*).
for (const base of ["bool", "int", "uint", "half", "float", "double", "fixed"]) {
  for (let r = 1; r <= 4; r++) {
    HLSL_TYPES.add(`${base}${r}`);
    for (let c = 1; c <= 4; c++) {
      HLSL_TYPES.add(`${base}${r}x${c}`);
    }
  }
}

const HLSL_INTRINSICS = new Set([
  "abs",
  "acos",
  "all",
  "any",
  "asfloat",
  "asin",
  "asint",
  "asuint",
  "atan",
  "atan2",
  "ceil",
  "clamp",
  "clip",
  "cos",
  "cosh",
  "cross",
  "ddx",
  "ddx_coarse",
  "ddx_fine",
  "ddy",
  "ddy_coarse",
  "ddy_fine",
  "degrees",
  "determinant",
  "distance",
  "dot",
  "exp",
  "exp2",
  "faceforward",
  "floor",
  "fma",
  "fmod",
  "frac",
  "fwidth",
  "isfinite",
  "isinf",
  "isnan",
  "ldexp",
  "length",
  "lerp",
  "lit",
  "log",
  "log10",
  "log2",
  "mad",
  "max",
  "min",
  "modf",
  "mul",
  "normalize",
  "pow",
  "radians",
  "rcp",
  "reflect",
  "refract",
  "round",
  "rsqrt",
  "saturate",
  "sign",
  "sin",
  "sincos",
  "sinh",
  "smoothstep",
  "sqrt",
  "step",
  "tan",
  "tanh",
  "tex1D",
  "tex2D",
  "tex2Dlod",
  "tex2Dproj",
  "tex3D",
  "texCUBE",
  "transpose",
  "trunc",
  "GroupMemoryBarrier",
  "GroupMemoryBarrierWithGroupSync",
  "DeviceMemoryBarrier",
  "DeviceMemoryBarrierWithGroupSync",
  "AllMemoryBarrier",
  "AllMemoryBarrierWithGroupSync",
  "InterlockedAdd",
  "InterlockedAnd",
  "InterlockedCompareExchange",
  "InterlockedCompareStore",
  "InterlockedExchange",
  "InterlockedMax",
  "InterlockedMin",
  "InterlockedOr",
  "InterlockedXor",
]);

const SEMANTIC_RE =
  /^(?:SV_[A-Za-z0-9_]+|POSITION\d*|NORMAL\d*|TANGENT\d*|BINORMAL\d*|COLOR\d*|TEXCOORD\d*|BLENDINDICES\d*|BLENDWEIGHT\d*|PSIZE\d*|VPOS|VFACE|DEPTH\d*|FOG)$/;

function readString(stream: StringStream): string {
  while (!stream.eol()) {
    const ch = stream.next();
    if (ch === "\\") {
      stream.next();
    } else if (ch === '"') {
      break;
    }
  }
  return "string";
}

function makeParser(alwaysCode: boolean): StreamParser<ShaderState> {
  return {
    name: alwaysCode ? "hlsl" : "shaderlab",
    startState: () => ({ inBlockComment: false, inCode: alwaysCode, expectSemantic: false }),
    token(stream, state) {
      if (state.inBlockComment) {
        if (stream.match(/.*?\*\//)) {
          state.inBlockComment = false;
        } else {
          stream.skipToEnd();
        }
        return "comment";
      }
      if (stream.eatSpace()) return null;

      // Comments
      if (stream.match("//")) {
        stream.skipToEnd();
        return "comment";
      }
      if (stream.match("/*")) {
        state.inBlockComment = true;
        if (stream.match(/.*?\*\//)) state.inBlockComment = false;
        else stream.skipToEnd();
        return "comment";
      }

      // Preprocessor directives inside HLSL/Cg blocks (or standalone HLSL).
      // Always advance via stream.match; bare `#` or `##` token-pasting falls through below.
      if (state.inCode && stream.match(/^#\s*[a-zA-Z_]\w*/)) {
        state.expectSemantic = false;
        return "meta";
      }

      // Strings
      if (stream.eat('"')) {
        state.expectSemantic = false;
        return readString(stream);
      }

      // ShaderLab texture property types (`2D`, `2DArray`, `3D`) must match before numbers.
      if (!state.inCode && stream.match(/^(?:2DArray|2D|3D)\b/)) {
        return "atom";
      }

      // Numbers (hex, float with suffix h/f/u, int)
      if (stream.match(/^0[xX][0-9a-fA-F]+[uUlL]*/)) {
        state.expectSemantic = false;
        return "number";
      }
      if (stream.match(/^(?:\d+\.\d*|\.\d+|\d+)(?:[eE][+-]?\d+)?[fFhHuUlL]?/)) {
        state.expectSemantic = false;
        return "number";
      }

      // ShaderLab property attributes `[HDR]`, `[MainTexture]`, `[Toggle(...)]`
      if (!state.inCode && stream.peek() === "[") {
        stream.next();
        return "bracket";
      }

      // Identifiers & keywords
      if (stream.match(/^[A-Za-z_]\w*/)) {
        const word = stream.current();
        if (!alwaysCode) {
          if (CODE_START.has(word)) {
            state.inCode = true;
            state.expectSemantic = false;
            return "meta";
          }
          if (CODE_END.has(word)) {
            state.inCode = false;
            state.expectSemantic = false;
            return "meta";
          }
        }
        if (state.inCode) {
          if (state.expectSemantic && SEMANTIC_RE.test(word)) {
            state.expectSemantic = false;
            return "attributeName";
          }
          state.expectSemantic = false;
          if (HLSL_KEYWORDS.has(word)) return "keyword";
          if (HLSL_TYPES.has(word)) return "typeName";
          if (HLSL_INTRINSICS.has(word)) return "standard";
          if (/^(?:UNITY|SHADER|TEXTURE2D|SAMPLER|CBUFFER|TRANSFORM_TEX|SAMPLE_TEXTURE)/.test(word)) {
            return "macroName";
          }
          if (stream.peek() === "(") return "variableName.function";
          return "variableName";
        }
        if (SHADERLAB_KEYWORDS.has(word)) return "keyword";
        if (SHADERLAB_VALUES.has(word)) return "atom";
        if (word.startsWith("_")) return "propertyName";
        return "variableName";
      }

      const ch = stream.next();
      if (ch === ":" && state.inCode) {
        state.expectSemantic = true;
        return "punctuation";
      }
      if (ch && ";,{}()".includes(ch)) {
        state.expectSemantic = false;
        return "punctuation";
      }
      if (ch && "+-*/%=!<>&|^~?".includes(ch)) {
        state.expectSemantic = false;
        return "operator";
      }
      return null;
    },
    languageData: {
      commentTokens: { line: "//", block: { open: "/*", close: "*/" } },
    },
  };
}

export const shaderlabLanguage = StreamLanguage.define(makeParser(false));
export const hlslLanguage = StreamLanguage.define(makeParser(true));
