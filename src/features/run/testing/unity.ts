/**
 * Unity tests run in the Editor through the link (no process): the backend writes each `tests/event`
 * as a marker line (`ide-unity` `link/test_run.rs`) followed by readable text. Tests are
 * `[class, method]`, and data-driven cases `[class, method, case]`, as for `dotnet test`.
 */
import type { RunConfig } from "@/platform/ipc";
import { formatArgs } from "@/platform/commandLine";
import { TestStdout, type TestEvent, type TestOutputParser } from "./types";

export const UNITY_TEST_MARKER = "##unity-test ";

interface UnityTestEvent {
  event?: string;
  fullName?: string;
  name?: string;
  className?: string;
  methodName?: string;
  status?: string;
  durationMs?: number;
  message?: string;
  stackTrace?: string;
  output?: string;
  passed?: number;
  failed?: number;
  skipped?: number;
}

function pathOf(e: UnityTestEvent): string[] | null {
  let cls = e.className ?? "";
  let method = e.methodName || e.name || "";
  if (!cls && e.fullName) {
    // No fixture type (a test outside a class): the parent of the full name.
    const bare = e.fullName.split("(")[0]!;
    const dot = bare.lastIndexOf(".");
    if (dot < 0) return null;
    cls = bare.slice(0, dot);
    method ||= bare.slice(dot + 1);
  }
  if (!cls || !method) return null;
  return e.name && e.name !== method ? [cls, method, e.name] : [cls, method];
}

/** First frame with a source position: `at A.B () [0x0001] in /p/Assets/T.cs:12`. */
function frameLocation(stack: string): { path: string; line: number } | null {
  const m = / in (.+?):(\d+)\s*$/m.exec(stack);
  return m ? { path: m[1]!, line: Number(m[2]) } : null;
}

function events(e: UnityTestEvent): TestEvent[] {
  if (e.event === "runFinished")
    return [{ type: "summary", passed: e.passed, failed: e.failed, skipped: e.skipped, durationMs: e.durationMs }];
  const path = pathOf(e);
  if (!path) return [];
  if (e.event === "testStarted") return [{ type: "start", path }];
  if (e.event !== "testFinished") return [];
  const status = e.status === "passed" ? "passed" : e.status === "failed" ? "failed" : "skipped";
  const out: TestEvent[] = [];
  const text = [e.message?.trim(), e.stackTrace?.trimEnd(), e.output?.trim()].filter(Boolean).join("\n");
  for (const line of text ? text.split("\n").slice(0, 200) : []) out.push({ type: "output", path, line });
  const at = status === "failed" ? frameLocation(e.stackTrace ?? "") : null;
  if (at) out.push({ type: "location", path, location: { path: at.path, line: at.line, resolve: "path" } });
  const durationMs = typeof e.durationMs === "number" ? Math.round(e.durationMs * 1000) / 1000 : undefined;
  out.push({ type: "result", path, status, durationMs });
  return out;
}

export function unityParser(): TestOutputParser {
  return {
    feed(line) {
      if (!line.startsWith(UNITY_TEST_MARKER)) return [];
      try {
        return events(JSON.parse(line.slice(UNITY_TEST_MARKER.length)) as UnityTestEvent);
      } catch {
        return [];
      }
    },
  };
}

/** Console text of a Unity test run: the marker lines only feed the test tree. */
export class UnityTestOutput extends TestStdout {
  constructor(feedLine: (line: string) => void) {
    super(feedLine, (line) => (line.startsWith(UNITY_TEST_MARKER) ? "" : line + "\n"));
  }
}

/** The run of only `paths`: full names of their fixtures or methods (a case runs its whole method). */
export function unityTestsConfig(config: RunConfig, paths: readonly string[][]): Pick<RunConfig, "args" | "params"> {
  const names = [...new Set(paths.filter((p) => p.length).map((p) => p.slice(0, 2).join(".")))];
  return withUnityTests(config, names);
}

/** `config` running only the tests or fixtures named (full names). */
export function withUnityTests(config: RunConfig, names: string[]): Pick<RunConfig, "args" | "params"> {
  return { args: names, params: { ...config.params, tests: formatArgs(names) } };
}
