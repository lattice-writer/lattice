/**
 * Column widths for the visual engine's tables (GPUI's Markdown island asks
 * for them through `tableColumnWidths`, visual-editor-props.ts): a pointer
 * on a column's border shows the resize cursor, a drag sets that column's
 * width (never under {@link MIN_COLUMN_WIDTH}; Escape or the window losing
 * focus puts it back), and a double-click fits it to its content.
 *
 * The widths are the host's to keep, never the document's: no attribute,
 * mark or Markdown carries them, so the file stays plain GFM and every other
 * reader of it sees nothing. The host keeps a file's list of saved tables,
 * each with its signature (its column count and header text, see
 * {@link tableSignature}) and the index it was saved at. A drawn table takes
 * the entry of its signature; when several tables or entries share one, only
 * the entry saved at the table's own index. So a table inserted, removed or
 * half-typed elsewhere never moves widths onto another table, a table whose
 * columns or header change draws at its content's widths, and nothing saved
 * is dropped while the file is open: an undo, or the header typed back,
 * finds the entry again. Entries no table matches go first once a file has
 * more than {@link MAX_SAVED}.
 *
 * Widths reach the table as a node decoration's spec, which
 * {@link SizedTableView} applies to the colgroup it owns (ProseMirror
 * ignores mutations outside a table's cells), so they are CSS pixels set on
 * `<col>` with a fixed layout. While a drag is under way, and until the host
 * keeps the committed widths, the plugin's own overlay is drawn instead; the
 * host's list is read again with every transaction, so a change of it needs
 * no message of its own. Without a host asking, the plugin draws nothing and
 * handles no event, so a table is laid out exactly as Tiptap's view lays it.
 *
 * Clean implementation for Lattice; spec: docs/visual-editor-spec.md.
 */
import { Extension } from "@tiptap/core";
import { TableView } from "@tiptap/extension-table";
import type { Node as PmNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { TableMap, cellAround } from "@tiptap/pm/tables";
import { Decoration, DecorationSet, type EditorView } from "@tiptap/pm/view";

/** One table's widths as the host keeps them. */
export type SavedTable = { index: number; signature: string; widths: readonly number[] };

/** What the host keeps of the open file's tables. */
export type TableColumnWidths = {
  tables: readonly SavedTable[];
  /** The file's saved tables after a resize: the whole list, to keep as it is. */
  onChange: (tables: SavedTable[]) => void;
};

/** The narrowest a dragged or fitted column gets, in CSS pixels. */
export const MIN_COLUMN_WIDTH = 48;
/** The most tables a file keeps widths for. */
const MAX_SAVED = 64;
/** The most columns a table's widths are saved for (the host keeps no more). */
const MAX_COLUMNS = 256;
/**
 * The longest signature, in UTF-16 units: at most 3 bytes each in UTF-8, so
 * within the 4096 bytes the host keeps. A longer one is its column count and
 * a hash of the whole.
 */
const MAX_SIGNATURE = 1024;
/** How near a cell's edge, in CSS pixels, the pointer starts a resize. */
const EDGE = 5;
const SEPARATOR = "\u001f";

/** FNV-1a over UTF-16 units, as eight hex digits. */
function hash(text: string): string {
  let value = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    value ^= text.charCodeAt(index);
    value = Math.imul(value, 0x01000193) >>> 0;
  }
  return value.toString(16).padStart(8, "0");
}

/** Widths drawn ahead of the host, by table index, for the table that had `signature` there. */
type Overlay = Readonly<Record<number, { signature: string; widths: readonly number[] }>>;
type WidthsState = {
  /** The host's list these were drawn from. */
  saved: readonly SavedTable[];
  /** Each table's position, in reading order. */
  tables: number[];
  signatures: string[];
  /** Each table's saved entry, when one is its. */
  matched: (SavedTable | null)[];
  overlay: Overlay;
  decorations: DecorationSet;
};
type Meta = { overlay: { table: number; signature: string; widths: readonly number[] | null } };

const tableWidthsKey = new PluginKey<WidthsState>("latticeTableColumnWidths");

/** What names a table across edits elsewhere: its column count and its header row's text. */
function tableSignature(table: PmNode): string {
  const header: string[] = [];
  table.firstChild?.forEach((cell) => header.push(cell.textContent.replace(/\s+/g, " ").trim()));
  const columns = String(TableMap.get(table).width);
  const signature = [columns, ...header].join(SEPARATOR);
  return signature.length <= MAX_SIGNATURE ? signature : `${columns}${SEPARATOR}#${hash(signature)}:${signature.length}`;
}

/**
 * Each table's saved entry: the one of its signature, or, when that
 * signature is not unique among the tables or the entries, the one of its
 * signature saved at its own index; else none.
 */
function matchSaved(signatures: readonly string[], saved: readonly SavedTable[]): (SavedTable | null)[] {
  return signatures.map((signature, index) => {
    const candidates = saved.filter((entry) => entry.signature === signature);
    if (candidates.length === 1 && signatures.filter((other) => other === signature).length === 1) return candidates[0]!;
    return candidates.find((entry) => entry.index === index) ?? null;
  });
}

/**
 * The list after table `table` takes `widths`: its entry replaced or added
 * (a table wider than {@link MAX_COLUMNS} is drawn at its widths but not
 * saved), every matched entry at its table's index now, and past
 * {@link MAX_SAVED} the oldest entries no table matches dropped first.
 */
function savedAfter(state: WidthsState, table: number, widths: readonly number[]): SavedTable[] {
  const own = state.matched[table];
  const kept = state.saved.filter((entry) => entry !== own).map((entry) => {
    const at = state.matched.indexOf(entry);
    return { entry: at >= 0 && at !== entry.index ? { ...entry, index: at } : entry, matched: at >= 0 };
  });
  if (widths.length <= MAX_COLUMNS) kept.push({ entry: { index: table, signature: state.signatures[table]!, widths: [...widths] }, matched: true });
  while (kept.length > MAX_SAVED) {
    const orphan = kept.findIndex((item) => !item.matched);
    kept.splice(orphan >= 0 ? orphan : 0, 1);
  }
  return kept.map((item) => item.entry);
}

/** The widths a table's decorations carry, when it has any. */
function decoratedWidths(decorations: readonly Decoration[]): readonly number[] | null {
  for (const decoration of decorations) {
    const widths = (decoration.spec as { columnWidths?: readonly number[] | null }).columnWidths;
    if (widths) return widths;
  }
  return null;
}

/** Tiptap's table view, sized by the widths its decorations carry. */
export class SizedTableView extends TableView {
  constructor(node: PmNode, cellMinWidth: number, view: EditorView, attributes: Record<string, unknown>, decorations: readonly Decoration[]) {
    super(node, cellMinWidth, view, attributes);
    this.size(decorations);
  }

  override update(node: PmNode, decorations: readonly Decoration[] = []): boolean {
    if (!super.update(node)) return false;
    this.size(decorations);
    return true;
  }

  private size(decorations: readonly Decoration[]) {
    const widths = decoratedWidths(decorations);
    const sized = this.table.hasAttribute("data-sized");
    if (!widths && !sized) return;
    Array.from(this.colgroup.children as HTMLCollectionOf<HTMLElement>).forEach((col, column) => {
      col.style.width = widths ? `${widths[column]}px` : "";
    });
    this.table.toggleAttribute("data-sized", Boolean(widths));
    if (!widths) {
      this.table.style.tableLayout = "";
      return;
    }
    this.table.style.width = `${widths.reduce((sum, width) => sum + width, 0)}px`;
    this.table.style.minWidth = "";
    this.table.style.tableLayout = "fixed";
  }
}

const isTable = (node: PmNode) => node.type.spec.tableRole === "table";
const sameWidths = (left: readonly number[] | undefined, right: readonly number[]) => (
  left?.length === right.length && left.every((width, column) => width === right[column])
);

function computeState(doc: PmNode, saved: readonly SavedTable[], drawn: Overlay): WidthsState {
  const tables: number[] = [];
  const nodes: PmNode[] = [];
  doc.descendants((node, pos) => {
    if (!isTable(node)) return !node.isTextblock && !node.isAtom;
    tables.push(pos);
    nodes.push(node);
    return false;
  });
  const signatures = nodes.map(tableSignature);
  const matched = matchSaved(signatures, saved);
  // An overlay stays only on the table it was drawn for, until the host keeps it.
  const overlay: Overlay = Object.fromEntries(Object.entries(drawn).filter(([index, own]) => (
    signatures[Number(index)] === own.signature && !sameWidths(matched[Number(index)]?.widths, own.widths)
  )));
  const decorations = nodes.map((node, index) => {
    const widths = overlay[index]?.widths ?? matched[index]?.widths ?? null;
    const fits = widths && widths.length === TableMap.get(node).width ? widths : null;
    return Decoration.node(tables[index]!, tables[index]! + node.nodeSize, { "data-lx-table": String(index) }, { columnWidths: fits });
  });
  return { saved, tables, signatures, matched, overlay, decorations: DecorationSet.create(doc, decorations) };
}

const INACTIVE: WidthsState = { saved: [], tables: [], signatures: [], matched: [], overlay: {}, decorations: DecorationSet.empty };

/** A column border under the pointer: its table and the column left of it. */
type Border = { table: number; tablePos: number; column: number };

function borderAt(view: EditorView, event: MouseEvent): Border | null {
  const target = event.target instanceof Element ? event.target : null;
  const cell = target?.closest<HTMLElement>("td, th");
  if (!cell || !view.dom.contains(cell)) return null;
  const rect = cell.getBoundingClientRect();
  const scale = cell.offsetWidth ? rect.width / cell.offsetWidth : 1;
  const edge = EDGE * scale;
  const side = event.clientX >= rect.right - edge ? 1 : event.clientX <= rect.left + edge ? -1 : 0;
  if (!side) return null;
  const $cell = cellAround(view.state.doc.resolve(view.posAtDOM(cell, 0)));
  if (!$cell) return null;
  const tableStart = $cell.start(-1);
  const map = TableMap.get($cell.node(-1));
  const found = map.findCell($cell.pos - tableStart);
  const column = side > 0 ? found.right - 1 : found.left - 1;
  const table = tableWidthsKey.getState(view.state)?.tables.indexOf(tableStart - 1) ?? -1;
  return column < 0 || table < 0 ? null : { table, tablePos: tableStart - 1, column };
}

/** The table element drawn for the table at `pos`. */
function tableElement(view: EditorView, pos: number): HTMLTableElement | null {
  const dom = view.nodeDOM(pos) as HTMLElement | null;
  return dom instanceof HTMLTableElement ? dom : dom?.querySelector("table") ?? null;
}

/** Each column's cells that span it alone, as drawn. */
function columnCells(view: EditorView, pos: number, table: PmNode, column: number): HTMLElement[] {
  const map = TableMap.get(table);
  const seen = new Set<number>();
  const cells: HTMLElement[] = [];
  for (let row = 0; row < map.height; row += 1) {
    const offset = map.map[row * map.width + column]!;
    if (seen.has(offset)) continue;
    seen.add(offset);
    if (table.nodeAt(offset)?.attrs.colspan !== 1) continue;
    const dom = view.nodeDOM(pos + 1 + offset);
    if (dom instanceof HTMLElement) cells.push(dom);
  }
  return cells;
}

/** The table's columns as drawn now, in CSS pixels. */
function drawnWidths(view: EditorView, pos: number, table: PmNode): number[] {
  const map = TableMap.get(table);
  return Array.from({ length: map.width }, (_, column) => {
    const cell = columnCells(view, pos, table, column)[0];
    if (cell) return cell.offsetWidth;
    const offset = map.map[column]!;
    const spanning = view.nodeDOM(pos + 1 + offset);
    const colspan = (table.nodeAt(offset)?.attrs.colspan as number | undefined) ?? 1;
    return spanning instanceof HTMLElement ? spanning.offsetWidth / colspan : MIN_COLUMN_WIDTH;
  }).map((width) => Math.max(MIN_COLUMN_WIDTH, Math.round(width)));
}

/**
 * The width `column` takes for its content: the table is laid out once with
 * that column free and the table as wide as its content, and put back before
 * anything paints. No wider than the table's frame, so one long cell does
 * not make a column the reader must scroll across.
 */
function fittedWidth(view: EditorView, pos: number, table: PmNode, column: number): number {
  const element = tableElement(view, pos);
  const col = element?.querySelector("colgroup")?.children[column] as HTMLElement | undefined;
  if (!element || !col) return MIN_COLUMN_WIDTH;
  const kept = { width: element.style.width, minWidth: element.style.minWidth, layout: element.style.tableLayout, col: col.style.width };
  element.style.tableLayout = "auto";
  element.style.width = "max-content";
  element.style.minWidth = "";
  col.style.width = "";
  const widest = Math.max(0, ...columnCells(view, pos, table, column).map((cell) => cell.offsetWidth));
  element.style.tableLayout = kept.layout;
  element.style.width = kept.width;
  element.style.minWidth = kept.minWidth;
  col.style.width = kept.col;
  const frame = element.parentElement?.clientWidth || Infinity;
  return Math.round(Math.max(MIN_COLUMN_WIDTH, Math.min(widest, frame)));
}

/** The pointer's side of a resize: the cursor, the drag, the fit. */
class ColumnResize {
  private hovering = false;
  /** Ends the drag under way: kept (true) or put back. */
  private finish: ((keep: boolean) => void) | null = null;

  constructor(private readonly view: EditorView, private readonly source: () => TableColumnWidths | undefined) {}

  private state() {
    return tableWidthsKey.getState(this.view.state) ?? INACTIVE;
  }

  /** The widths the table has now: its overlay's, its saved entry's, or as drawn. */
  private widthsOf(border: Border, table: PmNode): number[] {
    const state = this.state();
    const own = state.overlay[border.table]?.widths ?? state.matched[border.table]?.widths;
    return own && own.length === TableMap.get(table).width ? [...own] : drawnWidths(this.view, border.tablePos, table);
  }

  /**
   * Where the table pressed on is now: the same node (an edit elsewhere
   * keeps it), else the one table of its signature; null when it is gone or
   * cannot be told apart.
   */
  private indexOf(table: PmNode, signature: string): number | null {
    const state = this.state();
    const { doc } = this.view.state;
    const same = state.tables.findIndex((pos) => doc.nodeAt(pos) === table);
    if (same >= 0) return same;
    const named = state.signatures.flatMap((other, index) => (other === signature ? [index] : []));
    return named.length === 1 ? named[0]! : null;
  }

  private show(table: number | null, widths: readonly number[] | null) {
    if (table === null) return;
    const meta: Meta = { overlay: { table, signature: this.state().signatures[table] ?? "", widths } };
    this.view.dispatch(this.view.state.tr.setMeta(tableWidthsKey, meta).setMeta("addToHistory", false));
  }

  private commit(host: TableColumnWidths, table: number | null, widths: readonly number[]) {
    if (table === null) return;
    this.show(table, widths);
    host.onChange(savedAfter(this.state(), table, widths));
  }

  private setCursor(on: boolean) {
    if (on === this.hovering) return;
    this.hovering = on;
    const frame = this.view.dom.closest<HTMLElement>(".lx-md-editor") ?? this.view.dom.parentElement;
    if (on) frame?.setAttribute("data-column-resize", "");
    else frame?.removeAttribute("data-column-resize");
  }

  move(event: MouseEvent) {
    if (this.finish) return;
    this.setCursor(Boolean(this.source() && borderAt(this.view, event)));
  }

  leave() {
    if (!this.finish) this.setCursor(false);
  }

  /** A press on a border: a double-click fits the column, else the press drags it. */
  down(event: MouseEvent): boolean {
    const host = this.source();
    if (!host || event.button !== 0 || this.finish) return false;
    const border = borderAt(this.view, event);
    const table = border ? this.view.state.doc.nodeAt(border.tablePos) : null;
    if (!border || !table) return false;
    event.preventDefault();
    const widths = this.widthsOf(border, table);
    if (event.detail >= 2) {
      widths[border.column] = fittedWidth(this.view, border.tablePos, table, border.column);
      this.commit(host, border.table, widths);
      return true;
    }
    const element = tableElement(this.view, border.tablePos);
    const scale = element?.offsetWidth ? element.getBoundingClientRect().width / element.offsetWidth : 1;
    const start = event.clientX;
    const from = widths[border.column]!;
    const before = this.state().overlay[border.table]?.widths ?? null;
    const signature = this.state().signatures[border.table] ?? "";
    // The table is found again at each step: an edit elsewhere (a co-author's, an
    // undo) can move it to another index while the pointer is down.
    const at = () => this.indexOf(table, signature);
    let moved = false;
    const drag = (moveEvent: MouseEvent) => {
      const width = Math.max(MIN_COLUMN_WIDTH, Math.round(from + (moveEvent.clientX - start) / (scale || 1)));
      if (width === widths[border.column]) return;
      moved = true;
      widths[border.column] = width;
      this.show(at(), [...widths]);
    };
    const key = (keyEvent: KeyboardEvent) => {
      if (keyEvent.key !== "Escape") return;
      keyEvent.preventDefault();
      this.finish?.(false);
    };
    const up = () => this.finish?.(true);
    const blur = () => this.finish?.(false);
    this.finish = (keep) => {
      window.removeEventListener("mousemove", drag);
      window.removeEventListener("mouseup", up);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", blur);
      this.finish = null;
      if (!moved || this.view.isDestroyed) return;
      if (keep) this.commit(host, at(), widths);
      else this.show(at(), before);
    };
    window.addEventListener("mousemove", drag);
    window.addEventListener("mouseup", up);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", blur);
    return true;
  }

  destroy() {
    this.finish?.(false);
  }
}

const resizers = new WeakMap<EditorView, ColumnResize>();

export const TableColumnWidthsExtension = Extension.create<{ source: () => TableColumnWidths | undefined }>({
  name: "latticeTableColumnWidths",
  addOptions: () => ({ source: () => undefined }),
  addProseMirrorPlugins() {
    const { source } = this.options;
    return [new Plugin<WidthsState>({
      key: tableWidthsKey,
      state: {
        init: (_config, state) => {
          const host = source();
          return host ? computeState(state.doc, host.tables, {}) : INACTIVE;
        },
        apply: (transaction, value, _previous, state) => {
          const host = source();
          if (!host) return INACTIVE;
          const meta = transaction.getMeta(tableWidthsKey) as Meta | undefined;
          if (!meta && !transaction.docChanged && value !== INACTIVE && host.tables === value.saved) return value;
          let overlay: Overlay = value.overlay;
          if (meta) {
            const { table, signature, widths } = meta.overlay;
            const rest = Object.fromEntries(Object.entries(value.overlay).filter(([index]) => Number(index) !== table));
            overlay = widths ? { ...rest, [table]: { signature, widths } } : rest;
          }
          return computeState(state.doc, host.tables, overlay);
        },
      },
      props: {
        decorations: (state) => tableWidthsKey.getState(state)?.decorations,
        handleDOMEvents: {
          mousemove: (view, event) => {
            resizers.get(view)?.move(event);
            return false;
          },
          mouseleave: (view) => {
            resizers.get(view)?.leave();
            return false;
          },
          mousedown: (view, event) => resizers.get(view)?.down(event) ?? false,
        },
      },
      view: (view) => {
        const resize = new ColumnResize(view, source);
        resizers.set(view, resize);
        return {
          destroy: () => {
            resize.destroy();
            resizers.delete(view);
          },
        };
      },
    })];
  },
});
