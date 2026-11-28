import { describe, expect, it } from "vitest";
import type { MessageData } from "./messages";
import type { UnityDeps } from "./bases";
import { resolveUnityDocsUrl, symbolAtOffset, unityDocsVersion } from "./docs";

const DATA: MessageData = {
  unity: "6000.3",
  types: {
    MonoBehaviour: {
      base: null,
      docs: "https://docs.unity.com/en-us/engine/6000.3/script-reference/unityengine/monobehaviour",
      messages: [
        {
          name: "FixedUpdate",
          returns: "void",
          parameters: [],
          coroutine: false,
          static: false,
          description: "Frame-rate independent MonoBehaviour.FixedUpdate message.",
          docs: "https://docs.unity.com/en-us/engine/6000.3/script-reference/unityengine/monobehaviour/fixedupdate",
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

describe("External Unity Documentation", () => {
  it("extracts identifiers and PascalCase-qualified symbols around the caret", () => {
    const line = "  var hit = Physics.Raycast(origin, dir);";
    expect(symbolAtOffset(line, line.indexOf("Raycast") + 2)).toEqual({
      ident: "Raycast",
      qualified: "Physics.Raycast",
    });
    expect(symbolAtOffset(line, line.indexOf("Physics") + 2)).toEqual({
      ident: "Physics",
      qualified: "Physics",
    });
    const lower = '  other.CompareTag("Player");';
    expect(symbolAtOffset(lower, lower.indexOf("CompareTag") + 3)).toEqual({
      ident: "CompareTag",
      qualified: "CompareTag",
    });
  });

  it("parses major.minor from Unity editor versions", () => {
    expect(unityDocsVersion("6000.0.23f1")).toBe("6000.0");
    expect(unityDocsVersion("2022.3.62f1")).toBe("2022.3");
    expect(unityDocsVersion(null)).toBeNull();
  });

  it("resolves exact docs URLs for event functions, Unity base types, and versioned search fallback", async () => {
    const code = [
      "public class Player : MonoBehaviour {",
      "  void FixedUpdate() {",
      "    Physics.Raycast(a, b);",
      "  }",
      "}",
    ].join("\n");

    const onEvent = await resolveUnityDocsUrl(
      "/proj/Assets/Player.cs",
      code,
      code.indexOf("FixedUpdate") + 3,
      "6000.3.1f1",
      deps,
    );
    expect(onEvent).toBe(
      "https://docs.unity.com/en-us/engine/6000.3/script-reference/unityengine/monobehaviour/fixedupdate",
    );

    const onType = await resolveUnityDocsUrl(
      "/proj/Assets/Player.cs",
      code,
      code.indexOf("MonoBehaviour") + 4,
      "6000.3.1f1",
      deps,
    );
    expect(onType).toBe("https://docs.unity.com/en-us/engine/6000.3/script-reference/unityengine/monobehaviour");

    const onCall = await resolveUnityDocsUrl(
      "/proj/Assets/Player.cs",
      code,
      code.indexOf("Raycast") + 2,
      "2022.3.62f1",
      deps,
    );
    expect(onCall).toBe(
      "https://docs.unity3d.com/2022.3/Documentation/ScriptReference/30_search.html?q=Physics.Raycast",
    );
  });
});
