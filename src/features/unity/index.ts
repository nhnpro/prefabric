/** Unity code insight for C#: event function (message) completion. */
import { EditorState } from "@codemirror/state";
import { registerEditorContribution } from "@/editor/api";
import { useProject } from "@/platform/project";
import { clearUnityBases, unityCompletionDeps, unityMessageSource } from "./completion";
import { loadMessageData } from "./messages";

const deps = unityCompletionDeps(async () => {
  const unity = useProject.getState().project?.unity;
  return unity ? loadMessageData(unity.editorVersion) : null;
});

export function activate() {
  const unregister = registerEditorContribution({
    id: "unity.messages",
    extensions: (ctx) =>
      ctx.language === "csharp" && !ctx.readOnly
        ? EditorState.languageData.of(() => [{ openideaCompletion: unityMessageSource(ctx.path, deps) }])
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
