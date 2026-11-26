// @vitest-environment node
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { docsStages, holderRunsE2e, holdingInappLock, localStages } from "./ci-local.mjs";
import { ciTier } from "./ci-tier.mjs";
import { runStages } from "./gate-changed.mjs";

const repo = path.resolve(import.meta.dirname, "..");
const lock = path.join(repo, "scripts", "inapp-lock");

describe.skipIf(process.platform === "win32")("ci:local", () => {
  it("runs a windowed stage while it holds the in-app lock, and frees it after", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "ci-local-"));
    const env = { ...process.env, INAPP_LOCK_FILE: path.join(dir, "lock") };
    try {
      const stage = holdingInappLock({ name: "gpui-e2e", command: "sh", args: ["-c", '"$1" status; exit 4', "sh", lock], timeout: 1_000 });
      expect(stage.timeout).toBeGreaterThan(1_000);
      const run = spawnSync(stage.command, stage.args, { cwd: repo, env, encoding: "utf8" });
      expect(run.stdout).toMatch(/held by ci:local \S+ gpui-e2e/);
      expect(run.status).toBe(4);
      expect(spawnSync(lock, ["status"], { env, encoding: "utf8" }).stdout).toMatch(/free/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("builds beside a live e2e holder only, and waits for any other, a stale or an unreadable one", () => {
    const live = () => [process.pid];
    expect(holderRunsE2e({ kind: "e2e", pid: process.pid, command: "gpui/script/e2e --only smoke out.json" }, live)).toBe(true);
    expect(holderRunsE2e({ kind: "quiet", pid: process.pid, command: "script/e2e --only smoke out.json" }, live)).toBe(false);
    expect(holderRunsE2e({ pid: process.pid, command: "node scripts/gpui-e2e-light.mjs && measure/series.sh" }, live)).toBe(false);
    expect(holderRunsE2e({ pid: process.pid, command: "script/e2e" }, live)).toBe(true);
    expect(holderRunsE2e({ pid: process.pid, command: "gpui/script/e2e --only smoke out.json" }, live)).toBe(true);
    expect(holderRunsE2e({ pid: process.pid, command: "script/e2e-light" }, live)).toBe(false);
    expect(holderRunsE2e({ pid: process.pid }, live)).toBe(false);
    expect(holderRunsE2e({ kind: "e2e", pid: process.pid }, () => [])).toBe(false);
    const gone = spawnSync("sh", ["-c", "echo $$"], { encoding: "utf8" }).stdout.trim();
    expect(holderRunsE2e({ kind: "e2e", pid: Number(gone) }, () => [Number(gone)])).toBe(false);
    expect(holderRunsE2e(null)).toBe(false);
  });

  it("marks the e2e stages as e2e holders and the WebKit benchmark as quiet", () => {
    const kind = (name) => { const args = holdingInappLock({ name, command: "true", timeout: 1 }).args; return args[args.indexOf("--kind") + 1]; };
    expect([kind("gpui-e2e"), kind("gpui-main"), kind("webkit-bench")]).toEqual(["e2e", "e2e", "quiet"]);
  });

  it("runs a rescue only after the stage it rescues failed, and clears that failure when it passes", async () => {
    const stage = (name, ok, extra = {}) => ({ name, why: name, command: "sh", args: ["-c", ok ? "true" : "false"], timeout: 10_000, ...extra });
    expect(await runStages([stage("e2e", true), stage("rescue", false, { rescues: "e2e" })])).toBe(0);
    expect(await runStages([stage("e2e", false), stage("rescue", true, { rescues: "e2e" })])).toBe(0);
    expect(await runStages([stage("e2e", false), stage("rescue", false, { rescues: "e2e" })])).toBe(2);
  });

  it("runs the whole e2e before the main baseline when an ancestor has one, and records it first when none has", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "ci-local-baseline-"));
    const saved = process.env.LATTICE_GPUI_BUILD_DIR;
    process.env.LATTICE_GPUI_BUILD_DIR = dir;
    try {
      const head = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repo, encoding: "utf8" }).stdout.trim();
      const parent = spawnSync("git", ["rev-parse", "HEAD~1"], { cwd: repo, encoding: "utf8" }).stdout.trim();
      const files = ["gpui/crates/lattice-gpui/src/main.rs"];
      const order = () => localStages(ciTier(files), files, head, { full: true, modules: [] }).filter((stage) => ["gpui-main", "gpui-e2e"].includes(stage.name));
      expect(order().map((stage) => [stage.name, stage.rescues])).toEqual([["gpui-main", undefined], ["gpui-e2e", undefined]]);
      spawnSync("mkdir", ["-p", path.join(dir, "main-baseline")]);
      writeFileSync(path.join(dir, "main-baseline", `known-failures@${parent}.json`), JSON.stringify({ sha: parent, e2e: { failing: [] } }));
      expect(order().map((stage) => [stage.name, stage.rescues])).toEqual([["gpui-e2e", undefined], ["gpui-main", "gpui-e2e"]]);
      writeFileSync(path.join(dir, "main-baseline", `known-failures@${head}.json`), JSON.stringify({ sha: head, e2e: { failing: [] } }));
      expect(order().map((stage) => [stage.name, stage.skip ? "skip" : stage.rescues])).toEqual([["gpui-main", "skip"], ["gpui-e2e", undefined]]);
    } finally {
      if (saved === undefined) delete process.env.LATTICE_GPUI_BUILD_DIR;
      else process.env.LATTICE_GPUI_BUILD_DIR = saved;
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("runs the docs-only profile without a stage that opens a window, and builds", () => {
    const head = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repo, encoding: "utf8" }).stdout.trim();
    const names = (files) => docsStages(files, head).stages.filter((stage) => !stage.skip).map((stage) => stage.name);
    for (const files of [["docs/agent-validation.md"], ["src/release-notes/0.20.0.json", "src/locales/zh-CN/messages.po"], ["gpui/crates/lattice-i18n/locales/zh-CN/app.po"]]) {
      const run = names(files);
      for (const windowed of ["gpui-e2e", "gpui-main", "webkit-bench"]) expect(run, files.join(", ")).not.toContain(windowed);
      expect(run.some((name) => name === "build" || name === "bundle"), files.join(", ")).toBe(true);
    }
    expect(names(["gpui/crates/lattice-i18n/locales/zh-CN/app.po"])).toEqual(expect.arrayContaining(["gpui-build", "gpui-test"]));
  });
});
