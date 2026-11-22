import { describe, expect, it } from "vitest";
import { NEW_GROUP_DELAY, UndoGrouping, changeBetween } from "./undo-group";

describe("changeBetween", () => {
  it("finds the one replacement between two texts", () => {
    expect(changeBetween("ab quo cd", "ab quokka cd")).toEqual({ from: 6, toBefore: 6, toAfter: 9 });
    expect(changeBetween("abc", "abc")).toEqual({ from: 3, toBefore: 3, toAfter: 3 });
    expect(changeBetween("aaa", "aa")).toEqual({ from: 2, toBefore: 3, toAfter: 2 });
  });
});

describe("UndoGrouping", () => {
  it("joins a word typed in two publications within the delay", () => {
    const grouping = new UndoGrouping();
    expect(grouping.next("Some text.", "Some text. quo", 1_000)).toBe(false);
    expect(grouping.next("Some text. quo", "Some text. quokka", 1_000 + NEW_GROUP_DELAY - 1)).toBe(true);
  });

  it("starts a new group after the delay", () => {
    const grouping = new UndoGrouping();
    grouping.next("Some text.", "Some text. quo", 1_000);
    expect(grouping.next("Some text. quo", "Some text. quokka", 1_000 + NEW_GROUP_DELAY)).toBe(false);
  });

  it("starts a new group for an edit elsewhere in the text", () => {
    const grouping = new UndoGrouping();
    grouping.next("one two", "one two three", 1_000);
    expect(grouping.next("one two three", "zero one two three", 1_100)).toBe(false);
  });

  it("joins a deletion that touches the last insertion", () => {
    const grouping = new UndoGrouping();
    grouping.next("one", "one tow", 1_000);
    expect(grouping.next("one tow", "one t", 1_200)).toBe(true);
  });
});
