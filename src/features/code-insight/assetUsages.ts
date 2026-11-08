/** Unity asset usages (scenes, prefabs, clips) of the C# symbol at the caret, from the asset index. */
import type { AssetUsage } from "@/bindings";
import { formatMessage } from "@/platform/i18n";
import { call, commands } from "@/platform/ipc";
import { basename } from "@/platform/project";
import type { CaretContext } from "./ipc";
import type { Usage, UsageGroup } from "./usages";

/** Asset usages grouped by file, in the index's order; none outside Unity projects and C# files. */
export async function fetchAssetUsages(c: Pick<CaretContext, "doc" | "path" | "line" | "col">): Promise<UsageGroup[]> {
  if (!c.path.toLowerCase().endsWith(".cs")) return [];
  try {
    const found = await call(commands.unityAssetUsages(c.doc, { line: c.line, col: c.col }));
    return found ? assetGroups(found.usages) : [];
  } catch (e) {
    // The language server's usages still come; the asset index only adds to them.
    console.debug("[unity] asset usages", e);
    return [];
  }
}

export function assetGroups(usages: readonly AssetUsage[]): UsageGroup[] {
  const byPath = new Map<string, Usage[]>();
  for (const a of usages) {
    let list = byPath.get(a.path);
    if (!list) byPath.set(a.path, (list = []));
    list.push({ path: a.path, line: a.line, col: 0, endCol: 0, text: a.gameObject.join("/"), asset: a });
  }
  return [...byPath.entries()].map(([path, list]) => ({ path, usages: list, section: "assets" }));
}

/** The GameObject (or clip) and what the asset does with the symbol. */
export function assetUsageText(a: AssetUsage, tx: (text: string) => string): { label: string; detail: string } {
  const label = a.gameObject.join("/") || basename(a.path);
  const parts: string[] = [];
  const u = a.usage;
  if (u.kind === "unityEvent") parts.push(formatMessage(tx("UnityEvent {field}"), { field: u.field }));
  else if (u.kind === "animationEvent") parts.push(formatMessage(tx("Animation event at {time} s"), { time: u.time }));
  else if (u.kind === "fieldValue")
    parts.push(
      u.overridden
        ? formatMessage(tx("Overridden: {value}"), { value: u.value ?? "" })
        : formatMessage(tx("Value: {expression}"), { expression: u.value ?? "" }),
    );
  if (a.through) parts.push(formatMessage(tx("from {path}"), { path: basename(a.through).replace(/\.prefab$/i, "") }));
  return { label, detail: parts.join(" · ") };
}

/** Identifies an asset usage among those of its file (several GameObjects can share a line). */
export function assetUsageKey(a: AssetUsage): string {
  return `${a.gameObject.join("/")}|${a.through ?? ""}|${a.component ?? ""}|${a.usage.kind}`;
}
