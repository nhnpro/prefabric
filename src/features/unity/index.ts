/** Unity code insight for C#: event function completion and gutter icons, Unity-aware rename,
 * Code Vision, a Search Everywhere tab and a Hierarchy view of scenes and prefabs from the asset index;
 * navigation in assembly definitions; Unity properties in USS; Unity YAML read-only. */
import { EditorState } from "@codemirror/state";
import { CONTRIBUTED_RENAME, registerEditorContribution } from "@/editor/api";
import { registerActions } from "@/platform/actions";
import {
  registerEditorBanner,
  registerSearchEverywhereTab,
  registerStatusItem,
  registerTextEditorWrapper,
  registerTitleItem,
  registerToolWindow,
} from "@/workbench/contributions";
import { onIdeEvent } from "@/platform/ipc";
import { lazyComponent } from "@/ui";
import { useProject } from "@/platform/project";
import { clearUnityBases, unityDeps } from "./bases";
import { assemblyDefinitionSupport } from "./asmdef";
import { unityMessageSource } from "./completion";
import { eventFunctionGutter } from "./gutter";
import { hierarchyActions, hierarchyWrapper } from "./hierarchy";
import { showYamlOnNavigation } from "./hierarchy/follow";
import { isSceneOrPrefab } from "./hierarchy/modes";
import { unityLenses } from "./lenses";
import { unitySearchTab } from "./searchTab";
import { unityRename } from "./rename";
import { unityYamlBanner, unityYamlLock } from "./unityYaml";
import { ussSupport } from "./uss";
import { loadMessageData } from "./messages";
import { trackUnityLink, UnityLinkItem } from "./link";
import { playActions, trackPlayControls, UnityPlayControls } from "./playControls";
import { appendEntries, clearConsole } from "./console/store";
import { showInUnityAction, showInUnityMenus } from "./showInUnity";

const deps = unityDeps(async () => {
  const unity = useProject.getState().project?.unity;
  return unity ? loadMessageData(unity.editorVersion) : null;
});

const rename = unityRename(deps);

export function activate() {
  const unregister = registerEditorContribution({
    id: "unity.messages",
    extensions: (ctx) =>
      ctx.language === "csharp" && !ctx.readOnly && !ctx.virtual && ctx.doc >= 0
        ? [
            EditorState.languageData.of(() => [
              { openideaCompletion: unityMessageSource(ctx.path, deps), [CONTRIBUTED_RENAME]: rename },
            ]),
            eventFunctionGutter(ctx.path, deps),
            useProject.getState().project?.unity ? unityLenses(ctx.doc) : [],
          ]
        : ctx.language === "json" && !ctx.virtual
          ? assemblyDefinitionSupport(ctx.path)
          : ctx.language === "css" && !ctx.virtual
            ? ussSupport(ctx.path)
            : ctx.language === "yaml" && !ctx.virtual
              ? [unityYamlLock(ctx.path), isSceneOrPrefab(ctx.path) ? showYamlOnNavigation(ctx.path) : []]
              : [],
  });
  const unsubscribe = useProject.subscribe((s, prev) => {
    if (s.project?.root !== prev.project?.root) clearUnityBases();
  });
  const unregisterBanner = registerEditorBanner(unityYamlBanner);
  const unregisterSearch = registerSearchEverywhereTab(unitySearchTab);
  const unregisterHierarchy = registerTextEditorWrapper(hierarchyWrapper);
  const unregisterActions = registerActions([...hierarchyActions(), ...playActions(), showInUnityAction()]);
  const unregisterShowMenus = showInUnityMenus();
  const unregisterLink = registerStatusItem({
    id: "unity.link",
    title: "Unity Editor",
    align: "left",
    priority: 90,
    component: UnityLinkItem,
  });
  const untrackLink = trackUnityLink();
  const unregisterPlay = registerTitleItem({
    id: "unity.playControls",
    title: "Unity Play Mode",
    align: "right",
    priority: 1,
    component: UnityPlayControls,
  });
  const untrackPlay = trackPlayControls();
  const unregisterConsole = registerToolWindow({
    id: "unity.console",
    title: "Unity Console",
    icon: "toolwindow-logcat",
    anchor: "bottom",
    // Listed under the stripe's "…" until first opened: most projects are not Unity ones.
    stripe: "onUse",
    order: 27,
    component: lazyComponent(() => import("./console/ConsoleView").then((m) => m.ConsoleView)),
  });
  // Collected from the start, so the window shows what was logged before it opened.
  const offConsole = onIdeEvent("unityConsole", (e) => appendEntries(e.entries));
  const offProject = useProject.subscribe((s, prev) => {
    if (s.project?.root !== prev.project?.root) clearConsole();
  });
  return () => {
    unregisterShowMenus();
    offProject();
    offConsole();
    unregisterConsole();
    untrackPlay();
    unregisterPlay();
    untrackLink();
    unregisterLink();
    unregisterActions();
    unregisterHierarchy();
    unregisterSearch();
    unregisterBanner();
    unregister();
    unsubscribe();
  };
}
