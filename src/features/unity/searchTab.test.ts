import { describe, expect, it } from "vitest";
import { unityHitItem } from "./searchTab";

describe("Unity Search Everywhere tab", () => {
  it("shows a GameObject with the hierarchy above it and opens its line", () => {
    const item = unityHitItem(
      {
        kind: "gameObject",
        name: "Follower",
        indices: [0],
        parents: ["Squad"],
        path: "/p/Assets/Prefabs/Squad.prefab",
        line: 35,
      },
      "/p",
    );
    expect(item).toMatchObject({
      title: "Follower",
      detail: "Squad",
      location: "Squad.prefab",
      icon: "hierarchy",
      path: "/p/Assets/Prefabs/Squad.prefab",
      line: 35,
    });
  });

  it("shows an asset with its folder and opens the file", () => {
    const item = unityHitItem(
      {
        kind: "asset",
        name: "Player.prefab",
        indices: [],
        parents: [],
        path: "/p/Assets/Prefabs/Player.prefab",
        line: 0,
      },
      "/p",
    );
    expect(item.detail).toBe("Assets/Prefabs");
    expect(item.line).toBeUndefined();
    expect(item.location).toBeUndefined();
  });
});
