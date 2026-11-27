#!/usr/bin/env node
/**
 * `node scripts/gpui-e2e-modules.mjs`: which feature modules of the GPUI
 * `e2e` scenario a branch's change touches, for the no-mistakes test step's
 * light pass (`scripts/gpui-e2e-light.mjs`, which runs
 * `gpui/script/e2e --only <list>`: the smoke flow and those modules; see
 * `gpui/crates/lattice-gpui/src/scenario/e2e/registry.rs`).
 *
 * The full scenario had grown to over 200 steps and the test step re-ran it
 * whole on every round, under the machine-wide input lock, after the worker
 * had already run it on the same head; the full run now happens once per
 * merge, in the main baseline (`scripts/gpui-main-baseline.mjs`).
 *
 * The modules are read from the scenario's own files, as `build.rs` reads
 * them: a file under `scenario/e2e/` (or a folder's `mod.rs`) holding a
 * `pub(super) const MODULE` is the module named after it, and its `covers`
 * lists the app source it exercises, as path prefixes. So a new module is
 * one new file, with nothing to add here.
 *
 * A changed path picks a module when it is the module's own scenario file
 * (`scenario/e2e/<module>.rs` or a file under `scenario/e2e/<module>/`), or
 * when it starts with one of the module's `covers`. Anything else, a shared
 * file such as `shell.rs` or a dependency bump among them, picks no module:
 * the smoke flow covers what every module runs on, and the baseline covers
 * the rest after the merge.
 *
 * Options:
 *   --base REF   compare against REF's merge base (default origin/main)
 *   --json       print {gpui, modules, why} instead of the --only list
 *   FILE...      classify these paths instead of the branch's diff
 */
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SCENARIO = "gpui/crates/lattice-gpui/src/scenario/e2e/";

/** Each module's file under the scenario folder, `<name>.rs` or `<name>/mod.rs`. */
function moduleFiles(root = repo) {
  const dir = path.join(root, SCENARIO);
  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      const file = path.join(dir, entry.name, "mod.rs");
      if (existsSync(file)) files.push([entry.name, file]);
    } else if (entry.name.endsWith(".rs")) {
      files.push([entry.name.slice(0, -3), path.join(dir, entry.name)]);
    }
  }
  return files;
}

/** The scenario's modules, each with the source prefixes it covers, by name. */
export function readModules(root = repo) {
  const modules = new Map();
  for (const [name, file] of moduleFiles(root)) {
    const source = readFileSync(file, "utf8");
    if (!/^pub\(super\) const MODULE:/m.test(source)) continue;
    const list = source.match(/^\s*covers: &\[([\s\S]*?)\]/m);
    modules.set(name, list ? [...list[1].matchAll(/"([^"]+)"/g)].map((match) => match[1]) : []);
  }
  return new Map([...modules].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
}

const COVERS = readModules();
/** The scenario's modules, by name. */
export const MODULES = [...COVERS.keys()];

/** The modules `files` touch, with the paths that picked each. */
export function modulesFor(files) {
  const why = new Map();
  const pick = (module, file) => {
    if (!why.has(module)) why.set(module, []);
    if (!why.get(module).includes(file)) why.get(module).push(file);
  };
  for (const file of files) {
    if (file.startsWith(SCENARIO)) {
      const stem = file.slice(SCENARIO.length).split("/")[0].replace(/\.rs$/, "");
      if (COVERS.has(stem)) pick(stem, file);
    }
    for (const [module, prefixes] of COVERS) {
      if (prefixes.some((prefix) => file.startsWith(prefix))) pick(module, file);
    }
  }
  const modules = MODULES.filter((module) => why.has(module));
  return { gpui: files.some((file) => file.startsWith("gpui/") || file.startsWith("src-tauri/lattice-core/")), modules, why: Object.fromEntries(modules.map((module) => [module, why.get(module)])) };
}

/** Files the branch changed against `base`'s merge base, committed or not. */
export const KNOWN_FAILURES_FILE = "scripts/gpui-e2e-known-failures.json";

/**
 * The modules a change to the known failures (KNOWN_FAILURES_FILE) needs:
 * those whose scenario files name each step it adds, removes or edits; a
 * step only the smoke flow or the harness checks needs none. `full` when
 * one is named nowhere as written (a step built at run time). `sources`
 * maps scenario files to their text.
 */
export function knownFailuresNeeds(before, after, sources) {
  const entries = (text) => new Map(JSON.parse(text).steps.map((entry) => [entry.step, JSON.stringify(entry)]));
  const [old, now] = [entries(before), entries(after)];
  const changed = [...new Set([...old.keys(), ...now.keys()])].filter((step) => old.get(step) !== now.get(step));
  const files = new Set();
  for (const step of changed) {
    const named = [...sources].filter(([, text]) => text.includes(JSON.stringify(step))).map(([file]) => file);
    if (!named.length) return { full: true, modules: [], why: step };
    for (const file of named) if (file.startsWith(SCENARIO)) files.add(file);
  }
  return { full: false, modules: files.size ? modulesFor([...files]).modules : [], why: changed };
}

/** knownFailuresNeeds for the change from `base` to the working tree, or null when it cannot be read. */
export function knownFailuresDiff(base, root = repo) {
  try {
    const before = execFileSync("git", ["show", base + ":" + KNOWN_FAILURES_FILE], { cwd: root, encoding: "utf8", maxBuffer: 64 << 20 });
    const after = readFileSync(path.join(root, KNOWN_FAILURES_FILE), "utf8");
    const listed = execFileSync("git", ["ls-files", "gpui/crates/lattice-gpui/src/scenario"], { cwd: root, encoding: "utf8" }).split("\n");
    const sources = new Map(listed.filter((file) => file.endsWith(".rs") && existsSync(path.join(root, file))).map((file) => [file, readFileSync(path.join(root, file), "utf8")]));
    return knownFailuresNeeds(before, after, sources);
  } catch {
    return null;
  }
}

export function changedFiles(base) {
  const git = (...args) => execFileSync("git", args, { cwd: repo, encoding: "utf8" });
  const mergeBase = git("merge-base", base, "HEAD").trim();
  const lists = [git("diff", "--name-only", mergeBase), git("ls-files", "--others", "--exclude-standard")];
  return [...new Set(lists.join("\n").split("\n").filter(Boolean))];
}

function main() {
  const argv = process.argv.slice(2);
  let base = "origin/main";
  let json = false;
  const given = [];
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === "--base") base = argv[++index];
    else if (argv[index] === "--json") json = true;
    else given.push(argv[index]);
  }
  const result = modulesFor(given.length ? given : changedFiles(base));
  if (json) {
    console.log(JSON.stringify(result, null, 2));
    return;
  }
  for (const [module, files] of Object.entries(result.why)) console.error(`# ${module}: ${files.join(", ")}`);
  if (!result.gpui) console.error("# no GPUI change: the e2e scenario need not run");
  console.log(result.modules.length ? result.modules.join(",") : "smoke");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
