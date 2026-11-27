import { afterEach, describe, expect, it, vi } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import type { LensSummary, UnityLens } from "@/bindings";
import { codeLenses, viewShown } from "@/editor/api";
import { executeAction } from "@/platform/actions";
import { commands } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { lensTitle, unityLenses } from "./lenses";

vi.mock("@/platform/actions", () => ({
  executeAction: vi.fn(),
}));

describe("Unity Code Vision", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    useProject.setState({ settings: null });
  });

  it("summarises where a script class is used", () => {
    expect(lensTitle({ kind: "class", prefabs: 3, scenes: 1, other: 0 })).toEqual(["Used in 3 prefabs · 1 scene"]);
    expect(lensTitle({ kind: "class", prefabs: 0, scenes: 0, other: 2 })).toEqual(["Used in 2 assets"]);
  });

  it("names the asset calling a method, or counts them", () => {
    const method: LensSummary & { kind: "method" } = {
      kind: "method",
      unityEvents: 1,
      animationEvents: 0,
      files: ["Main.unity"],
      fileCount: 1,
    };
    expect(lensTitle(method)).toEqual(["Event handler in Main.unity"]);
    expect(lensTitle({ ...method, unityEvents: 0, animationEvents: 1, files: ["Walk.anim"] })).toEqual([
      "Animation event in Walk.anim",
    ]);
    expect(lensTitle({ ...method, fileCount: 4, files: ["A.unity", "B.prefab", "C.prefab"] })).toEqual([
      "Event handler in 4 assets",
    ]);
  });

  it("shows a field's Inspector value and its overrides", () => {
    expect(lensTitle({ kind: "field", values: ["2"], overrides: 2 })).toEqual(["Inspector: 2", "2 overrides"]);
    expect(lensTitle({ kind: "field", values: ["1", "3"], overrides: 0 })).toEqual(["Inspector: several values"]);
    expect(lensTitle({ kind: "field", values: [], overrides: 1 })).toEqual(["1 override"]);
  });

  it("uses the mapped position when clicking a lens after edits above it", async () => {
    vi.useFakeTimers();
    const lens: UnityLens = { line: 0, col: 6, summary: { kind: "class", prefabs: 1, scenes: 0, other: 0 } };
    vi.spyOn(commands, "unityLenses").mockResolvedValue({ status: "ok", data: [lens] });
    const host = document.createElement("div");
    document.body.appendChild(host);
    const view = new EditorView({
      parent: host,
      state: EditorState.create({ doc: "class Player : MonoBehaviour {}", extensions: [unityLenses(1)] }),
    });
    await vi.advanceTimersByTimeAsync(10);
    // Insert a line above before the 600ms debounce refresh runs.
    const prefix = "// comment\n";
    view.dispatch({ changes: { from: 0, insert: prefix } });
    const [active] = view.state.facet(codeLenses);
    expect(active?.pos).toBe(prefix.length + 6);
    active?.run?.(view, new MouseEvent("click"));
    expect(view.state.selection.main.head).toBe(prefix.length + 6);
    expect(executeAction).toHaveBeenCalledWith("find.showUsages");
    view.destroy();
    host.remove();
  });

  it("clears and restores lenses when inlay.codeVisionUsages toggles", async () => {
    vi.useFakeTimers();
    const lens: UnityLens = { line: 0, col: 6, summary: { kind: "class", prefabs: 1, scenes: 0, other: 0 } };
    vi.spyOn(commands, "unityLenses").mockResolvedValue({ status: "ok", data: [lens] });
    const host = document.createElement("div");
    document.body.appendChild(host);
    const view = new EditorView({
      parent: host,
      state: EditorState.create({ doc: "class Player : MonoBehaviour {}", extensions: [unityLenses(1)] }),
    });
    await vi.advanceTimersByTimeAsync(10);
    expect(view.state.facet(codeLenses)).toHaveLength(1);
    useProject.setState({ settings: { editor: { inlay: { codeVisionUsages: false } } } as never });
    await vi.advanceTimersByTimeAsync(10);
    expect(view.state.facet(codeLenses)).toHaveLength(0);
    useProject.setState({ settings: { editor: { inlay: { codeVisionUsages: true } } } as never });
    await vi.advanceTimersByTimeAsync(10);
    expect(view.state.facet(codeLenses)).toHaveLength(1);
    view.destroy();
    host.remove();
  });

  it("refreshes lenses via viewShown after changes occur while detached", async () => {
    vi.useFakeTimers();
    const lens: UnityLens = { line: 0, col: 6, summary: { kind: "class", prefabs: 1, scenes: 0, other: 0 } };
    const spy = vi.spyOn(commands, "unityLenses").mockResolvedValue({ status: "ok", data: [lens] });
    const host = document.createElement("div");
    document.body.appendChild(host);
    const view = new EditorView({
      parent: host,
      state: EditorState.create({ doc: "class Player : MonoBehaviour {}", extensions: [unityLenses(1)] }),
    });
    await vi.advanceTimersByTimeAsync(10);
    expect(view.state.facet(codeLenses)).toHaveLength(1);
    expect(spy).toHaveBeenCalledTimes(1);

    // Detach (simulating keptSlots), toggle setting off: must not dispatch while disconnected.
    host.remove();
    useProject.setState({ settings: { editor: { inlay: { codeVisionUsages: false } } } as never });
    await vi.advanceTimersByTimeAsync(10);
    expect(view.state.facet(codeLenses)).toHaveLength(1);

    // Re-attach and notify viewShown: lenses clear immediately.
    document.body.appendChild(host);
    view.dispatch({ effects: viewShown.of(null) });
    await vi.advanceTimersByTimeAsync(10);
    expect(view.state.facet(codeLenses)).toHaveLength(0);
    view.destroy();
    host.remove();
  });
});
