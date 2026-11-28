// @vitest-environment node
import { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { PLATFORMS, TIERS, judge, knownFailures, rejudge, rerunModules, settle } from "./gpui-e2e-verdict.mjs";

const entries = [
  { step: "known on both", platforms: ["macos", "windows"], reason: "r", followup: "LEO-14" },
  { step: "known on windows", platforms: ["windows"], reason: "r", followup: "LEO-15" },
  { step: "flaky on macos", platforms: ["macos"], tier: "flaky", reason: "r", followup: "LEO-14" },
];
const run = (steps, extra = {}) => ({ passed: false, steps: Object.entries(steps).map(([step, passed]) => ({ step, passed })), ...extra });

describe("gpui-e2e-verdict", () => {
  it("fails only on a step that is not a known failure on this platform", () => {
    const result = run({ "known on both": false, "known on windows": false, fine: true });
    expect(judge(result, entries, "windows")).toMatchObject({ ok: true, known: ["known on both", "known on windows"], fresh: [] });
    expect(judge(result, entries, "macos")).toMatchObject({ ok: false, known: ["known on both"], fresh: ["known on windows"] });
  });

  it("reports a known failure that passed, without failing", () => {
    expect(judge(run({ "known on both": true }), entries, "macos")).toMatchObject({ ok: true, fixed: ["known on both"] });
  });

  it("fails a run that did not finish or wrote nothing", () => {
    expect(judge(run({ fine: true }, { error: "the window closed" }), entries, "macos").ok).toBe(false);
    expect(judge(null, entries, "macos").ok).toBe(false);
  });

  it("reruns the modules of the new failures, the smoke flow for a step outside one", () => {
    const result = { steps: [
      { step: "a", passed: false, module: "tree" },
      { step: "b", passed: false, module: "tree" },
      { step: "c", passed: false, module: null },
      { step: "known on both", passed: false, module: "pdf" },
      { step: "d", passed: true, module: "find" },
    ] };
    expect(rerunModules(result, ["a", "b", "c"])).toEqual(["tree", "smoke"]);
  });

  it("calls a new failure that passed on the rerun flaky, and one that failed again new", () => {
    const verdict = judge(run({ a: false, b: false, fine: true }), entries, "macos");
    const settled = settle(verdict, run({ a: true, b: false }));
    expect(settled).toMatchObject({ ok: false, fresh: ["b"], flaky: ["a"] });
    expect(settle(verdict, run({ a: true, b: true }))).toMatchObject({ ok: true, fresh: [], flaky: ["a", "b"] });
    expect(settle(verdict, null)).toMatchObject({ ok: false, fresh: ["a", "b"], flaky: [] });
  });

  it("reruns a flaky-tier failure: flaky when it passes, a warning on a branch and a failure on main when it fails again", () => {
    const result = run({ "flaky on macos": false, fine: true });
    const branch = judge(result, entries, "macos");
    expect(branch).toMatchObject({ ok: true, quarantined: ["flaky on macos"], fresh: [], known: [] });
    expect(settle(branch, run({ "flaky on macos": true }))).toMatchObject({ ok: true, quarantined: [], flaky: ["flaky on macos"] });
    expect(settle(branch, run({ "flaky on macos": false }))).toMatchObject({ ok: true, quarantined: ["flaky on macos"], flaky: [] });
    const main = judge(result, entries, "macos", { strict: true });
    expect(main.ok).toBe(false);
    expect(settle(main, run({ "flaky on macos": true })).ok).toBe(true);
    expect(settle(main, run({ "flaky on macos": false })).ok).toBe(false);
    expect(settle(main, null)).toMatchObject({ ok: false, quarantined: ["flaky on macos"] });
    expect(judge(result, entries, "windows")).toMatchObject({ ok: false, fresh: ["flaky on macos"] });
  });

  it("does not report a flaky-tier step that passed as one to take off the list", () => {
    expect(judge(run({ "flaky on macos": true, "known on both": true }), entries, "macos")).toMatchObject({ ok: true, fixed: ["known on both"] });
  });

  it.skipIf(process.platform === "win32")("passes a run whose only new failure passes when the app runs its module again", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "verdict-"));
    try {
      const fixture = path.join(dir, "fixture");
      mkdirSync(fixture);
      writeFileSync(path.join(fixture, "main.tex"), "x");
      const app = path.join(dir, "app");
      // Writes a passing result for whatever --only names, and records the call.
      writeFileSync(app, `#!/bin/sh\necho "$@" > "${dir}/args"\nwhile [ "$1" != "--out" ]; do shift; done\nprintf '{"passed":true,"steps":[{"step":"flaky one","passed":true}]}' > "$2"\n`);
      chmodSync(app, 0o755);
      const result = path.join(dir, "result.json");
      writeFileSync(result, JSON.stringify(run({ "flaky one": false, fine: true }, { steps: [{ step: "flaky one", passed: false, module: "tree" }, { step: "fine", passed: true, module: null }] })));
      const verdict = spawnSync(process.execPath, [path.join(import.meta.dirname, "gpui-e2e-verdict.mjs"), "--platform", "macos", "--rerun-app", app, "--fixture", fixture, result], { encoding: "utf8", env: { ...process.env, GITHUB_STEP_SUMMARY: "" } });
      expect(verdict.stdout).toContain("0 new failure(s), 1 flaky");
      expect(verdict.stdout).toContain("::warning title=Flaky e2e step (macos)::flaky one");
      expect(verdict.status).toBe(0);
      expect(spawnSync("cat", [path.join(dir, "args")], { encoding: "utf8" }).stdout).toMatch(/--scenario e2e --only tree --out /);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("passes a saved verdict again only when the baseline also fails every new failure and nothing else failed", () => {
    expect(rejudge({ fresh: ["a", "b"], rest: true }, ["a", "b", "c"])).toEqual({ ok: true, fresh: [] });
    expect(rejudge({ fresh: ["a", "b"], rest: true }, ["a"])).toEqual({ ok: false, fresh: ["b"] });
    expect(rejudge({ fresh: ["a"], rest: false }, ["a"])).toEqual({ ok: false, fresh: [] });
  });

  it("saves the verdict for a later --rejudge, which passes only against a baseline failing the same new steps", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "verdict-"));
    try {
      const script = path.join(import.meta.dirname, "gpui-e2e-verdict.mjs");
      const result = path.join(dir, "result.json");
      const saved = path.join(dir, "saved.json");
      writeFileSync(result, JSON.stringify(run({ "new on this mac": false, fine: true })));
      const first = spawnSync(process.execPath, [script, "--platform", "macos", result], { encoding: "utf8", env: { ...process.env, GITHUB_STEP_SUMMARY: "", LATTICE_E2E_VERDICT_OUT: saved } });
      expect(first.status).toBe(1);
      expect(JSON.parse(readFileSync(saved, "utf8"))).toEqual({ fresh: ["new on this mac"], rest: true });
      const again = (failing) => {
        const baseline = path.join(dir, "baseline.json");
        writeFileSync(baseline, JSON.stringify({ e2e: { failing } }));
        return spawnSync(process.execPath, [script, "--rejudge", saved, "--baseline", baseline], { encoding: "utf8" });
      };
      expect(again(["new on this mac"]).status).toBe(0);
      const still = again(["something else"]);
      expect(still.status).toBe(1);
      expect(still.stdout).toContain("still failing against baseline.json: new on this mac");
      // A step an older baseline excused is judged again by the newer one.
      const older = path.join(dir, "older.json");
      writeFileSync(older, JSON.stringify({ e2e: { failing: ["new on this mac"] } }));
      writeFileSync(result, JSON.stringify(run({ "new on this mac": false, "branch broke it": false })));
      spawnSync(process.execPath, [script, "--platform", "macos", "--baseline", older, result], { encoding: "utf8", env: { ...process.env, GITHUB_STEP_SUMMARY: "", LATTICE_E2E_VERDICT_OUT: saved } });
      expect(JSON.parse(readFileSync(saved, "utf8")).fresh.toSorted()).toEqual(["branch broke it", "new on this mac"]);
      expect(again(["branch broke it"]).status).toBe(1);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it.skipIf(process.platform === "win32")("warns on a checked-in flaky-tier step that fails twice, and fails on it with --strict-flaky", () => {
    const entry = knownFailures().find((candidate) => candidate.tier === "flaky");
    const [step, platform] = [entry.step, entry.platforms[0]];
    const dir = mkdtempSync(path.join(tmpdir(), "verdict-"));
    try {
      const fixture = path.join(dir, "fixture");
      mkdirSync(fixture);
      const app = path.join(dir, "app");
      writeFileSync(app, `#!/bin/sh\nwhile [ "$1" != "--out" ]; do shift; done\nprintf '%s' '${JSON.stringify({ passed: false, steps: [{ step, passed: false }] })}' > "$2"\n`);
      chmodSync(app, 0o755);
      const result = path.join(dir, "result.json");
      writeFileSync(result, JSON.stringify({ passed: false, steps: [{ step, passed: false, module: "documents" }, { step: "fine", passed: true, module: null }] }));
      const verdict = (...flags) => spawnSync(process.execPath, [path.join(import.meta.dirname, "gpui-e2e-verdict.mjs"), "--platform", platform, ...flags, "--rerun-app", app, "--fixture", fixture, result], { encoding: "utf8", env: { ...process.env, GITHUB_STEP_SUMMARY: "" } });
      const branch = verdict();
      expect(branch.stdout).toContain(`::warning title=Flaky-tier e2e step failed twice (${platform})::${step}`);
      expect(branch.status).toBe(0);
      const main = verdict("--strict-flaky");
      expect(main.stdout).toContain(`::error title=Flaky-tier e2e step failed twice on main (${platform})::${step}`);
      expect(main.status).toBe(1);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("gives every checked-in entry a platform, a reason and an owner, once per platform", () => {
    const list = knownFailures();
    expect(list.length).toBeGreaterThan(0);
    for (const entry of list) {
      expect(entry.platforms.length, entry.step).toBeGreaterThan(0);
      expect(entry.platforms.every((platform) => PLATFORMS.includes(platform)), entry.step).toBe(true);
      expect(entry.reason.trim(), entry.step).not.toBe("");
      expect(entry.followup, entry.step).toMatch(/^[A-Z]+-\d+$/);
      expect(TIERS, entry.step).toContain(entry.tier ?? "failing");
    }
    // A step may have one entry per platform (known on one, flaky on the other), never two for one platform.
    const pairs = list.flatMap((entry) => entry.platforms.map((platform) => `${platform} ${entry.step}`));
    expect(new Set(pairs).size).toBe(pairs.length);
  });
});
