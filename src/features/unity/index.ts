/** Unity code insight for C#: event function completion and gutter icons, Unity-aware rename. */
import { EditorState } from "@codemirror/state";
import { CONTRIBUTED_RENAME, registerEditorContribution } from "@/editor/api";
import { useProject } from "@/platform/project";
import { clearUnityBases, unityDeps } from "./bases";
import { unityMessageSource } from "./completion";
import { eventFunctionGutter } from "./gutter";
import { unityRename } from "./rename";
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
          ]
        : [],
  });
  const unsubscribe = useProject.subscribe((s, prev) => {
    if (s.project?.root !== prev.project?.root) clearUnityBases();
  });
  return () => {
    unregister();
    unsubscribe();
  };
}
