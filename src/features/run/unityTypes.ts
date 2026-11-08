import type { RunConfigTypeContribution } from "@/workbench/contributions";
import { formatArgs, parseArgs } from "./configs";

/** Tests of one Unity test assembly, run in the connected Editor (no process: `ops/run/unity.rs`). */
export const unityTestType: RunConfigTypeContribution = {
  id: "unity-test",
  label: "Unity Tests",
  icon: "file-csharp",
  description: "Run tests in the Unity Editor",
  defaults: { mode: "editMode" },
  fields: [
    { key: "assembly", label: "Test assembly", mono: true },
    {
      key: "mode",
      label: "Test mode",
      control: "select",
      options: [
        { value: "editMode", label: "Edit Mode" },
        { value: "playMode", label: "Play Mode" },
      ],
    },
    { key: "tests", label: "Tests", mono: true, hint: "Full names of tests or classes; empty runs them all" },
  ],
  ownsWorkingDirectory: false,
  terminal: false,
  commandLine: (p) => ({ command: "unity", args: parseArgs(p.tests ?? "") }),
  paramsFrom: (config) =>
    config.kind === "unity-test" ? { ...config.params, tests: config.params?.tests ?? formatArgs(config.args) } : null,
};
