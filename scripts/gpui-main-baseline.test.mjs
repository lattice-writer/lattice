import { describe, expect, it } from "vitest";
import { baselineDir, chunkModules, mergeChunks, mergeQuits, parseE2e, parseGate } from "./gpui-main-baseline.mjs";

// The main-baseline cache is macOS-only tooling, and its paths are POSIX ones.
describe.skipIf(process.platform === "win32")("gpui-main-baseline", () => {
  it("reads the gate's stage lines", () => {
    const output = [
      "gate:changed: 0 changed file(s) against abc (abc123456)",
      "  lint       ok          12.3 s  (oxlint with the warning cap)",
      "  knip       FAILED       8.0 s  (unused files, exports and dependencies)",
      "    | Unused exports (2)",
      "  vitest     TIMED OUT after 900 s  901.0 s  (related tests)",
      "  i18n       skipped  (no source or catalog changed)",
      "gate:changed: 2 stage(s) failed",
    ].join("\n");
    expect(parseGate(output).map((stage) => [stage.name, stage.verdict])).toEqual([
      ["lint", "ok"],
      ["knip", "failed"],
      ["vitest", "failed"],
      ["i18n", "skipped"],
    ]);
  });

  it("names the failing e2e steps", () => {
    const result = { passed: false, steps: [{ step: "a", passed: true }, { step: "⌘, opens Settings", passed: false }] };
    expect(parseE2e(result)).toEqual({ passed: false, total: 2, failing: ["⌘, opens Settings"] });
    expect(parseE2e({ error: "boom" })).toMatchObject({ passed: false, error: "boom" });
    expect(parseE2e(null)).toMatchObject({ passed: false, error: "no result was written" });
  });

  it("keeps the steps a scenario recorded before it stopped", () => {
    const result = {
      passed: false,
      error: "the file did not open in an editor",
      steps: [
        { step: "a", passed: true },
        { step: "the e2e scenario runs to its end", passed: false },
      ],
    };
    expect(parseE2e(result)).toEqual({
      passed: false,
      total: 2,
      failing: ["the e2e scenario runs to its end"],
      error: "the file did not open in an editor",
    });
  });

  it("writes under the shared build directory", () => {
    expect(baselineDir({ LATTICE_GPUI_BUILD_DIR: "/x" })).toBe("/x/main-baseline");
  });
});

describe("gpui-main-baseline: the e2e in chunks", () => {
  it("splits the modules into contiguous chunks of near-equal size, every module once", () => {
    const modules = Array.from({ length: 106 }, (_, index) => `m${index}`);
    const chunks = chunkModules(modules, 6);
    expect(chunks.map((chunk) => chunk.length)).toEqual([18, 18, 18, 18, 18, 16]);
    expect(chunks.flat()).toEqual(modules);
    expect(chunkModules(["a", "b"], 6)).toEqual([["a"], ["b"]]);
  });

  it("merges the chunks into one result, a step failing when it failed in any chunk", () => {
    const smoke = (passed) => ({ step: "open", passed, module: undefined });
    const merged = mergeChunks([
      { passed: true, steps: [smoke(true), { step: "a1", passed: true, module: "a" }] },
      { passed: false, steps: [smoke(false), { step: "b1", passed: false, module: "b" }] },
    ]);
    expect(merged.passed).toBe(false);
    expect(merged.steps.map((step) => [step.step, step.passed])).toEqual([["open", false], ["a1", true], ["b1", false]]);
    expect(merged.error).toBeUndefined();
    expect(mergeChunks([{ passed: true, steps: [smoke(true)] }, { passed: true, steps: [smoke(true)] }]).passed).toBe(true);
  });

  it("fails closed on a chunk with no result or an error, such as one that never got the lock", () => {
    const merged = mergeChunks([{ passed: true, steps: [] }, null, { error: "the in-app lock was not free within 90 min" }]);
    expect(merged).toMatchObject({ passed: false, error: "chunk 2 wrote no result; chunk 3: the in-app lock was not free within 90 min" });
  });

  it("adds up the save-on-quit checks: saved only if every chunk saved", () => {
    expect(mergeQuits(["saved", "saved"])).toBe("saved");
    expect(mergeQuits(["saved", "lost", "unknown"])).toBe("lost");
    expect(mergeQuits(["saved", "unknown"])).toBe("unknown");
    expect(mergeQuits([])).toBe("unknown");
  });
});
