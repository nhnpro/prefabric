/**
 * Unity Console tool window: the Editor's log (virtualised, following the tail until
 * scrolled up), level toggles with counts, text filter, collapse and clear; the selected
 * message's full text and stack trace below, with frames that open their source.
 */
import { useLayoutEffect, useMemo, useRef } from "react";
import clsx from "clsx";
import { useVirtualizer } from "@tanstack/react-virtual";
import { tx } from "@/platform/i18n";
import { useProject } from "@/platform/project";
import { maybeEditorService } from "@/workbench/contributions";
import { EmptyState, IconButton, Input, T } from "@/ui";
import { Icon } from "@/ui/icons";
import { useUnityLink } from "../link";
import { firstLine, levelOf, linkable, parseFrames, resolveFile, shownRows, type ConsoleLevel } from "./model";
import { clearConsole, useUnityConsole } from "./store";
import styles from "./Console.module.css";

const ROW_HEIGHT = 22;
const LEVELS: { level: ConsoleLevel; icon: string; label: string }[] = [
  { level: "log", icon: "info", label: "Messages" },
  { level: "warning", icon: "warning", label: "Warnings" },
  { level: "error", icon: "error", label: "Errors" },
];

function openFrame(file: string, line: number) {
  const root = useProject.getState().project?.root;
  if (!root) return;
  void maybeEditorService()?.openFile(resolveFile(root, file), {
    selection: { line: Math.max(0, line - 1), col: 0 },
    focus: true,
  });
}

function Details({ id }: { id: number | null }) {
  const entry = useUnityConsole((s) => (id === null ? undefined : s.rows.find((r) => r.id === id)));
  const root = useProject((s) => s.project?.root ?? "");
  if (!entry) return null;
  return (
    <div className={styles.details} role="region" aria-label={tx("Message details")}>
      <div className={styles.detailsMessage}>{entry.message}</div>
      {parseFrames(entry.stackTrace ?? "").map((frame, i) =>
        frame.file && frame.line && linkable(root, frame.file) ? (
          <button
            key={i}
            type="button"
            className={styles.frameLink}
            onClick={() => openFrame(frame.file!, frame.line!)}
          >
            {frame.text}
          </button>
        ) : (
          <div key={i} className={styles.frame}>
            {frame.text}
          </div>
        ),
      )}
    </div>
  );
}

export function ConsoleView() {
  const rows = useUnityConsole((s) => s.rows);
  const counts = useUnityConsole((s) => s.counts);
  const levels = useUnityConsole((s) => s.levels);
  const query = useUnityConsole((s) => s.query);
  const collapse = useUnityConsole((s) => s.collapse);
  const follow = useUnityConsole((s) => s.follow);
  const selected = useUnityConsole((s) => s.selected);
  const connected = useUnityLink((s) => s.status?.connection === "connected");

  const shown = useMemo(() => shownRows(rows, levels, query, collapse), [rows, levels, query, collapse]);

  const scrollRef = useRef<HTMLDivElement>(null);
  /** The scrollTop we last wrote: only the user can move the view above it. */
  const written = useRef(0);
  const virtualizer = useVirtualizer({
    count: shown.rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    getItemKey: (index) => shown.rows[index]!.id,
    overscan: 16,
  });
  const totalSize = virtualizer.getTotalSize();
  useLayoutEffect(() => {
    if (!follow) return;
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    written.current = el.scrollTop;
  }, [shown, follow, totalSize]);

  return (
    <div className={styles.view}>
      <div className={styles.toolbar}>
        <IconButton icon="trash" title="Clear" size="small" onClick={clearConsole} />
        <button
          type="button"
          className={styles.toggle}
          aria-pressed={collapse}
          onClick={() => useUnityConsole.setState({ collapse: !collapse })}
        >
          <T>Collapse</T>
        </button>
        <Input
          className={styles.search}
          placeholder={tx("Filter messages")}
          aria-label={tx("Filter messages")}
          value={query}
          onChange={(e) => useUnityConsole.setState({ query: e.target.value })}
        />
        {LEVELS.map(({ level, icon, label }) => (
          <button
            key={level}
            type="button"
            className={styles.toggle}
            aria-pressed={levels[level]}
            aria-label={tx(label)}
            title={tx(label)}
            onClick={() => useUnityConsole.setState({ levels: { ...levels, [level]: !levels[level] } })}
          >
            <Icon name={icon} size={14} />
            <span className={styles.count}>{counts[level] > 999 ? "999+" : counts[level]}</span>
          </button>
        ))}
        <IconButton
          icon="arrow-down"
          title={follow ? "Following the end (click to stop)" : "Scroll to end"}
          size="small"
          pressed={follow}
          onClick={() => useUnityConsole.setState({ follow: !follow })}
        />
      </div>

      {!rows.length ? (
        <EmptyState title={connected ? tx("No messages yet") : tx("Unity Editor not connected")}>
          <span>
            {connected ? (
              <T>Messages the Unity Editor logs appear here.</T>
            ) : (
              <T>Open this project in Unity with Prefabric's package installed; the console follows its log.</T>
            )}
          </span>
        </EmptyState>
      ) : (
        <div
          ref={scrollRef}
          className={styles.log}
          role="log"
          aria-label={tx("Unity Console")}
          tabIndex={0}
          onScroll={(e) => {
            const el = e.currentTarget;
            const up = el.scrollTop < written.current - 4;
            written.current = el.scrollTop;
            if (up && follow) useUnityConsole.setState({ follow: false });
          }}
        >
          <div className={styles.canvas} style={{ height: totalSize }}>
            {virtualizer.getVirtualItems().map((item) => {
              const row = shown.rows[item.index]!;
              const count = shown.counts?.get(row.id) ?? 1;
              const level = levelOf(row.kind);
              return (
                <div
                  key={item.key}
                  ref={virtualizer.measureElement}
                  data-index={item.index}
                  role="option"
                  aria-selected={selected === row.id}
                  className={clsx(styles.row, styles[level], selected === row.id && styles.selected)}
                  style={{ transform: `translateY(${item.start}px)` }}
                  onClick={() => useUnityConsole.setState({ selected: row.id })}
                >
                  <span className={styles.time}>{row.time}</span>
                  <span className={styles.message}>{firstLine(row.message)}</span>
                  {collapse && count > 1 && <span className={styles.badge}>{count}</span>}
                </div>
              );
            })}
          </div>
          {!shown.rows.length && (
            <div className={styles.empty}>
              <T>No messages match the filter</T>
            </div>
          )}
        </div>
      )}
      <Details id={selected} />
    </div>
  );
}
