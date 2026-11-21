import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { contextConfig, fileRole } from "./context";
import { csharpTargets } from "./csharpTargets";
import { testHostProcessId } from "./dotnetDebug";
import { detectRunTargets, targetConfig } from "./targets";
import { produceContext, produceFile, OPTIMISTIC } from "./producers";
import { dotnetTestType } from "./dotnetTypes";
import type { RunConfig } from "@/platform/ipc";

const config = (project: string): RunConfig => ({
  id: project,
  name: project,
  command: "dotnet",
  args: ["test", project],
  kind: "dotnet-test",
  params: { project, framework: "net9.0" },
  cwd: null,
  env: {},
  detected: true,
  isTest: true,
  debugAdapter: null,
});
const source = `namespace Demo.Tests;
public class Calculator {
 // [Fact] public void NotATest() {}
 [Xunit.Fact]
 public void Adds() { var s = "}"; }
 [Theory, InlineData(1)]
 public async Task Values(int n) { }
 public class Nested {
   [TestMethod] public void Child() {}
 }
}`;
describe("C# editor test targets", () => {
  it("tracks lines across multiline tokens and separate test classes", () => {
    const text = `namespace Demo {
/* a comment
   across lines */
class First {
  [Fact]
  void One(
  ) {}
}
class Second {
  [TestCase(
    1)]
  void Two(int value) {}
}
}`;
    expect(csharpTargets(text).map((t) => [t.line, t.path])).toEqual([
      [3, ["Demo.First"]],
      [5, ["Demo.First", "One"]],
      [8, ["Demo.Second"]],
      [11, ["Demo.Second", "Two"]],
    ]);
  });
  it("runs only methods declared in the chosen file", () => {
    const result = produceFile(
      { path: "/repo/two/Tests.cs", root: "/repo", content: source, configs: [config("/repo/two/Two.csproj")] },
      OPTIMISTIC,
    );
    expect(JSON.parse(result!.config!.params!.testMethods!)).toEqual([
      "Demo.Tests.Calculator.Adds",
      "Demo.Tests.Calculator.Values",
      "Demo.Tests.Calculator+Nested.Child",
    ]);
  });
  it("finds qualified and multiline attributes, nested types, and skips literals/comments", () => {
    expect(
      csharpTargets(source)
        .filter((t) => t.kind === "test")
        .map((t) => [t.line, t.path]),
    ).toEqual([
      [4, ["Demo.Tests.Calculator", "Adds"]],
      [6, ["Demo.Tests.Calculator", "Values"]],
      [8, ["Demo.Tests.Calculator+Nested", "Child"]],
    ]);
    expect(csharpTargets("namespace A { namespace B { class C { [Test] void T() {} } } }").at(-1)?.path).toEqual([
      "A.B.C",
      "T",
    ]);
  });
  it("selects the owning project when same-named tests exist in sibling projects", () => {
    const targets = csharpTargets(source);
    const configs = [config("one/One.csproj"), config("two/Two.csproj")];
    const result = targetConfig(
      targets.find((t) => t.name === "Adds")!,
      "two/Tests.cs",
      configs,
      source,
    )!;
    expect(result.params?.project).toBe("two/Two.csproj");
    expect(dotnetTestType.commandLine(result.params!)?.args).toContain("FullyQualifiedName=Demo.Tests.Calculator.Adds");
  });
  it("normalizes absolute workspace project paths for caret/gutter launch", () => {
    const result = produceContext(
      {
        path: "/repo/two/Tests.cs",
        root: "/repo",
        content: source,
        line: 4,
        configs: [config("/repo/one/One.csproj"), config("/repo/two/Two.csproj")],
      },
      OPTIMISTIC,
    );
    expect(result?.config?.params?.project).toBe("two/Two.csproj");
  });
});

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

const pick = (ts: ReturnType<typeof detectRunTargets>) => ts.map((t) => [t.line, t.kind, t.path.join("/")]);

describe("C# run targets", () => {
  it("finds the fixture's xUnit facts and their class", () => {
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

  it("finds Unity's [UnityTest] coroutines", () => {
    const text = [
      "using System.Collections;",
      "using UnityEngine.TestTools;",
      "public class PlayerTests",
      "{",
      "    [UnityTest]",
      "    public IEnumerator Jumps() { yield return null; }",
      "}",
    ].join("\n");
    expect(pick(detectRunTargets(text, "Assets/Tests/PlayerTests.cs"))).toEqual([
      [2, "suite", "PlayerTests"],
      [5, "test", "PlayerTests/Jumps"],
    ]);
    expect(fileRole("Assets/Tests/PlayerTests.cs", text)).toBe("test");
  });

  it("declarations in the global namespace aren't top-level statements", () => {
    for (const text of [
      "public interface IStore { }",
      "public enum Color { Red }",
      "public delegate void H();",
      "public readonly ref struct Span2 { }",
      '[assembly: InternalsVisibleTo("X")]',
    ])
      expect(detectRunTargets(text, "A.cs"), text).toEqual([]);
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
    expect(detectRunTargets("class C { void Main() { } }", "C.cs")).toEqual([]);
    const fixture = readFileSync("fixtures/csharp-app/src/Tasks.Cli/Program.cs", "utf8");
    const main = detectRunTargets(fixture, "src/Tasks.Cli/Program.cs").filter((t) => t.kind === "main");
    expect(main).toHaveLength(1);
    expect(detectRunTargets("// comment\nusing System;\n\nConsole.WriteLine(1);\n", "a.cs")).toMatchObject([
      { line: 3, kind: "main" },
    ]);
    expect(detectRunTargets("namespace A;\npublic enum E { X }\n", "E.cs")).toEqual([]);
  });

  it("runs Main through its project's configuration without a launch profile", () => {
    const main = targetConfig(
      { line: 0, kind: "main", name: "Main", path: [] },
      "src/Tasks.Cli/Program.cs",
      CONFIGS,
      "Console.WriteLine();",
    );
    expect(main?.id).toBe("dotnet:src/Tasks.Cli/Tasks.Cli.csproj:run:");
    expect(
      contextConfig("tests/Other.Tests/ATests.cs", CONFIGS, "class A { [Fact] void T() {} }")?.params?.project,
    ).toBe("tests/Other.Tests/Other.Tests.csproj");
  });

  it("runs Program.cs through its project's configuration", () => {
    const content = readFileSync("fixtures/csharp-app/src/Tasks.Cli/Program.cs", "utf8");
    const produced = produceFile(
      { path: "/w/src/Tasks.Cli/Program.cs", root: "/w", content, configs: CONFIGS } as never,
      OPTIMISTIC,
    );
    expect(produced?.config?.id).toBe("dotnet:src/Tasks.Cli/Tasks.Cli.csproj:run:");
  });

  it("reads the waiting test host's process id", () => {
    expect(testHostProcessId("Process Id: 43650, Name: dotnet")).toBe(43650);
    expect(testHostProcessId("Process Id: 1, Name: testhost")).toBe(1);
    expect(testHostProcessId("Waiting for debugger attach...")).toBeNull();
  });
});
