/**
 * Scenes and prefabs as Unity's Hierarchy window shows them: a read-only GameObject tree with
 * components, prefab instances with their overrides and the prefab's objects (muted). A toggle
 * shows the raw YAML; activating a row opens the YAML at its line.
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import clsx from "clsx";
import type { Hierarchy } from "@/bindings";
import { FloatingToolbar, IconButton, Tree, showContextMenu } from "@/ui";
import { formatMessage, tx } from "@/platform/i18n";
import { call, commands, onIdeEvent } from "@/platform/ipc";
import { maybeEditorService } from "@/workbench/contributions";
import { hierarchyRows, initiallyExpanded, rowLine, type HierarchyRow } from "./model";
import { hierarchyModeOf, setHierarchyMode, useHierarchyModes } from "./modes";
import styles from "./Hierarchy.module.css";

export function HierarchyEditor({
  path,
  tabId,
  children,
  actionsInTabs = false,
}: {
  path: string;
  tabId: string;
  children: ReactNode;
  actionsInTabs?: boolean;
}) {
  const mode = useHierarchyModes((s) => hierarchyModeOf(s, path));
  const ref = useRef<HTMLDivElement>(null);
  return (
    <div ref={ref} className={clsx(styles.split, mode === "hierarchy" && styles.showTree)}>
      <div className={styles.editor}>{children}</div>
      {mode === "hierarchy" && <HierarchyView path={path} />}
      {!actionsInTabs && (
        <FloatingToolbar targetRef={ref}>
          <HierarchyModeActions path={path} tabId={tabId} />
        </FloatingToolbar>
      )}
    </div>
  );
}

export function HierarchyModeActions({ path }: { path: string; tabId: string }) {
  const mode = useHierarchyModes((s) => hierarchyModeOf(s, path));
  return (
    <div className={styles.modeActions} role="toolbar" aria-label={tx("Scene view")}>
      <IconButton
        icon="toolwindow-hierarchy"
        iconSize={16}
        size="small"
        title="Hierarchy"
        action="unity.showHierarchy"
        actionArg={path}
        pressed={mode === "hierarchy"}
      />
      <IconButton
        icon="editor-only"
        iconSize={16}
        size="small"
        title="YAML Source"
        action="unity.showYaml"
        actionArg={path}
        pressed={mode === "yaml"}
      />
    </div>
  );
}

type Loaded = { path: string; hierarchy: Hierarchy | null; error: string | null };

function useHierarchy(path: string): Loaded | null {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [revision, setRevision] = useState<number | null>(null);
  useEffect(
    () =>
      onIdeEvent("unityIndexState", (e) => {
        if (!e.indexing) setRevision(e.revision);
      }),
    [],
  );
  useEffect(() => {
    let live = true;
    call(commands.unityHierarchy(path)).then(
      (hierarchy) => live && setLoaded({ path, hierarchy, error: null }),
      (e: unknown) => live && setLoaded({ path, hierarchy: null, error: String((e as Error)?.message ?? e) }),
    );
    return () => {
      live = false;
    };
  }, [path, revision]);
  return loaded?.path === path ? loaded : null;
}

const labelOf = (r: HierarchyRow): string => {
  switch (r.kind) {
    case "node":
      return r.node.name || tx("(unnamed)");
    case "component":
      return r.name;
    case "overrides":
      return formatMessage(tx("Overrides ({count})"), { count: r.node.overrides.length });
    case "override":
      return r.property;
  }
};

const iconOf = (r: HierarchyRow): string => {
  switch (r.kind) {
    case "node":
      return r.node.kind === "prefabInstance" ? "symbol-package" : "symbol-module";
    case "component":
      return r.script ? "file-csharp" : "symbol-class";
    case "overrides":
      return "symbol-property";
    case "override":
      return "symbol-field";
  }
};

const detailOf = (r: HierarchyRow): string | undefined => {
  if (r.kind === "override") return r.target ? `${r.value} · ${r.target}` : r.value;
  if (r.kind === "node" && r.node.prefab) return r.node.prefab.split(/[\\/]/).pop();
  return undefined;
};

function HierarchyView({ path }: { path: string }) {
  const loaded = useHierarchy(path);
  const rows = useMemo(() => (loaded?.hierarchy ? hierarchyRows(loaded.hierarchy) : []), [loaded]);
  const expanded = useMemo(() => initiallyExpanded(rows), [rows]);
  const open = (r: HierarchyRow) => {
    setHierarchyMode(path, "yaml");
    void maybeEditorService()?.openFile(path, { selection: { line: rowLine(r), col: 0 }, focus: true });
  };
  let empty: ReactNode = null;
  if (!loaded) empty = tx("Loading…");
  else if (loaded.error) empty = loaded.error;
  else if (loaded.hierarchy?.unreadable) empty = tx("Not a text-serialized scene or prefab");
  else if (!rows.length) empty = tx("No GameObjects");
  return (
    <div className={styles.tree}>
      {empty ? (
        <div className={styles.empty}>{empty}</div>
      ) : (
        <Tree<HierarchyRow>
          key={path}
          roots={rows}
          getKey={(r) => r.key}
          getLabel={labelOf}
          getChildren={(r) => (r.kind === "node" || r.kind === "overrides" ? r.children : undefined)}
          isExpandable={(r) => (r.kind === "node" || r.kind === "overrides") && r.children.length > 0}
          getIcon={iconOf}
          getDetail={detailOf}
          rowClassName={(r) => (r.kind === "node" && r.node.kind === "fromPrefab" ? styles.fromPrefab : undefined)}
          defaultExpanded={expanded}
          onActivate={open}
          onContextMenu={(r, _selection, e) => {
            if (!r || r.kind === "overrides" || r.kind === "override") return;
            e.preventDefault();
            showContextMenu(e, [{ kind: "action", action: "unity.showInUnity", arg: { path, line: rowLine(r) } }]);
          }}
          speedSearch
          aria-label={tx("Hierarchy")}
        />
      )}
    </div>
  );
}
