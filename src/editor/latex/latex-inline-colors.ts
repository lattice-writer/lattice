import { RangeSetBuilder } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";

/**
 * Inline colours in LaTeX source (lattice#1328): the text a `\textcolor{c}{…}`
 * colours, and what a `\color{c}` declaration colours, from the declaration to
 * the end of its group (the `}` or `\end` that closes the group it stands
 * in, or the end of what was read). A colour is one of xcolor's base names or
 * `[HTML]{RRGGBB}`; any other colour (a model, a mix such as `red!50`, a
 * `\definecolor`) is the document's to say and is left unpainted. Where
 * colours nest, the innermost (the latest to start) wins, as in TeX.
 *
 * Drawn on the GPUI app's Visual page (`inlineColorPaint`), mirrored by the
 * source editor's `gpui/crates/lattice-editor/src/latex/inline_colors.rs`.
 */

/** xcolor's base colours (xcolor.sty, `\definecolor` in the rgb model). */
export const XCOLOR_BASE: Readonly<Record<string, string>> = {
  red: "#FF0000", green: "#00FF00", blue: "#0000FF", cyan: "#00FFFF", magenta: "#FF00FF", yellow: "#FFFF00",
  black: "#000000", white: "#FFFFFF", gray: "#808080", darkgray: "#404040", lightgray: "#BFBFBF",
  brown: "#BF8040", lime: "#BFFF00", olive: "#808000", orange: "#FF8000", pink: "#FFBFBF", purple: "#BF0040",
  teal: "#008080", violet: "#800080",
};

export type InlineColor = { from: number; to: number; hex: string };

/** `#RRGGBB` for a colour this paints, or null. */
export function resolveColor(model: string, spec: string): string | null {
  const value = spec.trim();
  if (model.trim().toLowerCase() === "html") return /^[0-9A-Fa-f]{6}$/.test(value) ? `#${value.toUpperCase()}` : null;
  if (model.trim()) return null;
  return XCOLOR_BASE[value] ?? null;
}

function skipComment(text: string, index: number): number {
  const newline = text.indexOf("\n", index);
  return newline < 0 ? text.length : newline;
}

/** Past the blanks before an argument, one line break among them, as TeX reads them. */
function skipSpace(text: string, index: number): number {
  let breaks = 0;
  for (;;) {
    const char = text[index];
    if (char === "\n" && breaks === 0) breaks += 1;
    else if (char !== " " && char !== "\t") return index;
    index += 1;
  }
}

/** Just past the brace that closes the group opened at `open`, or -1. */
function groupEnd(text: string, open: number): number {
  let depth = 0;
  for (let index = open; index < text.length; index += 1) {
    const char = text[index];
    if (char === "\\") index += 1;
    else if (char === "%") index = skipComment(text, index);
    else if (char === "{") depth += 1;
    else if (char === "}" && --depth === 0) return index + 1;
  }
  return -1;
}

/** Where the group a declaration at `from` stands in ends. */
function declarationEnd(text: string, from: number): number {
  let depth = 0;
  let environments = 0;
  for (let index = from; index < text.length; index += 1) {
    const char = text[index];
    if (char === "%") index = skipComment(text, index);
    else if (char === "{") depth += 1;
    else if (char === "}") {
      if (depth === 0) return index;
      depth -= 1;
    } else if (char === "\\") {
      const name = /^[A-Za-z@]+/.exec(text.slice(index + 1, index + 7))?.[0];
      if (name === "begin") environments += 1;
      else if (name === "end") {
        if (environments === 0 && depth === 0) return index;
        environments = Math.max(0, environments - 1);
      }
      index += name ? name.length : 1;
    }
  }
  return text.length;
}

/** `[model]{spec}` from `index`: the colour and just past it, or null. */
function colorArgument(text: string, index: number): { hex: string | null; end: number } | null {
  let at = skipSpace(text, index);
  let model = "";
  if (text[at] === "[") {
    const close = text.indexOf("]", at);
    if (close < 0) return null;
    model = text.slice(at + 1, close);
    at = skipSpace(text, close + 1);
  }
  if (text[at] !== "{") return null;
  const end = groupEnd(text, at);
  if (end < 0) return null;
  return { hex: resolveColor(model, text.slice(at + 1, end - 1)), end };
}

/** Every painted colour in `text`, flattened so no two overlap. */
export function inlineColors(text: string): InlineColor[] {
  const spans: InlineColor[] = [];
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (char === "%") {
      index = skipComment(text, index);
      continue;
    }
    if (char !== "\\") continue;
    const name = /^[A-Za-z@]+/.exec(text.slice(index + 1, index + 11))?.[0] ?? "";
    const command = name === "textcolor" || name === "color" ? name : null;
    if (!command) {
      index += Math.max(name.length, 1);
      continue;
    }
    const color = colorArgument(text, index + 1 + command.length);
    if (!color) continue;
    if (command === "color") {
      if (color.hex) spans.push({ from: color.end, to: declarationEnd(text, color.end), hex: color.hex });
    } else {
      const open = skipSpace(text, color.end);
      const end = text[open] === "{" ? groupEnd(text, open) : -1;
      if (end >= 0 && color.hex) spans.push({ from: open + 1, to: end - 1, hex: color.hex });
    }
    index = color.end - 1;
  }
  const cuts = [...new Set(spans.flatMap((span) => [span.from, span.to]))].sort((a, b) => a - b);
  const flat: InlineColor[] = [];
  for (let cut = 0; cut + 1 < cuts.length; cut += 1) {
    const [from, to] = [cuts[cut], cuts[cut + 1]];
    let inner: InlineColor | null = null;
    for (const span of spans) if (span.from <= from && to <= span.to && (!inner || span.from >= inner.from)) inner = span;
    if (!inner) continue;
    const last = flat.at(-1);
    if (last && last.to === from && last.hex === inner.hex) last.to = to;
    else if (to > from) flat.push({ from, to, hex: inner.hex });
  }
  return flat;
}

const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const gam = (c: number) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);

/** The most a colour's OKLab lightness may be in a light editor, and the least in a dark one. */
const LIGHT_MAX = 0.5;
const DARK_MIN = 0.74;

/**
 * `hex` as the editor draws it so its text stays readable: in a light editor
 * its OKLab lightness is brought down to at most 0.50, in a dark one up to at
 * least 0.74, keeping its hue and chroma (clipped to sRGB). Every base colour
 * then reads at 4.5:1 or more, WCAG's AA for body text, on white paper and on
 * a #1E1E1E ground; a colour already that dark or light is drawn as written.
 */
export function readableColor(hex: string, dark: boolean): string {
  const [r, g, b] = [1, 3, 5].map((at) => lin(parseInt(hex.slice(at, at + 2), 16) / 255));
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const lightness = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  const target = dark ? Math.max(lightness, DARK_MIN) : Math.min(lightness, LIGHT_MAX);
  if (target === lightness) return hex.toUpperCase();
  const l2 = (target + 0.3963377774 * a + 0.2158037573 * bb) ** 3;
  const m2 = (target - 0.1055613458 * a - 0.0638541728 * bb) ** 3;
  const s2 = (target - 0.0894841775 * a - 1.291485548 * bb) ** 3;
  const rgb = [
    4.0767416621 * l2 - 3.3077115913 * m2 + 0.2309699292 * s2,
    -1.2684380046 * l2 + 2.6097574011 * m2 - 0.3413193965 * s2,
    -0.0041960863 * l2 - 0.7034186147 * m2 + 1.707614701 * s2,
  ];
  return `#${rgb.map((c) => Math.round(Math.min(1, Math.max(0, gam(c))) * 255).toString(16).padStart(2, "0")).join("").toUpperCase()}`;
}

const luminance = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((at) => lin(parseInt(hex.slice(at, at + 2), 16) / 255));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const contrast = (a: string, b: string) => {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
};

/** The paper's ink, which a light editor draws a highlight's text in. */
const PAPER_INK = "#242426";

/**
 * The ink for text on a fill of `hex` in a light editor: the paper's black
 * ink, or white where white has the higher WCAG contrast with the fill (a
 * `\colorbox{blue}`), so the text on any fill reads at least as well as either.
 */
export function inkOn(hex: string): string {
  return contrast(hex, "#FFFFFF") > contrast(hex, PAPER_INK) ? "#FFFFFF" : PAPER_INK;
}

/** Only what is on screen is read, with this margin either side. */
const MARGIN = 2000;

/**
 * The stretches read: each visible range and its margin, from the start of a
 * line so a `%` comment is read whole, with stretches that touch merged so a
 * group open across a fold is read as one.
 */
function windows(view: EditorView): Array<[number, number]> {
  const { doc } = view.state;
  const read: Array<[number, number]> = [];
  for (const range of view.visibleRanges) {
    const from = doc.lineAt(Math.max(0, range.from - MARGIN)).from;
    const to = Math.min(doc.length, range.to + MARGIN);
    const last = read.at(-1);
    if (last && from <= last[1]) last[1] = Math.max(last[1], to);
    else read.push([from, to]);
  }
  return read;
}

function build(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const [from, to] of windows(view)) {
    for (const color of inlineColors(view.state.sliceDoc(from, to))) {
      const style = `--tc-light: ${readableColor(color.hex, false)}; --tc-dark: ${readableColor(color.hex, true)}`;
      builder.add(from + color.from, from + color.to, Decoration.mark({ class: "cm-tc", attributes: { style } }));
    }
  }
  return builder.finish();
}

/**
 * Paint the inline colours the page shows (the GPUI island's `.cm-tc`), as
 * outer decorations, so a passage stays one element around the syntax
 * colours and Visual mode's drawn commands inside it.
 */
export function inlineColorPaint() {
  const plugin = ViewPlugin.fromClass(class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = build(view);
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) this.decorations = build(update.view);
    }
  });
  return [plugin, EditorView.outerDecorations.of((view) => view.plugin(plugin)?.decorations ?? Decoration.none)];
}
