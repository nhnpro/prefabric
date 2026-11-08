/** Unity Console state: the Editor's log lines, kept from activation so nothing logged
 * before the window opens is lost, and the view's filters. */
import { create } from "zustand";
import type { UnityLogEntry } from "@/platform/ipc";
import { levelOf, type ConsoleLevel, type ConsoleRow, type LevelCounts } from "./model";

/** Kept rows; a chatty game must not grow the buffer without bound. */
export const MAX_ROWS = 20_000;

interface ConsoleState {
  rows: ConsoleRow[];
  /** Rows per level, kept as rows come and go (the toolbar shows them on every batch). */
  counts: LevelCounts;
  levels: Record<ConsoleLevel, boolean>;
  query: string;
  collapse: boolean;
  /** Keep the view on the last row; scrolling up turns it off. */
  follow: boolean;
  selected: number | null;
}

export const useUnityConsole = create<ConsoleState>(() => ({
  rows: [],
  counts: { log: 0, warning: 0, error: 0 },
  levels: { log: true, warning: true, error: true },
  query: "",
  collapse: false,
  follow: true,
  selected: null,
}));

let nextId = 0;

export function appendEntries(entries: readonly UnityLogEntry[]) {
  if (!entries.length) return;
  useUnityConsole.setState((s) => {
    const counts = { ...s.counts };
    const added = entries.map((e) => {
      counts[levelOf(e.kind)]++;
      return { ...e, id: nextId++ };
    });
    const all = s.rows.concat(added);
    const drop = all.length - MAX_ROWS;
    if (drop <= 0) return { rows: all, counts };
    for (let i = 0; i < drop; i++) counts[levelOf(all[i]!.kind)]--;
    return { rows: all.slice(drop), counts };
  });
}

export function clearConsole() {
  useUnityConsole.setState({ rows: [], counts: { log: 0, warning: 0, error: 0 }, selected: null });
}
