import { describe, expect, it } from "vitest";
import type { RunConfig } from "@/platform/ipc";
import { createTestParser, frameworkFor, parseAll } from "./index";
import { detectRunTargets, targetConfig } from "../targets";
import { testsConfig } from "./rerun";
import { UNITY_TEST_MARKER, UnityTestOutput } from "./unity";

const line = (e: object) => `${UNITY_TEST_MARKER}${JSON.stringify(e)}`;
const config = {
  id: "unity-test:Game.Tests",
  name: "Game.Tests (Unity tests)",
  kind: "unity-test",
  command: "unity",
  args: [],
  cwd: "Assets/Tests",
  env: {},
  detected: true,
  isTest: true,
  debugAdapter: null,
  params: { assembly: "Game.Tests", mode: "editMode" },
} as unknown as RunConfig;

describe("Unity tests", () => {
  it("turns the Editor's events into the test tree", () => {
    expect(frameworkFor(config)).toBe("unity");
    const text = [
      line({ event: "runStarted", count: 2 }),
      line({ event: "testStarted", className: "Game.T", methodName: "A", name: "A", fullName: "Game.T.A" }),
      line({
        event: "testFinished",
        className: "Game.T",
        methodName: "A",
        name: "A",
        status: "failed",
        durationMs: 3.25,
        message: "Expected: 2\n  But was: 1",
        stackTrace: "at Game.T.A () [0x0003a] in /p/Assets/Tests/T.cs:27\n",
        output: "",
      }),
      "Failed Game.T.A (3 ms)",
      line({ event: "testFinished", className: "Game.T", methodName: "Add", name: "Add(1,2)", status: "inconclusive" }),
      line({ event: "testFinished", fullName: "Lone.Test", status: "passed" }),
      line({ event: "runFinished", passed: 1, failed: 1, skipped: 1, durationMs: 900 }),
      `${UNITY_TEST_MARKER}{broken`,
    ].join("\n");
    expect(parseAll(createTestParser(config), text)).toEqual([
      { type: "start", path: ["Game.T", "A"] },
      { type: "output", path: ["Game.T", "A"], line: "Expected: 2" },
      { type: "output", path: ["Game.T", "A"], line: "  But was: 1" },
      { type: "output", path: ["Game.T", "A"], line: "at Game.T.A () [0x0003a] in /p/Assets/Tests/T.cs:27" },
      {
        type: "location",
        path: ["Game.T", "A"],
        location: { path: "/p/Assets/Tests/T.cs", line: 27, resolve: "path" },
      },
      { type: "result", path: ["Game.T", "A"], status: "failed", durationMs: 3.25 },
      { type: "result", path: ["Game.T", "Add", "Add(1,2)"], status: "skipped", durationMs: undefined },
      { type: "result", path: ["Lone", "Test"], status: "passed", durationMs: undefined },
      { type: "summary", passed: 1, failed: 1, skipped: 1, durationMs: 900 },
    ]);
  });

  it("hides the marker lines from the console", () => {
    const fed: string[] = [];
    const out = new UnityTestOutput((l) => fed.push(l));
    expect(out.feed(`${line({ event: "runStarted" })}\nPassed Game.T.A (1 ms)\n`)).toBe("Passed Game.T.A (1 ms)\n");
    expect(fed).toHaveLength(2);
  });

  it("reruns tests by their full names, a case by its method", () => {
    const rerun = testsConfig(config, [["Game.T", "A"], ["Game.T", "Add", "Add(1,2)"], ["Game.U"]], "Rerun");
    expect(rerun?.args).toEqual(["Game.T.A", "Game.T.Add", "Game.U"]);
    expect(rerun?.params).toEqual({ assembly: "Game.Tests", mode: "editMode", tests: "Game.T.A Game.T.Add Game.U" });
  });

  it("runs a test or class from the gutter in its assembly's configuration", () => {
    const rel = "Assets/Tests/PlayerTests.cs";
    const test = targetConfig({ line: 9, kind: "test", name: "A", path: ["Game.T", "A"] }, rel, [config])!;
    expect(test).toMatchObject({ kind: "unity-test", args: ["Game.T.A"], name: "A", isTest: true });
    expect(test.params).toEqual({ assembly: "Game.Tests", mode: "editMode", tests: "Game.T.A" });
    const suite = targetConfig({ line: 6, kind: "suite", name: "T", path: ["Game.T"] }, rel, [config])!;
    expect(suite.args).toEqual(["Game.T"]);
    expect(
      targetConfig({ line: 9, kind: "test", name: "A", path: ["Game.T", "A"] }, "Assets/Scripts/P.cs", [config]),
    ).toBeNull();
  });

  it("finds [UnityTest] coroutines as tests", () => {
    const text = [
      "namespace Game.Tests",
      "{",
      "    public class Waits",
      "    {",
      "        [UnityTest]",
      "        public IEnumerator Slow() { yield return null; }",
      "    }",
      "}",
    ].join("\n");
    const targets = detectRunTargets(text, "Assets/Tests/Waits.cs").map((t) => [t.kind, t.path.join(".")]);
    expect(targets).toEqual([
      ["suite", "Game.Tests.Waits"],
      ["test", "Game.Tests.Waits.Slow"],
    ]);
  });
});
