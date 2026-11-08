/** The Hierarchy view of scenes and prefabs, with Hierarchy / YAML actions. */
import type { Action } from "@/platform/actions";
import { getContext } from "@/platform/context";
import { useProject } from "@/platform/project";
import { lazyComponent } from "@/ui";
import type { TextEditorWrapperContribution } from "@/workbench/contributions";
import { HierarchyModeActions } from "./HierarchyEditor";
import { isSceneOrPrefab, setHierarchyMode, type HierarchyMode } from "./modes";

export const hierarchyWrapper: TextEditorWrapperContribution = {
  id: "unity.hierarchy",
  matches: (path) => isSceneOrPrefab(path) && !!useProject.getState().project?.unity,
  component: lazyComponent(() => import("./HierarchyEditor").then((m) => m.HierarchyEditor)),
  tabActions: HierarchyModeActions,
};

export function hierarchyActions(): Action[] {
  const mode = (id: string, title: string, m: HierarchyMode, icon: string): Action => ({
    id,
    title,
    category: "Unity",
    icon,
    keywords: ["unity", "scene", "prefab", "hierarchy", "yaml"],
    when: "textEditorActive",
    run: ({ arg }) => {
      const path = typeof arg === "string" ? arg : getContext().editorPath;
      if (typeof path === "string" && isSceneOrPrefab(path)) setHierarchyMode(path, m);
    },
  });
  return [
    mode("unity.showHierarchy", "Hierarchy", "hierarchy", "toolwindow-hierarchy"),
    mode("unity.showYaml", "YAML Source", "yaml", "editor-only"),
  ];
}
