/** Unity Console rows: levels, filters, collapsing equal messages, and stack frames that link to source. */
import type { UnityLogEntry, UnityLogKind } from "@/platform/ipc";

export type ConsoleLevel = "log" | "warning" | "error";

export interface ConsoleRow extends UnityLogEntry {
  /** Grows forever: the buffer drops old rows, so positions are not stable keys. */
  id: number;
}

/** Rows as shown, and how many equal messages a collapsed row stands for (absent: 1). */
export interface Shown {
  rows: readonly ConsoleRow[];
  counts?: ReadonlyMap<number, number>;
}

export function levelOf(kind: UnityLogKind): ConsoleLevel {
  return kind === "log" ? "log" : kind === "warning" ? "warning" : "error";
}

export type LevelCounts = Record<ConsoleLevel, number>;

export function countLevels(rows: readonly ConsoleRow[]): LevelCounts {
  const counts: LevelCounts = { log: 0, warning: 0, error: 0 };
  for (const r of rows) counts[levelOf(r.kind)]++;
  return counts;
}

/** Rows of the shown levels whose message contains `query` (case-insensitive); with
 * `collapse`, one row per distinct message and stack trace, where it first appeared.
 * Runs on every batch over the whole buffer, so the usual case (everything shown) is free. */
export function shownRows(
  rows: readonly ConsoleRow[],
  levels: Readonly<Record<ConsoleLevel, boolean>>,
  query: string,
  collapse: boolean,
): Shown {
  const needle = query.trim().toLowerCase();
  const everything = levels.log && levels.warning && levels.error && !needle;
  if (everything && !collapse) return { rows };
  const keep = (r: ConsoleRow) => levels[levelOf(r.kind)] && (!needle || r.message.toLowerCase().includes(needle));
  if (!collapse) return { rows: rows.filter(keep) };
  const groups = new Map<string, ConsoleRow>();
  const counts = new Map<number, number>();
  for (const row of rows) {
    if (!everything && !keep(row)) continue;
    const key = `${row.kind}\u0000${row.message}\u0000${row.stackTrace ?? ""}`;
    const first = groups.get(key);
    if (first) counts.set(first.id, (counts.get(first.id) ?? 1) + 1);
    else groups.set(key, row);
  }
  return { rows: [...groups.values()], counts };
}

/** One line of a stack trace, with its source location when it has one. */
export interface StackFrame {
  text: string;
  /** Project-relative or absolute path. */
  file?: string;
  /** 1-based. */
  line?: number;
}

/** Unity frames end in `(at Assets/Scripts/Player.cs:25)`. */
const LOCATION = /\(at (.+):(\d+)\)\s*$/;

export function parseFrames(stackTrace: string): StackFrame[] {
  return stackTrace
    .split("\n")
    .map((t) => t.trimEnd())
    .filter(Boolean)
    .map((text) => {
      const m = LOCATION.exec(text);
      return m && !m[1]!.startsWith("<") ? { text, file: m[1]!, line: Number(m[2]) } : { text };
    });
}

/** First line of a message: rows show only that, the details pane the rest. */
export function firstLine(message: string): string {
  const at = message.indexOf("\n");
  return at < 0 ? message : message.slice(0, at);
}

export function resolveFile(root: string, file: string): string {
  const f = file.replaceAll("\\", "/");
  if (f.startsWith("/") || /^[A-Za-z]:\//.test(f)) return f;
  return `${root.replace(/[\\/]+$/, "")}/${f}`;
}

/** Frames worth a link: project-relative paths (`Assets/`, `Packages/`, `Library/PackageCache/`) and
 * absolute ones inside the project. Unity's own frames name its build machine's files. */
export function linkable(root: string, file: string): boolean {
  const f = file.replaceAll("\\", "/");
  const absolute = f.startsWith("/") || /^[A-Za-z]:\//.test(f);
  if (!absolute) return true;
  const r = root.replaceAll("\\", "/").replace(/\/+$/, "");
  return f.toLowerCase().startsWith(`${r.toLowerCase()}/`);
}
