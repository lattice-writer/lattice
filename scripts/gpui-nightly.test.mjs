// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { failuresOf, issueBody, listedFailures, nightlyFor, planIssue } from "./gpui-nightly.mjs";

const green = { gate: { status: "ok", failing: [] }, build: { status: "ok" }, e2e: { status: "ok", result: "e2e@x.json" } };
const clean = { error: null, fresh: [], quarantined: [], flaky: [], fixed: [] };
const sha = "0123456789abcdef";

describe("gpui-nightly: what failed", () => {
  it("is nothing for a green gate, build and e2e that saved on quit", () => {
    expect(failuresOf(green, clean, "saved")).toEqual([]);
  });

  it("fails closed on the save on quit: lost, or not checked, even beside known failures", () => {
    expect(failuresOf(green, { ...clean, known: ["settings: theme"] }, "lost")).toEqual(["e2e: quitting did not save the typed text"]);
    expect(failuresOf(green, clean, "unknown")).toEqual(["e2e: could not check that quitting saved the typed text"]);
    expect(failuresOf(green, clean, null)).toEqual(["e2e: could not check that quitting saved the typed text"]);
  });

  it("counts a gate that failed without naming a stage", () => {
    expect(failuresOf({ ...green, gate: { status: "failed", failing: [] } }, clean, "saved")).toEqual(["gate: failed"]);
  });

  it("counts gate stages, the build, a scenario that did not finish, new steps and flaky-tier steps that failed twice", () => {
    expect(failuresOf({ ...green, gate: { failing: ["knip"] } }, { ...clean, fresh: ["pdf: zoom"], quarantined: ["clipboard: paste"] })).toEqual(["gate stage knip", "e2e step pdf: zoom", "e2e step clipboard: paste (flaky tier, failed twice)", "e2e: could not check that quitting saved the typed text"]);
    expect(failuresOf({ ...green, build: { status: "failed" }, e2e: { status: "not run", reason: "the build failed" } }, null)).toEqual(["release build: failed"]);
    expect(failuresOf(green, { ...clean, error: "the app crashed" }, "unknown")).toEqual(["e2e did not finish: the app crashed"]);
    expect(failuresOf({ ...green, e2e: { status: "failed", after_quit: "e2e: not saved" } }, clean, "saved")).toEqual(["e2e: quitting did not save the typed text"]);
  });

  it("fails closed: no record, or a built app with no e2e result, is a failure", () => {
    expect(failuresOf(null)).toEqual(["the nightly run wrote no baseline record"]);
    expect(failuresOf({ ...green, e2e: { status: "timed out" } }, null)).toEqual(["e2e: timed out"]);
  });

  it("fails closed on a record from another run or another commit, so a night that wrote none never reports an older green one", () => {
    const saved = { sha, run: "41", failures: [], flaky: [] };
    expect(nightlyFor(saved, { sha, subject: "x", run: "41" })).toBe(saved);
    expect(nightlyFor(saved, { sha, subject: "x", run: "42" }).failures).toEqual([expect.stringMatching(/another run's/)]);
    expect(nightlyFor({ ...saved, sha: "fedcba" }, { sha, subject: "x", run: "41" }).failures).toHaveLength(1);
    expect(nightlyFor(null, { sha, subject: "x", run: "42" }).failures).toEqual([expect.stringMatching(/wrote no record/)]);
  });

  it("does not count flaky steps that passed on the rerun, or known failures", () => {
    expect(failuresOf(green, { ...clean, flaky: ["tree: rename"], fixed: ["settings: theme"] }, "saved")).toEqual([]);
  });
});

describe("gpui-nightly: the issue", () => {
  const body = issueBody({ sha, subject: "x", failures: ["e2e step a", "gate stage knip"] });

  it("records the failures it lists in its body", () => {
    expect(listedFailures(body)).toEqual(["e2e step a", "gate stage knip"]);
    expect(listedFailures("no marker")).toEqual([]);
    expect(listedFailures("<!-- nightly-failures [broken -->")).toEqual([]);
  });

  it("does nothing on a green night with no issue open, and comments and closes an open one", () => {
    expect(planIssue({ issue: null, failures: [], sha, subject: "x" })).toEqual([]);
    expect(planIssue({ issue: { number: 7, body }, failures: [], sha, subject: "x" }).map((action) => action.kind)).toEqual(["comment", "close"]);
  });

  it("opens one for the first failures", () => {
    const [action, ...rest] = planIssue({ issue: null, failures: ["e2e step a"], sha, subject: "x" });
    expect(rest).toEqual([]);
    expect(action).toMatchObject({ kind: "create", title: "Nightly e2e on main fails (1)" });
    expect(listedFailures(action.body)).toEqual(["e2e step a"]);
  });

  it("rewrites the open one every night, and comments only the failures it did not list", () => {
    expect(planIssue({ issue: { number: 7, body }, failures: ["e2e step a"], sha, subject: "x" }).map((action) => action.kind)).toEqual(["edit"]);
    const plan = planIssue({ issue: { number: 7, body }, failures: ["e2e step a", "e2e step b"], sha, subject: "x" });
    expect(plan.map((action) => action.kind)).toEqual(["edit", "comment"]);
    expect(plan[1].body).toContain("- e2e step b");
    expect(plan[1].body).not.toContain("- e2e step a");
    // An issue whose marker was lost counts every failure as new.
    expect(planIssue({ issue: { number: 7, body: "edited by hand" }, failures: ["e2e step a"], sha, subject: "x" }).map((action) => action.kind)).toEqual(["edit", "comment"]);
  });
});

describe("gpui-nightly: the workflow", () => {
  const workflow = readFileSync(new URL("../.github/workflows/nightly-e2e.yml", import.meta.url), "utf8");

  it("runs on main every night on the Mac mini, under the in-app lock, and reports even when the run failed", () => {
    expect(workflow).toMatch(/schedule:\n\s+#.*\n\s+- cron: "\d+ \d+ \* \* \*"/);
    expect(workflow).toMatch(/runs-on: \$\{\{ vars\.LATTICE_MACOS_RUNNER \|\| 'lattice-mac' \}\}\n/);
    expect(workflow).toMatch(/fetch-depth: 0\n/);
    expect(workflow).toMatch(/ref: main\n/);
    expect(workflow).toMatch(/scripts\/inapp-lock --thread .* -- node scripts\/gpui-nightly\.mjs run/);
    expect(workflow).toMatch(/if: \$\{\{ !cancelled\(\) \}\}\n(.*\n){0,3}\s+run: node scripts\/gpui-nightly\.mjs report/);
    expect(workflow).toMatch(/issues: write/);
  });
});
