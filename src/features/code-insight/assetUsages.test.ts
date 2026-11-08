import { describe, expect, it, vi } from "vitest";
import type { AssetUsage, References } from "@/bindings";

const assets: AssetUsage[] = [
  {
    path: "/p/Assets/Scenes/Main.unity",
    line: 121,
    gameObject: ["Squad", "Follower"],
    through: "Assets/Prefabs/Squad.prefab",
    component: "11",
    usage: { kind: "component" },
  },
  {
    path: "/p/Assets/Scenes/Main.unity",
    line: 121,
    gameObject: ["Squad", "Leader"],
    through: "Assets/Prefabs/Squad.prefab",
    component: "12",
    usage: { kind: "component" },
  },
  {
    path: "/p/Assets/Prefabs/Player.prefab",
    line: 82,
    gameObject: ["Player"],
    through: null,
    component: "13",
    usage: { kind: "component" },
  },
];

let answerServer: (refs: References) => void = () => {};
vi.mock("@/platform/ipc", async (orig) => {
  const actual = await orig<typeof import("@/platform/ipc")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      unityAssetUsages: async () => ({ status: "ok", data: { symbol: "Player", usages: assets } }),
    },
  };
});
vi.mock("./ipc", async (orig) => ({
  ...(await orig<typeof import("./ipc")>()),
  lspRequest: () => new Promise<References>((resolve) => (answerServer = resolve)),
  readManyFileLines: async () => new Map([["/p/Assets/Scripts/Spawner.cs", ["", "    Player p;"]]]),
}));

const { fetchUsages } = await import("./usages");
const { assetUsageKey, assetUsageText } = await import("./assetUsages");

const caret = { doc: 1, path: "/p/Assets/Scripts/Player.cs", line: 5, col: 18, word: "Player", view: null };

describe("asset usages in Find Usages", () => {
  it("arrive before the server's references and are listed first", async () => {
    const early = vi.fn();
    const done = fetchUsages(caret, 1, early);
    await vi.waitFor(() => expect(early).toHaveBeenCalled());
    const [groups] = early.mock.calls[0]!;
    expect(groups.map((g: { path: string; section?: string }) => [g.path, g.section])).toEqual([
      ["/p/Assets/Scenes/Main.unity", "assets"],
      ["/p/Assets/Prefabs/Player.prefab", "assets"],
    ]);
    expect(groups[0].usages.map((u: { text: string }) => u.text)).toEqual(["Squad/Follower", "Squad/Leader"]);

    answerServer({
      locations: [
        { path: "/p/Assets/Scripts/Spawner.cs", range: { start: { line: 1, col: 4 }, end: { line: 1, col: 10 } } },
      ],
      groups: [],
    } as unknown as References);
    const { groups: all } = await done;
    expect(all.map((g) => g.section ?? "code")).toEqual(["assets", "assets", "code"]);
  });

  it("are described by GameObject and what the asset does", () => {
    const tx = (s: string) => s;
    expect(assetUsageText(assets[0]!, tx)).toEqual({ label: "Squad/Follower", detail: "from Squad" });
    expect(
      assetUsageText({ ...assets[2]!, usage: { kind: "fieldValue", value: "5", overridden: true }, through: null }, tx),
    ).toEqual({ label: "Player", detail: "Overridden: 5" });
    expect(
      assetUsageText({ ...assets[2]!, gameObject: ["Walk"], usage: { kind: "animationEvent", time: "0.5" } }, tx)
        .detail,
    ).toBe("Animation event at 0.5 s");
  });

  it("tell apart same-named GameObjects of one instance by their component", () => {
    const wheel = { ...assets[0]!, gameObject: ["Car", "Wheel"] };
    expect(assetUsageKey({ ...wheel, component: "1" })).not.toBe(assetUsageKey({ ...wheel, component: "2" }));
  });

  it("aren't asked for outside C# files", async () => {
    const { fetchAssetUsages } = await import("./assetUsages");
    expect(await fetchAssetUsages({ ...caret, path: "/p/a.ts" })).toEqual([]);
  });
});
