// @vitest-environment node
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, utimesSync } from "node:fs";
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

  it("serves waiters in the order they came, and skips one that died waiting", async () => {
    const log = path.join(dir, "log");
    expect(lock("--thread", "A", "hold", "30").status).toBe(0);
    const queued = () => (existsSync(`${env.INAPP_LOCK_FILE}.queue`) ? readdirSync(`${env.INAPP_LOCK_FILE}.queue`).filter((name) => !name.startsWith(".")).length : 0);
    const waiter = (name) => {
      const child = spawn(script, ["--thread", name, "--wait", "30", "--", "sh", "-c", `echo ${name} >> "$1"`, "sh", log], { env });
      return { child, done: new Promise((resolve) => child.on("exit", resolve)) };
    };
    // Each joins the line once the one before it is in it.
    const waiting = [];
    for (const name of ["B", "C", "D", "E"]) {
      waiting.push(waiter(name));
      const deadline = Date.now() + 10_000;
      while (queued() < waiting.length) {
        if (Date.now() > deadline) throw new Error(`${name} never queued`);
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
    }
    waiting[1].child.kill("SIGKILL");
    expect(lock("--thread", "A", "release").status).toBe(0);
    expect(await Promise.all(waiting.map(({ done }) => done))).toEqual([0, null, 0, 0]);
    expect(readFileSync(log, "utf8").trim().split("\n")).toEqual(["B", "D", "E"]);
    expect(queued()).toBe(0);
  });

  it("passes over a waiter that stopped polling", async () => {
    const log = path.join(dir, "log");
    expect(lock("--thread", "A", "hold", "30").status).toBe(0);
    const queue = `${env.INAPP_LOCK_FILE}.queue`;
    const stopped = spawn(script, ["--thread", "S", "--wait", "60", "--", "sh", "-c", 'echo S >> "$1"', "sh", log], { env });
    const deadline = Date.now() + 10_000;
    while (!(existsSync(queue) && readdirSync(queue).some((name) => !name.startsWith(".")))) {
      if (Date.now() > deadline) throw new Error("S never queued");
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    stopped.kill("SIGSTOP");
    const ticket = path.join(queue, readdirSync(queue).find((name) => !name.startsWith(".")));
    utimesSync(ticket, new Date(Date.now() - 60_000), new Date(Date.now() - 60_000));
    expect(lock("--thread", "A", "release").status).toBe(0);
    expect(lock("--thread", "B", "--wait", "5", "--", "sh", "-c", 'echo B >> "$1"', "sh", log).status).toBe(0);
    stopped.kill("SIGCONT");
    expect(await new Promise((resolve) => stopped.on("exit", resolve))).toBe(0);
    expect(readFileSync(log, "utf8").trim().split("\n")).toEqual(["B", "S"]);
  });

  it("keeps a hold after the process group that started it is killed", () => {
    const starter = spawn("sh", ["-c", `"${script}" --thread A hold 30; sleep 30`], { env, detached: true });
    const deadline = Date.now() + 10_000;
    // `status`, not a try with --wait 0: that gives up while the hold is
    // still in line ahead of it, before the hold has the lock.
    while (lock("status").status !== 1) {
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
    // Each next taker starts once the one before it has begun, so a loaded
    // machine's slow process start cannot reorder them.
    const begun = async (name) => {
      const deadline = Date.now() + 20_000;
      while (!(existsSync(log) && readFileSync(log, "utf8").includes(`start ${name}`))) {
        if (Date.now() > deadline) throw new Error(`${name} never started`);
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
    };
    const e2e = start(["--thread", "E", "--kind", "e2e", "--", "sh", "-c", note("e2e", 2), "sh", log]);
    await begun("e2e");
    const builds = start(["build", "--", "sh", "-c", note("build", 3), "sh", log]);
    await begun("build");
    const quiet = start(["--thread", "Q", "--", "sh", "-c", note("quiet", 1), "sh", log]);
    await begun("quiet");
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
