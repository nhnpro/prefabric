/**
 * USS (Unity style sheets) open as CSS; the CSS server knows the standard properties. This adds
 * Unity's own `-unity-*` properties and their keyword values to completion in `.uss` files.
 */
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { EditorState, type Extension } from "@codemirror/state";
import { tx } from "@/platform/i18n";

export interface UssProperty {
  name: string;
  description: string;
  /** Keyword values; empty for colors, lengths, assets. */
  values: string[];
}

export const USS_PROPERTIES: UssProperty[] = [
  { name: "-unity-background-image-tint-color", description: "Tint color of the background image.", values: [] },
  {
    name: "-unity-background-scale-mode",
    description: "How the background image fills the element.",
    values: ["stretch-to-fill", "scale-and-crop", "scale-to-fit"],
  },
  {
    name: "-unity-editor-text-rendering-mode",
    description: "Text rendering in the Editor.",
    values: ["sdf", "bitmap"],
  },
  { name: "-unity-font", description: "Font asset, as resource(…) or url(…).", values: [] },
  { name: "-unity-font-definition", description: "Font asset or font definition (Text Core).", values: [] },
  {
    name: "-unity-font-style",
    description: "Font style and weight.",
    values: ["normal", "italic", "bold", "bold-and-italic"],
  },
  {
    name: "-unity-overflow-clip-box",
    description: "Box that clips the content when overflow is hidden.",
    values: ["padding-box", "content-box"],
  },
  { name: "-unity-paragraph-spacing", description: "Space between paragraphs.", values: [] },
  { name: "-unity-slice-bottom", description: "Unscaled border of a 9-sliced image, bottom.", values: [] },
  { name: "-unity-slice-left", description: "Unscaled border of a 9-sliced image, left.", values: [] },
  { name: "-unity-slice-right", description: "Unscaled border of a 9-sliced image, right.", values: [] },
  { name: "-unity-slice-top", description: "Unscaled border of a 9-sliced image, top.", values: [] },
  { name: "-unity-slice-scale", description: "Scale of the sliced borders.", values: [] },
  {
    name: "-unity-slice-type",
    description: "How the center and edges of a sliced image fill.",
    values: ["sliced", "tiled"],
  },
  {
    name: "-unity-text-align",
    description: "Horizontal and vertical alignment of text.",
    values: [
      "upper-left",
      "middle-left",
      "lower-left",
      "upper-center",
      "middle-center",
      "lower-center",
      "upper-right",
      "middle-right",
      "lower-right",
    ],
  },
  { name: "-unity-text-auto-size", description: "Fit the font size to the element.", values: ["none", "best-fit"] },
  { name: "-unity-text-generator", description: "Text generator.", values: ["standard", "advanced"] },
  { name: "-unity-text-outline", description: "Shorthand for the text outline width and color.", values: [] },
  { name: "-unity-text-outline-color", description: "Color of the text outline.", values: [] },
  { name: "-unity-text-outline-width", description: "Width of the text outline.", values: [] },
  {
    name: "-unity-text-overflow-position",
    description: "Where the ellipsis goes when text overflows.",
    values: ["start", "middle", "end"],
  },
];

const BY_NAME = new Map(USS_PROPERTIES.map((p) => [p.name, p]));

/** What is being typed at `pos`: a property name, or the value of `property`. */
export function ussContext(
  text: string,
  pos: number,
): { kind: "property"; from: number } | { kind: "value"; property: string; from: number } | null {
  let i = pos;
  while (i > 0 && /[\w-]/.test(text[i - 1]!)) i--;
  const from = i;
  // Back to the start of the declaration.
  let j = from;
  while (j > 0 && !";{}".includes(text[j - 1]!)) j--;
  const head = text.slice(j, from);
  if (j === 0 || text[j - 1] === "}") return null;
  if (/^\s*$/.test(head)) return { kind: "property", from };
  const value = /^\s*(-?[\w-]+)\s*:\s*(?:[\w-]+\s+)*$/.exec(head);
  return value ? { kind: "value", property: value[1]!, from } : null;
}

const info = (p: UssProperty) => () => {
  const dom = document.createElement("div");
  dom.textContent = tx(p.description);
  return dom;
};

export async function ussSource(ctx: CompletionContext): Promise<CompletionResult | null> {
  const text = ctx.state.doc.toString();
  const at = ussContext(text, ctx.pos);
  if (!at || (at.from === ctx.pos && !ctx.explicit && at.kind === "property")) return null;
  let options: Array<Completion & { icon?: string; typeText?: string }>;
  if (at.kind === "property") {
    options = USS_PROPERTIES.map((p) => ({
      label: p.name,
      type: "property",
      icon: "symbol-property",
      typeText: "Unity",
      info: info(p),
      apply: `${p.name}: `,
    }));
  } else {
    const property = BY_NAME.get(at.property);
    if (!property?.values.length) return null;
    options = property.values.map((v) => ({
      label: v,
      type: "keyword",
      icon: "symbol-constant",
      typeText: property.name,
    }));
  }
  return { from: at.from, options, validFor: /^[\w-]*$/ };
}

export function ussSupport(path: string): Extension {
  if (!/\.uss$/i.test(path)) return [];
  return EditorState.languageData.of(() => [{ openideaCompletion: ussSource }]);
}
