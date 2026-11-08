/**
 * The Hierarchy view hides the YAML; navigating into the file (Find Usages, Search Everywhere,
 * Go to Line) moves its caret, so the YAML is shown at that line.
 */
import { ViewPlugin, type ViewUpdate } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import { hierarchyModeOf, setHierarchyMode, useHierarchyModes } from "./modes";

export function showYamlOnNavigation(path: string): Extension {
  const show = () => {
    if (hierarchyModeOf(useHierarchyModes.getState(), path) === "hierarchy") setHierarchyMode(path, "yaml");
  };
  return ViewPlugin.define((view) => {
    // Opened at a line: the caret is already there.
    if (view.state.selection.main.head > 0) queueMicrotask(show);
    return {
      update(u: ViewUpdate) {
        if (u.selectionSet && u.state.selection.main.head !== u.startState.selection.main.head) show();
      },
    };
  });
}
