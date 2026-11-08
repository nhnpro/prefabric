import { describe, expect, it } from "vitest";
import type { UnityLogKind } from "@/platform/ipc";
import { countLevels, firstLine, linkable, parseFrames, resolveFile, shownRows, type ConsoleRow } from "./model";
import { MAX_ROWS, appendEntries, clearConsole, useUnityConsole } from "./store";

let id = 0;
const row = (kind: UnityLogKind, message: string, stackTrace = ""): ConsoleRow => ({
  id: id++,
  kind,
  message,
  stackTrace,
  time: "12:00:00.000",
});
const all = { log: true, warning: true, error: true };

describe("Unity Console model", () => {
  it("filters by level and text", () => {
    const rows = [
      row("log", "hello"),
      row("warning", "Careful"),
      row("exception", "NullReference"),
      row("assert", "x"),
    ];
    expect(countLevels(rows)).toEqual({ log: 1, warning: 1, error: 2 });
    expect(shownRows(rows, all, "", false).rows).toBe(rows);
    expect(shownRows(rows, { ...all, log: false }, "", false).rows.map((r) => r.message)).toEqual([
      "Careful",
      "NullReference",
      "x",
    ]);
    expect(shownRows(rows, all, "careful", false).rows.map((r) => r.message)).toEqual(["Careful"]);
  });

  it("collapses equal messages where they first appeared", () => {
    const rows = [row("log", "a", "s1"), row("log", "b"), row("log", "a", "s1"), row("log", "a", "s2")];
    const shown = shownRows(rows, all, "", true);
    expect(shown.rows.map((r) => [r.message, shown.counts?.get(r.id) ?? 1])).toEqual([
      ["a", 2],
      ["b", 1],
      ["a", 1],
    ]);
  });

  it("links stack frames to source", () => {
    const frames = parseFrames(
      "UnityEngine.Debug:Log (object)\nGame.Player:Update () (at Assets/Scripts/Player.cs:25)\n" +
        "Foo:Bar () (at <a1b2c3>:0)\n",
    );
    expect(frames).toEqual([
      { text: "UnityEngine.Debug:Log (object)" },
      { text: "Game.Player:Update () (at Assets/Scripts/Player.cs:25)", file: "Assets/Scripts/Player.cs", line: 25 },
      { text: "Foo:Bar () (at <a1b2c3>:0)" },
    ]);
    expect(resolveFile("/p/Game/", "Assets/A.cs")).toBe("/p/Game/Assets/A.cs");
    expect(resolveFile("/p/Game", "/abs/B.cs")).toBe("/abs/B.cs");
    expect(resolveFile("C:\\p", "C:\\x\\A.cs")).toBe("C:/x/A.cs");
    expect(firstLine("one\ntwo")).toBe("one");
    expect(linkable("/p/Game", "Assets/A.cs")).toBe(true);
    expect(linkable("/p/Game", "/p/Game/Library/PackageCache/x/A.cs")).toBe(true);
    expect(linkable("/p/Game", "/Users/bokken/build/output/unity/Editor/Mono/EditorApplication.cs")).toBe(false);
    expect(linkable("C:\\p\\Game", "c:/p/game/Assets/A.cs")).toBe(true);
  });

  it("keeps a bounded buffer", () => {
    clearConsole();
    const entries = Array.from({ length: MAX_ROWS + 10 }, (_, i) => ({
      kind: "log" as const,
      message: `m${i}`,
      stackTrace: "",
      time: "",
    }));
    appendEntries(entries.slice(0, 100));
    appendEntries(entries.slice(100));
    const rows = useUnityConsole.getState().rows;
    expect(rows).toHaveLength(MAX_ROWS);
    expect(rows.at(-1)!.message).toBe(`m${MAX_ROWS + 9}`);
    expect(rows[0]!.message).toBe("m10");
    expect(useUnityConsole.getState().counts).toEqual({ log: MAX_ROWS, warning: 0, error: 0 });
  });

  it("stays cheap with a full buffer", () => {
    clearConsole();
    const kinds: UnityLogKind[] = ["log", "warning", "error"];
    appendEntries(
      Array.from({ length: MAX_ROWS }, (_, i) => ({
        kind: kinds[i % 3]!,
        message: `m${i % 500}`,
        stackTrace: "",
        time: "",
      })),
    );
    const levels = { log: true, warning: true, error: true };
    const time = (f: () => void) => {
      const t = performance.now();
      for (let i = 0; i < 10; i++) f();
      return (performance.now() - t) / 10;
    };
    const batch = Array.from({ length: 17 }, () => ({ kind: "log" as const, message: "x", stackTrace: "", time: "" }));
    // One batch as the Editor sends them (17 rows, ~10,000 a minute), and what the view derives from it.
    const append = time(() => appendEntries(batch));
    const rows = useUnityConsole.getState().rows;
    const plain = time(() => shownRows(rows, levels, "", false));
    const collapsed = time(() => shownRows(rows, levels, "", true));
    const filtered = time(() => shownRows(rows, levels, "m1", false));

    // Loose bounds: the suite runs files in parallel. They catch going back to per-batch rescans.
    expect(append + plain).toBeLessThan(20);
    expect(collapsed).toBeLessThan(100);
    expect(filtered).toBeLessThan(100);
  });
});
