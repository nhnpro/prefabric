import { describe, expect, it } from "vitest";
import type { RunConfig } from "@/platform/ipc";
import { createTestParser, frameworkFor } from "./index";
import { rerunFailedConfig } from "./rerun";
import { TestResults } from "./results";
import { dotnetFilter, trxEvents, withTrxLogger } from "./trx";

// Trimmed from a real `dotnet test --logger trx` run of fixtures/csharp-app (xUnit 2.8, SDK 9.0.100).
const TRX = `﻿<?xml version="1.0" encoding="utf-8"?>
<TestRun id="a3" name="run" xmlns="http://microsoft.com/schemas/VisualStudio/TeamTest/2010">
  <Results>
    <UnitTestResult executionId="e1" testId="t1" testName="Tasks.Tests.TaskStoreTests.PendingPutsHighPriorityFirst" duration="00:00:00.0014363" outcome="Passed" />
    <UnitTestResult executionId="e2" testId="t2" testName="Tasks.Tests.TaskStoreTests.SearchMatchesSubstrings(text: &quot;xyz&quot;, expected: 0)" duration="00:00:01.5" outcome="Passed" />
    <UnitTestResult executionId="e3" testId="t3" testName="Tasks.Tests.TaskStoreTests.SearchIgnoresCase" duration="00:00:00.0015376" outcome="Failed">
      <Output>
        <ErrorInfo>
          <Message>Assert.Single() Failure: The collection was empty</Message>
          <StackTrace>   at Tasks.Tests.TaskStoreTests.SearchIgnoresCase() in /w/csharp-app/tests/Tasks.Tests/TaskStoreTests.cs:line 64
   at System.RuntimeMethodHandle.InvokeMethod(Object target, Void** arguments, Signature sig, Boolean isConstructor)</StackTrace>
        </ErrorInfo>
      </Output>
    </UnitTestResult>
    <UnitTestResult executionId="e4" testId="t4" testName="Tasks.Tests.TaskStoreTests.Later" outcome="NotExecuted" />
  </Results>
  <TestDefinitions>
    <UnitTest name="a" id="t1"><TestMethod className="Tasks.Tests.TaskStoreTests" name="PendingPutsHighPriorityFirst" /></UnitTest>
    <UnitTest name="b" id="t2"><TestMethod className="Tasks.Tests.TaskStoreTests" name="SearchMatchesSubstrings" /></UnitTest>
    <UnitTest name="c" id="t3"><TestMethod className="Tasks.Tests.TaskStoreTests" name="SearchIgnoresCase" /></UnitTest>
    <UnitTest name="d" id="t4"><TestMethod className="Tasks.Tests.TaskStoreTests" name="Later" /></UnitTest>
  </TestDefinitions>
</TestRun>`;

const C = "Tasks.Tests.TaskStoreTests";

const config = (over: Partial<RunConfig>): RunConfig =>
  ({
    id: "t",
    name: "Tests",
    kind: "dotnet-test",
    command: "dotnet",
    args: ["test", "tests/Tasks.Tests"],
    cwd: "/w",
    env: {},
    params: { project: "tests/Tasks.Tests", programArgs: "" },
    isTest: true,
    ...over,
  }) as RunConfig;

describe("TRX reports", () => {
  it("become a class / method / case tree with results, output and failure locations", () => {
    const events = trxEvents(TRX);
    const results = events.filter((e) => e.type === "result");
    expect(results.map((e) => [e.path, e.status, e.durationMs])).toEqual([
      [[C, "PendingPutsHighPriorityFirst"], "passed", 1.436],
      [[C, "SearchMatchesSubstrings", 'SearchMatchesSubstrings(text: "xyz", expected: 0)'], "passed", 1500],
      [[C, "SearchIgnoresCase"], "failed", 1.538],
      [[C, "Later"], "skipped", undefined],
    ]);
    expect(events).toContainEqual({
      type: "location",
      path: [C, "SearchIgnoresCase"],
      location: { path: "/w/csharp-app/tests/Tasks.Tests/TaskStoreTests.cs", line: 64, resolve: "path" },
    });
    expect(events).toContainEqual({
      type: "output",
      path: [C, "SearchIgnoresCase"],
      line: "Assert.Single() Failure: The collection was empty",
    });
    const tree = new TestResults();
    tree.apply(events);
    tree.finish(1);
    expect(tree.counts()).toMatchObject({ passed: 2, failed: 1, skipped: 1 });
  });

  it("splits a test name without its definition before the arguments", () => {
    const xml = `<TestRun xmlns="http://microsoft.com/schemas/VisualStudio/TeamTest/2010"><Results>
      <UnitTestResult testId="x" testName="Ns.C.Add(x: 1.5)" outcome="Passed" /></Results></TestRun>`;
    expect(trxEvents(xml).find((e) => e.type === "result")?.path).toEqual(["Ns.C", "Add", "Add(x: 1.5)"]);
  });

  it("ignores files that aren't XML", () => {
    expect(trxEvents("not xml")).toEqual([]);
  });

  it("are requested before RunSettings arguments", () => {
    expect(withTrxLogger(["test", "p", "--", "RunConfiguration.X=1"], "/w/TestResults/prefabric")).toEqual([
      "test",
      "p",
      "--logger",
      "trx;LogFilePrefix=prefabric",
      "--results-directory",
      "/w/TestResults/prefabric",
      "--",
      "RunConfiguration.X=1",
    ]);
  });

  it("dotnet test is its own framework, with no live parsing", () => {
    expect(frameworkFor(config({}))).toBe("dotnet");
    expect(frameworkFor({ kind: "shell", command: "dotnet", args: ["test", "x.sln"] })).toBe("dotnet");
    expect(createTestParser(config({})).feed("  Passed X [1 ms]")).toEqual([]);
  });
});

describe("rerunning failed .NET tests", () => {
  it("filters by fully qualified name; a case reruns its method", () => {
    const exact = (n: string) => `FullyQualifiedName=${n}|FullyQualifiedName~${n}\\(`;
    expect(dotnetFilter([[C, "A"], [C, "B", "B(x: 1)"], [C, "B", "B(x: 2)"], [C]])).toBe(
      `${exact(`${C}.A`)}|${exact(`${C}.B`)}`,
    );
    expect(dotnetFilter([["N.C", "M=x"]])).toBe(exact("N.C.M\\=x"));
  });

  it("puts the filter in a dotnet-test configuration's test arguments, replacing an old one", () => {
    const rerun = rerunFailedConfig(config({ params: { project: "p", programArgs: "--filter Old -v n" } }), [
      [C, "SearchIgnoresCase"],
    ]);
    expect(rerun?.name).toBe("Failed: Tests");
    expect(rerun?.params?.programArgs).toBe(
      `-v n --filter 'FullyQualifiedName=${C}.SearchIgnoresCase|FullyQualifiedName~${C}.SearchIgnoresCase\\('`,
    );
  });

  it("adds the filter to a plain dotnet test command", () => {
    const rerun = rerunFailedConfig(config({ kind: "shell", args: ["test", "x.sln", "--filter=Old"] }), [[C, "A"]]);
    expect(rerun?.args).toEqual([
      "test",
      "x.sln",
      "--filter",
      `FullyQualifiedName=${C}.A|FullyQualifiedName~${C}.A\\(`,
    ]);
  });
});
