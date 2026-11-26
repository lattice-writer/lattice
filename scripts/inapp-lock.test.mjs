// @vitest-environment node
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

const script = path.resolve(import.meta.dirname, "inapp-lock");

describe.skipIf(process.platform === "win32")("inapp-lock", () => {
  let dir;
  let env;
  const lock = (...args) => spawnSync(script, args, { env, encoding: "utf8" });
  beforeEach(() => {
    dir = mkdtempSync(path.join(tmpdir(), "inapp-lock-"));
    env = { ...process.env, INAPP_LOCK_FILE: path.join(dir, "lock") };
  });
  afterEach(() => {
    lock("--thread", "A", "release");
    rmSync(dir, { recursive: true, force: true });
  });

  it("runs two concurrent holders one after the other", async () => {
    const log = path.join(dir, "log");
    const runner = (name) =>
      new Promise((resolve) => {
        const child = spawn(script, ["--thread", name, "--", "sh", "-c", `echo start ${name} >> "$1"; sleep 0.5; echo end ${name} >> "$1"`, "sh", log], { env });
        child.on("exit", resolve);
      });
    expect(await Promise.all([runner("A"), runner("B")])).toEqual([0, 0]);
    const lines = readFileSync(log, "utf8").trim().split("\n");
    expect(lines).toHaveLength(4);
    expect(lines[1]).toBe(lines[0].replace("start", "end"));
    expect(lines[3]).toBe(lines[2].replace("start", "end"));
  });

  it("keeps a hold after the process group that started it is killed", () => {
    const starter = spawn("sh", ["-c", `"${script}" --thread A hold 30; sleep 30`], { env, detached: true });
    const deadline = Date.now() + 10_000;
    while (lock("--wait", "0", "--", "true").status !== 75) {
      if (Date.now() > deadline) throw new Error("the hold never took the lock");
    }
    process.kill(-starter.pid, "SIGKILL");
    expect(lock("--thread", "B", "--wait", "1", "--", "true").status).toBe(75);
    expect(lock("--thread", "A", "release").status).toBe(0);
    expect(lock("--thread", "B", "--wait", "1", "--", "true").status).toBe(0);
  });

  it("returns the run's status and frees the lock when its process group may not be signalled", () => {
    // macOS has refused killpg with EPERM once a long run ended; stand that in.
    const refuse = "import errno, os, runpy, sys\n" +
      "def killpg(pgid, sig): raise PermissionError(errno.EPERM, 'Operation not permitted')\n" +
      "os.killpg = killpg\n" +
      "sys.argv = sys.argv[1:]\n" +
      "runpy.run_path(sys.argv[0], run_name='__main__')\n";
    const run = spawnSync("python3", ["-c", refuse, script, "--thread", "A", "--", "sh", "-c", "exit 3"], { env, encoding: "utf8" });
    expect(run.stderr).not.toContain("Traceback");
    expect(run.status).toBe(3);
    expect(lock("--thread", "B", "--wait", "0", "--", "true").status).toBe(0);
  });

  it("starts a quiet holder only once the builds running beside an e2e end, and holds new builds off", async () => {
    const log = path.join(dir, "log");
    const note = (name, seconds) => `echo start ${name} >> "$1"; sleep ${seconds}; echo end ${name} >> "$1"`;
    const start = (args) => new Promise((resolve) => spawn(script, args, { env }).on("exit", resolve));
    const e2e = start(["--thread", "E", "--kind", "e2e", "--", "sh", "-c", note("e2e", 0.3), "sh", log]);
    execFileSync("sleep", ["0.1"]);
    const builds = start(["build", "--", "sh", "-c", note("build", 1.5), "sh", log]);
    execFileSync("sleep", ["0.1"]);
    const quiet = start(["--thread", "Q", "--", "sh", "-c", note("quiet", 1), "sh", log]);
    execFileSync("sleep", ["2"]);
    const late = start(["build", "--", "sh", "-c", note("late", 0.1), "sh", log]);
    expect(await Promise.all([e2e, builds, quiet, late])).toEqual([0, 0, 0, 0]);
    const lines = readFileSync(log, "utf8").trim().split("\n");
    expect(lines.indexOf("start quiet")).toBeGreaterThan(lines.indexOf("end build"));
    expect(lines.indexOf("start late")).toBeGreaterThan(lines.indexOf("end quiet"));
    expect(lines.indexOf("start build")).toBeLessThan(lines.indexOf("end e2e"));
  });

  it("lets the builds lock go when the build ends, whatever it leaves running", () => {
    const daemon = spawnSync(script, ["build", "--", "sh", "-c", "(sleep 5 &) ; exit 2"], { env, encoding: "utf8" });
    expect(daemon.status).toBe(2);
    expect(lock("--thread", "Q", "--wait", "1", "--", "true").status).toBe(0);
  });

  it("runs a build under a holder of the lock at once", () => {
    expect(lock("--thread", "A", "--", script, "build", "--", "true").status).toBe(0);
  });

  it("keeps the lock when someone deletes and recreates its file", () => {
    expect(lock("--thread", "A", "hold", "30").status).toBe(0);
    rmSync(env.INAPP_LOCK_FILE);
    execFileSync("sleep", ["1.5"]);
    expect(lock("--thread", "B", "--wait", "1", "--", "true").status).toBe(75);
  });
});
