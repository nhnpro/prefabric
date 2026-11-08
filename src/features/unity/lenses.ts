/**
 * Unity Code Vision above C# declarations, from the asset index: "Used in 3 prefabs · 1 scene" on a
 * script class, "Event handler in Main.unity" on a method, the Inspector value and override count
 * on a serialized field. A click shows the usages (asset usages first). Refreshed after edits and
 * whenever the index changes (a saved prefab).
 */
import { EditorSelection, MapMode, StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import type { LensSummary, UnityLens } from "@/bindings";
import { codeLenses, type CodeLens } from "@/editor/api";
import { executeAction } from "@/platform/actions";
import { editorPrefs } from "@/platform/editorSettings";
import { formatMessage, tx } from "@/platform/i18n";
import { call, commands, onIdeEvent } from "@/platform/ipc";

const DEBOUNCE_MS = 600;

const count = (n: number, one: string, many: string) => (n === 1 ? tx(one) : formatMessage(tx(many), { count: n }));

/** The lens text of a summary. */
export function lensTitle(s: LensSummary): string[] {
  if (s.kind === "class") {
    const places = [
      s.prefabs && count(s.prefabs, "1 prefab", "{count} prefabs"),
      s.scenes && count(s.scenes, "1 scene", "{count} scenes"),
      s.other && count(s.other, "1 asset", "{count} assets"),
    ].filter(Boolean);
    return [formatMessage(tx("Used in {places}"), { places: places.join(" · ") })];
  }
  if (s.kind === "method") {
    const where = s.fileCount === 1 ? s.files[0]! : count(s.fileCount, "1 asset", "{count} assets");
    return [formatMessage(tx(s.unityEvents ? "Event handler in {where}" : "Animation event in {where}"), { where })];
  }
  const out: string[] = [];
  if (s.values.length === 1) out.push(formatMessage(tx("Inspector: {value}"), { value: s.values[0]! }));
  else if (s.values.length > 1) out.push(tx("Inspector: several values"));
  if (s.overrides) out.push(count(s.overrides, "1 override", "{count} overrides"));
  return out;
}

const setLenses = StateEffect.define<CodeLens[]>();

const lensField = StateField.define<CodeLens[]>({
  create: () => [],
  update(lenses, tr) {
    for (const e of tr.effects) if (e.is(setLenses)) return e.value;
    if (!tr.docChanged || !lenses.length) return lenses;
    const out: CodeLens[] = [];
    for (const l of lenses) {
      const pos = tr.changes.mapPos(l.pos, -1, MapMode.TrackDel);
      if (pos !== null) out.push({ ...l, pos });
    }
    return out;
  },
  provide: (f) => codeLenses.from(f),
});

function showUsages(view: EditorView, offset: number) {
  view.dispatch({ selection: EditorSelection.cursor(offset), userEvent: "select.lens" });
  view.focus();
  void executeAction("find.showUsages");
}

/** Lenses of `found` in `view`'s document. */
function toLenses(view: EditorView, found: readonly UnityLens[]): CodeLens[] {
  const doc = view.state.doc;
  const out: CodeLens[] = [];
  for (const l of found) {
    if (l.line >= doc.lines) continue;
    const line = doc.line(l.line + 1);
    const pos = Math.min(line.from + l.col, line.to);
    lensTitle(l.summary).forEach((title, i) =>
      out.push({
        pos,
        title,
        tooltip: tx("Show usages in Unity assets"),
        order: 15 + i,
        run: (v) => showUsages(v, Math.min(pos, v.state.doc.length)),
      }),
    );
  }
  return out;
}

export function unityLenses(doc: number): Extension {
  const plugin = ViewPlugin.fromClass(
    class {
      private timer: ReturnType<typeof setTimeout> | null = null;
      private generation = 0;
      private revision: number | null = null;
      private off: () => void;

      constructor(private view: EditorView) {
        this.schedule(0);
        this.off = onIdeEvent("unityIndexState", (e) => {
          if (e.indexing || e.revision === this.revision) return;
          this.revision = e.revision;
          this.schedule(0);
        });
      }

      update(u: ViewUpdate) {
        if (u.docChanged) this.schedule(DEBOUNCE_MS);
      }

      private schedule(ms: number) {
        if (this.timer) clearTimeout(this.timer);
        this.timer = setTimeout(() => void this.run(), ms);
      }

      private async run() {
        this.timer = null;
        const gen = ++this.generation;
        const text = this.view.state.doc;
        let found: UnityLens[] = [];
        try {
          // The usages lens setting covers these too: they are usages in assets.
          if (editorPrefs().inlay.codeVisionUsages) found = await call(commands.unityLenses(doc));
        } catch (e) {
          console.debug("[unity] lenses", e);
          return;
        }
        if (gen !== this.generation || this.view.state.doc !== text || !this.view.dom.isConnected) return;
        const lenses = toLenses(this.view, found);
        const current = this.view.state.field(lensField);
        const same =
          current.length === lenses.length &&
          current.every((l, i) => l.pos === lenses[i]!.pos && l.title === lenses[i]!.title);
        if (!same) this.view.dispatch({ effects: setLenses.of(lenses) });
      }

      destroy() {
        if (this.timer) clearTimeout(this.timer);
        this.generation++;
        this.off();
      }
    },
  );
  return [lensField, plugin];
}
