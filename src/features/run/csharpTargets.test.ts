import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { RunConfig } from "@/platform/ipc";
import { contextConfig } from "./context";
import { testHostProcessId } from "./dotnetDebug";
import { detectRunTargets, targetConfig } from "./targets";

const pick = (ts: ReturnType<typeof detectRunTargets>) => ts.map((t) => [t.line, t.kind, t.path.join("/")]);

const dotnet = (project: string, test: boolean, launchProfile = ""): RunConfig => ({
  id: `dotnet:${project}:${test ? "test" : "run"}:${launchProfile}`,
  name: project,
  kind: test ? "dotnet-test" : "dotnet",
  command: "dotnet",
  args: test ? ["test", project] : ["run", "--project", project],
  cwd: null,
  env: {},
  detected: true,
  isTest: test,
  debugAdapter: test ? null : "netcoredbg",
  params: { project, framework: "net8.0", launchProfile, programArgs: "" },
});

const CONFIGS = [
  dotnet("src/Tasks.Cli/Tasks.Cli.csproj", false),
  dotnet("src/Tasks.Cli/Tasks.Cli.csproj", false, "Dev"),
  dotnet("tests/Tasks.Tests/Tasks.Tests.csproj", true),
  dotnet("tests/Other.Tests/Other.Tests.csproj", true),
];

describe("C# run targets", () => {
  it("finds the fixture's xUnit facts, theory and class", () => {
    const text = readFileSync("fixtures/csharp-app/tests/Tasks.Tests/TaskStoreTests.cs", "utf8");
    const targets = detectRunTargets(text, "tests/Tasks.Tests/TaskStoreTests.cs");
    const C = "Tasks.Tests.TaskStoreTests";
    expect(pick(targets).slice(0, 4)).toEqual([
      [5, "suite", C],
      [8, "test", `${C}/AddAssignsIncreasingIds`],
      [16, "test", `${C}/AddRejectsEmptyTitles`],
      [22, "test", `${C}/CompleteMarksTheTaskDone`],
    ]);
    expect(targets.find((t) => t.name === "SearchMatchesSubstrings")?.line).toBe(35);
  });

  it("NUnit and MSTest, block namespaces, nested classes and combined attributes", () => {
    const text = [
      "using NUnit.Framework;",
      "namespace Game.Tests",
      "{",
      "    [TestFixture]",
      "    public class Inventory",
      "    {",
      '        [Test, Category("fast")]',
      "        public void Adds() { }",
      '        [TestCase(1, "Test")] [TestCase(2, "x")]',
      "        public async Task Counts(int n, string s) { }",
      "        public class Nested",
      "        {",
      "            [TestMethod] public void Inner() { }",
      "        }",
      "        public void Helper() { }",
      "    }",
      "    public class NotTests { public void M() { } }",
      "}",
    ].join("\n");
    expect(pick(detectRunTargets(text, "Tests/Inventory.cs"))).toEqual([
      [4, "suite", "Game.Tests.Inventory"],
      [7, "test", "Game.Tests.Inventory/Adds"],
      [9, "test", "Game.Tests.Inventory/Counts"],
      [10, "suite", "Game.Tests.Inventory+Nested"],
      [12, "test", "Game.Tests.Inventory+Nested/Inner"],
    ]);
  });

  it("Main and top-level statements", () => {
    expect(
      pick(
        detectRunTargets(
          "namespace App;\nstatic class Program\n{\n    static async Task<int> Main(string[] a) => 0;\n}\n",
          "Program.cs",
        ),
      ),
    ).toEqual([[3, "main", ""]]);
    const fixture = readFileSync("fixtures/csharp-app/src/Tasks.Cli/Program.cs", "utf8");
    const main = detectRunTargets(fixture, "src/Tasks.Cli/Program.cs").filter((t) => t.kind === "main");
    expect(main).toHaveLength(1);
    expect(detectRunTargets("// comment\nusing System;\n\nConsole.WriteLine(1);\n", "a.cs")).toMatchObject([
      { line: 3, kind: "main" },
    ]);
    expect(detectRunTargets("namespace A;\npublic enum E { X }\n", "E.cs")).toEqual([]);
  });

  it("runs one test or class through the closest test project's filter", () => {
    const rel = "tests/Tasks.Tests/TaskStoreTests.cs";
    const C = "Tasks.Tests.TaskStoreTests";
    const test = targetConfig({ line: 9, kind: "test", name: "Adds", path: [C, "Adds"] }, rel, CONFIGS)!;
    expect(test.kind).toBe("dotnet-test");
    expect(test.params?.project).toBe("tests/Tasks.Tests/Tasks.Tests.csproj");
    expect(test.params?.programArgs).toBe(`--filter FullyQualifiedName=${C}.Adds`);
    expect(test.name).toBe("Adds");
    const suite = targetConfig({ line: 6, kind: "suite", name: "TaskStoreTests", path: [C] }, rel, CONFIGS)!;
    expect(suite.params?.programArgs).toBe(`--filter FullyQualifiedName~${C}.|FullyQualifiedName~${C}+`);
  });

  it("runs Main through its project's configuration without a launch profile", () => {
    const main = targetConfig(
      { line: 0, kind: "main", name: "Main", path: [] },
      "src/Tasks.Cli/Program.cs",
      CONFIGS,
      "Console.WriteLine();",
    );
    expect(main?.id).toBe("dotnet:src/Tasks.Cli/Tasks.Cli.csproj:run:");
    expect(contextConfig("tests/Other.Tests/ATests.cs", CONFIGS)?.params?.project).toBe(
      "tests/Other.Tests/Other.Tests.csproj",
    );
  });

  it("reads the waiting test host's process id", () => {
    expect(testHostProcessId("Process Id: 43650, Name: dotnet")).toBe(43650);
    expect(testHostProcessId("Process Id: 1, Name: testhost")).toBe(1);
    expect(testHostProcessId("Waiting for debugger attach...")).toBeNull();
  });
});
