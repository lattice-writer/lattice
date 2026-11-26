/**
 * How an author's text colour (a Markdown `<span style="color: …">`, read by
 * inline-color.ts) is drawn: refitted to read at 4.5:1 on the editor ground of
 * the theme in use, as a code theme's tokens are (code-themes'
 * `fitEditorText`), and refitted again when the theme changes. The mark and
 * the file keep the colour as written; only the paint moves.
 */
import type { MarkViewRenderer } from "@tiptap/core";
import type { Theme } from "../../../settings/app-settings";
import { fitEditorText } from "../../../settings/code-themes";
import { foregroundColor } from "./inline-color";

let context: CanvasRenderingContext2D | null | undefined;
const painted = new Map<string, string>();
const repaints = new Set<() => void>();
let observer: MutationObserver | null = null;

const themeKind = (): Theme => (document.documentElement.dataset.theme === "dark" ? "dark" : "light");

/**
 * `color` as sRGB bytes with alpha. A canvas resolves every colour syntax
 * WebKit draws (named, hex, rgb, hsl, oklch, color(), color-mix) to the same
 * pixels it would paint; without one (a test DOM) there is nothing to fit.
 */
function srgb(color: string): Uint8ClampedArray | null {
  if (context === undefined) {
    try {
      // eslint-disable-next-line lingui/no-unlocalized-strings -- DOM API names
      context = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
    } catch {
      context = null;
    }
  }
  if (!context) return null;
  context.clearRect(0, 0, 1, 1);
  context.fillStyle = color;
  context.fillRect(0, 0, 1, 1);
  return context.getImageData(0, 0, 1, 1).data;
}

/** The colour `color` is painted in on a `kind` editor ground. */
export function shownColor(color: string, kind: Theme = themeKind()): string {
  // currentColor takes the enclosing colour, which is already fitted; a
  // fully transparent run is hidden on purpose.
  if (/currentcolor/i.test(color)) return color;
  const key = `${kind}\n${color}`;
  const known = painted.get(key);
  if (known) return known;
  const bytes = srgb(color);
  const alpha = bytes ? bytes[3]! / 255 : 0;
  const hex = bytes && `#${Array.from(bytes.subarray(0, 3), (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
  const shown = (hex && alpha > 0 && fitEditorText(hex, alpha, kind)) || color;
  painted.set(key, shown);
  return shown;
}

/** Call `repaint` whenever the root's `data-theme` changes, until the returned stop. */
function onThemeChange(repaint: () => void): () => void {
  repaints.add(repaint);
  if (!observer) {
    observer = new MutationObserver(() => repaints.forEach((each) => each()));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  }
  return () => {
    repaints.delete(repaint);
    if (repaints.size || !observer) return;
    observer.disconnect();
    observer = null;
  };
}

/**
 * The colour mark's view: the span `renderHTML` writes, repainted in place on
 * a theme change. ProseMirror is told to ignore that repaint; its style is
 * paint, not content.
 */
export const colorMarkView = (): MarkViewRenderer => ({ mark, HTMLAttributes }) => {
  const dom = document.createElement("span");
  for (const [name, value] of Object.entries(HTMLAttributes)) if (value != null) dom.setAttribute(name, String(value));
  const color = foregroundColor(mark.attrs.color);
  if (!color) return { dom, contentDOM: dom };
  dom.setAttribute("data-lattice-color", color);
  const paint = () => { dom.style.color = shownColor(color); };
  paint();
  return {
    dom,
    contentDOM: dom,
    ignoreMutation: (mutation) => mutation.type === "attributes" && mutation.target === dom,
    destroy: onThemeChange(paint),
  };
};
