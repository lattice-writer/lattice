/**
 * A press on an inline formula (R-INL-2) in the real editor: it selects the
 * formula and opens its field, with no native selection started inside the
 * atom, and the click that follows leaves the field open.
 *
 * Clean implementation for Lattice; spec: docs/visual-editor-spec.md.
 */
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NodeSelection, TextSelection } from "@tiptap/pm/state";
import type { Editor } from "@tiptap/react";
import { afterEach, describe, expect, it } from "vitest";
import { LatticeVisualMarkdownEditor } from "./lattice-visual-editor";

const surface = () => screen.getByRole("textbox", { name: "Markdown document editor" }) as HTMLElement & { editor: Editor };
const formulaField = () => screen.queryByRole("textbox", { name: "Formula" });
const settle = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

async function renderFormula(text: string) {
  render(<LatticeVisualMarkdownEditor text={text} activePath="notes.md" onChangeMarkdown={() => true} onUndo={() => true} onRedo={() => true} />);
  await waitFor(() => expect(surface().querySelector(".lx-md-math .katex")).not.toBeNull());
  const { editor } = surface();
  let position = -1;
  editor.state.doc.descendants((node, offset) => {
    if (node.type.name === "latticeMath" && position < 0) position = offset;
  });
  return { editor, position, glyph: surface().querySelector<HTMLElement>(".lx-md-math .katex")! };
}

afterEach(() => {
  cleanup();
  Reflect.deleteProperty(document, "elementFromPoint");
});

describe("pressing an inline formula", () => {
  it("selects the formula and keeps its field open through the click", async () => {
    const { editor, position, glyph } = await renderFormula("拆成两件事： `cnt` 字典管次数（$O(1)$） + `arr` 有序列表管大小顺序。");
    // Prevented, so the browser starts no native selection inside the atom.
    expect(fireEvent.mouseDown(glyph, { button: 0 })).toBe(false);
    expect(editor.state.selection).toBeInstanceOf(NodeSelection);
    expect(editor.state.selection.from).toBe(position);
    await waitFor(() => expect(formulaField()).toHaveFocus());
    fireEvent.mouseUp(glyph, { button: 0 });
    fireEvent.click(glyph, { button: 0 });
    await settle(50);
    expect(formulaField()).toHaveFocus();
    expect(editor.state.selection).toBeInstanceOf(NodeSelection);
    expect(editor.state.selection.from).toBe(position);
    // A second press on the selected formula leaves its open field alone.
    expect(fireEvent.mouseDown(glyph, { button: 0 })).toBe(false);
    await settle(50);
    expect(formulaField()).toHaveFocus();
  });

  it("leaves a Shift-press on the formula, and a press beside it, to the editor", async () => {
    const { editor, glyph } = await renderFormula("Before $x^2$ after.");
    act(() => editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, 2))));
    // ProseMirror's own press handling hit-tests the point; jsdom has no layout to hit.
    Object.defineProperty(document, "elementFromPoint", { configurable: true, value: () => null });
    expect(fireEvent.mouseDown(glyph, { button: 0, shiftKey: true })).toBe(true);
    fireEvent.mouseUp(glyph, { button: 0, shiftKey: true });
    const paragraph = surface().querySelector("p")!;
    expect(fireEvent.mouseDown(paragraph, { button: 0 })).toBe(true);
    fireEvent.mouseUp(paragraph, { button: 0 });
    expect(editor.state.selection).not.toBeInstanceOf(NodeSelection);
    expect(formulaField()).not.toBeInTheDocument();
  });
});
