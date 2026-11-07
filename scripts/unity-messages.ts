// Builds the Unity message data (event functions Unity calls by name: `Update`,
// `OnTriggerEnter`, …) from Unity's scripting reference, one JSON file per Unity version:
//   node scripts/unity-messages.ts            # every version in VERSIONS
//   node scripts/unity-messages.ts 6000.3     # one version
// Output: src/features/unity/messages/<version>.json (used by completion and gutter icons).

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const VERSIONS = ["2022.3", "6000.3"];
/** Types whose subclasses get messages, with the Unity type they derive from. */
export const TYPES: Record<string, string | null> = {
  MonoBehaviour: null,
  ScriptableObject: null,
  Editor: "ScriptableObject",
  EditorWindow: "ScriptableObject",
  AssetPostprocessor: null,
};

export interface Parameter {
  name: string;
  type: string;
}

export interface Message {
  name: string;
  /** `void` for most; `bool`/`string`/… for the few that answer Unity. */
  returns: string;
  parameters: Parameter[];
  /** Unity also accepts it as a coroutine (`IEnumerator Start()`). */
  coroutine: boolean;
  static: boolean;
  description: string;
  docs: string;
}

export interface MessageData {
  unity: string;
  types: Record<string, { base: string | null; docs: string; messages: Message[] }>;
}

const decode = (html: string) =>
  html
    .replace(/<[^>]+>/g, "")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/\s+/g, " ")
    .trim();

type Row = { name: string; href: string; description: string };

/** The rows of the table after the heading `title` (`Messages`, `Public Methods`, …), in the
 * classic reference's markup and in docs.unity.com's (where 6000.x redirects). */
export function tableRows(typePage: string, title: string): Row[] {
  const start = typePage.search(new RegExp(`>${title}</h[1-6]>`));
  if (start < 0) return [];
  const end = typePage.indexOf("</table>", start);
  const table = typePage.slice(start, end < 0 ? undefined : end);
  const rows: Row[] = [];
  for (const m of table.matchAll(/<a\b[^>]*\bhref="([^"]+)"[^>]*>([^<]+)<\/a><\/td><td\b[^>]*>([\s\S]*?)<\/td>/g)) {
    rows.push({ href: m[1]!, name: decode(m[2]!), description: decode(m[3]!) });
  }
  return rows;
}

/** What a type page lists as messages. docs.unity.com (6000.x) lists them among the methods: there a
 * method is a message when the classic reference called it one, or when it is an `On…` name that
 * was not a method there (messages added since). */
export function messageRows(typePage: string, classic?: { messages: Set<string>; methods: Set<string> }): Row[] {
  // Some classic tables list properties too (`EditorWindow.hasUnsavedChanges`).
  const listed = tableRows(typePage, "Messages").filter((row) => /^[A-Z]/.test(row.name));
  if (listed.length || !classic) return listed;
  return [...tableRows(typePage, "Methods"), ...tableRows(typePage, "Static Methods")].filter(
    (row) => classic.messages.has(row.name) || (/^On[A-Z]/.test(row.name) && !classic.methods.has(row.name)),
  );
}

/** The message's signature in the page's C# examples, the most reliable source. */
export function exampleSignature(page: string, name: string): Omit<Message, "description" | "docs"> | null {
  const blocks = page.matchAll(
    /<(pre|code)\b[^>]*>([\s\S]*?)<\/\1>|<span class="[^"]*MuiTypography-code[^"]*">([\s\S]*?)<\/span>/g,
  );
  const code = [...blocks].map((m) => decode((m[2] ?? m[3]!).replace(/<br\s*\/?>/g, "\n")));
  const declaration = new RegExp(
    `(?:^|[\\s;{}])((?:(?:public|private|protected|internal|static|override|virtual)\\s+)*)(void|bool|string|int|float|double|long|object|[A-Z][\\w.<>\\[\\]]*)\\s+${name}\\s*\\(([^)]*)\\)`,
    "g",
  );
  let found: Omit<Message, "description" | "docs"> | null = null;
  let coroutine = false;
  for (const text of code) {
    for (const m of text.matchAll(declaration)) {
      const returns = m[2]!;
      if (returns === "new" || returns === "return") continue;
      if (returns === "IEnumerator") {
        coroutine = true;
        if (found) continue;
      }
      const parameters = m[3]!
        .split(",")
        .map((p) => p.trim().replace(/\s*=.*$/, ""))
        .filter(Boolean)
        .map((p) => {
          const parts = p.split(/\s+/);
          return { name: parts.pop()!, type: parts.filter((t) => t !== "ref" && t !== "out" && t !== "in").join(" ") };
        });
      if (!found || (found.returns === "IEnumerator" && returns !== "IEnumerator")) {
        found = { name, returns, parameters, coroutine: false, static: /\bstatic\b/.test(m[1]!) };
      }
    }
  }
  if (!found) return null;
  if (found.returns === "IEnumerator") found.returns = "void";
  found.coroutine = coroutine;
  return found;
}

/** Parameters from the page's table when no example declares the message: the type is the first linked type of the description. */
export function tableParameters(page: string): Parameter[] | null {
  const start = page.search(/>Parameters<\/h[23]>/);
  if (start < 0) return [];
  const table = page.slice(start, page.indexOf("</table>", start));
  const parameters: Parameter[] = [];
  for (const m of table.matchAll(/<td class="name lbl">([^<]+)<\/td>\s*<td class="desc">([\s\S]*?)<\/td>/g)) {
    const desc = m[2]!;
    const linked = /<a href="([A-Za-z0-9_.]+)\.html">/.exec(desc)?.[1];
    const type = linked ?? (/^\s*(True|False|true|false)\b/.test(decode(desc)) ? "bool" : null);
    if (!type) return null;
    parameters.push({ name: decode(m[1]!), type: type.replace(/^.*\./, "") });
  }
  return parameters;
}

/** The page and its address after redirects (6000.x pages move to docs.unity.com). */
async function fetchPage(url: string): Promise<{ text: string; url: string }> {
  for (let attempt = 0; ; attempt++) {
    const response = await fetch(url, { headers: { "user-agent": "prefabric-unity-messages" } });
    if (response.ok) return { text: await response.text(), url: response.url || url };
    if (attempt >= 2 || response.status === 404) throw new Error(`${url}: HTTP ${response.status}`);
    await new Promise((r) => setTimeout(r, 1000 * (attempt + 1)));
  }
}

/** Fills the classic data's guessed signatures from the newer reference's declarations (Unity does not change
 * message signatures between these versions). */
export function backfill(classic: MessageData, newer: MessageData, guessed: Set<string>): MessageData {
  for (const [type, entry] of Object.entries(classic.types)) {
    const later = new Map(newer.types[type]?.messages.map((m) => [m.name, m]) ?? []);
    for (const message of entry.messages) {
      const next = later.get(message.name);
      if (next && guessed.has(`${type}.${message.name}`)) {
        message.parameters = next.parameters.map((p) => ({ ...p }));
        message.returns = next.returns;
      }
    }
  }
  return classic;
}

/** docs.unity.com's signatures name parameters `arg0` and drop `static`: take both from the classic
 * reference's declaration of the same message when the parameter types agree. */
export function mergeClassic(data: MessageData, classic: MessageData): MessageData {
  for (const [type, entry] of Object.entries(data.types)) {
    const before = new Map(classic.types[type]?.messages.map((m) => [m.name, m]) ?? []);
    for (const message of entry.messages) {
      const old = before.get(message.name);
      if (!old) continue;
      message.static ||= old.static;
      message.coroutine ||= old.coroutine;
      const sameTypes =
        old.parameters.length === message.parameters.length &&
        old.parameters.every((p, i) => p.type === message.parameters[i]!.type);
      if (sameTypes) message.parameters = old.parameters.map((p) => ({ ...p }));
    }
  }
  return data;
}

type Classic = Record<string, { messages: Set<string>; methods: Set<string> }>;

/** The classic reference's lists, to tell messages from methods on docs.unity.com. */
async function classicLists(): Promise<Classic> {
  const base = `https://docs.unity3d.com/${VERSIONS[0]}/Documentation/ScriptReference/`;
  const lists: Classic = {};
  for (const type of Object.keys(TYPES)) {
    const page = (await fetchPage(`${base}${type}.html`)).text;
    const names = (title: string) => tableRows(page, title).map((r) => r.name);
    lists[type] = {
      messages: new Set(names("Messages")),
      methods: new Set([...names("Public Methods"), ...names("Static Methods"), ...names("Protected Methods")]),
    };
  }
  return lists;
}

/** Messages whose page shows no declaration and no parameters: their signature is a guess. */
export const unsure = new Set<string>();

export async function build(version: string, classic?: Classic): Promise<MessageData> {
  const base = `https://docs.unity3d.com/${version}/Documentation/ScriptReference/`;
  const data: MessageData = { unity: version, types: {} };
  for (const [type, parent] of Object.entries(TYPES)) {
    const typePage = await fetchPage(`${base}${type}.html`);
    const rows = messageRows(typePage.text, classic?.[type]);
    const messages: Message[] = [];
    for (const row of rows) {
      const fetched = await fetchPage(new URL(row.href, typePage.url).href);
      const page = fetched.text;
      const url = fetched.url;
      const fromExample = exampleSignature(page, row.name);
      const parameters = fromExample?.parameters ?? tableParameters(page);
      if (!parameters?.length && !fromExample) unsure.add(`${type}.${row.name}`);
      messages.push({
        name: row.name,
        returns: fromExample?.returns ?? "void",
        parameters: parameters ?? [],
        coroutine: fromExample?.coroutine ?? false,
        static: fromExample?.static ?? false,
        description: row.description,
        docs: url,
      });
    }
    data.types[type] = { base: parent, docs: typePage.url, messages };
  }
  return data;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const out = join(dirname(fileURLToPath(import.meta.url)), "../src/features/unity/messages");
  mkdirSync(out, { recursive: true });
  const classic = await classicLists();
  const classicData = await build(VERSIONS[0]!, classic);
  const guessed = new Set(unsure);
  const built = new Map<string, MessageData>([[VERSIONS[0]!, classicData]]);
  for (const version of VERSIONS.slice(1)) built.set(version, mergeClassic(await build(version, classic), classicData));
  backfill(classicData, built.get(VERSIONS.at(-1)!)!, guessed);
  for (const version of process.argv.length > 2 ? process.argv.slice(2) : VERSIONS) {
    const data = built.get(version);
    if (!data) throw new Error(`unknown version ${version}; known: ${VERSIONS.join(", ")}`);
    writeFileSync(join(out, `${version}.json`), JSON.stringify(data, null, 2) + "\n");
    const counts = Object.entries(data.types).map(([t, d]) => `${t} ${d.messages.length}`);
    console.log(`${version}: ${counts.join(", ")}`);
  }
}
