import { describe, expect, it } from "vitest";
import type { AttachableProcess } from "@/platform/ipc";
import { unityEditorFor } from "./attach";

const unity = (pid: number, commandLine: string, host: string | null = null): AttachableProcess => ({
  pid,
  name: "Unity",
  commandLine,
  kind: "unityMono",
  adapter: "unity-debugger",
  host,
  port: 56000 + (pid % 1000),
});

describe("unityEditorFor", () => {
  const app = "/Applications/Unity/Hub/Editor/6000.6.8f1/Unity.app/Contents/MacOS/Unity";
  const list = [
    unity(11, `${app} -projectPath /work/Other -logFile x.log`),
    unity(12, `${app} -projectPath /work/My Game -debugCodeOptimization`),
    unity(13, `${app} -projectPath /work/My Game`, "192.168.1.5"),
    { ...unity(14, `java -jar app.jar -projectPath /work/My Game`), kind: "java" } as AttachableProcess,
  ];

  it("finds the Editor that has the project open", () => {
    expect(unityEditorFor(list, "/work/My Game/")?.pid).toBe(12);
    expect(unityEditorFor(list, "/work/other")?.pid).toBe(11);
  });

  it("ignores players, other kinds and other projects", () => {
    expect(unityEditorFor(list.slice(2), "/work/My Game")).toBeNull();
    expect(unityEditorFor(list, "/work/My")).toBeNull();
  });

  it("reads a quoted Windows path", () => {
    const win = unity(15, `"C:\\Unity\\Unity.exe" -projectPath "C:\\Games\\My Game" -nographics`);
    expect(unityEditorFor([win], "c:/Games/My Game")?.pid).toBe(15);
  });
});
