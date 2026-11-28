import { describe, expect, it } from "vitest";
import type { MessageData } from "./messages";
import type { UnityDeps } from "./bases";
import { analyzeUnityHotPaths, hotPathRuleMessage } from "./performance";

const DATA: MessageData = {
  unity: "6000.3",
  types: {
    MonoBehaviour: {
      base: null,
      docs: "https://docs.unity.com/en-us/engine/6000.3/script-reference/unityengine/monobehaviour",
      messages: [
        {
          name: "Update",
          returns: "void",
          parameters: [],
          coroutine: false,
          static: false,
          description: "Update is called every frame.",
          docs: "https://docs.unity.com/en-us/engine/6000.3/script-reference/unityengine/monobehaviour/update",
        },
      ],
    },
  },
};

const deps: UnityDeps = {
  data: async () => DATA,
  supertypes: async () => [],
  text: async () => "",
};

describe("Unity hot-path performance & allocation analysis", () => {
  it("tracks transitive helper methods called from Update and FixedUpdate", async () => {
    const code = [
      "public class Player : MonoBehaviour {",
      "  void Start() {",
      "    var cam = Camera.main;",
      "    ColdHelper();",
      "  }",
      "  void Update() {",
      "    Move();",
      "  }",
      "  void Move() {",
      "    ApplyGravity();",
      "  }",
      "  void ApplyGravity() {",
      "    var rb = GetComponent<Rigidbody>();",
      "  }",
      "  void ColdHelper() {",
      "    var x = GetComponent<Collider>();",
      "  }",
      "}",
    ].join("\n");

    const res = await analyzeUnityHotPaths("/proj/Assets/Player.cs", code, deps);
    expect(res.transitiveMethods.map((m) => ({ name: m.name, chain: m.chain }))).toEqual([
      { name: "Move", chain: ["Update", "Move"] },
      { name: "ApplyGravity", chain: ["Update", "Move", "ApplyGravity"] },
    ]);
    // Only the GetComponent inside ApplyGravity (hot path) is flagged; Start and ColdHelper are ignored.
    expect(res.issues.map((i) => ({ rule: i.rule, chain: i.chain }))).toEqual([
      { rule: "get-component", chain: ["Update", "Move", "ApplyGravity"] },
    ]);
  });

  it("detects lookups, string APIs, physics allocators, LINQ, and heap allocations while ignoring comments and strings", async () => {
    const code = [
      "public class Enemy : MonoBehaviour {",
      "  void FixedUpdate() {",
      "    // Camera.main and GetComponent<Rigidbody>() in a comment should be ignored",
      '    var note = "Physics.RaycastAll(ray) and Camera.main";',
      "    var cam = Camera.main;",
      '    if (other.tag == "Player") {}',
      '    mat.SetFloat("_Glow", 1f);',
      '    animator.SetTrigger("Attack");',
      "    var hits = Physics.RaycastAll(ray);",
      "    var list = new List<int>();",
      "    var filtered = items.Where(x => x > 0).ToList();",
      '    var msg = $"hp={hp}";',
      "  }",
      "}",
    ].join("\n");

    const res = await analyzeUnityHotPaths("/proj/Assets/Enemy.cs", code, deps);
    const rules = res.issues.map((i) => i.rule);
    expect(rules).toContain("camera-main");
    expect(rules).toContain("tag-comparison");
    expect(rules).toContain("shader-string");
    expect(rules).toContain("animator-string");
    expect(rules).toContain("physics-alloc");
    expect(rules).toContain("collection-alloc");
    expect(rules).toContain("linq-alloc");
    expect(rules).toContain("string-alloc");
    expect(rules.filter((r) => r === "camera-main")).toHaveLength(1);
    for (const issue of res.issues) {
      expect(hotPathRuleMessage(issue.rule).length).toBeGreaterThan(10);
    }
  });

  it("does not flag non-Unity classes", async () => {
    const code = [
      "public class PlainService {",
      "  void Update() {",
      "    var cam = Camera.main;",
      "    var list = new List<int>();",
      "  }",
      "}",
    ].join("\n");

    const res = await analyzeUnityHotPaths("/proj/Assets/PlainService.cs", code, deps);
    expect(res.transitiveMethods).toEqual([]);
    expect(res.issues).toEqual([]);
  });

  it("handles expression-bodied methods in the hot-path graph", async () => {
    const code = [
      "public class Hud : MonoBehaviour {",
      "  void LateUpdate() => Refresh();",
      '  string Refresh() => $"Score: {score}";',
      "}",
    ].join("\n");

    const res = await analyzeUnityHotPaths("/proj/Assets/Hud.cs", code, deps);
    expect(res.transitiveMethods.map((m) => m.name)).toEqual(["Refresh"]);
    expect(res.issues.map((i) => i.rule)).toEqual(["string-alloc"]);
  });
});
