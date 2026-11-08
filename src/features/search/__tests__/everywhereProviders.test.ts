import { afterEach, describe, expect, it, vi } from "vitest";
import { registerSearchEverywhereTab, type SearchEverywhereResult } from "@/workbench/contributions";
import { SeController, type SeDeps, type SeSnapshot } from "../everywhere/controller";
import { buildRows, parseCommand, sectionTitle, sectionsFor, seTabs } from "../everywhere/model";

const flush = () => new Promise((r) => setTimeout(r, 0));

function controller() {
  let id = 0;
  const deps: SeDeps = {
    findFiles: async () => [],
    findSymbols: async () => [],
    searchText: async () => {},
    cancel: () => {},
    nextRequestId: () => ++id,
    actions: () => [],
    isEnabled: () => true,
    recentFiles: async () => [],
    recentActions: () => [],
    root: () => "/p",
    now: () => 0,
  };
  const snaps: SeSnapshot[] = [];
  return { c: new SeController(deps, (s) => snaps.push(s)), last: () => snaps[snaps.length - 1]! };
}

let unregister: (() => void) | null = null;
afterEach(() => {
  unregister?.();
  unregister = null;
});

describe("contributed Search Everywhere tabs", () => {
  it("are tabs with their own section and command, placed by order", () => {
    unregister = registerSearchEverywhereTab({
      id: "unity",
      label: "Unity",
      order: 45,
      sectionTitle: "Unity Objects",
      search: async () => ({ items: [], hasMore: false }),
    });
    expect(seTabs().map((t) => t.id)).toEqual(["all", "classes", "files", "symbols", "actions", "unity", "text"]);
    expect(sectionsFor("unity", false)).toEqual(["unity"]);
    expect(sectionsFor("unity", true)).toEqual([]);
    // Not in All unless asked.
    expect(sectionsFor("all", false)).not.toContain("unity");
    expect(sectionTitle("unity")).toBe("Unity Objects");
    expect(parseCommand("/un x")).toBeNull();
    expect(parseCommand("/unity hero")).toEqual({ kind: "run", tab: "unity", rest: "hero" });
  });

  it("hide while not visible", () => {
    unregister = registerSearchEverywhereTab({
      id: "unity",
      label: "Unity",
      order: 45,
      visible: () => false,
      search: async () => ({ items: [], hasMore: false }),
    });
    expect(seTabs().map((t) => t.id)).not.toContain("unity");
  });

  it("answer through the controller, abort superseded searches, and mix into All", async () => {
    const signals: AbortSignal[] = [];
    const search = vi.fn(
      async (query: string, _limit: number, signal: AbortSignal): Promise<SearchEverywhereResult> => {
        signals.push(signal);
        return {
          items: [{ key: query, title: `Hero ${query}`, indices: [0], path: "/p/Assets/Main.unity", line: 3 }],
          hasMore: false,
        };
      },
    );
    unregister = registerSearchEverywhereTab({ id: "unity", label: "Unity", order: 45, inAll: true, search });
    const { c, last } = controller();
    c.run({ query: "a", tab: "unity", includeNonProject: false, extra: {} });
    c.run({ query: "b", tab: "unity", includeNonProject: false, extra: {} });
    await flush();
    expect(signals[0]!.aborted).toBe(true);
    expect(signals[1]!.aborted).toBe(false);
    const rows = buildRows(last().sections, { headers: false });
    expect(rows.map((r) => (r.kind === "provided" ? r.title : r.kind))).toEqual(["Hero b"]);

    c.run({ query: "hero", tab: "all", includeNonProject: false, extra: {} });
    await flush();
    expect(sectionsFor("all", false)).toContain("unity");
    expect(last().sections.get("unity")!.items).toHaveLength(1);
    const mixed = buildRows(last().sections, { headers: false, mix: "hero" });
    expect(mixed.some((r) => r.kind === "provided")).toBe(true);
  });
});
