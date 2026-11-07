/**
 * .NET tests (`dotnet test`, any adapter: xUnit, NUnit, MSTest) report through VSTest's TRX logger:
 * the run writes `.trx` files into a results folder and the tree is built from them when the run
 * ends. Tests are `[class, method]`, and data-driven cases `[class, method, case]`.
 */
import { parseArgs, formatArgs } from "@/platform/commandLine";
import type { RunConfig } from "@/platform/ipc";
import type { TestEvent, TestStatus } from "./types";

/** Results folder of a run, relative to its working directory. */
export const TRX_DIR = "TestResults/prefabric";

/** `dotnet test` arguments plus the TRX logger writing into `dir` (before any `--` RunSettings arguments). */
export function withTrxLogger(args: readonly string[], dir: string): string[] {
  const own = ["--logger", "trx;LogFilePrefix=prefabric", "--results-directory", dir];
  const at = args.indexOf("--");
  return at < 0 ? [...args, ...own] : [...args.slice(0, at), ...own, ...args.slice(at)];
}

/** `00:00:01.2345678` → ms. */
function duration(text: string | null): number | undefined {
  const m = /^(\d+):(\d+):(\d+(?:\.\d+)?)$/.exec(text ?? "");
  if (!m) return undefined;
  const ms = ((Number(m[1]) * 60 + Number(m[2])) * 60 + Number(m[3])) * 1000;
  return Math.round(ms * 1000) / 1000;
}

function status(outcome: string | null): Exclude<TestStatus, "running"> {
  switch (outcome) {
    case "Passed":
    case "PassedButRunAborted":
      return "passed";
    case "NotExecuted":
    case "Inconclusive":
    case "Pending":
    case "NotRunnable":
    case "Disconnected":
    case "Warning":
      return "skipped";
    default:
      return "failed";
  }
}

/** First frame with a source position: `at A.B.C() in /p/CTests.cs:line 64`. */
function frameLocation(stack: string): { path: string; line: number } | null {
  const m = / in (.+?):line (\d+)\s*$/m.exec(stack);
  return m ? { path: m[1]!, line: Number(m[2]) } : null;
}

const child = (el: Element, name: string) => el.getElementsByTagName(name)[0] ?? null;

/** One TRX file → events. */
export function trxEvents(xml: string): TestEvent[] {
  const doc = new DOMParser().parseFromString(xml.replace(/^﻿/, ""), "application/xml");
  if (doc.getElementsByTagName("parsererror").length) return [];
  const methods = new Map<string, { className: string; name: string }>();
  for (const test of doc.getElementsByTagName("UnitTest")) {
    const method = child(test, "TestMethod");
    const id = test.getAttribute("id");
    if (id && method) {
      methods.set(id, { className: method.getAttribute("className") ?? "", name: method.getAttribute("name") ?? "" });
    }
  }
  const events: TestEvent[] = [];
  for (const result of doc.getElementsByTagName("UnitTestResult")) {
    const testName = result.getAttribute("testName") ?? "";
    const method = methods.get(result.getAttribute("testId") ?? "");
    const className = method?.className || testName.slice(0, Math.max(0, testName.lastIndexOf(".")));
    const name = method?.name || testName.slice(className.length + 1);
    if (!className || !name) continue;
    // The display name, without the class: `Search(text: "a")` for a case of `Search`.
    const display = testName.startsWith(`${className}.`) ? testName.slice(className.length + 1) : testName;
    const path = display !== name && display.startsWith(name) ? [className, name, display] : [className, name];
    const s = status(result.getAttribute("outcome"));
    const message = child(result, "Message")?.textContent?.trim() ?? "";
    const stack = child(result, "StackTrace")?.textContent ?? "";
    const stdout = child(result, "StdOut")?.textContent?.trim() ?? "";
    const text = [message, stack.trimEnd(), stdout].filter(Boolean).join("\n");
    for (const line of text ? text.split("\n").slice(0, 200) : []) events.push({ type: "output", path, line });
    const at = s === "failed" ? frameLocation(stack) : null;
    if (at) events.push({ type: "location", path, location: { path: at.path, line: at.line, resolve: "path" } });
    events.push({ type: "result", path, status: s, durationMs: duration(result.getAttribute("duration")) });
  }
  return events;
}

/** `--filter` expression selecting the tests at `paths` (a case runs its whole method). */
export function dotnetFilter(paths: readonly string[][]): string | null {
  const names = [...new Set(paths.filter((p) => p.length >= 2).map((p) => `${p[0]}.${p[1]}`))];
  if (!names.length) return null;
  const escape = (s: string) => s.replace(/[\\()&|=!~]/g, "\\$&");
  return names.map((n) => `FullyQualifiedName=${escape(n)}`).join("|");
}

/** Arguments with any `--filter` replaced by one selecting `paths`. */
function filteredArgs(args: readonly string[], filter: string): string[] {
  const out: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const a = args[i]!;
    if (a === "--filter") {
      i++;
      continue;
    }
    if (a.startsWith("--filter=")) continue;
    out.push(a);
  }
  const at = out.indexOf("--");
  const own = ["--filter", filter];
  return at < 0 ? [...out, ...own] : [...out.slice(0, at), ...own, ...out.slice(at)];
}

/**
 * The run of only `paths`: a `dotnet-test` configuration rebuilds its command line from its
 * parameters at launch, so the filter goes into its test arguments; other commands get it directly.
 */
export function dotnetTestsConfig(
  config: RunConfig,
  paths: readonly string[][],
): Pick<RunConfig, "args" | "params"> | null {
  const filter = dotnetFilter(paths);
  return filter ? withDotnetFilter(config, filter) : null;
}

/** `config` running only the tests `filter` (a `--filter` expression) selects. */
export function withDotnetFilter(config: RunConfig, filter: string): Pick<RunConfig, "args" | "params"> {
  if (config.kind !== "dotnet-test") return { args: filteredArgs(config.args, filter), params: config.params };
  const programArgs = formatArgs(filteredArgs(parseArgs(config.params?.programArgs ?? ""), filter));
  return { args: config.args, params: { ...config.params, programArgs } };
}
