/**
 * Unity performance & allocation analysis for frequently called code ("hot paths"):
 * - Builds an intra-file call graph from Unity per-frame callbacks (`Update`, `FixedUpdate`,
 *   `LateUpdate`, `OnGUI`, `OnTriggerStay`, etc.) to identify transitively called helper methods.
 * - Marks transitive hot-path methods in the gutter with their call chain.
 * - Highlights expensive lookups, string-based Unity API calls, physics allocators, LINQ, and
 *   managed heap allocations inside hot-path methods with inline underlines and hover tooltips.
 */
import {
  Compartment,
  RangeSet,
  RangeSetBuilder,
  StateEffect,
  StateField,
  type Extension,
  type Text,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  GutterMarker,
  ViewPlugin,
  hoverTooltip,
  type DecorationSet,
  type Tooltip,
  type ViewUpdate,
} from "@codemirror/view";
import { iconAreaSource, mapLineStart, shownAgain } from "@/editor/api";
import { tx } from "@/platform/i18n";
import { iconMarkup } from "@/ui";
import markerStyles from "@/editor/navigation/LineMarkers.module.css";
import { unityBaseOf, type UnityDeps } from "./bases";
import { classes, codeOnly, type ClassInfo } from "./csharp";
import { unityTypeNames } from "./messages";
import styles from "./Unity.module.css";

const DEBOUNCE_MS = 450;

/** Unity event functions invoked per frame, per physics tick, or per render/GUI pass. */
export const HOT_PATH_ROOTS: ReadonlySet<string> = new Set([
  "Update",
  "FixedUpdate",
  "LateUpdate",
  "OnGUI",
  "OnTriggerStay",
  "OnTriggerStay2D",
  "OnCollisionStay",
  "OnCollisionStay2D",
  "OnWillRenderObject",
  "OnRenderObject",
  "OnPreRender",
  "OnPostRender",
  "OnPreCull",
  "OnAnimatorIK",
  "OnAnimatorMove",
  "OnAudioFilterRead",
  "OnParticleCollision",
  "OnParticleTrigger",
]);

export interface MethodBody {
  name: string;
  /** Offset of the method name. */
  nameAt: number;
  /** Start of the method body (`{` or `=>`). */
  bodyStart: number;
  /** Exclusive end of the method body. */
  bodyEnd: number;
}

export type HotPathKind = "expensive" | "allocation";

export type HotPathRuleId =
  | "camera-main"
  | "get-component"
  | "find-object"
  | "send-message"
  | "tag-comparison"
  | "shader-string"
  | "animator-string"
  | "physics-alloc"
  | "wait-for-seconds"
  | "collection-alloc"
  | "linq-alloc"
  | "string-alloc";

export interface HotPathMethod {
  name: string;
  at: number;
  /** Call chain from a Unity per-frame callback to this method (e.g. `["Update", "Move", "ApplyGravity"]`). */
  chain: string[];
}

export interface HotPathIssue {
  from: number;
  to: number;
  rule: HotPathRuleId;
  kind: HotPathKind;
  /** Call chain of the enclosing hot-path method (e.g. `["Update"]` or `["Update", "Move"]`). */
  chain: string[];
}

export interface HotPathAnalysis {
  /** Transitively called helper methods (excluding the root callbacks themselves). */
  transitiveMethods: HotPathMethod[];
  /** Performance & allocation issues inside hot-path roots and transitive helpers. */
  issues: HotPathIssue[];
}

/** Extracts methods declared directly in `cls` together with their body spans. */
export function methodBodies(text: string, cls: ClassInfo): MethodBody[] {
  const code = codeOnly(text);
  let level = "";
  let depth = 0;
  for (let i = cls.bodyStart; i < cls.bodyEnd; i++) {
    const c = code[i]!;
    if (c === "{") {
      level += depth === 0 ? "{" : " ";
      depth++;
    } else if (c === "}") {
      depth--;
      level += " ";
    } else {
      level += depth === 0 || c === "\n" ? c : " ";
    }
  }
  const found: MethodBody[] = [];
  const sig = /\b[A-Za-z_][\w.<>[\],]*\s+([A-Za-z_]\w*)\s*(?:<[^>{};]*>\s*)?\([^)]*\)\s*(\{|=>)/g;
  for (const m of level.matchAll(sig)) {
    const name = m[1]!;
    const paren = m[0].indexOf("(");
    const nameAt = cls.bodyStart + m.index + m[0].lastIndexOf(name, paren);
    const opener = cls.bodyStart + m.index + m[0].length - m[2]!.length;
    let bodyEnd = cls.bodyEnd;
    if (m[2] === "{") {
      let d = 0;
      for (let i = opener; i < cls.bodyEnd; i++) {
        if (code[i] === "{") d++;
        else if (code[i] === "}" && --d === 0) {
          bodyEnd = i + 1;
          break;
        }
      }
    } else {
      // Expression-bodied method: `=> expr;`
      let d = 0;
      for (let i = opener + 2; i < cls.bodyEnd; i++) {
        const ch = code[i]!;
        if (ch === "{" || ch === "(" || ch === "[") d++;
        else if (ch === "}" || ch === ")" || ch === "]") d = Math.max(0, d - 1);
        else if (ch === ";" && d === 0) {
          bodyEnd = i + 1;
          break;
        }
      }
    }
    found.push({ name, nameAt, bodyStart: opener, bodyEnd });
  }
  return found;
}

interface RuleSpec {
  id: HotPathRuleId;
  kind: HotPathKind;
  pattern: RegExp;
  /** Optional guard receiving the matched code slice and offset within the method body. */
  accept?: (bodyCode: string, match: RegExpExecArray) => boolean;
}

const RULES: readonly RuleSpec[] = [
  {
    id: "camera-main",
    kind: "expensive",
    pattern: /\bCamera\s*\.\s*main\b/g,
  },
  {
    id: "get-component",
    kind: "expensive",
    pattern:
      /\b(?:GetComponent|GetComponents|GetComponentInChildren|GetComponentsInChildren|GetComponentInParent|GetComponentsInParent)\s*(?:<[^>;\n]+>)?\s*\(/g,
  },
  {
    id: "find-object",
    kind: "expensive",
    pattern:
      /\b(?:FindObjectOfType|FindObjectsOfType|FindFirstObjectByType|FindAnyObjectByType|FindObjectsByType|GameObject\s*\.\s*Find(?:WithTag|GameObjectWithTag|GameObjectsWithTag)?|Resources\s*\.\s*Load(?:All)?)\s*(?:<[^>;\n]+>)?\s*\(/g,
  },
  {
    id: "send-message",
    kind: "expensive",
    pattern: /\b(?:SendMessage|SendMessageUpwards|BroadcastMessage)\s*\(/g,
  },
  {
    id: "tag-comparison",
    kind: "allocation",
    pattern: /\.tag\s*(?:==|!=)/g,
  },
  {
    id: "shader-string",
    kind: "expensive",
    pattern:
      /\.\s*(?:SetFloat|GetFloat|SetInt|GetInt|SetColor|GetColor|SetVector|GetVector|SetTexture|GetTexture|SetMatrix|GetMatrix|HasProperty|HasFloat|HasColor|HasTexture|HasInt|HasVector)\s*\(\s*"/g,
    accept: (bodyCode, m) => {
      // Avoid flagging `animator.SetFloat("Speed", ...)` or `PlayerPrefs`/`EditorPrefs` as shader lookups.
      const before = bodyCode.slice(Math.max(0, m.index - 24), m.index);
      return !/\b(?:anim|animator|PlayerPrefs|EditorPrefs)\s*$/i.test(before);
    },
  },
  {
    id: "animator-string",
    kind: "expensive",
    pattern:
      /\.\s*(?:SetBool|GetBool|SetInteger|GetInteger|SetFloat|GetFloat|SetTrigger|ResetTrigger|Play|CrossFade)\s*\(\s*"/g,
    accept: (bodyCode, m) => {
      // Only treat SetFloat/GetFloat as Animator lookups when the receiver looks like an Animator.
      if (!/^\.\s*(?:SetFloat|GetFloat)\b/.test(m[0])) return true;
      const before = bodyCode.slice(Math.max(0, m.index - 24), m.index);
      return /\b(?:anim|animator)\s*$/i.test(before);
    },
  },
  {
    id: "physics-alloc",
    kind: "allocation",
    pattern:
      /\b(?:Physics\s*\.\s*(?:RaycastAll|SphereCastAll|BoxCastAll|CapsuleCastAll|OverlapSphere|OverlapBox|OverlapCapsule)|Physics2D\s*\.\s*(?:RaycastAll|OverlapCircleAll|OverlapBoxAll|OverlapAreaAll))\s*\(/g,
  },
  {
    id: "wait-for-seconds",
    kind: "allocation",
    pattern: /\bnew\s+(?:WaitForSeconds|WaitForSecondsRealtime)\s*\(/g,
  },
  {
    id: "collection-alloc",
    kind: "allocation",
    pattern:
      /\bnew\s+(?:(?:List|Dictionary|HashSet|Queue|Stack)\s*<[^>;\n]+>|StringBuilder\s*\(|[A-Za-z_][\w.]*\s*\[[^\]]*\])/g,
  },
  {
    id: "linq-alloc",
    kind: "allocation",
    pattern:
      /\.\s*(?:Where|Select|SelectMany|OrderBy|OrderByDescending|GroupBy|ToList|ToArray|Any|All|First|FirstOrDefault|Last|LastOrDefault|Single|SingleOrDefault|Count|Distinct)\s*(?:<[^>;\n]+>)?\s*\(/g,
  },
  {
    id: "string-alloc",
    kind: "allocation",
    pattern: /\b(?:string|String)\s*\.\s*(?:Format|Concat|Join)\s*\(|(?:\$@?|@\$)"/g,
  },
];

/** Localized explanation for a hot-path rule. */
export function hotPathRuleMessage(rule: HotPathRuleId): string {
  switch (rule) {
    case "camera-main":
      return tx("Camera.main searches tagged objects; cache the Camera reference in Awake or Start.");
    case "get-component":
      return tx("Expensive component lookup in frequently called code; cache the component in Awake or Start.");
    case "find-object":
      return tx("Expensive scene or asset lookup in frequently called code; cache the result outside the update loop.");
    case "send-message":
      return tx(
        "Reflection-based message dispatch is slow in frequently called code; call the method directly or use an event.",
      );
    case "tag-comparison":
      return tx("Comparing .tag allocates a managed string; use CompareTag(...) instead.");
    case "shader-string":
      return tx("String-based shader property lookup in frequently called code; cache an ID with Shader.PropertyToID.");
    case "animator-string":
      return tx("String-based Animator lookup in frequently called code; cache a hash with Animator.StringToHash.");
    case "physics-alloc":
      return tx("Allocates a new array every call; prefer the NonAlloc overload with a preallocated buffer.");
    case "wait-for-seconds":
      return tx("Allocates a WaitForSeconds instance; cache and reuse it.");
    case "collection-alloc":
      return tx("Heap allocation in frequently called code; reuse a preallocated collection or buffer.");
    case "linq-alloc":
      return tx("LINQ allocates enumerators and closures in frequently called code; prefer a for loop.");
    case "string-alloc":
      return tx("String formatting allocates on the managed heap in frequently called code.");
  }
}

/** Finds direct intra-class callees invoked inside `bodyCode`. */
function directCallees(bodyCode: string, classMethodNames: ReadonlySet<string>): string[] {
  const callees = new Set<string>();
  const callRe = /(?:(\bthis\s*\.\s*)|(?<![.\w]))([A-Za-z_]\w*)\s*(?:<[^>{};()\n]*>\s*)?\(/g;
  for (const m of bodyCode.matchAll(callRe)) {
    const name = m[2]!;
    if (classMethodNames.has(name)) callees.add(name);
  }
  return [...callees];
}

/** Pure analysis of a C# file's Unity classes, given a predicate/resolver for whether a class is a Unity type. */
export async function analyzeUnityHotPaths(path: string, text: string, deps: UnityDeps): Promise<HotPathAnalysis> {
  const data = await deps.data();
  if (!data) return { transitiveMethods: [], issues: [] };
  const unityTypes = unityTypeNames(data);
  const code = codeOnly(text);
  const transitiveMethods: HotPathMethod[] = [];
  const issues: HotPathIssue[] = [];

  for (const cls of classes(text)) {
    const methods = methodBodies(text, cls);
    if (!methods.length) continue;
    const base = await unityBaseOf(path, text, cls, unityTypes, deps);
    if (!base) continue;

    // Fast path: skip call-graph construction when the class declares no per-frame Unity callback.
    const chainByName = new Map<string, string[]>();
    const queue: string[] = [];
    for (const m of methods) {
      if (HOT_PATH_ROOTS.has(m.name) && !chainByName.has(m.name)) {
        chainByName.set(m.name, [m.name]);
        queue.push(m.name);
      }
    }
    if (!queue.length) continue;

    // Build intra-class adjacency list: caller name -> callee names.
    const methodNames = new Set(methods.map((m) => m.name));
    const edges = new Map<string, Set<string>>();
    for (const m of methods) {
      const slice = code.slice(m.bodyStart, m.bodyEnd);
      let set = edges.get(m.name);
      if (!set) {
        set = new Set<string>();
        edges.set(m.name, set);
      }
      for (const callee of directCallees(slice, methodNames)) {
        if (callee !== m.name) set.add(callee);
      }
    }

    // BFS from every per-frame Unity callback in this class.
    while (queue.length > 0) {
      const cur = queue.shift()!;
      const chain = chainByName.get(cur)!;
      for (const next of edges.get(cur) ?? []) {
        if (!chainByName.has(next)) {
          chainByName.set(next, [...chain, next]);
          queue.push(next);
        }
      }
    }

    // Record transitive helpers and scan all hot-path methods for performance issues.
    for (const m of methods) {
      const chain = chainByName.get(m.name);
      if (!chain) continue;
      if (chain.length > 1) {
        transitiveMethods.push({ name: m.name, at: m.nameAt, chain });
      }
      const slice = code.slice(m.bodyStart, m.bodyEnd);
      const seenOffsets = new Set<number>();
      for (const rule of RULES) {
        rule.pattern.lastIndex = 0;
        let match: RegExpExecArray | null;
        while ((match = rule.pattern.exec(slice)) !== null) {
          if (rule.accept && !rule.accept(slice, match)) continue;
          const from = m.bodyStart + match.index;
          if (seenOffsets.has(from)) continue;
          seenOffsets.add(from);
          const trimmed = match[0].replace(/\s*\(\s*"?$/, "");
          const to = from + (trimmed.length || match[0].length);
          issues.push({ from, to, rule: rule.id, kind: rule.kind, chain });
        }
      }
    }
  }

  transitiveMethods.sort((a, b) => a.at - b.at);
  issues.sort((a, b) => a.from - b.from || a.to - b.to);
  return { transitiveMethods, issues };
}

interface PlacedHotMethod {
  pos: number;
  method: HotPathMethod;
}

interface PerfState {
  methods: PlacedHotMethod[];
  issues: HotPathIssue[];
}

const setPerfState = StateEffect.define<PerfState>();

const perfField = StateField.define<PerfState>({
  create: () => ({ methods: [], issues: [] }),
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setPerfState)) return e.value;
    if (!tr.docChanged) return value;
    const methods = value.methods.map((m) => ({ ...m, pos: mapLineStart(tr, m.pos) }));
    const issues: HotPathIssue[] = [];
    for (const i of value.issues) {
      const from = tr.changes.mapPos(i.from, 1);
      const to = tr.changes.mapPos(i.to, -1);
      if (to > from) issues.push({ ...i, from, to });
    }
    return { methods, issues };
  },
});

const issueDecorations = EditorView.decorations.compute([perfField], (state): DecorationSet => {
  const { issues } = state.field(perfField);
  if (!issues.length) return Decoration.none;
  const builder = new RangeSetBuilder<Decoration>();
  const mark = Decoration.mark({ class: styles.hotPathIssue! });
  let lastEnd = -1;
  for (const issue of issues) {
    if (issue.from < lastEnd || issue.to > state.doc.length) continue;
    builder.add(issue.from, issue.to, mark);
    lastEnd = issue.to;
  }
  return builder.finish();
});

function placeHotMethods(doc: Text, found: readonly HotPathMethod[]): PlacedHotMethod[] {
  const placed: PlacedHotMethod[] = [];
  for (const method of found) {
    if (method.at > doc.length) continue;
    const pos = doc.lineAt(method.at).from;
    if (placed[placed.length - 1]?.pos !== pos) placed.push({ pos, method });
  }
  return placed;
}

function hotMethodTooltip(m: HotPathMethod): string {
  return `${tx("Frequently called Unity code")}\n${m.chain.join(" → ")}`;
}

class HotPathMarker extends GutterMarker {
  constructor(readonly method: HotPathMethod) {
    super();
  }
  override eq(other: GutterMarker): boolean {
    return (
      other instanceof HotPathMarker &&
      other.method.name === this.method.name &&
      other.method.chain.join(">") === this.method.chain.join(">")
    );
  }
  override toDOM(): Node {
    const el = document.createElement("span");
    el.className = `${markerStyles.marker!} ${styles.hotPathMarker!}`;
    const tip = hotMethodTooltip(this.method);
    el.title = tip;
    el.setAttribute("aria-label", tip);
    el.innerHTML = iconMarkup("warning", 12) ?? "";
    return el;
  }
}

const hotPathColumn: Extension = iconAreaSource.of({
  order: 65,
  markers: (view) =>
    RangeSet.of(
      view.state.field(perfField).methods.map((p) => new HotPathMarker(p.method).range(p.pos)),
      true,
    ),
});

const hotPathTooltip: Extension = hoverTooltip((view, pos): Tooltip | null => {
  const { issues } = view.state.field(perfField);
  const hit = issues.find((i) => i.from <= pos && pos <= i.to);
  if (!hit) return null;
  return {
    pos: hit.from,
    end: hit.to,
    above: true,
    create() {
      const dom = document.createElement("div");
      dom.className = styles.hotPathTooltip!;
      const title = document.createElement("div");
      title.className = styles.hotPathTitle!;
      title.textContent =
        hit.kind === "allocation" ? tx("Unity allocation in hot path") : tx("Unity performance warning");
      const detail = document.createElement("div");
      detail.className = styles.hotPathDetail!;
      detail.textContent = hotPathRuleMessage(hit.rule);
      const chain = document.createElement("div");
      chain.className = styles.hotPathChain!;
      chain.textContent = `${tx("Hot path:")} ${hit.chain.join(" → ")}`;
      dom.append(title, detail, chain);
      return { dom };
    },
  };
});

class UnityPerformancePlugin {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private generation = 0;
  private shown = false;
  private skipped = false;

  constructor(
    private view: EditorView,
    private path: string,
    private deps: UnityDeps,
    private column: Compartment,
  ) {
    this.schedule(0);
  }

  update(u: ViewUpdate) {
    if (u.docChanged) this.schedule(DEBOUNCE_MS);
    else if (this.skipped && shownAgain(u)) {
      this.skipped = false;
      this.schedule(0);
    }
  }

  private schedule(delay: number) {
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.run(), delay);
  }

  private async run() {
    this.timer = null;
    if (!this.view.dom.isConnected) {
      this.skipped = true;
      return;
    }
    this.skipped = false;
    const gen = ++this.generation;
    const doc = this.view.state.doc;
    try {
      const analysis = await analyzeUnityHotPaths(this.path, doc.toString(), this.deps);
      if (gen !== this.generation || this.view.state.doc !== doc) return;
      if (!this.view.dom.isConnected) {
        this.skipped = true;
        return;
      }
      const methods = placeHotMethods(doc, analysis.transitiveMethods);
      const current = this.view.state.field(perfField);
      const shouldShow = methods.length > 0;
      const sameMethods =
        current.methods.length === methods.length &&
        current.methods.every(
          (p, i) =>
            p.pos === methods[i]!.pos &&
            p.method.name === methods[i]!.method.name &&
            p.method.chain.join(">") === methods[i]!.method.chain.join(">"),
        );
      const sameIssues =
        current.issues.length === analysis.issues.length &&
        current.issues.every(
          (it, i) =>
            it.from === analysis.issues[i]!.from &&
            it.to === analysis.issues[i]!.to &&
            it.rule === analysis.issues[i]!.rule,
        );
      if (sameMethods && sameIssues && shouldShow === this.shown) return;
      const effects: StateEffect<unknown>[] = [setPerfState.of({ methods, issues: analysis.issues })];
      if (shouldShow !== this.shown) {
        this.shown = shouldShow;
        effects.push(this.column.reconfigure(shouldShow ? hotPathColumn : []));
      }
      this.view.dispatch({ effects });
    } catch (e) {
      console.debug("[unity performance]", e);
    }
  }

  destroy() {
    if (this.timer) clearTimeout(this.timer);
    this.generation++;
  }
}

export function unityPerformanceExtension(path: string, deps: UnityDeps): Extension {
  const column = new Compartment();
  return [
    perfField,
    issueDecorations,
    hotPathTooltip,
    column.of([]),
    ViewPlugin.define((view) => new UnityPerformancePlugin(view, path, deps, column)),
  ];
}
