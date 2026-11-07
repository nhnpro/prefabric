/** Unity's message data (`pnpm gen:unity-messages`) for the project's Unity version. */

export interface UnityMessage {
  name: string;
  returns: string;
  parameters: Array<{ name: string; type: string }>;
  coroutine: boolean;
  static: boolean;
  description: string;
  docs: string;
}

export interface MessageData {
  unity: string;
  types: Record<string, { base: string | null; docs: string; messages: UnityMessage[] }>;
}

/** Loaded on first use: the data is 50 KB per version. */
const LOADERS = {
  "2022.3": () => import("./messages/2022.3.json"),
  "6000.3": () => import("./messages/6000.3.json"),
} as const;
const loaded = new Map<string, Promise<MessageData>>();

/** Unity base classes that are MonoBehaviours themselves (packages): their subclasses get its messages. */
const MONOBEHAVIOUR_KINDS = ["UIBehaviour", "NetworkBehaviour"];

/** Which data an editor version (`2022.3.62f1`, `6000.3.1f1`) uses: the closest older one. */
export function dataVersion(editorVersion: string | null | undefined): keyof typeof LOADERS {
  const major = Number.parseInt(editorVersion ?? "", 10);
  return major && major < 6000 ? "2022.3" : "6000.3";
}

export function loadMessageData(editorVersion: string | null | undefined): Promise<MessageData> {
  const version = dataVersion(editorVersion);
  let data = loaded.get(version);
  if (!data) {
    data = LOADERS[version]().then((m) => m.default as MessageData);
    loaded.set(version, data);
  }
  return data;
}

/** Type names that make a class a Unity class. */
export function unityTypeNames(data: MessageData): Set<string> {
  return new Set([...Object.keys(data.types), ...MONOBEHAVIOUR_KINDS]);
}

/** The messages a subclass of `type` can implement, its own and its Unity bases' (Editor gets ScriptableObject's). */
export function messagesFor(data: MessageData, type: string): Array<UnityMessage & { owner: string }> {
  const out: Array<UnityMessage & { owner: string }> = [];
  const seen = new Set<string>();
  let current: string | null = MONOBEHAVIOUR_KINDS.includes(type) ? "MonoBehaviour" : type;
  while (current && data.types[current]) {
    const entry: MessageData["types"][string] = data.types[current]!;
    for (const m of entry.messages) {
      if (!seen.has(m.name)) {
        seen.add(m.name);
        out.push({ ...m, owner: current });
      }
    }
    current = entry.base;
  }
  return out;
}

/** The method declaration for a message: `private void OnTriggerEnter(Collider other)`. */
export function declaration(m: UnityMessage, typed: { modifiers: string[]; returns?: string }): string {
  const modifiers = typed.modifiers.length ? typed.modifiers : ["private"];
  const all = m.static && !modifiers.includes("static") ? [...modifiers, "static"] : modifiers;
  const returns = typed.returns === "IEnumerator" && m.coroutine ? "IEnumerator" : m.returns;
  const parameters = m.parameters.map((p) => `${p.type} ${p.name}`).join(", ");
  return `${all.join(" ")} ${returns} ${m.name}(${parameters})`;
}
