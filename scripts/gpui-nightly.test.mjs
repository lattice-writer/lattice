// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { failuresOf, guardFailures, issueBody, listedFailures, nightlyFor, planIssue, vitestFailures } from "./gpui-nightly.mjs";

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

describe("gpui-nightly: the guards", () => {
  it("is nothing when every check passed, and names each check that failed", () => {
    const checks = [{ name: "idle with the editor focused, only the caret draws", passed: true }, { name: "idle in the background, the window draws nothing", passed: false }];
    expect(guardFailures("idle guard", { passed: true, checks: [checks[0]] }, 0)).toEqual([]);
    expect(guardFailures("idle guard", { passed: false, checks }, 1)).toEqual(["idle guard: idle in the background, the window draws nothing"]);
    expect(guardFailures("typing guard", { passed: false, checks }, 1)).toEqual(["typing guard: idle in the background, the window draws nothing"]);
  });

  it("fails closed: no result, a scenario that did not finish, or no checks", () => {
    expect(guardFailures("idle guard", null, 1)).toEqual(["idle guard: no result (exit 1)"]);
    expect(guardFailures("idle guard", { error: "the editor has not been laid out" }, 1)).toEqual(["idle guard did not finish: the editor has not been laid out"]);
    expect(guardFailures("idle guard", { passed: true, checks: [] }, 0)).toEqual(["idle guard: ran no checks"]);
    expect(guardFailures("idle guard", { passed: true, checks: [{ name: "x", passed: true }] }, 3)).toEqual(["idle guard: exited 3"]);
  });
});

describe("gpui-nightly: the whole Vitest suite", () => {
  it("is nothing when it passed, names each file that failed, and fails closed without a result or with a bare failure", () => {
    const file = (name, status) => ({ name: new URL(`../${name}`, import.meta.url).pathname, status });
    expect(vitestFailures({ success: true, testResults: [file("src/a.test.ts", "passed"), file("src/b.test.ts", "skipped")] }, 0)).toEqual([]);
    expect(vitestFailures({ success: false, testResults: [file("src/a.test.ts", "failed"), file("src/b.test.ts", "passed")] }, 1)).toEqual(["vitest: src/a.test.ts"]);
    expect(vitestFailures(null, "SIGTERM")).toEqual(["vitest: no result (exit SIGTERM)"]);
    expect(vitestFailures({ success: false, testResults: [] }, 1)).toEqual(["vitest: failed (exit 1) without a failing file"]);
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

  it("runs on main every night on the Mac mini, a chunk at a time under the in-app lock, and reports even when the run failed", () => {
    expect(workflow).toMatch(/schedule:\n\s+#.*\n\s+- cron: "\d+ \d+ \* \* \*"/);
    expect(workflow).toMatch(/runs-on: \$\{\{ vars\.LATTICE_MACOS_RUNNER \|\| 'lattice-mac' \}\}\n/);
    expect(workflow).toMatch(/fetch-depth: 0\n/);
    expect(workflow).toMatch(/ref: main\n/);
    // It takes the in-app lock per chunk (gpui-main-baseline.mjs --chunks), never for the whole run.
    expect(workflow).toMatch(/\n\s+run: node scripts\/gpui-nightly\.mjs run\n/);
    expect(workflow).not.toMatch(/run: .*inapp-lock/);
    expect(readFileSync(new URL("./gpui-nightly.mjs", import.meta.url), "utf8")).toMatch(/"--chunks", String\(CHUNKS\)/);
    expect(workflow).toMatch(/if: \$\{\{ !cancelled\(\) \}\}\n(.*\n){0,3}\s+run: node scripts\/gpui-nightly\.mjs report/);
    expect(workflow).toMatch(/issues: write/);
    // The build tree check runs before the lock, with a short timeout of its own.
    expect(workflow).toMatch(/- name: Reach the shared build tree\n\s+timeout-minutes: 2\n/);
  });

  it("backs its cron up, and a backup runs only when no run started since 18:00 UTC", () => {
    expect(workflow.match(/- cron: "\d+ (\d+) \* \* \*"/g)).toHaveLength(3);
    expect(workflow).toMatch(/e2e:\n\s+needs: due\n\s+if: needs\.due\.outputs\.run == 'true'\n/);
    expect(workflow).toMatch(/if \[ "\$EVENT" != schedule \]; then echo run=true/);
    expect(workflow).toMatch(/actions: read/);
  });
});
