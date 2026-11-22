/**
 * Tables' column widths kept by the host (table-column-widths.ts), driven
 * through the editor as a reader resizes a column and edits around it.
 *
 * Clean implementation for Lattice; spec: docs/visual-editor-spec.md.
 */
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { TableView } from "@tiptap/extension-table";
import type { Node as PmNode } from "@tiptap/pm/model";
import type { Editor } from "@tiptap/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VisualMarkdownEditorProps } from "../visual-editor-props";
import { LatticeVisualMarkdownEditor } from "./lattice-visual-editor";
import { MIN_COLUMN_WIDTH, type SavedTable, type TableColumnWidths } from "./table-column-widths";

type Props = VisualMarkdownEditorProps;

const A = ["| Model | Score |", "| --- | --- |", "| Base | 1 |"].join("\n");
const B = ["| Step | Owner | Due |", "| --- | --- | --- |", "| x | y | z |"].join("\n");
const doc = (...blocks: string[]) => blocks.join("\n\n");
const TABLES = doc(A, "Between.", B);
const sig = (...header: string[]) => [String(header.length), ...header].join("\u001f");
const SIG_A = sig("Model", "Score");
const SIG_B = sig("Step", "Owner", "Due");

const surface = () => screen.getByRole("textbox", { name: "Markdown document editor" }) as HTMLElement & { editor: Editor };
const tables = () => [...document.querySelectorAll<HTMLTableElement>(".lx-md-surface table")];
const tableNamed = (header: string) => tables().find((table) => table.querySelector("th")?.textContent === header)!;
const colWidths = (table: HTMLTableElement) => [...table.querySelectorAll("col")].map((col) => col.style.width);

function renderEditor(given: Partial<Props>) {
  const onChangeMarkdown = vi.fn<Props["onChangeMarkdown"]>(() => true);
  let current: Props = { text: TABLES, activePath: "notes.md", onChangeMarkdown, onUndo: () => true, onRedo: () => true, ...given };
  const view = render(<LatticeVisualMarkdownEditor {...current} />);
  return {
    onChangeMarkdown,
    rerender: (next: Partial<Props>) => view.rerender(<LatticeVisualMarkdownEditor {...(current = { ...current, ...next })} />),
  };
}

/** A host keeping a file's saved tables as the GPUI island does. */
function keeper(tables: SavedTable[] = []) {
  const onChange = vi.fn<TableColumnWidths["onChange"]>();
  return { tables, onChange };
}

/**
 * jsdom lays nothing out: every cell is drawn 100px wide from x = 0, and a
 * table laid out as wide as its content (the fit's measure) 180px.
 */
beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (this: HTMLElement) {
    return this.closest("table")?.style.width === "max-content" ? 180 : 100;
  });
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(() => new DOMRect(0, 0, 100, 20));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const header = (table: HTMLTableElement) => table.querySelector("th")!;
const pressBorder = (table: HTMLTableElement, detail = 1) => fireEvent.mouseDown(header(table), { clientX: 98, button: 0, detail });

describe("table column widths", () => {
  it("leaves tables exactly as Tiptap's view draws them when the host keeps no widths", async () => {
    renderEditor({});
    await waitFor(() => expect(tables()).toHaveLength(2));
    expect(document.querySelector("[data-lx-table]")).toBeNull();
    const nodes: PmNode[] = [];
    surface().editor.state.doc.descendants((node) => {
      if (node.type.name === "table") nodes.push(node);
      return node.type.name !== "table";
    });
    tables().forEach((table, index) => {
      const classic = new TableView(nodes[index]!, 25).table;
      expect(table.getAttributeNames().sort()).toEqual(classic.getAttributeNames().sort());
      expect(table.getAttribute("style")).toBe(classic.getAttribute("style"));
      expect(table.querySelector("colgroup")!.outerHTML).toBe(classic.querySelector("colgroup")!.outerHTML);
    });
    fireEvent.mouseMove(header(tables()[0]!), { clientX: 98 });
    expect(document.querySelector(".lx-md-editor")).not.toHaveAttribute("data-column-resize");
  });

  it("sizes the table its signature names, and keeps the widths out of the Markdown", async () => {
    const host = keeper([{ index: 1, signature: SIG_B, widths: [120, 200, 80] }]);
    const { onChangeMarkdown } = renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tableNamed("Step")).toHaveAttribute("data-sized"));
    expect(tableNamed("Model")).not.toHaveAttribute("data-sized");
    expect(colWidths(tableNamed("Step"))).toEqual(["120px", "200px", "80px"]);
    expect(tableNamed("Step").style.width).toBe("400px");
    expect(tableNamed("Step").style.tableLayout).toBe("fixed");

    const editor = surface().editor;
    let at = -1;
    editor.state.doc.descendants((node, pos) => {
      if (at < 0 && node.isText && node.text === "x") at = pos + 1;
      return at < 0;
    });
    act(() => { editor.view.dispatch(editor.state.tr.insertText("q", at)); });
    await waitFor(() => expect(onChangeMarkdown).toHaveBeenCalled());
    expect(String(onChangeMarkdown.mock.lastCall?.[0])).toBe(TABLES.replace("| x | y | z |", "| xq | y | z |"));
    expect(host.onChange).not.toHaveBeenCalled();
  });

  it("shows the resize cursor on a column border, and not inside a cell", async () => {
    renderEditor({ tableColumnWidths: keeper() });
    await waitFor(() => expect(tables()).toHaveLength(2));
    const frame = document.querySelector(".lx-md-editor")!;
    fireEvent.mouseMove(header(tables()[0]!), { clientX: 98 });
    expect(frame).toHaveAttribute("data-column-resize");
    fireEvent.mouseMove(header(tables()[0]!), { clientX: 50 });
    expect(frame).not.toHaveAttribute("data-column-resize");
  });

  it("drags a column border to resize the column, never under the minimum, and hands the host the file's list", async () => {
    const kept: SavedTable = { index: 1, signature: SIG_B, widths: [60, 70, 80] };
    const host = keeper([kept]);
    const { rerender } = renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tableNamed("Step")).toHaveAttribute("data-sized"));
    pressBorder(tableNamed("Model"));
    fireEvent.mouseMove(window, { clientX: 148 });
    expect(colWidths(tableNamed("Model"))).toEqual(["150px", "100px"]);
    fireEvent.mouseMove(window, { clientX: -400 });
    expect(colWidths(tableNamed("Model"))).toEqual([`${MIN_COLUMN_WIDTH}px`, "100px"]);
    fireEvent.mouseMove(window, { clientX: 158 });
    fireEvent.mouseUp(window, { clientX: 158 });
    expect(host.onChange).toHaveBeenCalledTimes(1);
    const next = host.onChange.mock.lastCall![0];
    expect(next).toEqual([kept, { index: 0, signature: SIG_A, widths: [160, 100] }]);

    rerender({ tableColumnWidths: { ...host, tables: next } });
    await waitFor(() => expect(colWidths(tableNamed("Model"))).toEqual(["160px", "100px"]));
    expect(colWidths(tableNamed("Step"))).toEqual(["60px", "70px", "80px"]);
  });

  it("does nothing on a press that does not move, and puts a drag back on Escape or a blur", async () => {
    const host = keeper();
    renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tables()).toHaveLength(2));
    pressBorder(tables()[0]!);
    fireEvent.mouseUp(window, { clientX: 98 });
    pressBorder(tables()[0]!);
    fireEvent.mouseMove(window, { clientX: 150 });
    expect(tables()[0]).toHaveAttribute("data-sized");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(tables()[0]).not.toHaveAttribute("data-sized");
    fireEvent.mouseUp(window, { clientX: 150 });
    pressBorder(tables()[0]!);
    fireEvent.mouseMove(window, { clientX: 150 });
    fireEvent.blur(window);
    expect(tables()[0]).not.toHaveAttribute("data-sized");
    expect(host.onChange).not.toHaveBeenCalled();
  });

  it("ends a drag whose mouseup never reached the page: the next press drags again and the cursor leaves the page", async () => {
    const host = keeper();
    renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tables()).toHaveLength(2));
    const frame = document.querySelector(".lx-md-editor")!;
    fireEvent.mouseMove(header(tables()[0]!), { clientX: 98 });
    pressBorder(tables()[0]!);
    fireEvent.mouseMove(window, { clientX: 148 });
    // The release is lost: the next press, on the other table, still drags.
    pressBorder(tableNamed("Step"));
    expect(host.onChange).toHaveBeenCalledTimes(1);
    expect(host.onChange.mock.lastCall![0]).toEqual([{ index: 0, signature: SIG_A, widths: [150, 100] }]);
    fireEvent.mouseMove(window, { clientX: 128 });
    fireEvent.mouseUp(window, { clientX: 128 });
    expect(colWidths(tableNamed("Step"))).toEqual(["130px", "100px", "100px"]);
    expect(host.onChange).toHaveBeenCalledTimes(2);
    expect(frame).not.toHaveAttribute("data-column-resize");
    fireEvent.mouseMove(header(tables()[0]!), { clientX: 50 });
    expect(frame).not.toHaveAttribute("data-column-resize");
  });

  it("puts a drag back when the pointer is cancelled or the page is hidden", async () => {
    const host = keeper();
    renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tables()).toHaveLength(2));
    pressBorder(tables()[0]!);
    fireEvent.mouseMove(window, { clientX: 148 });
    expect(tables()[0]).toHaveAttribute("data-sized");
    fireEvent(window, new Event("pointercancel"));
    expect(tables()[0]).not.toHaveAttribute("data-sized");
    pressBorder(tables()[0]!);
    fireEvent.mouseMove(window, { clientX: 148 });
    vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    fireEvent(document, new Event("visibilitychange"));
    expect(tables()[0]).not.toHaveAttribute("data-sized");
    fireEvent.mouseMove(window, { clientX: 300 });
    expect(tables()[0]).not.toHaveAttribute("data-sized");
    expect(host.onChange).not.toHaveBeenCalled();
  });

  it("fits a column to its content on a double-click on its border", async () => {
    const host = keeper([{ index: 0, signature: SIG_A, widths: [60, 90] }]);
    renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tableNamed("Model")).toHaveAttribute("data-sized"));
    pressBorder(tableNamed("Model"), 2);
    expect(host.onChange).toHaveBeenLastCalledWith([{ index: 0, signature: SIG_A, widths: [180, 90] }]);
    expect(colWidths(tableNamed("Model"))).toEqual(["180px", "90px"]);
  });

  it("keeps each table's widths when a table with the same column count is inserted above them", async () => {
    const saved = [{ index: 0, signature: SIG_A, widths: [111, 112] }, { index: 1, signature: SIG_B, widths: [121, 122, 123] }];
    const host = keeper(saved);
    const { rerender } = renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tableNamed("Step")).toHaveAttribute("data-sized"));
    const inserted = ["| One | Two | Three |", "| --- | --- | --- |", "| 1 | 2 | 3 |"].join("\n");
    rerender({ text: doc(inserted, TABLES) });
    await waitFor(() => expect(tables()).toHaveLength(3));
    expect(tableNamed("One")).not.toHaveAttribute("data-sized");
    expect(colWidths(tableNamed("Model"))).toEqual(["111px", "112px"]);
    expect(colWidths(tableNamed("Step"))).toEqual(["121px", "122px", "123px"]);
    expect(host.onChange).not.toHaveBeenCalled();
  });

  it("keeps the widths while a table above is half typed, and draws them once it is whole", async () => {
    const saved = [{ index: 0, signature: SIG_A, widths: [111, 112] }, { index: 1, signature: SIG_B, widths: [121, 122, 123] }];
    const host = keeper(saved);
    const { rerender } = renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tableNamed("Step")).toHaveAttribute("data-sized"));
    rerender({ text: doc("| Left | Right |\n| --- |", TABLES) });
    await waitFor(() => expect(surface().textContent).toContain("Left"));
    expect(colWidths(tableNamed("Model"))).toEqual(["111px", "112px"]);
    rerender({ text: doc("| Left | Right |\n| --- | --- |", TABLES) });
    await waitFor(() => expect(tables()).toHaveLength(3));
    expect(tableNamed("Left")).not.toHaveAttribute("data-sized");
    expect(colWidths(tableNamed("Model"))).toEqual(["111px", "112px"]);
    expect(colWidths(tableNamed("Step"))).toEqual(["121px", "122px", "123px"]);
    expect(host.onChange).not.toHaveBeenCalled();
  });

  it("never gives one table another's widths, even at its index with the same column count", async () => {
    const host = keeper([{ index: 0, signature: sig("Name", "Value"), widths: [150, 150] }]);
    renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tables()).toHaveLength(2));
    expect(tableNamed("Model")).not.toHaveAttribute("data-sized");
    expect(host.onChange).not.toHaveBeenCalled();
  });

  it("draws a table whose columns changed at its content's widths, and its own again once the change is undone", async () => {
    const host = keeper([{ index: 0, signature: SIG_A, widths: [111, 112] }]);
    const { rerender } = renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tableNamed("Model")).toHaveAttribute("data-sized"));
    const wider = ["| Model | Score | Extra |", "| --- | --- | --- |", "| Base | 1 | 2 |"].join("\n");
    rerender({ text: doc(wider, "Between.", B) });
    await waitFor(() => expect(colWidths(tableNamed("Model"))).toHaveLength(3));
    expect(tableNamed("Model")).not.toHaveAttribute("data-sized");
    rerender({ text: TABLES });
    await waitFor(() => expect(colWidths(tableNamed("Model"))).toEqual(["111px", "112px"]));
    expect(host.onChange).not.toHaveBeenCalled();
  });

  it("tells tables of one signature apart by the index each was saved at", async () => {
    const host = keeper([{ index: 1, signature: SIG_A, widths: [70, 80] }]);
    renderEditor({ tableColumnWidths: host, text: doc(A, "Between.", A) });
    await waitFor(() => expect(tables()[1]).toHaveAttribute("data-sized"));
    expect(tables()[0]).not.toHaveAttribute("data-sized");
    expect(colWidths(tables()[1]!)).toEqual(["70px", "80px"]);
  });

  it("keeps a table with a very long header within the signature's bound, and finds it again", async () => {
    const long = "w".repeat(5000);
    const text = doc(`| ${long} | Short |\n| --- | --- |\n| a | b |`);
    const host = keeper();
    const { rerender } = renderEditor({ tableColumnWidths: host, text });
    await waitFor(() => expect(tables()).toHaveLength(1));
    pressBorder(tables()[0]!);
    fireEvent.mouseMove(window, { clientX: 148 });
    fireEvent.mouseUp(window, { clientX: 148 });
    const next = host.onChange.mock.lastCall![0];
    expect(next).toHaveLength(1);
    expect(next[0]!.signature.length).toBeLessThanOrEqual(1024);
    expect(next[0]!.signature.startsWith("2\u001f#")).toBe(true);
    rerender({ tableColumnWidths: { ...host, tables: next } });
    await waitFor(() => expect(colWidths(tables()[0]!)).toEqual(["150px", "100px"]));
  });

  it("commits a drag to the table it started on when a table is inserted above it meanwhile", async () => {
    const host = keeper();
    const { rerender } = renderEditor({ tableColumnWidths: host });
    await waitFor(() => expect(tables()).toHaveLength(2));
    pressBorder(tableNamed("Model"));
    fireEvent.mouseMove(window, { clientX: 148 });
    const inserted = ["| One | Two |", "| --- | --- |", "| 1 | 2 |"].join("\n");
    rerender({ text: doc(inserted, TABLES) });
    await waitFor(() => expect(tables()).toHaveLength(3));
    fireEvent.mouseMove(window, { clientX: 158 });
    fireEvent.mouseUp(window, { clientX: 158 });
    expect(host.onChange).toHaveBeenLastCalledWith([{ index: 1, signature: SIG_A, widths: [160, 100] }]);
    expect(tableNamed("One")).not.toHaveAttribute("data-sized");
    expect(colWidths(tableNamed("Model"))).toEqual(["160px", "100px"]);
  });
});
