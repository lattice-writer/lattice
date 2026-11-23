/**
 * Whether the visual editor's edit joins the previous one's undo group, said
 * with `md:change` (`join`) so ⌘Z in the buffer undoes what the Tauri app's
 * canvas would. There the page's published edits land in CodeMirror, whose
 * `history()` groups an edit with the last when it comes within
 * `newGroupDelay` (500 ms) of it and touches the text it changed. Here they
 * cross to the buffer, whose own grouping is by arrival (300 ms): a word
 * typed with a short pause, published as two edits 200 ms after each pause
 * (`publicationIdleMs`), was two undo steps, and how many depended on how
 * fast the messages crossed.
 *
 * The delay is measured between the writer's inputs, as CodeMirror measures
 * it between transactions: from the last input of the previous publication
 * to the first input of this one. A publication's own time comes late by the
 * idle delay plus however long the page's timers lag (a busy or inactive
 * window throttles them), which split a word typed with a short pause on a
 * slow machine. An edit with no input behind it (a command, a drop) is
 * placed at its publication.
 */

/** CodeMirror history's `newGroupDelay`, in milliseconds. */
export const NEW_GROUP_DELAY = 500;

/** The text an edit replaced, `from`–`toBefore` of its base, now `from`–`toAfter`. */
type Change = { from: number; toBefore: number; toAfter: number };

/** The single replacement turning `base` into `next` (the owner applies the same one). */
export function changeBetween(base: string, next: string): Change {
  const limit = Math.min(base.length, next.length);
  let from = 0;
  while (from < limit && base.charCodeAt(from) === next.charCodeAt(from)) from += 1;
  let end = 0;
  while (end < limit - from && base.charCodeAt(base.length - 1 - end) === next.charCodeAt(next.length - 1 - end)) end += 1;
  return { from, toBefore: base.length - end, toAfter: next.length - end };
}

/** Decides each published edit's `join` from the last one's time and place. */
export class UndoGrouping {
  private last: (Change & { end: number }) | null = null;
  /** The first and last input since the last publication. */
  private inputs: { first: number; last: number } | null = null;

  /** An input (a key, an input method's text, a paste) at `at`. */
  input(at: number) {
    this.inputs = { first: this.inputs?.first ?? at, last: at };
  }

  /** The edit `base` → `next` published at `at`: whether it joins the last. */
  next(base: string, next: string, at: number): boolean {
    const change = changeBetween(base, next);
    const { first, last: end } = this.inputs ?? { first: at, last: at };
    const last = this.last;
    this.last = { ...change, end };
    this.inputs = null;
    return last != null && first - last.end < NEW_GROUP_DELAY && change.from <= last.toAfter && change.toBefore >= last.from;
  }
}
