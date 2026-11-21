import type { RunTarget } from "./targets";

/**
 * Test methods and their classes, `static Main` and a file of top-level statements.
 * Comments and literals are masked without changing UTF-16 offsets or line breaks.
 */
export function csharpTargets(text: string): RunTarget[] {
  const code = text.replace(
    /\/\*[\s\S]*?\*\/|\/\/[^\n]*|@"(?:""|[^"])*"|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/g,
    (value) => value.replace(/[^\r\n]/g, " "),
  );
  const tokens =
    /\bnamespace\s+([\w.]+)\s*([;{])|\b(?:class|record|struct)\s+(\w+)[^;{}]*\{|\[([^\]]*)\]|\b(\w+)\s*\([^;{}]*\)\s*(?=\{|=>)|[{};]/g;
  const scopes: Array<{ name: string; depth: number; line: number; kind: "namespace" | "class" }> = [];
  const out: RunTarget[] = [];
  const suites = new Set<string>();
  let depth = 0;
  let line = 0;
  let offset = 0;
  let fileNamespace = "";
  let test = false;
  for (const match of code.matchAll(tokens)) {
    const value = match[0];
    while (offset < match.index) {
      if (code.charCodeAt(offset) === 10) line++;
      offset++;
    }
    if (match[1]) {
      if (match[2] === ";") fileNamespace = match[1];
      else scopes.push({ name: match[1], depth: ++depth, line, kind: "namespace" });
    } else if (match[3]) {
      scopes.push({ name: match[3], depth: ++depth, line, kind: "class" });
      test = false;
    } else if (match[4] !== undefined) {
      test ||=
        /(?:^|[.,\s])(?:Fact|Theory|Test|TestCase|TestCaseSource|TestMethod|DataTestMethod|UnityTest)(?:Attribute)?\b/.test(
          match[4],
        );
    } else if (match[5]) {
      if (test && scopes.some((s) => s.kind === "class")) {
        const className = [
          fileNamespace,
          ...scopes.filter((s) => s.kind === "namespace").map((s) => s.name),
          scopes
            .filter((s) => s.kind === "class")
            .map((s) => s.name)
            .join("+"),
        ]
          .filter(Boolean)
          .join(".");
        const owner = scopes.filter((s) => s.kind === "class").at(-1)!;
        if (!suites.has(className)) {
          suites.add(className);
          out.push({ line: owner.line, kind: "suite", name: owner.name, path: [className] });
        }
        out.push({ line, kind: "test", name: match[5], path: [className, match[5]] });
      } else if (match[5] === "Main" && scopes.some((s) => s.kind === "class") && isEntryPoint(code, match.index)) {
        out.push({ line, kind: "main", name: "Main", path: [] });
      }
      test = false;
    } else if (value === "{") depth++;
    else if (value === "}") {
      depth--;
      while (scopes.length && scopes.at(-1)!.depth > depth) scopes.pop();
      test = false;
    } else test = false;
  }
  const topLevel = topLevelStatement(code);
  if (topLevel !== null) out.push({ line: topLevel, kind: "main", name: "Main", path: [] });
  return out.sort((a, b) => a.line - b.line);
}

/** `static [async] void|int|Task|Task<int> Main(` ending at `at`. */
function isEntryPoint(code: string, at: number): boolean {
  const head = code.slice(code.lastIndexOf("\n", at) + 1, at).replace(/\basync\s+/, "");
  return /\bstatic\b/.test(head) && /\b(?:void|int|Task(?:\s*<\s*int\s*>)?)\s+$/.test(head);
}

/** The line of a file's first top-level statement, if it starts with one (C# 9 programs). */
function topLevelStatement(code: string): number | null {
  const lines = code.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!.trim();
    if (!line || /^(?:using\b|global\s+using\b|#|extern\s+alias\b)/.test(line)) continue;
    const declaration =
      /^(?:\[|(?:(?:public|internal|private|protected|static|sealed|abstract|partial|file|unsafe|readonly|ref|new)\s+)*(?:namespace|class|record|struct|interface|enum|delegate)\b)/;
    return declaration.test(line) ? null : i;
  }
  return null;
}
