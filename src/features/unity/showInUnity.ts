/** "Show in Unity": select a scene or prefab's GameObject, or an asset, in the connected Editor. */
import type { Action } from "@/platform/actions";
import { getContext } from "@/platform/context";
import { call, commands } from "@/platform/ipc";
import { tx } from "@/platform/i18n";
import { useProject } from "@/platform/project";
import { notifications, registerMenuGroup, type MenuGroupContext } from "@/workbench/contributions";
import { useUnityLink } from "./link";
import { refreshesAssets } from "./playControls";

export const SHOW_IN_UNITY = "unity.showInUnity";

/** What to show: a file, and the 0-based line of the object in a scene or prefab. */
export interface ShowTarget {
  path: string;
  line?: number;
}

function target(arg: unknown): ShowTarget | null {
  if (typeof arg === "string") return { path: arg };
  if (arg && typeof arg === "object" && typeof (arg as ShowTarget).path === "string") return arg as ShowTarget;
  const path = getContext().editorPath;
  return typeof path === "string" ? { path } : null;
}

/** Files Unity knows: under `Assets/` or `Packages/` of the open Unity project. */
export function showable(path: string): boolean {
  const root = useProject.getState().project?.root;
  return !!root && !!useProject.getState().project?.unity && refreshesAssets(root, path);
}

const connected = () => useUnityLink.getState().status?.connection === "connected";

export function showInUnityAction(): Action {
  return {
    id: SHOW_IN_UNITY,
    title: "Show in Unity",
    category: "Unity",
    icon: "locate",
    keywords: ["unity", "select", "ping", "editor", "gameobject", "asset"],
    when: "unity.connected",
    run: async ({ arg }) => {
      const t = target(arg);
      if (!t || !showable(t.path)) return;
      try {
        const found = await call(commands.unityShowInEditor(t.path, t.line ?? null));
        if (!found)
          notifications().notify({
            group: "run",
            level: "info",
            title: tx("Unity didn't find it"),
            body: tx("Open its scene in Unity to select a GameObject; the scene is selected meanwhile."),
          });
      } catch (e) {
        notifications().notify({
          group: "run",
          level: "error",
          title: tx("Show in Unity"),
          body: e instanceof Error ? e.message : String(e),
        });
      }
    },
  };
}

/** The project view's and the editor's context menus; the editor passes the caret line. */
export function showInUnityMenus(): () => void {
  return registerMenuGroup({
    id: "unity.showInUnity",
    menus: ["projectView", "editor"],
    order: 45,
    items: (menu, ctx: MenuGroupContext) => {
      const [path] = ctx.paths;
      if (ctx.paths.length !== 1 || !path || !connected() || !showable(path)) return [];
      const arg: ShowTarget = menu === "editor" && ctx.line !== undefined ? { path, line: ctx.line } : { path };
      return [{ kind: "action", action: SHOW_IN_UNITY, arg }];
    },
  });
}
