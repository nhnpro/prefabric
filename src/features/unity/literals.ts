/**
 * Unity C# string-literal completion:
 * - Tags (`CompareTag`, `FindWithTag`, `.tag ==`)
 * - Layers (`LayerMask.NameToLayer`, `LayerMask.GetMask`) & Sorting Layers (`SortingLayer.NameToID`)
 * - Scenes (`SceneManager.LoadScene`, `LoadSceneAsync`, `GetSceneByName`)
 * - Input axes (`Input.GetAxis`, `GetAxisRaw`, `GetButton*`)
 * - Animator parameters (`SetBool`/`SetFloat`/`SetInteger`/`SetTrigger`/`Animator.StringToHash`)
 * - Shader properties (`Shader.PropertyToID`, `SetFloat`/`SetColor`/`SetTexture`/`HasProperty`)
 * - Enclosing-class methods (`Invoke`, `InvokeRepeating`, `CancelInvoke`, `StartCoroutine`, `StopCoroutine`)
 */
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { call, commands, type UnityProjectSymbols } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { classes, declaredMethods } from "./csharp";

export type UnityLiteralKind =
  | "tag"
  | "layer"
  | "sortingLayer"
  | "scene"
  | "inputAxis"
  | "animatorParam"
  | "shaderProperty"
  | "method";

export interface UnityLiteralContext {
  kind: UnityLiteralKind;
  /** Offset right after the opening `"` of the string literal. */
  from: number;
  /** Offset of the closing `"` (or cursor if unclosed). */
  to: number;
}

const TAG_CALL = /\b(?:CompareTag|FindWithTag|FindGameObjectWithTag|FindGameObjectsWithTag)\s*\(\s*$/;
const TAG_OP = /\.tag\s*(?:==|!=|=)\s*$/;
const LAYER_CALL = /\bLayerMask\s*\.\s*(?:NameToLayer|GetMask)\s*\((?:[^()]*,\s*)?$/;
const SORTING_LAYER_CALL = /\bSortingLayer\s*\.\s*NameToID\s*\(\s*$/;
const SCENE_CALL = /\bSceneManager\s*\.\s*(?:LoadScene|LoadSceneAsync|GetSceneByName|UnloadSceneAsync)\s*\(\s*$/;
const INPUT_CALL = /\bInput\s*\.\s*(?:GetAxis|GetAxisRaw|GetButton|GetButtonDown|GetButtonUp)\s*\(\s*$/;
const ANIMATOR_CALL =
  /\b(?:SetBool|SetFloat|SetInteger|SetTrigger|ResetTrigger|GetBool|GetFloat|GetInteger|StringToHash)\s*\(\s*$/;
const SHADER_CALL =
  /\b(?:PropertyToID|SetColor|SetVector|SetTexture|SetMatrix|SetBuffer|SetConstantBuffer|SetInt|GetColor|GetVector|GetTexture|GetMatrix|GetInt|HasProperty|HasFloat|HasColor|HasTexture|HasInt|HasVector)\s*\(\s*$/;
const METHOD_CALL = /\b(?:Invoke|InvokeRepeating|CancelInvoke|IsInvoking|StartCoroutine|StopCoroutine)\s*\(\s*$/;

/** Finds the string literal around `pos` on the current line and classifies the Unity API call preceding it. */
export function unityLiteralContext(line: string, lineFrom: number, pos: number): UnityLiteralContext | null {
  const col = pos - lineFrom;
  let open = -1;
  let close = -1;
  for (let i = 0; i < line.length && i <= col; i++) {
    if (line[i] === "/" && line[i + 1] === "/") break;
    // Skip C# character literals (`'"'`, `'\''`) so quotes inside them don't desync scanning.
    if (line[i] === "'") {
      let j = i + 1;
      while (j < line.length && line[j] !== "'") {
        j += line[j] === "\\" ? 2 : 1;
      }
      i = j;
      continue;
    }
    if (line[i] !== '"') continue;
    const verbatim = i > 0 && line[i - 1] === "@";
    let j = i + 1;
    while (j < line.length) {
      if (verbatim && line[j] === '"' && line[j + 1] === '"') {
        j += 2;
      } else if (!verbatim && line[j] === "\\") {
        j += 2;
      } else if (line[j] === '"') {
        break;
      } else {
        j++;
      }
    }
    if (i < col && col <= j) {
      open = i;
      close = j;
      break;
    }
    i = j;
  }
  if (open < 0) return null;
  const before = line.slice(0, open > 0 && line[open - 1] === "@" ? open - 1 : open);
  const from = lineFrom + open + 1;
  const to = lineFrom + Math.min(close, line.length);
  if (TAG_CALL.test(before) || TAG_OP.test(before)) return { kind: "tag", from, to };
  if (LAYER_CALL.test(before)) return { kind: "layer", from, to };
  if (SORTING_LAYER_CALL.test(before)) return { kind: "sortingLayer", from, to };
  if (SCENE_CALL.test(before)) return { kind: "scene", from, to };
  if (INPUT_CALL.test(before)) return { kind: "inputAxis", from, to };
  if (METHOD_CALL.test(before)) return { kind: "method", from, to };
  if (SHADER_CALL.test(before)) return { kind: "shaderProperty", from, to };
  if (ANIMATOR_CALL.test(before)) {
    // Disambiguate `material.SetFloat(...)` vs `animator.SetFloat(...)` using receiver hints when present.
    if (/\b(?:mat|material|sharedMaterial|mpb|block|Shader)\s*\.\s*(?:SetFloat|GetFloat)\s*\(\s*$/i.test(before)) {
      return { kind: "shaderProperty", from, to };
    }
    return { kind: "animatorParam", from, to };
  }
  return null;
}

const CACHE_TTL_MS = 10_000;
let cacheGen = 0;
let cached: { root: string; at: number; symbols: UnityProjectSymbols } | null = null;
let inflight: Promise<UnityProjectSymbols | null> | null = null;

export function clearUnitySymbolsCache() {
  cacheGen++;
  cached = null;
  inflight = null;
}

async function loadProjectSymbols(): Promise<UnityProjectSymbols | null> {
  const project = useProject.getState().project;
  if (!project?.unity) return null;
  const root = project.root;
  const now = Date.now();
  if (cached && cached.root === root && now - cached.at < CACHE_TTL_MS) return cached.symbols;
  if (!inflight) {
    const gen = cacheGen;
    const req = call(commands.unityProjectSymbols())
      .then((symbols) => {
        if (gen === cacheGen && useProject.getState().project?.root === root) {
          cached = { root, at: Date.now(), symbols };
        }
        return symbols;
      })
      .catch(() => null)
      .finally(() => {
        if (inflight === req) inflight = null;
      });
    inflight = req;
  }
  return inflight;
}

function itemsFor(kind: Exclude<UnityLiteralKind, "method">, s: UnityProjectSymbols): readonly string[] {
  switch (kind) {
    case "tag":
      return s.tags;
    case "layer":
      return s.layers;
    case "sortingLayer":
      return s.sortingLayers;
    case "scene":
      return s.scenes;
    case "inputAxis":
      return s.inputAxes;
    case "animatorParam":
      return s.animatorParams;
    case "shaderProperty":
      return s.shaderProperties;
  }
}

const KIND_TYPE: Record<UnityLiteralKind, string> = {
  tag: "Tag",
  layer: "Layer",
  sortingLayer: "Sorting Layer",
  scene: "Scene",
  inputAxis: "Input Axis",
  animatorParam: "Animator",
  shaderProperty: "Shader",
  method: "Method",
};

/** Completion source for Unity C# string literals. */
export function unityLiteralSource(loadSymbols: () => Promise<UnityProjectSymbols | null> = loadProjectSymbols) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const hit = unityLiteralContext(line.text, line.from, ctx.pos);
    if (!hit) return null;

    if (hit.kind === "method") {
      const text = ctx.state.doc.toString();
      const cls =
        classes(text)
          .filter((c) => c.bodyStart <= ctx.pos && ctx.pos <= c.bodyEnd)
          .sort((a, b) => b.bodyStart - a.bodyStart)[0] ?? null;
      if (!cls) return null;
      const names = [...declaredMethods(text, cls)].sort();
      if (!names.length) return null;
      const options: Completion[] = names.map((name) => ({
        label: name,
        type: "method",
        detail: KIND_TYPE.method,
      }));
      return { from: hit.from, to: hit.to, options, validFor: /^[^"]*$/ };
    }

    const symbols = await loadSymbols();
    if (!symbols || ctx.aborted) return null;
    const values = itemsFor(hit.kind, symbols);
    if (!values.length) return null;
    const tag = KIND_TYPE[hit.kind];
    const options: Completion[] = values.map((value) => ({
      label: value,
      type: "text",
      detail: tag,
    }));
    return { from: hit.from, to: hit.to, options, validFor: /^[^"]*$/ };
  };
}
