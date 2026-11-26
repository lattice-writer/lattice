import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, describe, expect, it } from "vitest";
import { latex } from "./latex-language";
import { highlightPaint } from "./latex-highlight-paint";

const views: EditorView[] = [];
afterEach(() => views.splice(0).forEach((view) => view.destroy()));

function mount(doc: string, boxes: boolean): EditorView {
  const state = EditorState.create({ doc, extensions: [latex(), EditorView.lineWrapping, highlightPaint({ boxes })] });
  const view = new EditorView({ parent: document.body, state });
  views.push(view);
  return view;
}

const BOXED = String.raw`\colorbox[HTML]{FFF1A6}{a long boxed passage \textbf{with bold}}`;
const NAMED = String.raw`\colorbox{yellow}{named box}`;
const SOUL = String.raw`\hl{a soul highlight that wraps}`;
const DOC = `Before ${BOXED} middle ${NAMED} and ${SOUL} after.`;

describe("highlightPaint", () => {
  it("draws a \\colorbox as one unbreakable box on the Visual page, and leaves \\hl inline", () => {
    const view = mount(DOC, true);
    const boxes = [...view.contentDOM.querySelectorAll(".cm-hl-box")];
    // Each \colorbox is one element wrapping its whole command, never split
    // by the syntax colours or the paint inside it.
    expect(boxes.map((box) => box.textContent)).toEqual([BOXED, NAMED]);
    const fill = boxes[0].querySelectorAll(".cm-hl-paint.cm-hl-box-fill");
    expect(fill).toHaveLength(1);
    expect(fill[0].textContent).toBe(String.raw`a long boxed passage \textbf{with bold}`);
    // A colour by name is a box too, without a paint of its own.
    expect(boxes[1].querySelector(".cm-hl-paint")).toBeNull();
    // Soul's \hl breaks across lines in TeX, so it is no box.
    expect(view.contentDOM.textContent).toContain(SOUL);
    expect(boxes.some((box) => box.textContent?.includes(String.raw`\hl`))).toBe(false);
  });

  it("keeps the source's wrapping highlighter without boxes", () => {
    const view = mount(DOC, false);
    expect(view.contentDOM.querySelector(".cm-hl-box")).toBeNull();
    const paint = view.contentDOM.querySelectorAll(".cm-hl-paint");
    expect(paint).toHaveLength(1);
    expect(paint[0].classList.contains("cm-hl-box-fill")).toBe(false);
  });
});
