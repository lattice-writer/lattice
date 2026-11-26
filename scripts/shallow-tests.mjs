#!/usr/bin/env node
/**
 * `node scripts/shallow-tests.mjs <test file>...`: runs those Vitest files in a
 * one-commit copy of this checkout with no remote, as CI's test job sees a tag
 * (actions/checkout's depth 1, no origin/main). A scripts test that reads
 * HEAD~1 or origin/main passes in every Mac checkout, which has the history,
 * and fails there: two did on v0.20.1 (lattice#1339).
 *
 * The copy is the working tree as it stands, uncommitted and untracked files
 * included and ignored ones left out, committed once in a scratch repository
 * that borrows this checkout's node_modules. The tests run without the global
 * and system git config, as a runner has none (a commit with no identity
 * fails there). gate:changed runs it for the scripts tests near a change
 * (gateStages).
 */
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tests = process.argv.slice(2);
if (!tests.length) {
  console.error("usage: node scripts/shallow-tests.mjs <test file>...");
  process.exit(2);
}
const scratch = mkdtempSync(path.join(tmpdir(), "shallow-tests-"));
let status = 1;
try {
  // The working tree as a tree object, through an index of its own so the
  // checkout's staging is left as it is.
  const index = { ...process.env, GIT_INDEX_FILE: path.join(scratch, "index") };
  execFileSync("git", ["read-tree", "HEAD"], { cwd: repo, env: index });
  execFileSync("git", ["add", "-A"], { cwd: repo, env: index });
  const tree = execFileSync("git", ["write-tree"], { cwd: repo, env: index, encoding: "utf8" }).trim();
  const copy = path.join(scratch, "copy");
  execFileSync("sh", ["-c", 'mkdir "$1" && git archive "$2" | tar -x -C "$1"', "sh", copy, tree], { cwd: repo });
  // A GIT_DIR, GIT_WORK_TREE or the like from a hook would point the copy at this checkout.
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("GIT_")));
  Object.assign(env, { GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_NOSYSTEM: "1" });
  const git = (...args) => execFileSync("git", ["-c", "user.name=shallow-tests", "-c", "user.email=shallow-tests@localhost", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", ...args], { cwd: copy, env, stdio: "ignore" });
  git("init", "-q");
  git("add", "-A");
  git("commit", "-q", "-m", "the working tree, alone");
  symlinkSync(path.join(repo, "node_modules"), path.join(copy, "node_modules"));
  const run = spawnSync(path.join(copy, "node_modules", ".bin", "vitest"), ["run", ...tests], { cwd: copy, env, stdio: "inherit" });
  if (run.error) console.error(`shallow-tests: ${run.error.message} (is node_modules installed?)`);
  status = run.status ?? 1;
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
process.exit(status);
