import { describe, expect, it, vi } from "vitest";
import type { OverleafSyncResult } from "../app-types";
import { overleafSyncWarnings } from "./overleaf-sync-warnings";

const result = (patch: Partial<OverleafSyncResult>): OverleafSyncResult => ({
  pulled: [], pushed: [], merged: [], conflicts: [], deletedLocal: [], skippedRemoteDeletes: [], ...patch,
});

describe("overleafSyncWarnings", () => {
  it("names a file newly held with conflict markers, and offers to resolve it", () => {
    const resolve = vi.fn();
    const warnings = overleafSyncWarnings(result({
      unresolvedConflicts: ["main.tex", "intro.tex"], newlyUnresolvedConflicts: ["main.tex"],
    }), resolve);
    expect(warnings.map((warning) => warning.message)).toEqual([
      "main.tex still has conflict markers. Resolve them to bring in Overleaf's newer edits.",
    ]);
    void warnings[0].action?.onClick();
    expect(resolve).toHaveBeenCalledWith("main.tex");
  });

  it("names each file whose name Overleaf refuses, with the rule it breaks", () => {
    const skippedNames = [
      { path: "cvpr/\\paperbib.bib", problem: "backslash" as const },
      { path: "figs/*.png", problem: "asterisk" as const },
      { path: "fig-😀.png", problem: "supplementaryCharacter" as const },
    ];
    expect(overleafSyncWarnings(result({ skippedNames }), vi.fn())).toEqual([
      { message: "Overleaf does not allow a backslash in file names, so cvpr/\\paperbib.bib stays on this machine. Rename it to sync it." },
      { message: "Overleaf does not allow an asterisk in file names, so figs/*.png stays on this machine. Rename it to sync it." },
      { message: "Overleaf does not allow emoji or some rare characters in file names, so fig-😀.png stays on this machine. Rename it to sync it." },
    ]);
  });

  it("names every file uploaded over a co-author's edit in one warning", () => {
    const warnings = overleafSyncWarnings(result({ pushed: ["a.tex", "b.tex"], uploadedOverEdits: ["a.tex", "b.tex"] }), vi.fn());
    expect(warnings).toEqual([{
      message: "Someone edited the Overleaf project while a.tex, b.tex uploaded. An upload replaces the whole file, so it may have overwritten their latest typing there. Check the file's history on Overleaf to bring back anything missing.",
    }]);
  });

  it("names each edited file held for a rename on Overleaf, with the way out", () => {
    const warnings = overleafSyncWarnings(result({ heldRenames: ["sections/method.tex"] }), vi.fn());
    expect(warnings).toEqual([{
      message: "Overleaf renamed or removed sections/method.tex while it was edited here, and Lattice could not tell where the edit belongs, so it stays here unsent. Copy the edit into the right file, then delete sections/method.tex here.",
    }]);
  });

  it("names a folder gone from Overleaf that stays for the writer's own files", () => {
    const keptFolders = [{ folder: "A", files: ["A/main (local conflict 20260701-1200).pdf"] }];
    expect(overleafSyncWarnings(result({ keptFolders }), vi.fn())).toEqual([{
      message: "Overleaf no longer has the folder A. It stays here because it still holds files that never sync: A/main (local conflict 20260701-1200).pdf. Delete them here if you no longer need them.",
    }]);
  });

  it("says nothing for a sync whose uploads met no one else's edit", () => {
    expect(overleafSyncWarnings(result({ pushed: ["a.tex"], uploadedOverEdits: [] }), vi.fn())).toEqual([]);
  });

  it("stays quiet about a file the previous sync was already holding", () => {
    expect(overleafSyncWarnings(result({ unresolvedConflicts: ["main.tex"] }), vi.fn())).toEqual([]);
  });

  it("summarizes long file lists in warnings instead of dumping every path", () => {
    const paths = Array.from({ length: 9 }, (_, i) => `data/file_${i + 1}.csv`);
    const warnings = overleafSyncWarnings(result({ skippedLarge: paths }), vi.fn());
    expect(warnings).toEqual([{
      message: "Too large for Overleaf, so left on this machine: data/file_1.csv, data/file_2.csv, data/file_3.csv, data/file_4.csv, data/file_5.csv and 4 more files.",
    }]);
  });

  it("keeps every path in actionable warnings even when more than six files are affected", () => {
    const paths = Array.from({ length: 8 }, (_, i) => `sections/ch_${i + 1}.tex`);
    const warnings = overleafSyncWarnings(
      result({ refusedIncoming: paths, uploadedOverEdits: paths }),
      vi.fn(),
    );
    expect(warnings).toHaveLength(2);
    for (const warning of warnings) {
      for (const path of paths) expect(warning.message).toContain(path);
      expect(warning.message).not.toContain("more files");
    }
  });
});
