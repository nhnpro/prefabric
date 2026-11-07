/**
 * Gutter icons on Unity event functions (`Update`, `OnTriggerEnter`…): the methods of a class
 * deriving from a Unity type named like one of its messages. The tooltip describes the message;
 * a click opens its documentation. The column exists only while a file has such methods.
 */
import { Compartment, RangeSet, StateEffect, StateField, type Extension, type Text } from "@codemirror/state";
import { EditorView, GutterMarker, ViewPlugin, gutter, type ViewUpdate } from "@codemirror/view";
import { openExternalUrl } from "@/platform/openUrl";
import { tx } from "@/platform/i18n";
import { gutterIconMarkup } from "@/ui";
import styles from "@/editor/navigation/LineMarkers.module.css";
import { unityBaseOf, type UnityDeps } from "./bases";
import { classes, methodDeclarations } from "./csharp";
import { messagesFor, unityTypeNames, type UnityMessage } from "./messages";

const DEBOUNCE_MS = 400;

export interface EventFunction {
  /** Offset of the method name. */
  at: number;
  message: UnityMessage & { owner: string };
}

/** The event functions declared in a C# file. */
export async function eventFunctions(path: string, text: string, deps: UnityDeps): Promise<EventFunction[]> {
  const data = await deps.data();
  if (!data) return [];
  const unityTypes = unityTypeNames(data);
  const found: EventFunction[] = [];
  for (const cls of classes(text)) {
    const methods = methodDeclarations(text, cls);
    if (!methods.length) continue;
    const base = await unityBaseOf(path, text, cls, unityTypes, deps);
    if (!base) continue;
    const messages = new Map(messagesFor(data, base).map((m) => [m.name, m]));
    for (const m of methods) {
      const message = messages.get(m.name);
      if (message) found.push({ at: m.at, message });
    }
  }
  return found.sort((a, b) => a.at - b.at);
}

interface Placed {
  pos: number;
  message: EventFunction["message"];
}

const setPlaced = StateEffect.define<Placed[]>();

const placedField = StateField.define<Placed[]>({
  create: () => [],
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setPlaced)) return e.value;
    if (!tr.docChanged || !value.length) return value;
    return value.map((p) => ({ ...p, pos: tr.state.doc.lineAt(tr.changes.mapPos(p.pos, -1)).from }));
  },
});

/** One marker per line (the first event function on it), at line starts. */
export function placeEventFunctions(doc: Text, found: readonly EventFunction[]): Placed[] {
  const placed: Placed[] = [];
  for (const f of found) {
    if (f.at > doc.length) continue;
    const pos = doc.lineAt(f.at).from;
    if (placed[placed.length - 1]?.pos !== pos) placed.push({ pos, message: f.message });
  }
  return placed;
}

export function eventFunctionTooltip(m: EventFunction["message"]): string {
  return `${tx`Unity event function ${m.name} of ${m.owner}`}\n${m.description}`;
}

class EventFunctionMarker extends GutterMarker {
  constructor(readonly message: EventFunction["message"]) {
    super();
  }
  override eq(other: GutterMarker): boolean {
    return (
      other instanceof EventFunctionMarker &&
      other.message.name === this.message.name &&
      other.message.owner === this.message.owner
    );
  }
  override toDOM(): Node {
    const el = document.createElement("button");
    el.type = "button";
    el.className = styles.marker!;
    const tooltip = eventFunctionTooltip(this.message);
    el.title = tooltip;
    el.setAttribute("aria-label", tooltip);
    el.innerHTML = gutterIconMarkup("gutter-event-function") ?? "";
    const docs = this.message.docs;
    el.addEventListener("mousedown", (e) => e.preventDefault());
    el.addEventListener("click", (e) => {
      e.preventDefault();
      if (docs) void openExternalUrl(docs).catch(() => undefined);
    });
    return el;
  }
}

const markerColumn: Extension = gutter({
  class: `${styles.gutter} cm-openidea-gutter-unityEvents`,
  markers: (view) =>
    RangeSet.of(
      view.state.field(placedField).map((p) => new EventFunctionMarker(p.message).range(p.pos)),
      true,
    ),
});

class EventFunctionsPlugin {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private generation = 0;
  private shown = false;

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
  }

  private schedule(delay: number) {
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.run(), delay);
  }

  private async run() {
    this.timer = null;
    const gen = ++this.generation;
    const doc = this.view.state.doc;
    try {
      const found = await eventFunctions(this.path, doc.toString(), this.deps);
      if (gen !== this.generation || this.view.state.doc !== doc || !this.view.dom.isConnected) return;
      const placed = placeEventFunctions(doc, found);
      if (!placed.length && !this.view.state.field(placedField).length) return;
      const effects: StateEffect<unknown>[] = [setPlaced.of(placed)];
      if (placed.length > 0 !== this.shown) {
        this.shown = placed.length > 0;
        effects.push(this.column.reconfigure(this.shown ? markerColumn : []));
      }
      this.view.dispatch({ effects });
    } catch (e) {
      console.debug("[unity event functions]", e);
    }
  }

  destroy() {
    if (this.timer) clearTimeout(this.timer);
    this.generation++;
  }
}

export function eventFunctionGutter(path: string, deps: UnityDeps): Extension {
  const column = new Compartment();
  return [placedField, column.of([]), ViewPlugin.define((view) => new EventFunctionsPlugin(view, path, deps, column))];
}
