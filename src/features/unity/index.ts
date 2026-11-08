/** Unity code insight for C#: event function completion and gutter icons, Unity-aware rename,
 * Code Vision, a Search Everywhere tab and a Hierarchy view of scenes and prefabs from the asset index;
 * navigation in assembly definitions; Unity properties in USS; Unity YAML read-only. */
import { EditorState } from "@codemirror/state";
import { CONTRIBUTED_RENAME, registerEditorContribution } from "@/editor/api";
import { registerActions } from "@/platform/actions";
import {
  registerEditorBanner,
  registerSearchEverywhereTab,
  registerTextEditorWrapper,
} from "@/workbench/contributions";
import { useProject } from "@/platform/project";
import { clearUnityBases, unityDeps } from "./bases";
import { assemblyDefinitionSupport } from "./asmdef";
import { unityMessageSource } from "./completion";
import { eventFunctionGutter } from "./gutter";
import { hierarchyActions, hierarchyWrapper } from "./hierarchy";
import { unityLenses } from "./lenses";
import { unitySearchTab } from "./searchTab";
import { unityRename } from "./rename";
import { unityYamlBanner, unityYamlLock } from "./unityYaml";
import { ussSupport } from "./uss";
import { loadMessageData } from "./messages";

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
              ? unityYamlLock(ctx.path)
              : [],
  });
  const unsubscribe = useProject.subscribe((s, prev) => {
    if (s.project?.root !== prev.project?.root) clearUnityBases();
  });
  const unregisterBanner = registerEditorBanner(unityYamlBanner);
  const unregisterSearch = registerSearchEverywhereTab(unitySearchTab);
  const unregisterHierarchy = registerTextEditorWrapper(hierarchyWrapper);
  const unregisterActions = registerActions(hierarchyActions());
  return () => {
    unregisterActions();
    unregisterHierarchy();
    unregisterSearch();
    unregisterBanner();
    unregister();
    unsubscribe();
  };
}
