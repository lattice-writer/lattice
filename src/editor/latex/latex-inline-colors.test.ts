import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, describe, expect, it } from "vitest";
import { latex } from "./latex-language";
import { inlineColorPaint, inlineColors, readableColor, resolveColor, XCOLOR_BASE } from "./latex-inline-colors";

const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const luminance = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((at) => lin(parseInt(hex.slice(at, at + 2), 16) / 255));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const contrast = (a: string, b: string) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

const spans = (text: string) => inlineColors(text).map((span) => [text.slice(span.from, span.to), span.hex]);

describe("inlineColors", () => {
  it("reads \\textcolor's text and a \\color declaration to the end of its group", () => {
    expect(spans(String.raw`a \textcolor{red}{revised} b {\color{blue} new text} c`)).toEqual([
      ["revised", "#FF0000"],
      [" new text", "#0000FF"],
    ]);
    expect(spans(String.raw`\textcolor[HTML]{1a7f37}{ok}`)).toEqual([["ok", "#1A7F37"]]);
    expect(spans("\\textcolor{red}\n  {next line}")).toEqual([["next line", "#FF0000"]]);
  });

  it("ends a declaration at the environment that holds it, past nested groups and environments", () => {
    const text = String.raw`\begin{quote}\color{red} one {two} \begin{itemize}\item x\end{itemize} y\end{quote} after`;
    expect(spans(text)).toEqual([[String.raw` one {two} \begin{itemize}\item x\end{itemize} y`, "#FF0000"]]);
  });

  it("runs a top-level declaration to the end of what was read, and lets the innermost colour win", () => {
    const text = String.raw`\color{red} a \textcolor{blue}{b} c`;
    expect(spans(text)).toEqual([[String.raw` a \textcolor{blue}{`, "#FF0000"], ["b", "#0000FF"], ["} c", "#FF0000"]]);
  });

  it("leaves unknown colours, mixes, other models, comments and other commands unpainted", () => {
    expect(inlineColors(String.raw`\textcolor{red!50}{a} \color[rgb]{1,0,0} b \textcolor{myaccent}{c}`)).toEqual([]);
    expect(inlineColors(String.raw`\colorbox{red}{a} \colorlet{x}{red} % \color{red} hidden`)).toEqual([]);
    expect(spans(String.raw`{\color{red} a % }
 b} c`)).toEqual([[" a % }\n b", "#FF0000"]]);
    expect(resolveColor("", "Red")).toBeNull();
    // An unmatched \end inside a group doesn't end the group's colour early.
    expect(spans(String.raw`{\color{red} a {\end{x}} \begin{y}b\end{y} c} d`)).toEqual([
      [String.raw` a {\end{x}} \begin{y}b\end{y} c`, "#FF0000"],
    ]);
  });
});

describe("readableColor", () => {
  it("keeps every base colour at 4.5:1 or more on white paper and on a dark ground", () => {
    for (const hex of Object.values(XCOLOR_BASE)) {
      expect(contrast(readableColor(hex, false), "#FFFFFF")).toBeGreaterThanOrEqual(4.5);
      expect(contrast(readableColor(hex, true), "#1E1E1E")).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("draws a colour that already reads as written", () => {
    expect(readableColor("#0000FF", false)).toBe("#0000FF");
    expect(readableColor("#FFFF00", true)).toBe("#FFFF00");
    expect(readableColor("#FF0000", false)).not.toBe("#FF0000");
  });
});

describe("inlineColorPaint", () => {
  const views: EditorView[] = [];
  afterEach(() => views.splice(0).forEach((view) => view.destroy()));

  it("marks the coloured text with its light and dark colours", () => {
    const doc = String.raw`Before {\color{blue} revised \textbf{bold}} and \textcolor{red}{fixed}.`;
    const view = new EditorView({ parent: document.body, state: EditorState.create({ doc, extensions: [latex(), inlineColorPaint()] }) });
    views.push(view);
    const marks = [...view.contentDOM.querySelectorAll<HTMLElement>(".cm-tc")];
    // One element each, around the syntax colours inside it.
    expect(marks.map((mark) => mark.textContent)).toEqual([String.raw` revised \textbf{bold}`, "fixed"]);
    expect(marks[0].style.getPropertyValue("--tc-light")).toBe(readableColor("#0000FF", false));
    expect(marks[1].style.getPropertyValue("--tc-dark")).toBe(readableColor("#FF0000", true));
  });
});
