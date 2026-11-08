/** Rows of the Hierarchy view: GameObjects and instances, then their components and overrides. */
import type { Hierarchy, HierarchyNode } from "@/bindings";

export type HierarchyRow =
  | { kind: "node"; key: string; node: HierarchyNode; children: HierarchyRow[] }
  | { kind: "component"; key: string; name: string; script: string | null; line: number | null; owner: HierarchyNode }
  | { kind: "overrides"; key: string; node: HierarchyNode; children: HierarchyRow[] }
  | { kind: "override"; key: string; property: string; value: string; target: string | null; line: number };

function nodeRow(node: HierarchyNode, key: string): HierarchyRow {
  const children: HierarchyRow[] = node.components.map((c, i) => ({
    kind: "component",
    key: `${key}/c${i}`,
    name: c.name,
    script: c.script,
    line: c.line,
    owner: node,
  }));
  if (node.overrides.length)
    children.push({
      kind: "overrides",
      key: `${key}/o`,
      node,
      children: node.overrides.map((o, i) => ({ kind: "override", key: `${key}/o${i}`, ...o })),
    });
  node.children.forEach((c, i) => children.push(nodeRow(c, `${key}/${i}`)));
  return { kind: "node", key, node, children };
}

export function hierarchyRows(h: Hierarchy): HierarchyRow[] {
  return h.roots.map((n, i) => nodeRow(n, String(i)));
}

/** Keys of the GameObject rows to show expanded at first: the roots. */
export function initiallyExpanded(rows: readonly HierarchyRow[]): string[] {
  return rows.map((r) => r.key);
}

/** The line in the file a row stands for. */
export function rowLine(row: HierarchyRow): number {
  switch (row.kind) {
    case "node":
    case "overrides":
      return row.node.line;
    case "component":
      return row.line ?? row.owner.line;
    case "override":
      return row.line;
  }
}
