/**
 * Attach to Process (⌥⇧F5): the popup model (filter + groups by debugger) and the
 * attach request each debugger needs, like IDEA's `XAttachDebuggerProvider`s.
 */
import { workflowSettings } from "@/platform/workflowSettings";
import { call, commands, type AttachKind, type AttachableProcess, type LaunchConfig } from "@/platform/ipc";
import { javaAttachConfig } from "./java";

export const KIND_LABELS: Record<AttachKind, string> = {
  java: "Java (JDWP)",
  node: "Node.js",
  python: "Python (debugpy)",
  unityMono: "Unity (Mono)",
  dotnet: ".NET (netcoredbg)",
  go: "Go (Delve)",
  native: "Native (LLDB)",
};

const KIND_ORDER: AttachKind[] = ["unityMono", "java", "node", "python", "dotnet", "go", "native"];

export type AttachRow =
  | { kind: "group"; key: string; label: string; count: number }
  | { kind: "process"; key: string; process: AttachableProcess };

/** Case-insensitive match on pid, name and command line; every word must match. */
export function matchesFilter(p: AttachableProcess, filter: string): boolean {
  const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const hay = `${p.pid} ${p.name} ${p.commandLine}`.toLowerCase();
  return words.every((w) => hay.includes(w));
}

/** Rows of the popup: a header per debugger, then its processes (interpreters first). */
export function attachRows(list: readonly AttachableProcess[], filter: string): AttachRow[] {
  const rows: AttachRow[] = [];
  for (const kind of KIND_ORDER) {
    const items = list.filter((p) => p.kind === kind && matchesFilter(p, filter));
    if (!items.length) continue;
    rows.push({ kind: "group", key: `group:${kind}`, label: KIND_LABELS[kind], count: items.length });
    for (const p of items) rows.push({ kind: "process", key: `${kind}:${p.pid}`, process: p });
  }
  return rows;
}

/** A Unity player on a USB device: `adb:<serial>` (Android) or `usb:<udid>` (iOS). */
const deviceTarget = (p: AttachableProcess) => (p.host && /^(adb|usb):/.test(p.host) ? p.host : null);

/** A short description of how the process will be attached to. */
export function attachDetail(p: AttachableProcess): string {
  if (deviceTarget(p)) return `USB:${p.port}`;
  if (p.port != null) return `${p.host ?? "localhost"}:${p.port}`;
  return `pid ${p.pid}`;
}

function base(p: AttachableProcess, cwd: string, extra: Record<string, unknown>): LaunchConfig {
  return {
    name: `${p.name} (${p.pid})`,
    adapter: p.adapter,
    request: "attach",
    program: null,
    args: [],
    cwd,
    env: {},
    extraJson: JSON.stringify(extra),
  };
}

/** The attach request for `p`; `root` is the project root (sources, java module). */
export async function attachConfig(p: AttachableProcess, root: string): Promise<LaunchConfig> {
  const skip = workflowSettings().debugger.skipLibraryCode;
  switch (p.kind) {
    case "java":
      return javaAttachConfig(
        { name: `${p.name} (${p.pid})`, module: "", hostName: p.host ?? "localhost", port: p.port! },
        root,
      );
    case "node":
      return base(p, root, {
        type: "pwa-node",
        ...(p.port != null ? { address: p.host ?? "localhost", port: p.port } : { processId: String(p.pid) }),
        skipFiles: skip ? ["<node_internals>/**", "**/node_modules/**"] : [],
        // Without `--inspect`, js-debug enables the inspector by signalling the process.
        continueOnAttach: true,
      });
    case "python":
      return base(
        p,
        root,
        // `debugpy --listen` runs its own adapter on that port: talk DAP to it directly.
        // (`--pid` injection ships only an x86_64 helper on macOS.)
        p.port != null ? { debugServer: p.port, justMyCode: skip } : { processId: p.pid, justMyCode: skip },
      );
    case "unityMono": {
      // A player on a USB device is reached through a local port forwarded to the device.
      const device = deviceTarget(p);
      if (device) {
        const port = await call(commands.debugUnityForward(device, p.port!));
        return base(p, root, { address: "127.0.0.1", port });
      }
      // An Editor or a player found on the network: connect to its Mono debugger port.
      return base(p, root, { address: p.host ?? "127.0.0.1", port: p.port });
    }
    case "dotnet":
      return base(p, root, { processId: p.pid, justMyCode: skip });
    case "go":
      return base(p, root, { mode: "local", processId: p.pid });
    case "native":
      return base(p, root, { pid: p.pid });
  }
}

/** The Unity Editor (not a player) that has the project at `root` open, from its `-projectPath`. */
export function unityEditorFor(list: readonly AttachableProcess[], root: string): AttachableProcess | null {
  const norm = (p: string) => p.replaceAll("\\", "/").replace(/\/+$/, "").toLowerCase();
  const want = norm(root);
  return (
    list.find((p) => {
      if (p.kind !== "unityMono" || p.host != null) return false;
      const m = /(?:^|\s)-projectPath\s+(?:"([^"]+)"|(.+?))(?=\s+-\w|\s*$)/i.exec(p.commandLine);
      return !!m && norm(m[1] ?? m[2] ?? "") === want;
    }) ?? null
  );
}
