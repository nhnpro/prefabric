import { describe, expect, it } from "vitest";
import { getFileDecoration, setFileDecorations } from "./contributions";

describe("file decorations", () => {
  it("applies subtree decorations to everything below a folder", () => {
    setFileDecorations(
      "vcs",
      new Map([
        ["/p/nested", { color: "red", subtree: true, propagate: true }],
        ["/p/build", { faded: true, subtree: true }],
        ["/p/nested/own.txt", { color: "blue" }],
      ]),
    );
    expect(getFileDecoration("/p/nested/a/b.rs")?.color).toBe("red");
    expect(getFileDecoration("/p/nested/own.txt")?.color).toBe("blue");
    expect(getFileDecoration("/p/build/out/x.o")?.faded).toBe(true);
    expect(getFileDecoration("/p/src/main.rs")).toBeUndefined();
    expect(getFileDecoration("/p")?.color).toBe("red");
    setFileDecorations("vcs", new Map());
    expect(getFileDecoration("/p/nested/a/b.rs")).toBeUndefined();
  });

  it("preserves VCS folder color when a descendant file has a problem error", () => {
    setFileDecorations("vcs", new Map([["/p/src/a.ts", { color: "var(--vcs-modified)", propagate: true }]]));
    setFileDecorations("problems", new Map([["/p/src/b.ts", { error: true, propagate: true, tooltip: "1 error" }]]));
    expect(getFileDecoration("/p/src")).toEqual({ color: "var(--vcs-modified)", error: true });
    setFileDecorations("vcs", new Map());
    setFileDecorations("problems", new Map());
  });
});
