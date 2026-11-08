/** Search Everywhere's Unity tab: GameObjects and prefab instances across scenes and prefabs, and
 * assets, by name, from the asset index; each shows where it is. */
import type { UnityHit } from "@/bindings";
import { call, commands } from "@/platform/ipc";
import { basename, relativeDir, useProject } from "@/platform/project";
import { fileIconName } from "@/ui";
import type { SearchEverywhereItem, SearchEverywhereTab } from "@/workbench/contributions";

export function unityHitItem(h: UnityHit, root: string): SearchEverywhereItem {
  const asset = h.kind === "asset";
  return {
    key: `${h.path}:${h.line}:${h.kind}:${h.name}`,
    title: h.name,
    indices: h.indices,
    // Where it is: the hierarchy above a GameObject, the folder of an asset.
    detail: asset ? relativeDir(root, h.path) : h.parents.join("/") || undefined,
    location: asset ? undefined : basename(h.path),
    icon: asset ? fileIconName(h.name, h.path) : h.kind === "prefabInstance" ? "module" : "hierarchy",
    path: h.path,
    ...(asset ? {} : { line: h.line, col: 0 }),
  };
}

export const unitySearchTab: SearchEverywhereTab = {
  id: "unity",
  label: "Unity",
  // Between Actions and Text.
  order: 45,
  visible: () => !!useProject.getState().project?.unity,
  async search(query, limit, signal) {
    const found = await call(commands.unitySearch(query, limit));
    if (signal.aborted) return { items: [], hasMore: false };
    const root = useProject.getState().project?.root ?? "";
    return { items: found.hits.map((h) => unityHitItem(h, root)), hasMore: found.more };
  },
};
