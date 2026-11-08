import { describe, expect, it } from "vitest";
import type { Hierarchy, HierarchyNode } from "@/bindings";
import { hierarchyRows, initiallyExpanded, rowLine, type HierarchyRow } from "./model";

const node = (n: Partial<HierarchyNode>): HierarchyNode => ({
  kind: "gameObject",
  name: "",
  line: 0,
  prefab: null,
  components: [],
  overrides: [],
  children: [],
  ...n,
});

const squad: Hierarchy = {
  unreadable: false,
  roots: [
    node({
      name: "Squad",
      line: 2,
      components: [{ name: "Transform", script: null, line: 18 }],
      children: [
        node({
          kind: "prefabInstance",
          name: "Follower",
          line: 35,
          prefab: "/p/Assets/Prefabs/Player.prefab",
          components: [{ name: "Player", script: "/p/Assets/Scripts/Player.cs", line: null }],
          overrides: [{ property: "m_LocalPosition.x", value: "1.5", target: "Follower (Transform)", line: 47 }],
          children: [node({ kind: "fromPrefab", name: "Model", line: 35 })],
        }),
      ],
    }),
  ],
};

const flat = (rows: readonly HierarchyRow[], depth = 0): string[] =>
  rows.flatMap((r) => [
    `${"  ".repeat(depth)}${r.kind}:${rowLine(r)}`,
    ...("children" in r ? flat(r.children, depth + 1) : []),
  ]);

describe("Hierarchy rows", () => {
  it("list components, then overrides, then child GameObjects, each at its line", () => {
    const rows = hierarchyRows(squad);
    expect(flat(rows)).toEqual([
      "node:2",
      "  component:18",
      "  node:35",
      "    component:35",
      "    overrides:35",
      "      override:47",
      "    node:35",
    ]);
    expect(initiallyExpanded(rows)).toEqual(["0"]);
  });
});
