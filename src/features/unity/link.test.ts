import { describe, expect, it } from "vitest";
import type { UnityLinkStatus } from "@/platform/ipc";
import { linkLabel } from "./link";

const connected = (editor: UnityLinkStatus["editor"]): UnityLinkStatus => ({
  connection: "connected",
  editor,
  unityVersion: "6000.6.8f1",
});
const state = (over: Partial<NonNullable<UnityLinkStatus["editor"]>> = {}) => ({
  playMode: "edit" as const,
  compiling: false,
  updating: false,
  ...over,
});

describe("Unity link status item", () => {
  it("is hidden without an Editor", () => {
    expect(linkLabel(null)).toBeNull();
    expect(linkLabel({ connection: "disconnected", editor: null, unityVersion: null })).toBeNull();
  });

  it("shows the play mode, with compiling and importing first", () => {
    expect(linkLabel(connected(state()))?.label).toBe("Unity: connected");
    expect(linkLabel(connected(state({ playMode: "playing" })))?.label).toBe("Unity: playing");
    expect(linkLabel(connected(state({ playMode: "paused" })))?.label).toBe("Unity: paused");
    expect(linkLabel(connected(state({ playMode: "playing", compiling: true })))?.label).toBe("Unity: compiling");
    expect(linkLabel(connected(state({ updating: true })))?.label).toBe("Unity: importing");
    expect(linkLabel(connected(state()))?.tooltip).toBe("Connected to Unity 6000.6.8f1");
  });

  it("says when connecting or when the package is too old", () => {
    expect(linkLabel({ connection: "connecting", editor: null, unityVersion: null })?.label).toBe("Unity: connecting");
    expect(linkLabel({ connection: "incompatible", editor: null, unityVersion: null })?.label).toBe(
      "Unity: update the package",
    );
  });
});

describe("refresh on save", () => {
  it("refreshes for files Unity imports", async () => {
    const { refreshesAssets } = await import("./playControls");
    expect(refreshesAssets("/p/Game", "/p/Game/Assets/Scripts/Player.cs")).toBe(true);
    expect(refreshesAssets("/p/Game/", "/p/Game/Packages/com.x/Runtime/A.cs")).toBe(true);
    expect(refreshesAssets("C:\\p\\Game", "C:\\p\\Game\\Assets\\A.cs")).toBe(true);
    expect(refreshesAssets("/p/Game", "/p/Game/README.md")).toBe(false);
    expect(refreshesAssets("/p/Game", "/p/Game/Library/x.cs")).toBe(false);
    expect(refreshesAssets("/p/Game", "/p/Game/Assets/.hidden")).toBe(false);
    expect(refreshesAssets("/p/Game", "/p/GameOther/Assets/A.cs")).toBe(false);
  });
});
