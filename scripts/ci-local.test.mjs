// @vitest-environment node
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { docsStages, holdingInappLock } from "./ci-local.mjs";

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
