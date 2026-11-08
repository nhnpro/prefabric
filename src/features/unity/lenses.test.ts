import { describe, expect, it } from "vitest";
import type { LensSummary } from "@/bindings";
import { lensTitle } from "./lenses";

describe("Unity Code Vision", () => {
  it("summarises where a script class is used", () => {
    expect(lensTitle({ kind: "class", prefabs: 3, scenes: 1, other: 0 })).toEqual(["Used in 3 prefabs · 1 scene"]);
    expect(lensTitle({ kind: "class", prefabs: 0, scenes: 0, other: 2 })).toEqual(["Used in 2 assets"]);
  });

  it("names the asset calling a method, or counts them", () => {
    const method: LensSummary & { kind: "method" } = {
      kind: "method",
      unityEvents: 1,
      animationEvents: 0,
      files: ["Main.unity"],
      fileCount: 1,
    };
    expect(lensTitle(method)).toEqual(["Event handler in Main.unity"]);
    expect(lensTitle({ ...method, unityEvents: 0, animationEvents: 1, files: ["Walk.anim"] })).toEqual([
      "Animation event in Walk.anim",
    ]);
    expect(lensTitle({ ...method, fileCount: 4, files: ["A.unity", "B.prefab", "C.prefab"] })).toEqual([
      "Event handler in 4 assets",
    ]);
  });

  it("shows a field's Inspector value and its overrides", () => {
    expect(lensTitle({ kind: "field", values: ["2"], overrides: 2 })).toEqual(["Inspector: 2", "2 overrides"]);
    expect(lensTitle({ kind: "field", values: ["1", "3"], overrides: 0 })).toEqual(["Inspector: several values"]);
    expect(lensTitle({ kind: "field", values: [], overrides: 1 })).toEqual(["1 override"]);
  });
});
