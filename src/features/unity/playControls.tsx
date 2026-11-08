/** Play, Pause and Step in the connected Unity Editor (main toolbar and actions), and
 * refreshing its assets when a file is saved. */
import { onEditorEvent } from "@/editor/api";
import type { Action } from "@/platform/actions";
import { executeAction } from "@/platform/actions";
import { setContext } from "@/platform/context";
import { call, commands, type UnityEditorCommand } from "@/platform/ipc";
import { shortcutLabel } from "@/platform/keymap";
import { useProject } from "@/platform/project";
import { notifications } from "@/workbench/contributions";
import { IconButton } from "@/ui";
import { useUnityLink } from "./link";
import styles from "./Unity.module.css";

const CONNECTED = "unity.connected";

function editorCommand(command: UnityEditorCommand): Promise<void> {
  return call(commands.unityEditorCommand(command)).then(
    () => {},
    (e: unknown) => {
      notifications().notify({
        group: "run",
        level: "error",
        title: "Unity Editor",
        body: e instanceof Error ? e.message : String(e),
      });
    },
  );
}

const playMode = () => useUnityLink.getState().status?.editor?.playMode ?? "edit";

/** Attaches the debugger to the Editor (the debugger feature's action), then enters Play mode. */
async function attachAndPlay() {
  const attached = await new Promise<boolean>((resolve) => {
    void executeAction("debug.attachUnityEditor", { attached: resolve }).then((ran) => {
      if (!ran) resolve(false);
    });
  });
  if (attached && playMode() === "edit") await editorCommand("play");
}

export function playActions(): Action[] {
  const action = (id: string, title: string, icon: string, run: () => unknown): Action => ({
    id,
    title,
    category: "Unity",
    icon,
    keywords: ["unity", "editor", "play mode"],
    when: CONNECTED,
    run: () => void run(),
  });
  return [
    action("unity.play", "Play in Unity", "run", () => editorCommand(playMode() === "edit" ? "play" : "stop")),
    action("unity.attachAndPlay", "Attach to Unity Editor and Play", "debug", attachAndPlay),
    {
      id: "unity.pauseEditor",
      title: "Pause Unity Play Mode",
      category: "Unity",
      hidden: true,
      description: "Pauses the Editor's Play mode (a pausepoint's hit); nothing in Edit mode",
      run: () => {
        if (playMode() === "playing") void editorCommand("pausePlay");
      },
    },
    action("unity.pause", "Pause in Unity", "pause", () => editorCommand("pause")),
    action("unity.step", "Step in Unity", "step-over", () => editorCommand("step")),
    action("unity.refreshAssets", "Refresh Unity Assets", "refresh", () => editorCommand("refreshAssets")),
  ];
}

/** Main toolbar buttons, shown while an Editor is connected. */
export function UnityPlayControls() {
  const status = useUnityLink((s) => s.status);
  if (status?.connection !== "connected") return null;
  const mode = status.editor?.playMode ?? "edit";
  const playing = mode !== "edit";
  return (
    <div className={styles.playControls} role="group" aria-label="Unity Play Mode">
      {!playing && (
        <IconButton
          icon="debug"
          size="large"
          iconSize={16}
          className={styles.titleButton}
          title="Attach to Unity Editor and Play"
          shortcut={shortcutLabel("unity.attachAndPlay")}
          onClick={() => void executeAction("unity.attachAndPlay")}
        />
      )}
      <IconButton
        icon={playing ? "stop" : "run"}
        size="large"
        iconSize={16}
        className={styles.titleButton}
        title={playing ? "Stop Playing in Unity" : "Play in Unity"}
        shortcut={shortcutLabel("unity.play")}
        pressed={playing}
        onClick={() => void executeAction("unity.play")}
      />
      <IconButton
        icon="pause"
        size="large"
        iconSize={16}
        className={styles.titleButton}
        title="Pause in Unity"
        shortcut={shortcutLabel("unity.pause")}
        pressed={mode === "paused"}
        onClick={() => void executeAction("unity.pause")}
      />
      <IconButton
        icon="step-over"
        size="large"
        iconSize={16}
        className={styles.titleButton}
        title="Step in Unity"
        shortcut={shortcutLabel("unity.step")}
        disabled={!playing}
        onClick={() => void executeAction("unity.step")}
      />
    </div>
  );
}

/** Whether saving `path` should refresh Unity's assets: files Unity imports, in `Assets/` or `Packages/`. */
export function refreshesAssets(root: string, path: string): boolean {
  const norm = (p: string) => p.replaceAll("\\", "/").replace(/\/+$/, "");
  const prefix = `${norm(root)}/`;
  const file = norm(path);
  if (!file.startsWith(prefix)) return false;
  const rel = file.slice(prefix.length);
  return /^(Assets|Packages)\//.test(rel) && !/(^|\/)\.[^/]*$/.test(rel);
}

const REFRESH_DELAY_MS = 300;

/** Context key for `when` clauses, and refresh on save; returns the unsubscribe function. */
export function trackPlayControls(): () => void {
  const offStatus = useUnityLink.subscribe((s) => setContext(CONNECTED, s.status?.connection === "connected"));
  let timer: ReturnType<typeof setTimeout> | undefined;
  const offSave = onEditorEvent((e) => {
    if (e.type !== "docSaved") return;
    const project = useProject.getState();
    const root = project.project?.root;
    if (!root || !project.project?.unity) return;
    if (project.settings?.languages?.csharp?.refreshUnityAssetsOnSave === false) return;
    if (useUnityLink.getState().status?.connection !== "connected") return;
    if (!refreshesAssets(root, e.path)) return;
    clearTimeout(timer);
    // Save All writes several files; one refresh covers them.
    timer = setTimeout(() => void call(commands.unityEditorCommand("refreshAssets")).catch(() => {}), REFRESH_DELAY_MS);
  });
  return () => {
    clearTimeout(timer);
    offStatus();
    offSave();
  };
}
