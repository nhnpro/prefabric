/** Scenes and prefabs open as their GameObject tree or as raw YAML, per file. */
import { create } from "zustand";

export type HierarchyMode = "hierarchy" | "yaml";

interface ModesState {
  byPath: Record<string, HierarchyMode>;
}

export const useHierarchyModes = create<ModesState>(() => ({ byPath: {} }));

export const hierarchyModeOf = (s: ModesState, path: string): HierarchyMode => s.byPath[path] ?? "hierarchy";

export function setHierarchyMode(path: string, mode: HierarchyMode) {
  useHierarchyModes.setState((s) => ({ byPath: { ...s.byPath, [path]: mode } }));
}

export const isSceneOrPrefab = (path: string) => /\.(unity|prefab)$/i.test(path);
