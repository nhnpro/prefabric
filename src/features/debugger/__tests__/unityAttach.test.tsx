import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { AttachableProcess, ProjectInfo, RunConfig } from "@/platform/ipc";

const mocks = vi.hoisted(() => ({ discover: vi.fn(), players: vi.fn(), start: vi.fn(), install: vi.fn() }));
vi.mock("@/platform/ipc", async (original) => {
  const real = await original<typeof import("@/platform/ipc")>();
  return {
    ...real,
    isTauri: () => true,
    commands: {
      ...real.commands,
      debugAttachableProcesses: mocks.discover,
      debugUnityPlayers: mocks.players,
    },
  };
});
vi.mock("../state", async (original) => ({
  ...(await original<typeof import("../state")>()),
  startSession: mocks.start,
}));

import { registerAction } from "@/platform/actions";
import { useProject } from "@/platform/project";
import { openAttachPopup, installAttachAdapter } from "../attachPopup";
import { DebuggerOverlays } from "../ui/Overlays";
import { useOverlays } from "../ui/popoverStore";

const editor: AttachableProcess = {
  pid: 42,
  name: "Unity Editor — game",
  commandLine: "Unity -projectPath /game -useHub",
  adapter: "unity-debugger",
  kind: "unityMono",
  host: null,
  port: 56042,
};
const java: AttachableProcess = {
  pid: 7,
  name: "java",
  commandLine: "java -jar app.jar",
  adapter: "java-debug",
  kind: "java",
  host: null,
  port: 5005,
};
const ok = (data: AttachableProcess[]) => ({ status: "ok" as const, data });
function deferred() {
  let resolve!: (result: ReturnType<typeof ok>) => void;
  const promise = new Promise<ReturnType<typeof ok>>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
let dispose: () => void;
beforeEach(() => {
  vi.clearAllMocks();
  useProject.setState({ project: { root: "/game" } as ProjectInfo });
  useOverlays.setState({ attach: null });
  mocks.discover.mockResolvedValue(ok([editor]));
  mocks.players.mockResolvedValue(ok([]));
  mocks.start.mockResolvedValue(1);
  mocks.install.mockResolvedValue(undefined);
  dispose = registerAction({ id: "tools.install", title: "Install", run: mocks.install });
});
afterEach(() => {
  cleanup();
  dispose();
  useOverlays.setState({ attach: null });
  useProject.setState({ project: null });
});

it("lists only Unity targets, the project's Editor first, and attaches to its debugger port", async () => {
  const otherEditor = { ...editor, pid: 84, commandLine: "Unity -projectPath /other-game", port: 56084 };
  mocks.discover.mockResolvedValueOnce(ok([java, otherEditor, editor]));
  await openAttachPopup("unity");
  expect(useOverlays.getState().attach?.processes?.map((p) => p.pid)).toEqual([42, 84]);
  render(<DebuggerOverlays />);
  expect(screen.getByRole("dialog", { name: "Attach to Unity Process" })).toBeTruthy();
  expect(screen.getByText(/switch to Debug Mode/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Attach" }));
  await waitFor(() =>
    expect(mocks.start).toHaveBeenCalledWith(
      expect.objectContaining({ request: "attach", adapter: "unity-debugger", cwd: "/game" }),
      { rerunnable: false },
    ),
  );
  expect(JSON.parse(mocks.start.mock.calls[0]![0].extraJson)).toEqual({ address: "127.0.0.1", port: 56042 });
  expect(useOverlays.getState().attach).toBeNull();
});

it("adds Unity players found on the network after the Editors", async () => {
  const player = { ...editor, pid: 4242, name: "Game — OSXPlayer", host: "192.168.1.20", port: 56242 };
  mocks.players.mockResolvedValueOnce(ok([player]));
  await openAttachPopup("unity");
  await waitFor(() => expect(useOverlays.getState().attach?.processes?.map((p) => p.pid)).toEqual([42, 4242]));
});

it("associates the selected run configuration after refreshing the picker", async () => {
  const runConfig = { id: "unity-editor", name: "Unity Editor", kind: "unity-editor" } as RunConfig;
  await openAttachPopup("unity", runConfig);
  render(<DebuggerOverlays />);
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await waitFor(() => expect(useOverlays.getState().attach?.processes).toEqual([editor]));
  fireEvent.click(screen.getByRole("button", { name: "Attach" }));
  await waitFor(() => expect(mocks.start).toHaveBeenCalledWith(expect.anything(), { rerunnable: false, runConfig }));
});

it("does not replace a refreshed picker with an older discovery response", async () => {
  const old = deferred();
  mocks.discover.mockReturnValueOnce(old.promise);
  const first = openAttachPopup("unity");
  await openAttachPopup("unity");
  old.resolve(ok([]));
  await first;
  expect(useOverlays.getState().attach?.processes).toEqual([editor]);
});

it("does not reopen a cancelled picker when discovery finishes", async () => {
  const pending = deferred();
  mocks.discover.mockReturnValueOnce(pending.promise);
  const request = openAttachPopup("unity");
  useOverlays.setState({ attach: null });
  pending.resolve(ok([editor]));
  await request;
  expect(useOverlays.getState().attach).toBeNull();
});

it("does not attach to an old project's process or retry installation after switching projects", async () => {
  await openAttachPopup("unity");
  render(<DebuggerOverlays />);
  act(() => useProject.setState({ project: { root: "/other" } as ProjectInfo }));
  fireEvent.click(screen.getByRole("button", { name: "Attach" }));
  expect(mocks.start).not.toHaveBeenCalled();
  act(() =>
    useOverlays.setState({
      attach: { processes: [], error: "missing", mode: "unity", projectRoot: "/game", installTool: "unity-debugger" },
    }),
  );
  await installAttachAdapter();
  expect(mocks.discover).toHaveBeenCalledTimes(1);
});
