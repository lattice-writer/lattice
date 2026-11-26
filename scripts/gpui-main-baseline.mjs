#!/usr/bin/env node
/**
 * `node scripts/gpui-main-baseline.mjs`: records what is already red on a main
 * commit, so a validation run on a branch can tell a failure it caused from
 * one it inherited, without building and running main itself.
 *
 * Every lane's test step re-proved the same red main: knip's unused exports
 * and two Settings e2e steps failed for every branch at once, and three lanes
 * each spent a test round and a needs-decision on them while a fourth re-ran
 * the whole e2e on main to show they were pre-existing. This runs those
 * checks once per merge instead, in a checkout of main:
 *
 *   1. `pnpm gate:changed --base <sha>`, which on main itself runs the stages
 *      that always run (lint with the warning cap, knip, typecheck and the
 *      invariant tests);
 *   2. the release build with `offscreen-shots` through
 *      `scripts/gpui-build.mjs`, under the machine-wide build lock;
 *   3. the full `e2e` scenario through `gpui/script/e2e`, on a scratch copy of
 *      `gpui/fixtures/e2e` with a scratch HOME, taking the machine-wide input
 *      lock around its foreground steps as every run does.
 *
 * and writes, into the shared baseline directory
 * (`$LATTICE_GPUI_BUILD_DIR/main-baseline`, by default
 * ~/Library/Caches/lattice-gpui-build/main-baseline on macOS):
 *
 *   known-failures@<sha>.json  the failing gate stages and e2e step names
 *                              (the quarantined ones among them listed again), the
 *                              build's outcome, and how long each part took;
 *   e2e@<sha>.json             the scenario's whole result;
 *   logs@<sha>/                each part's full output;
 *   latest.json                the newest commit's known-failures record, with
 *                              `file` naming it; it moves only forward, to a
 *                              commit that descends from the one it names.
 *
 * `scripts/gpui-main-baseline-agent.mjs install` runs this by itself, as a
 * launchd agent, once main has moved (docs/agent-validation.md).
 *
 * A step whose name is in `e2e.failing` (or a gate stage in `gate.failing`)
 * for the merge base of a branch is pre-existing on main.
 *
 * It refuses a checkout with uncommitted or untracked files, since the build
 * mirrors them and the record would no longer be of the commit. With
 * `--sha REF` it first checks REF out (detached) and installs the frontend's
 * dependencies; without it, it records HEAD.
 *
 * Options:
 *   --sha REF    record REF instead of HEAD (fetches origin first)
 *   --out DIR    write into DIR instead of the shared baseline directory
 *   --skip-e2e   record the gate and the build only
 *   --chunks N   run the e2e as N chunks of modules (`--only`, each with the
 *                smoke flow), each under scripts/inapp-lock and letting it go
 *                between chunks, so a queued gate or lab run gets the screen
 *                in between instead of waiting out the whole scenario
 *                (scripts/gpui-nightly.mjs); the chunks' results are merged
 *                into the one e2e@<sha>.json (mergeChunks)
 */
import { execFileSync, spawn } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import { readModules } from "./gpui-e2e-modules.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const git = (...args) => execFileSync("git", args, { cwd: repo, encoding: "utf8" }).trim();

function parseArgs(argv) {
  const options = { sha: null, out: null, e2e: true, chunks: 0 };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--sha") options.sha = argv[++index];
    else if (arg === "--out") options.out = argv[++index];
    else if (arg === "--skip-e2e") options.e2e = false;
    else if (arg === "--chunks") options.chunks = Number(argv[++index]);
    else throw new Error(`unknown option ${arg}`);
  }
  return options;
}

export function baselineDir(env = process.env) {
  const root = env.LATTICE_GPUI_BUILD_DIR || (process.platform === "darwin" ? path.join(os.homedir(), "Library", "Caches", "lattice-gpui-build") : path.join(os.homedir(), ".cache", "lattice-gpui-build"));
  return path.join(root, "main-baseline");
}

/** The gate's stage lines: `  <name>  <verdict>  ...`, as gate-changed.mjs prints them. */
export function parseGate(output) {
  const stages = [];
  for (const line of output.split("\n")) {
    const match = /^ {2}(\S+)\s+(ok|FAILED|TIMED OUT after \d+ s|skipped)\s+(?:(\d+(?:\.\d+)?) s)?/.exec(line);
    if (!match) continue;
    stages.push({ name: match[1], verdict: match[2] === "ok" || match[2] === "skipped" ? match[2] : "failed", detail: match[2], seconds: match[3] ? Number(match[3]) : null });
  }
  return stages;
}

/**
 * The failing steps of an e2e result, by name. A scenario that stopped early
 * still lists the steps it recorded before it stopped (with a failed step
 * saying so), and `error` says why; one that never got to a step has `error`
 * alone.
 */
export function parseE2e(result) {
  if (!result) return { passed: false, total: 0, failing: [], error: "no result was written" };
  const steps = Array.isArray(result.steps) ? result.steps : [];
  const parsed = {
    passed: result.passed === true && !result.error,
    total: steps.length,
    failing: steps.filter((step) => step.passed !== true).map((step) => step.step),
  };
  if (result.error) parsed.error = String(result.error);
  return parsed;
}

/** `modules` in at most `count` contiguous chunks of near-equal size. */
export function chunkModules(modules, count) {
  const size = Math.max(1, Math.ceil(modules.length / Math.max(1, count)));
  return Array.from({ length: Math.ceil(modules.length / size) }, (_, index) => modules.slice(index * size, (index + 1) * size));
}

/**
 * One e2e result from the chunks' results: each step once, in the order it
 * first ran, failing when it failed in any chunk (the smoke flow runs in
 * every one). A chunk with no result or an `error` makes the whole an error,
 * so a chunk that never ran is never read as a pass.
 */
export function mergeChunks(results) {
  const steps = new Map();
  const errors = [];
  results.forEach((result, index) => {
    if (!result) errors.push(`chunk ${index + 1} wrote no result`);
    else if (result.error) errors.push(`chunk ${index + 1}: ${result.error}`);
    for (const step of result?.steps ?? []) {
      const seen = steps.get(step.step);
      if (!seen || (seen.passed === true && step.passed !== true)) steps.set(step.step, step);
    }
  });
  const merged = { scenario: "e2e", passed: errors.length === 0 && results.every((result) => result?.passed === true), modules: null, steps: [...steps.values()] };
  if (errors.length) merged.error = errors.join("; ");
  return merged;
}

/** What the chunks' save-on-quit checks add up to: lost if any lost, saved only if every one saved. */
export function mergeQuits(quits) {
  if (quits.includes("lost")) return "lost";
  return quits.length && quits.every((quit) => quit === "saved") ? "saved" : "unknown";
}

/**
 * What one chunk's run left: its result when the run wrote one now, else an
 * error naming why. Files from an earlier run of the same sha are removed
 * before each chunk, and a chunk that gave up on the lock is an error
 * whatever is on disk, so neither is ever read as tonight's pass.
 */
export function chunkResult(chunkRun, text) {
  if (chunkRun.code === 75) return { error: "the in-app lock was not free in time" };
  if (text === null) return chunkRun.code === 0 ? null : { error: `the chunk exited with ${chunkRun.code ?? chunkRun.status} and wrote no result` };
  try {
    return JSON.parse(text);
  } catch {
    return { error: "the chunk's result is not JSON" };
  }
}

// How long all the chunks may take, lock waits included: the workflow step's
// 555 minutes less the gate's and the build's timeouts (45 + 90), the
// rerun's lock wait and run (30 + 15), the idle guard's lock wait and runs
// (30 + 35) and the whole Vitest suite's 90 (scripts/gpui-nightly.mjs).
const CHUNKS_BUDGET_MS = 220 * 60_000;
// A chunk's own run, at most; a lock wait leaves this much of the budget.
const CHUNK_RUN_MS = 25 * 60_000;

/** The whole scenario in chunks, each under the in-app lock (`--chunks`); resolves like `run`, having written `resultFile`. */
async function runChunks(count, { sha, resultFile, logs }) {
  const chunks = chunkModules([...readModules(repo).keys()], count);
  const started = Date.now();
  const deadline = started + CHUNKS_BUDGET_MS;
  const results = [];
  const quits = [];
  const outputs = [];
  let ok = true;
  for (const [index, chunk] of chunks.entries()) {
    // inapp-lock's waiters try every second: the pause lets one in.
    if (index) await sleep(10_000);
    const chunkFile = path.join(logs, `e2e-chunk-${index + 1}.json`);
    const quitFile = `${chunkFile}.quit`;
    const label = `gpui-main-baseline ${sha.slice(0, 9)} e2e ${index + 1}/${chunks.length}`;
    rmSync(chunkFile, { force: true });
    rmSync(quitFile, { force: true });
    const waitS = Math.floor((deadline - Date.now() - CHUNK_RUN_MS) / 1000);
    if (waitS < 0) {
      ok = false;
      results.push({ error: `the nightly ran out of time before chunk ${index + 1}` });
      quits.push("unknown");
      continue;
    }
    console.log(`gpui-main-baseline: e2e chunk ${index + 1}/${chunks.length}: ${chunk.join(", ")}`);
    const chunkRun = await run(path.join(repo, "scripts", "inapp-lock"), ["--thread", label, "--wait", String(waitS), "--kind", "e2e", "--", path.join(repo, "gpui", "script", "e2e"), "--only", chunk.join(","), chunkFile], {
      log: path.join(logs, `e2e-${index + 1}.log`),
      timeoutMs: waitS * 1000 + CHUNK_RUN_MS,
      env: { ...process.env, LATTICE_E2E_QUIT_RESULT: quitFile },
    });
    outputs.push(chunkRun.output);
    ok &&= chunkRun.status === "ok";
    results.push(chunkResult(chunkRun, existsSync(chunkFile) ? readFileSync(chunkFile, "utf8") : null));
    quits.push(existsSync(quitFile) ? readFileSync(quitFile, "utf8").trim() : "unknown");
  }
  writeJson(resultFile, mergeChunks(results));
  if (process.env.LATTICE_E2E_QUIT_RESULT) writeFileSync(process.env.LATTICE_E2E_QUIT_RESULT, `${mergeQuits(quits)}\n`);
  return { status: ok ? "ok" : "failed", code: ok ? 0 : 1, seconds: Math.round((Date.now() - started) / 1000), output: outputs.join("\n") };
}

/** Runs a command, teeing its output to the terminal and to `log`; resolves with its status and output. */
function run(command, args, { log, cwd = repo, timeoutMs, env = process.env }) {
  return new Promise((resolve, reject) => {
    const started = Date.now();
    const file = createWriteStream(log);
    const child = spawn(command, args, { cwd, env, stdio: ["ignore", "pipe", "pipe"] });
    let output = "";
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGTERM");
    }, timeoutMs);
    for (const stream of [child.stdout, child.stderr]) {
      stream.on("data", (chunk) => {
        (stream === child.stdout ? process.stdout : process.stderr).write(chunk);
        file.write(chunk);
        output += chunk.toString("utf8");
      });
    }
    child.on("error", reject);
    child.on("close", (code) => {
      clearTimeout(timer);
      file.end();
      resolve({ status: timedOut ? "timed out" : code === 0 ? "ok" : "failed", code, seconds: Math.round((Date.now() - started) / 1000), output });
    });
  });
}

function writeJson(file, value) {
  const temporary = `${file}.tmp-${process.pid}`;
  writeFileSync(temporary, `${JSON.stringify(value, null, 2)}\n`);
  renameSync(temporary, file);
}

function isAncestor(older, newer) {
  try {
    execFileSync("git", ["merge-base", "--is-ancestor", older, newer], { cwd: repo, stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const dirty = git("status", "--porcelain", "--untracked-files=normal");
  if (dirty) throw new Error(`${repo} has uncommitted or untracked files; run this in a clean checkout of main`);
  if (options.sha) {
    execFileSync("git", ["fetch", "--quiet", "origin"], { cwd: repo, stdio: "inherit" });
    execFileSync("git", ["checkout", "--quiet", "--detach", options.sha], { cwd: repo, stdio: "inherit" });
    execFileSync("pnpm", ["install", "--frozen-lockfile", "--prefer-offline"], { cwd: repo, stdio: "inherit" });
  }
  const sha = git("rev-parse", "HEAD");
  const out = options.out ? path.resolve(options.out) : baselineDir();
  const logs = path.join(out, `logs@${sha}`);
  mkdirSync(logs, { recursive: true });
  const started = new Date();
  console.log(`gpui-main-baseline: ${sha.slice(0, 9)} ${git("log", "-1", "--format=%s", sha)} -> ${out}`);

  const gateRun = await run("pnpm", ["gate:changed", "--base", sha], { log: path.join(logs, "gate.log"), timeoutMs: 45 * 60_000 });
  const stages = parseGate(gateRun.output);
  const gate = { status: gateRun.status, seconds: gateRun.seconds, failing: stages.filter((stage) => stage.verdict === "failed").map((stage) => stage.name), stages };

  const buildRun = await run("node", ["scripts/gpui-build.mjs"], { log: path.join(logs, "build.log"), timeoutMs: 90 * 60_000 });
  const build = { status: buildRun.status, seconds: buildRun.seconds };

  let e2e = { status: "not run", reason: options.e2e ? "the build failed" : "--skip-e2e" };
  if (options.e2e && buildRun.status === "ok") {
    const resultFile = path.join(out, `e2e@${sha}.json`);
    rmSync(resultFile, { force: true });
    const e2eRun = options.chunks ? await runChunks(options.chunks, { sha, resultFile, logs }) : await run(path.join(repo, "gpui", "script", "e2e"), [resultFile], { log: path.join(logs, "e2e.log"), timeoutMs: 60 * 60_000 });
    let result = null;
    try {
      result = JSON.parse(readFileSync(resultFile, "utf8"));
    } catch {
      // Missing or cut short; parseE2e records that.
    }
    const parsed = parseE2e(result);
    // The quarantined steps (gpui-e2e-quarantine.json) main fails too, which
    // a branch's light pass then counts as pre-existing.
    let quarantine = [];
    try {
      quarantine = JSON.parse(readFileSync(path.join(repo, "scripts", "gpui-e2e-quarantine.json"), "utf8")).steps;
    } catch {
      // A commit from before the quarantine list.
    }
    parsed.quarantined_failing = parsed.failing.filter((step) => quarantine.includes(step));
    e2e = { status: e2eRun.status, seconds: e2eRun.seconds, ...parsed, result: existsSync(resultFile) ? path.basename(resultFile) : null };
    // script/e2e also checks after the app quits that the typed text was saved.
    if (e2eRun.status !== "ok" && parsed.passed) e2e.after_quit = e2eRun.output.split("\n").filter((line) => line.startsWith("e2e:")).join("\n");
  }

  const record = {
    sha,
    subject: git("log", "-1", "--format=%s", sha),
    committed: git("log", "-1", "--format=%cI", sha),
    started: started.toISOString(),
    finished: new Date().toISOString(),
    host: os.hostname(),
    green: gate.status === "ok" && build.status === "ok" && (e2e.status === "ok" || !options.e2e),
    gate,
    build,
    e2e,
    logs: path.basename(logs),
  };
  const file = path.join(out, `known-failures@${sha}.json`);
  writeJson(file, record);

  const latestFile = path.join(out, "latest.json");
  let latest = null;
  try {
    latest = JSON.parse(readFileSync(latestFile, "utf8"));
  } catch {
    // None yet.
  }
  if (!latest?.sha || latest.sha === sha || isAncestor(latest.sha, sha)) writeJson(latestFile, { ...record, file: path.basename(file) });
  else console.log(`gpui-main-baseline: latest.json stays at ${latest.sha.slice(0, 9)}, which ${sha.slice(0, 9)} does not descend from`);

  console.log(`gpui-main-baseline: ${record.green ? "green" : "red"}: gate failing [${gate.failing.join(", ")}], build ${build.status}, e2e ${e2e.status}${e2e.failing?.length ? ` failing ${e2e.failing.length}/${e2e.total}` : ""}`);
  console.log(`gpui-main-baseline: ${file}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main().catch((error) => {
    console.error(`gpui-main-baseline: ${error.message}`);
    process.exit(1);
  });
}
