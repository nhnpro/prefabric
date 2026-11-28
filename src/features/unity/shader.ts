/**
 * Unity ShaderLab & HLSL editor support:
 * - Go to Definition on `#include "..."` directives (resolves relative paths, `Assets/...`,
 *   `Packages/<pkg>/...`, and `Library/PackageCache/<pkg>@*`).
 * - Completion for `#pragma` directives, ShaderLab blocks/render states, and Unity HLSL macros.
 */
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { EditorState, type Extension } from "@codemirror/state";
import { CONTRIBUTED_DEFINITION, type ContributedDefinition } from "@/editor/api";
import { tx } from "@/platform/i18n";
import { call, commands } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { editorService, notifications } from "@/workbench/contributions";

export interface IncludeRef {
  path: string;
  from: number;
  to: number;
}

const isShaderFile = (path: string) => /\.(shader|hlsl|cginc|glslinc|compute|raytrace|shaderinc)$/i.test(path);

/** Extracts the `#include "..."` path covering `pos` on a single line. */
export function includeAtLine(line: string, lineFrom: number, pos: number): IncludeRef | null {
  const m = /^\s*#\s*include(?:_with_pragmas)?\s+"([^"]+)"/.exec(line);
  if (!m) return null;
  const target = m[1]!;
  const quoteIdx = line.indexOf('"');
  const from = lineFrom + quoteIdx + 1;
  const to = from + target.length;
  if (pos < from - 1 || pos > to + 1) return null;
  return { path: target, from, to };
}

/** Extracts the `#include "..."` path covering `pos`, if any. */
export function includeAt(text: string, pos: number): IncludeRef | null {
  const lineStart = pos <= 0 ? 0 : text.lastIndexOf("\n", pos - 1) + 1;
  let lineEnd = text.indexOf("\n", pos);
  if (lineEnd < 0) lineEnd = text.length;
  return includeAtLine(text.slice(lineStart, lineEnd), lineStart, pos);
}

function norm(p: string): string {
  return p.replaceAll("\\", "/");
}

function dirname(p: string): string {
  const n = norm(p);
  const i = n.lastIndexOf("/");
  return i > 0 ? n.slice(0, i) : "";
}

function joinPath(base: string, rel: string): string {
  const parts = `${norm(base)}/${norm(rel)}`.split("/");
  const out: string[] = [];
  for (const part of parts) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (out.length && out[out.length - 1] !== "..") out.pop();
    } else {
      out.push(part);
    }
  }
  return (norm(base).startsWith("/") ? "/" : "") + out.join("/");
}

/** Candidate filesystem paths for a Unity shader `#include` target (before PackageCache lookup). */
export function includeCandidates(fromFile: string, projectRoot: string | null, includePath: string): string[] {
  const clean = norm(includePath).replace(/^\/+/, "");
  const out: string[] = [];
  const dir = dirname(fromFile);
  if (dir && !clean.startsWith("Assets/") && !clean.startsWith("Packages/")) {
    out.push(joinPath(dir, clean));
  }
  if (projectRoot) {
    const root = norm(projectRoot).replace(/\/+$/, "");
    if (clean.startsWith("Assets/") || clean.startsWith("Packages/")) {
      out.push(joinPath(root, clean));
    } else if (!clean.startsWith("./") && !clean.startsWith("../")) {
      out.push(joinPath(root, `Assets/${clean}`));
    }
  }
  return [...new Set(out)];
}

async function fileExists(path: string): Promise<boolean> {
  try {
    await call(commands.fsProbe(path));
    return true;
  } catch {
    return false;
  }
}

/** Resolves `Packages/<pkg>/<subpath>` inside `Library/PackageCache/<pkg>@<ver>/<subpath>`. */
async function resolvePackageCache(projectRoot: string, includePath: string): Promise<string | null> {
  const m = /^Packages\/([^/]+)\/(.+)$/.exec(norm(includePath));
  if (!m) return null;
  const [, pkgName, rest] = m;
  const cacheDir = joinPath(projectRoot, "Library/PackageCache");
  const entries = await call(commands.fsReadDir(cacheDir)).catch(() => null);
  if (!entries) return null;
  const prefix = `${pkgName}@`;
  for (const entry of entries) {
    if (entry.kind !== "dir" && !(entry.kind === "symlink" && entry.targetKind === "dir")) continue;
    if (entry.name === pkgName || entry.name.startsWith(prefix)) {
      const candidate = joinPath(`${cacheDir}/${entry.name}`, rest!);
      if (await fileExists(candidate)) return candidate;
    }
  }
  return null;
}

async function openInclude(fromFile: string, includePath: string) {
  const projectRoot = useProject.getState().project?.root ?? null;
  for (const candidate of includeCandidates(fromFile, projectRoot, includePath)) {
    if (await fileExists(candidate)) {
      await editorService().openFile(candidate, { focus: true });
      return;
    }
  }
  if (projectRoot) {
    const cached = await resolvePackageCache(projectRoot, includePath);
    if (cached) {
      await editorService().openFile(cached, { focus: true });
      return;
    }
  }
  notifications().notify({
    level: "warning",
    title: tx`Shader include '${includePath}' not found in project`,
    key: "unity.shader",
  });
}

type ShaderItem = readonly [name: string, sig: string | undefined, kind: string, doc: string];

function toCompletions(items: readonly ShaderItem[]): Completion[] {
  return items.map(([name, sig, kind, doc]) => ({
    label: name,
    ...(sig ? { detail: sig } : {}),
    type: kind,
    info: doc,
  }));
}

const PRAGMA_ITEMS: readonly ShaderItem[] = [
  ["vertex", "vert", "keyword", "Declares the vertex shader entry function."],
  ["fragment", "frag", "keyword", "Declares the fragment shader entry function."],
  ["surface", "surf Standard", "keyword", "Declares a Built-in RP surface shader."],
  ["kernel", "CSMain", "keyword", "Declares a compute shader kernel entry point."],
  ["target", "3.5", "keyword", "Sets the shader compilation target level."],
  ["multi_compile", "_ KEYWORD", "keyword", "Compiles shader variants for every keyword."],
  ["multi_compile_local", "_ KEYWORD", "keyword", "Local-keyword multi_compile variants."],
  ["multi_compile_fog", undefined, "keyword", "Compiles built-in fog shader variants."],
  ["multi_compile_instancing", undefined, "keyword", "Compiles GPU instancing shader variants."],
  ["shader_feature", "_ KEYWORD", "keyword", "Compiles variants only when enabled on materials."],
  ["shader_feature_local", "_ KEYWORD", "keyword", "Local-keyword shader_feature variants."],
  ["require", "2darray", "keyword", "Declares required GPU hardware features."],
  ["prefer_hlslcc", "gles", "keyword", "Prefers HLSLcc translator for specified backends."],
  ["only_renderers", undefined, "keyword", "Restricts compilation to specific graphics APIs."],
  ["exclude_renderers", undefined, "keyword", "Excludes specific graphics APIs from compilation."],
];

const SHADERLAB_ITEMS: readonly ShaderItem[] = [
  ["Properties", undefined, "keyword", "Material properties exposed in the Unity Inspector."],
  ["SubShader", undefined, "keyword", "SubShader block for a target render pipeline / LOD."],
  ["Pass", undefined, "keyword", "Rendering pass inside a SubShader."],
  ["Tags", '{ "RenderType"="Opaque" }', "keyword", "Key-value tags for render pipeline and queue."],
  ["HLSLPROGRAM", undefined, "keyword", "Starts an HLSL shader program block (URP / HDRP / SRP)."],
  ["ENDHLSL", undefined, "keyword", "Ends an HLSLPROGRAM or HLSLINCLUDE block."],
  ["HLSLINCLUDE", undefined, "keyword", "Shared HLSL code included in every pass of this SubShader."],
  ["CGPROGRAM", undefined, "keyword", "Starts a Cg/HLSL shader program block (Built-in RP)."],
  ["ENDCG", undefined, "keyword", "Ends a CGPROGRAM or CGINCLUDE block."],
  ["CGINCLUDE", undefined, "keyword", "Shared Cg/HLSL code included in every pass."],
  ["Blend", "SrcAlpha OneMinusSrcAlpha", "keyword", "Alpha blending state."],
  ["Cull", "Back | Front | Off", "keyword", "Face culling mode."],
  ["ZWrite", "On | Off", "keyword", "Controls writing to the depth buffer."],
  ["ZTest", "LEqual | Always | Greater", "keyword", "Depth test comparison mode."],
  ["Stencil", undefined, "keyword", "Stencil buffer test and operations."],
  ["Fallback", '"Hidden/InternalErrorShader"', "keyword", "Fallback shader if no SubShader matches."],
  ["CustomEditor", undefined, "keyword", "Custom ShaderGUI class name."],
];

const UNITY_HLSL_ITEMS: readonly ShaderItem[] = [
  ["TEXTURE2D", "(textureName)", "function", "Declares a 2D texture (URP/HDRP Core.hlsl)."],
  ["SAMPLER", "(samplerName)", "function", "Declares a texture sampler state (URP/HDRP Core.hlsl)."],
  ["SAMPLE_TEXTURE2D", "(tex, sampler, uv)", "function", "Samples a 2D texture with an explicit sampler."],
  ["CBUFFER_START", "(UnityPerMaterial)", "function", "Starts an SRP Batcher-compatible constant buffer."],
  ["CBUFFER_END", undefined, "keyword", "Ends a CBUFFER_START block."],
  ["TRANSFORM_TEX", "(uv, texName)", "function", "Applies material tiling and offset (_ST) to UVs."],
  [
    "TransformObjectToHClip",
    "(positionOS)",
    "function",
    "Transforms object-space position to homogeneous clip space (URP/SRP).",
  ],
  ["TransformObjectToWorld", "(positionOS)", "function", "Transforms object-space position to world space (URP/SRP)."],
  [
    "TransformObjectToWorldNormal",
    "(normalOS)",
    "function",
    "Transforms object-space normal to world space (URP/SRP).",
  ],
  ["UnityObjectToClipPos", "(pos)", "function", "Transforms object-space position to clip space (Built-in RP)."],
  ["UnityObjectToWorldNormal", "(norm)", "function", "Transforms object-space normal to world space (Built-in RP)."],
  ["UNITY_MATRIX_MVP", undefined, "constant", "Model-View-Projection matrix."],
  ["UNITY_MATRIX_M", undefined, "constant", "Object-to-World model matrix."],
  ["UNITY_MATRIX_V", undefined, "constant", "World-to-View camera matrix."],
  ["UNITY_MATRIX_P", undefined, "constant", "Projection matrix."],
  ["UNITY_VERTEX_INPUT_INSTANCE_ID", undefined, "constant", "Declares GPU instancing ID in vertex input struct."],
  ["UNITY_SETUP_INSTANCE_ID", "(input)", "function", "Initializes GPU instancing ID in shader function."],
];

const PRAGMA_COMPLETIONS = toCompletions(PRAGMA_ITEMS);
const SHADERLAB_COMPLETIONS = toCompletions(SHADERLAB_ITEMS);
const UNITY_HLSL_COMPLETIONS = toCompletions(UNITY_HLSL_ITEMS);
const ALL_SHADERLAB_COMPLETIONS = [...SHADERLAB_COMPLETIONS, ...UNITY_HLSL_COMPLETIONS];

export function shaderCompletionSource(path: string) {
  const options = path.toLowerCase().endsWith(".shader") ? ALL_SHADERLAB_COMPLETIONS : UNITY_HLSL_COMPLETIONS;
  return (ctx: CompletionContext): CompletionResult | null => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    const pragma = /^\s*#\s*pragma\s+(\w*)$/.exec(before);
    if (pragma) {
      const word = pragma[1]!;
      return { from: ctx.pos - word.length, options: PRAGMA_COMPLETIONS, validFor: /^\w*$/ };
    }
    const word = ctx.matchBefore(/[A-Za-z_]\w*/);
    if (!word && !ctx.explicit) return null;
    const from = word ? word.from : ctx.pos;
    return { from, options, validFor: /^\w*$/ };
  };
}

export function shaderSupport(path: string): Extension {
  if (!isShaderFile(path)) return [];
  const completion = shaderCompletionSource(path);
  const definition: ContributedDefinition = (view, pos) => {
    const line = view.state.doc.lineAt(pos);
    const hit = includeAtLine(line.text, line.from, pos);
    if (!hit) return false;
    void openInclude(path, hit.path).catch((e) => console.debug("[shader]", e));
    return true;
  };
  return [
    EditorState.languageData.of((state, pos) => {
      const line = state.doc.lineAt(pos);
      return [
        { openideaCompletion: completion },
        ...(includeAtLine(line.text, line.from, pos) ? [{ [CONTRIBUTED_DEFINITION]: definition }] : []),
      ];
    }),
  ];
}
