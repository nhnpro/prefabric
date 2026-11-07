/** A light C# scanner for Unity features: classes, their bases and methods, without a parser. */

export interface ClassInfo {
  name: string;
  /** Base class and interfaces as written (`MonoBehaviour`, `UnityEngine.MonoBehaviour`). */
  bases: string[];
  /** Offset of the class name. */
  nameAt: number;
  /** Between the braces of the body. */
  bodyStart: number;
  bodyEnd: number;
}

/** `text` with comments and string/char literals blanked out (same length), so braces in them don't count. */
export function codeOnly(text: string): string {
  const out = text.split("");
  let i = 0;
  const blank = (from: number, to: number) => {
    for (let j = from; j < to && j < out.length; j++) if (out[j] !== "\n") out[j] = " ";
  };
  while (i < text.length) {
    const c = text[i]!;
    const next = text[i + 1];
    if (c === "/" && next === "/") {
      const end = text.indexOf("\n", i);
      const stop = end < 0 ? text.length : end;
      blank(i, stop);
      i = stop;
    } else if (c === "/" && next === "*") {
      const end = text.indexOf("*/", i + 2);
      const stop = end < 0 ? text.length : end + 2;
      blank(i, stop);
      i = stop;
    } else if (c === '"' || c === "'") {
      const verbatim = text[i - 1] === "@" || (text[i - 1] === "$" && text[i - 2] === "@");
      let j = i + 1;
      while (j < text.length) {
        if (!verbatim && text[j] === "\\") j += 2;
        else if (verbatim && text[j] === '"' && text[j + 1] === '"') j += 2;
        else if (text[j] === c || (!verbatim && text[j] === "\n")) break;
        else j++;
      }
      blank(i + 1, j);
      i = j + 1;
    } else i++;
  }
  return out.join("");
}

/** Every class or struct of the file with its body range. */
export function classes(text: string): ClassInfo[] {
  const code = codeOnly(text);
  const found: ClassInfo[] = [];
  const declaration = /\b(?:class|struct)\s+([A-Za-z_]\w*)(\s*<[^>{]*>)?\s*(?::\s*([^{;]+))?\{/g;
  for (const m of code.matchAll(declaration)) {
    const open = m.index + m[0].length - 1;
    let depth = 0;
    let end = code.length;
    for (let i = open; i < code.length; i++) {
      if (code[i] === "{") depth++;
      else if (code[i] === "}" && --depth === 0) {
        end = i;
        break;
      }
    }
    const bases = (m[3] ?? "")
      .replace(/\bwhere\b[\s\S]*$/, "")
      .split(",")
      .map((b) => b.trim().replace(/<.*$/, ""))
      .filter(Boolean);
    found.push({
      name: m[1]!,
      bases,
      nameAt: m.index + m[0].indexOf(m[1]!, m[0].search(/\s/)),
      bodyStart: open + 1,
      bodyEnd: end,
    });
  }
  return found;
}

/** The innermost class whose body holds `pos` directly (not inside one of its members). */
export function classAtMemberLevel(text: string, pos: number): ClassInfo | null {
  const code = codeOnly(text);
  const containing = classes(text)
    .filter((c) => c.bodyStart <= pos && pos <= c.bodyEnd)
    .sort((a, b) => b.bodyStart - a.bodyStart)[0];
  if (!containing) return null;
  let depth = 0;
  for (let i = containing.bodyStart; i < pos; i++) {
    if (code[i] === "{") depth++;
    else if (code[i] === "}") depth--;
  }
  return depth === 0 ? containing : null;
}

/** The methods declared in a class body (its own level only), with the offset of each name. */
export function methodDeclarations(text: string, cls: ClassInfo): Array<{ name: string; at: number }> {
  const code = codeOnly(text);
  // The body with member bodies blanked (same offsets): only this level's declarations remain.
  let level = "";
  let depth = 0;
  for (let i = cls.bodyStart; i < cls.bodyEnd; i++) {
    const c = code[i]!;
    if (c === "{") {
      level += depth === 0 ? "{" : " ";
      depth++;
    } else if (c === "}") {
      depth--;
      level += " ";
    } else level += depth === 0 || c === "\n" ? c : " ";
  }
  const found: Array<{ name: string; at: number }> = [];
  for (const m of level.matchAll(/\b[A-Za-z_][\w.<>[\],]*\s+([A-Za-z_]\w*)\s*\([^)]*\)\s*(?:\{|=>)/g)) {
    found.push({ name: m[1]!, at: cls.bodyStart + m.index + m[0].lastIndexOf(m[1]!, m[0].indexOf("(")) });
  }
  return found;
}

/** Names of the methods declared in a class body (its own level only). */
export function declaredMethods(text: string, cls: ClassInfo): Set<string> {
  return new Set(methodDeclarations(text, cls).map((m) => m.name));
}

const simpleName = (name: string) =>
  name
    .replace(/^global::/, "")
    .split(".")
    .pop()!;

/**
 * The Unity type `cls` derives from, following bases declared in the same file; `undefined` when a
 * base is declared elsewhere (another file decides), null when none leads to a Unity type.
 */
export function unityBaseInFile(
  text: string,
  cls: ClassInfo,
  unityTypes: ReadonlySet<string>,
): string | null | undefined {
  const byName = new Map(classes(text).map((c) => [c.name, c]));
  const seen = new Set<string>();
  let unknown = false;
  const visit = (c: ClassInfo): string | null => {
    if (seen.has(c.name)) return null;
    seen.add(c.name);
    for (const base of c.bases.map(simpleName)) {
      if (unityTypes.has(base)) return base;
      const local = byName.get(base);
      if (local) {
        const found = visit(local);
        if (found) return found;
      } else if (!/^I[A-Z][a-z]/.test(base)) unknown = true; // `IDamageable`, not `IAPStoreBase`
    }
    return null;
  };
  const found = visit(cls);
  return found ?? (unknown ? undefined : null);
}
