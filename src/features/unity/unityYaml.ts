/**
 * Unity's serialized assets (scenes, prefabs, materials… in Unity's YAML dialect) open read-only:
 * Unity owns them, and a hand edit easily breaks the references between their objects. A banner
 * explains why and offers "Edit Anyway", which unlocks the file until the app restarts.
 */
import { EditorState, StateEffect, StateField, type Extension, type Text } from "@codemirror/state";
import { EditorView, ViewPlugin } from "@codemirror/view";
import { create } from "zustand";
import type { EditorBannerContribution } from "@/workbench/contributions";

/** `%YAML 1.1` then `%TAG !u! tag:unity3d.com,2011:`. */
export function isUnityYaml(doc: Text): boolean {
  if (doc.lines < 2) return false;
  return /^%YAML\s/.test(doc.line(1).text) && /^%TAG\s+!u!\s+tag:unity3d\.com,/.test(doc.line(2).text);
}

interface UnityYamlState {
  /** Unity YAML files open in an editor, by path: whether they are still locked. */
  locked: Record<string, boolean>;
  /** Unlocked with "Edit Anyway" in this session. */
  unlocked: Set<string>;
}

export const useUnityYaml = create<UnityYamlState>(() => ({ locked: {}, unlocked: new Set() }));

export function unlockUnityYaml(path: string) {
  useUnityYaml.setState((s) => ({ unlocked: new Set(s.unlocked).add(path), locked: { ...s.locked, [path]: false } }));
}

const setUnlocked = StateEffect.define<boolean>();

const unlockedField = StateField.define<boolean>({
  create: () => false,
  update: (value, tr) => tr.effects.reduce((v, e) => (e.is(setUnlocked) ? e.value : v), value),
});

const locked = (s: EditorState) => !s.field(unlockedField) && isUnityYaml(s.doc);

export function unityYamlLock(path: string): Extension {
  return [
    unlockedField.init(() => useUnityYaml.getState().unlocked.has(path)),
    EditorState.readOnly.compute(["doc", unlockedField], locked),
    EditorView.editable.compute(["doc", unlockedField], (s) => !locked(s)),
    ViewPlugin.define((view) => {
      const publish = () => {
        const isLocked = locked(view.state);
        const known = useUnityYaml.getState().locked[path];
        if (known !== isLocked && (isLocked || known !== undefined)) {
          useUnityYaml.setState((s) => ({ locked: { ...s.locked, [path]: isLocked } }));
        }
      };
      publish();
      const off = useUnityYaml.subscribe((s, prev) => {
        if (s.unlocked.has(path) && !prev.unlocked.has(path) && !view.state.field(unlockedField)) {
          queueMicrotask(() => view.dispatch({ effects: setUnlocked.of(true) }));
        }
      });
      return {
        update: (u) => {
          if (u.docChanged || u.transactions.some((t) => t.effects.some((e) => e.is(setUnlocked)))) publish();
        },
        destroy: off,
      };
    }),
  ];
}

export const unityYamlBanner: EditorBannerContribution = {
  id: "unity.yaml",
  order: -5,
  useBanner: (path) => {
    const isLocked = useUnityYaml((s) => s.locked[path] ?? false);
    return isLocked
      ? {
          text: "Unity saves this file: it opens read-only, as edits here can break references between its objects.",
          icon: "lock",
          actions: [{ label: "Edit Anyway", run: () => unlockUnityYaml(path) }],
        }
      : null;
  },
};
