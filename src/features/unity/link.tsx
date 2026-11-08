/** The status bar item of the Unity Editor link (`docs/unity-link.md`): whether an Editor has the
 * project open, and its play mode. */
import { create } from "zustand";
import { executeAction } from "@/platform/actions";
import { tx } from "@/platform/i18n";
import { call, commands, onIdeEvent, type UnityLinkStatus } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { BarButton } from "@/ui";

export const useUnityLink = create<{ status: UnityLinkStatus | null }>(() => ({ status: null }));

/** Status bar text and tooltip; `null` hides the item. */
export function linkLabel(status: UnityLinkStatus | null): { label: string; tooltip: string } | null {
  if (!status) return null;
  switch (status.connection) {
    case "disconnected":
      return null;
    case "connecting":
      return { label: tx("Unity: connecting"), tooltip: tx("Connecting to the Unity Editor") };
    case "incompatible":
      return {
        label: tx("Unity: update the package"),
        tooltip: tx("The Prefabric package in this project is older than the IDE. Click to update it."),
      };
  }
  const editor = status.editor;
  const version = status.unityVersion ?? "";
  const tooltip = tx`Connected to Unity ${version}`;
  if (editor?.compiling) return { label: tx("Unity: compiling"), tooltip };
  if (editor?.updating) return { label: tx("Unity: importing"), tooltip };
  if (editor?.playMode === "playing") return { label: tx("Unity: playing"), tooltip };
  if (editor?.playMode === "paused") return { label: tx("Unity: paused"), tooltip };
  return { label: tx("Unity: connected"), tooltip };
}

export function UnityLinkItem() {
  const status = useUnityLink((s) => s.status);
  const shown = linkLabel(status);
  if (!shown) return null;
  const incompatible = status?.connection === "incompatible";
  return (
    <BarButton
      tooltip={shown.tooltip}
      aria-label={shown.label}
      onClick={incompatible ? () => void executeAction("unity.installPackage") : undefined}
    >
      {shown.label}
    </BarButton>
  );
}

/** Follows the link of the open project; returns the unsubscribe function. */
export function trackUnityLink(): () => void {
  let generation = 0;
  const load = () => {
    const current = ++generation;
    if (!useProject.getState().project?.unity) {
      useUnityLink.setState({ status: null });
      return;
    }
    void call(commands.unityLinkStatus())
      .then((status) => {
        if (current === generation) useUnityLink.setState({ status });
      })
      .catch(() => {});
  };
  load();
  const offEvent = onIdeEvent("unityLink", (e) => {
    generation++;
    useUnityLink.setState({ status: e.status });
  });
  const offProject = useProject.subscribe((s, prev) => {
    if (s.project?.root !== prev.project?.root) load();
  });
  return () => {
    offEvent();
    offProject();
  };
}
