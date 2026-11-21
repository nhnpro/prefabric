/** Search Everywhere's Unity tab: GameObjects and prefab instances across scenes and prefabs, and
 * assets, by name, from the asset index; each shows where it is. A search category, as a plugin's,
 * offered while the project is a Unity one. */
import type { PluginSearchItem, UnityHit } from "@/bindings";
import { call, commands } from "@/platform/ipc";
import { relativeDir, useProject } from "@/platform/project";
import { registerSearchCategory, type SearchCategory } from "@/workbench/contributions";

export function unityHitItem(h: UnityHit, root: string): PluginSearchItem {
  const asset = h.kind === "asset";
  const at = { line: asset ? 0 : h.line, col: 0 };
  return {
    id: `${h.path}:${h.line}:${h.kind}:${h.name}`,
    title: h.name,
    // Where it is: the hierarchy above a GameObject, the folder of an asset.
    description: (asset ? relativeDir(root, h.path) : h.parents.join("/")) || null,
    target: { kind: "location", value: { path: h.path, range: { start: at, end: at } } },
  };
}

export const unitySearchCategory: SearchCategory = {
  id: "plugin:unity:unity",
  title: "Unity",
  async search(_request, query, limit) {
    const found = await call(commands.unitySearch(query, limit));
    const root = useProject.getState().project?.root ?? "";
    return { items: found.hits.map((h) => unityHitItem(h, root)), hasMore: found.more };
  },
  command: () => undefined,
};

/** Offers the tab while the open project is a Unity one. */
export function trackUnitySearchCategory(): () => void {
  let unregister: (() => void) | null = null;
  const sync = (unity: boolean) => {
    if (unity && !unregister) unregister = registerSearchCategory(unitySearchCategory);
    else if (!unity && unregister) {
      unregister();
      unregister = null;
    }
  };
  sync(!!useProject.getState().project?.unity);
  const unsubscribe = useProject.subscribe((s) => sync(!!s.project?.unity));
  return () => {
    unsubscribe();
    sync(false);
  };
}
