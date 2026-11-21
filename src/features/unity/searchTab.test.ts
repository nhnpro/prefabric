import { afterEach, describe, expect, it } from "vitest";
import { useProject } from "@/platform/project";
import { searchCategories } from "@/workbench/contributions";
import { trackUnitySearchCategory, unityHitItem } from "./searchTab";

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
      description: "Squad",
      target: {
        kind: "location",
        value: { path: "/p/Assets/Prefabs/Squad.prefab", range: { start: { line: 35, col: 0 } } },
      },
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
    expect(item.description).toBe("Assets/Prefabs");
    expect(item.target).toMatchObject({ value: { range: { start: { line: 0, col: 0 } } } });
  });

  describe("category", () => {
    const initial = useProject.getState();
    afterEach(() => useProject.setState(initial, true));

    it("is offered only while the project is a Unity one", () => {
      const ids = () => searchCategories().map((c) => c.id);
      const setUnity = (unity: boolean) =>
        useProject.setState({
          project: { ...(initial.project ?? {}), root: "/p", unity: unity ? {} : null } as never,
        });
      const stop = trackUnitySearchCategory();
      expect(ids()).not.toContain("plugin:unity:unity");
      setUnity(true);
      expect(ids()).toContain("plugin:unity:unity");
      setUnity(false);
      expect(ids()).not.toContain("plugin:unity:unity");
      setUnity(true);
      stop();
      expect(ids()).not.toContain("plugin:unity:unity");
    });
  });
});
