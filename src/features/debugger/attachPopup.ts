import { executeAction } from "@/platform/actions";
import { call, commands, isTauri, type AttachableProcess, type RunConfig } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { notifications } from "@/workbench/contributions";
import { unityEditorFor } from "./attach";
import { useOverlays } from "./ui/popoverStore";

/** Unity targets (`unity` mode): the project's Editor first, then the other Editors. */
function unityTargets(list: readonly AttachableProcess[], root: string | null): AttachableProcess[] {
  const unity = list.filter((p) => p.kind === "unityMono");
  const own = root ? unityEditorFor(unity, root) : null;
  return own ? [own, ...unity.filter((p) => p !== own)] : unity;
}

/** Applies only the latest request of the open popup; results for a closed or refreshed one are dropped. */
export async function openAttachPopup(mode: "all" | "unity" = "all", runConfig?: RunConfig) {
  if (!isTauri()) {
    notifications().notify({ group: "run", level: "warning", title: "Attaching needs the desktop app" });
    return;
  }
  const projectRoot = useProject.getState().project?.root ?? null;
  const pending = { processes: null, error: null, mode, projectRoot, runConfig };
  useOverlays.setState({ attach: pending });
  // The popup state this request shows; replaced (refresh, close, another project) means stale.
  let shown: object = pending;
  const current = () =>
    useOverlays.getState().attach === shown && (useProject.getState().project?.root ?? null) === projectRoot;
  // Unity players announce themselves over UDP about once a second; they join the list later.
  const players =
    mode === "unity" || useProject.getState().project?.unity
      ? call(commands.debugUnityPlayers()).catch(() => [] as AttachableProcess[])
      : null;
  try {
    const all = await call(commands.debugAttachableProcesses());
    const processes = mode === "unity" ? unityTargets(all, projectRoot) : all;
    if (!current()) return;
    const next = { ...pending, processes };
    useOverlays.setState({ attach: next });
    shown = next;
  } catch (e) {
    if (current())
      useOverlays.setState({
        attach: {
          ...pending,
          processes: [],
          error: (e instanceof Error ? e.message : String(e)) || "Cannot list processes",
        },
      });
    return;
  }
  const found = players ? await players : [];
  if (found.length && current()) {
    const open = useOverlays.getState().attach!;
    useOverlays.setState({ attach: { ...open, processes: [...(open.processes ?? []), ...found] } });
  }
}

/** Installs the tool the popup reported missing, then lists the processes again. */
export async function installAttachAdapter() {
  const state = useOverlays.getState().attach;
  if (!state?.installTool) return;
  await executeAction("tools.install", state.installTool);
  if (useOverlays.getState().attach === state && useProject.getState().project?.root === state.projectRoot) {
    await openAttachPopup(state.mode, state.runConfig);
  }
}
