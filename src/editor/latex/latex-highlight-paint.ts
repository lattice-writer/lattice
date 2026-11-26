import { RangeSetBuilder } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { latexHighlights } from "./latex-highlights";
import { inkOn, resolveColor } from "./latex-inline-colors";

/**
 * A highlight looks like one in the source, not only in the PDF: its argument
 * is painted with its paper tint (in the light editor under the paper's black
 * ink, as it prints; in the dark one as a wash and an underline in its color,
 * since a pastel block would put the editor's light ink on a light ground),
 * and the `\colorbox[HTML]{…}{` wrapper recedes behind a swatch of its color.
 *
 * Only what is on screen is scanned, with a margin either side so a highlight
 * that starts or ends just off screen is still found whole; a document of
 * megabytes costs what its viewport costs.
 *
 * With `boxes` (the GPUI app's Visual page) a `\colorbox` is drawn as TeX
 * sets it, one box that never breaks across lines (lattice#1327): the whole
 * command is one outer mark the line cannot wrap inside, so a box too long for
 * the line runs past it as an overfull box does, and its argument is filled
 * square with `\fboxsep` either side, in an xcolor base colour as well as an
 * HTML one, its text in the ink that reads on the fill (`inkOn`). Soul's
 * `\hl` is left as it is.
 */
const MARGIN = 2000;

class Swatch extends WidgetType {
  constructor(readonly hex: string) {
    super();
  }
  eq(other: Swatch) {
    return other.hex === this.hex;
  }
  toDOM() {
    const swatch = document.createElement("span");
    swatch.className = "cm-hl-swatch";
    swatch.style.background = this.hex;
    swatch.setAttribute("aria-hidden", "true");
    return swatch;
  }
  ignoreEvent() {
    return false;
  }
}

type Built = { decorations: DecorationSet; boxes: DecorationSet };

function sorted(marks: Array<[number, number, Decoration]>): DecorationSet {
  marks.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const builder = new RangeSetBuilder<Decoration>();
  for (const [from, to, decoration] of marks) builder.add(from, to, decoration);
  return builder.finish();
}

const BOX = Decoration.mark({ class: "cm-hl-box" });

function build(view: EditorView, boxes: boolean): Built {
  const marks: Array<[number, number, Decoration]> = [];
  const units: Array<[number, number, Decoration]> = [];
  const seen = new Set<number>();
  for (const range of view.visibleRanges) {
    const from = Math.max(0, range.from - MARGIN);
    const to = Math.min(view.state.doc.length, range.to + MARGIN);
    for (const highlight of latexHighlights(view.state.sliceDoc(from, to))) {
      const start = from + highlight.from;
      if (seen.has(start)) continue;
      seen.add(start);
      const isBox = boxes && highlight.color !== null;
      if (isBox) units.push([start, from + highlight.to, BOX]);
      const written = highlight.color && /^#[0-9A-F]{6}$/i.test(highlight.color) ? highlight.color : null;
      // A box on the page fills an xcolor base name too (`\colorbox{yellow}`),
      // its text in the ink that reads on that fill.
      const hex = written ?? (isBox && highlight.color ? resolveColor("", highlight.color) : null);
      if (!hex) continue;
      const style = isBox ? `--hl: ${hex}; --hl-ink: ${inkOn(hex)}` : `--hl: ${hex}`;
      marks.push([start, start, Decoration.widget({ widget: new Swatch(hex), side: -1 })]);
      marks.push([start, from + highlight.contentFrom, Decoration.mark({ class: "cm-hl-wrap" })]);
      if (highlight.contentTo > highlight.contentFrom) {
        marks.push([from + highlight.contentFrom, from + highlight.contentTo, Decoration.mark({ class: isBox ? "cm-hl-paint cm-hl-box-fill" : "cm-hl-paint", attributes: { style } })]);
      }
      marks.push([from + highlight.contentTo, from + highlight.to, Decoration.mark({ class: "cm-hl-wrap" })]);
    }
  }
  return { decorations: sorted(marks), boxes: sorted(units) };
}

export function highlightPaint({ boxes = false }: { boxes?: boolean } = {}) {
  const plugin = ViewPlugin.fromClass(class {
    built: Built;
    constructor(view: EditorView) {
      this.built = build(view, boxes);
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) this.built = build(update.view, boxes);
    }
  }, { decorations: (plugin) => plugin.built.decorations });
  if (!boxes) return plugin;
  // Outer decorations wrap every other mark (syntax colours, spelling, the
  // paint itself), so nothing splits a box into pieces a line could break
  // between.
  return [plugin, EditorView.outerDecorations.of((view) => view.plugin(plugin)?.built.boxes ?? Decoration.none)];
}
