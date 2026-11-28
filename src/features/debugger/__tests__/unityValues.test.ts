import { describe, expect, it } from "vitest";
import { shownValue } from "../values";
import { quaternionToEuler, shortUnityType, unityShownValue } from "../unityValues";

describe("Unity debugger variable visualizers", () => {
  it("shortens UnityEngine, UnityEditor, and Unity.* namespaces", () => {
    expect(shortUnityType("UnityEngine.Vector3")).toBe("Vector3");
    expect(shortUnityType("UnityEngine.UI.Image")).toBe("Image");
    expect(shortUnityType("UnityEditor.SerializedProperty")).toBe("SerializedProperty");
    expect(shortUnityType("Unity.Collections.NativeArray<System.Single>")).toBe("NativeArray<System.Single>");
    expect(shortUnityType("Unity.Mathematics.float3")).toBe("float3");
    expect(shortUnityType("System.String")).toBeNull();
  });

  it("formats Vector2/3/4 and Unity.Mathematics vectors cleanly", () => {
    expect(shownValue("(1.00, 2.50, -3.00)", "UnityEngine.Vector3")).toEqual({
      label: "{Vector3}",
      text: "(1, 2.5, -3)",
    });
    expect(shownValue("(0.000, -0.000)", "UnityEngine.Vector2")).toEqual({
      label: "{Vector2}",
      text: "(0, 0)",
    });
    expect(shownValue("(1, 2, 3)", "UnityEngine.Vector3Int")).toEqual({
      label: "{Vector3Int}",
      text: "(1, 2, 3)",
    });
  });

  it("converts Unity Quaternions to Euler degrees and displays both", () => {
    expect(quaternionToEuler(0, Math.SQRT1_2, 0, Math.SQRT1_2)).toEqual([0, 90, 0]);
    expect(quaternionToEuler(Math.SQRT1_2, 0, 0, Math.SQRT1_2)).toEqual([90, 0, 0]);
    expect(quaternionToEuler(0, 0, Math.SQRT1_2, Math.SQRT1_2)).toEqual([0, 0, 90]);
    expect(shownValue("(0.00000, 0.70711, 0.00000, 0.70711)", "UnityEngine.Quaternion")).toEqual({
      label: "{Quaternion}",
      text: "(0, 0.707, 0, 0.707)  euler(0°, 90°, 0°)",
    });
  });

  it("formats Color and Color32 with hex readout and RGBA swatch", () => {
    expect(shownValue("RGBA(1.000, 0.502, 0.251, 1.000)", "UnityEngine.Color")).toEqual({
      label: "{Color}",
      text: "#FF8040 (1, 0.5, 0.25, 1)",
      swatch: "rgba(255, 128, 64, 1)",
    });
    expect(shownValue("RGBA(0, 128, 255, 128)", "UnityEngine.Color32")).toEqual({
      label: "{Color32}",
      text: "#0080FF80 (0, 128, 255, 128)",
      swatch: "rgba(0, 128, 255, 0.502)",
    });
  });

  it("formats Rect, Bounds, and LayerMask", () => {
    expect(shownValue("(x:10.00, y:20.00, width:100.00, height:50.00)", "UnityEngine.Rect")).toEqual({
      label: "{Rect}",
      text: "(10, 20, 100 × 50)",
    });
    expect(shownValue("Center: (0.00, 1.00, 0.00), Extents: (0.50, 1.50, 2.00)", "UnityEngine.Bounds")).toEqual({
      label: "{Bounds}",
      text: "center (0, 1, 0), size (1, 3, 4)",
    });
    expect(shownValue("0", "UnityEngine.LayerMask")).toEqual({
      label: "{LayerMask}",
      text: "0 (Nothing)",
    });
    expect(shownValue("-1", "UnityEngine.LayerMask")).toEqual({
      label: "{LayerMask}",
      text: "-1 (Everything)",
    });
    expect(shownValue("{m_Bits: 5}", "UnityEngine.LayerMask")).toEqual({
      label: "{LayerMask}",
      text: "5 (bits 0, 2)",
    });
  });

  it("cleans up GameObject, Transform, custom MonoBehaviours, and destroyed objects", () => {
    expect(shownValue("Player (UnityEngine.GameObject)", "UnityEngine.GameObject")).toEqual({
      label: "{GameObject}",
      text: '"Player"',
    });
    expect(shownValue("Main Camera (UnityEngine.Transform)", "UnityEngine.Transform")).toEqual({
      label: "{Transform}",
      text: '"Main Camera"',
    });
    expect(shownValue("Hero (UnityEngine.MonoBehaviour)", "MyGame.HeroController")).toEqual({
      label: "{HeroController}",
      text: '"Hero"',
    });
    expect(shownValue('"null"', "UnityEngine.GameObject")).toEqual({
      label: "{GameObject}",
      text: "null (destroyed)",
    });
    expect(shownValue("null", "UnityEngine.Transform")).toEqual({
      label: "{Transform}",
      text: "null",
    });
  });

  it("leaves non-Unity values untouched", () => {
    expect(unityShownValue("42", "int")).toBeNull();
    expect(unityShownValue("HashMap@10 size=1", "HashMap")).toBeNull();
  });
});
