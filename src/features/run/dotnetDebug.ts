/**
 * Debug a .NET test run: `dotnet test` with `VSTEST_HOST_DEBUG=1` makes each test host print its
 * process id and wait for a debugger; netcoredbg attaches to it, then the tests run. Output and
 * the test tree stay in the Run tab.
 */
import type { RunConfig } from "@/platform/ipc";
import { executeAction } from "@/platform/actions";
import { startRun, useRun } from "./store";

/** `Process Id: 43650, Name: dotnet` (VSTest's host debugging prompt). */
export function testHostProcessId(line: string): number | null {
  const m = /^\s*Process Id:\s*(\d+),\s*Name:/.exec(line);
  return m ? Number(m[1]) : null;
}

export async function debugDotnetTest(config: RunConfig): Promise<void> {
  let sessionId: string | null = null;
  const waiting: number[] = [];
  const attach = (pid: number) => {
    const runId = useRun.getState().sessions.find((s) => s.id === sessionId)?.runId ?? null;
    void executeAction("debug.attachDotnet", { name: config.name, pid, process: runId, runConfig: config });
  };
  const session = await startRun(config, undefined, undefined, {
    launch: { ...config, env: { ...config.env, VSTEST_HOST_DEBUG: "1" } },
    onLine: (line) => {
      const pid = testHostProcessId(line);
      if (pid === null) return;
      if (sessionId) attach(pid);
      else waiting.push(pid);
    },
  });
  if (!session) return;
  sessionId = session.id;
  waiting.splice(0).forEach(attach);
}
