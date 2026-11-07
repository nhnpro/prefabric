import { create } from "zustand";
import { call, commands } from "@/platform/ipc";
import { tx } from "@/platform/i18n";
import { useProject } from "@/platform/project";
import { notifications } from "@/workbench/contributions";
import type { ProjectInfo } from "@/bindings";

/** What the Unity notice is doing: installing the package or running Unity to generate project files. */
export const useUnityPackageTask = create<{ running: "install" | "generate" | null }>(() => ({ running: null }));

function applyUnityInfo(info: ProjectInfo) {
  const current = useProject.getState().project;
  if (current && current.root === info.root) useProject.setState({ project: { ...current, unity: info.unity } });
}

function errorText(e: unknown): string {
  return typeof e === "object" && e && "message" in e ? String((e as { message: unknown }).message) : String(e);
}

async function runTask(kind: "install" | "generate", task: () => Promise<ProjectInfo>, done: string, failed: string) {
  if (useUnityPackageTask.getState().running) return;
  useUnityPackageTask.setState({ running: kind });
  try {
    applyUnityInfo(await task());
    notifications().notify({ level: "info", title: done, key: "unity.package" });
  } catch (e) {
    notifications().notify({ level: "error", title: failed, body: errorText(e), key: "unity.package" });
  } finally {
    useUnityPackageTask.setState({ running: null });
  }
}

/** Copies Prefabric's Unity package into the project's `Packages/` (also updates an older copy). */
export function installUnityPackage(): Promise<void> {
  return runTask(
    "install",
    () => call(commands.unityInstallPackage()),
    tx`Unity package installed. Unity loads it when you switch to it; then choose Prefabric in Preferences > External Tools.`,
    tx`Cannot install the Unity package`,
  );
}

/** Runs the project's Unity version in batch mode to write the C# solution and projects. */
export function generateUnityProjectFiles(): Promise<void> {
  return runTask(
    "generate",
    () => call(commands.unityGenerateProjectFiles()),
    tx`C# project files generated`,
    tx`Cannot generate the C# project files`,
  );
}
