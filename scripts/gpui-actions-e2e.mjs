/**
 * The e2e a change to the shell's command bindings needs
 * (gpui/crates/lattice-gpui/src/shell/actions.rs), read hunk by hunk instead
 * of file by file. The file is about a hundred one-line `.on_action(...)`
 * bindings, `command_context` (what the command table knows about the
 * window) and a few small handlers, so taking the whole scenario for any edit
 * (E2E_TABLE's FULL row, which still applies when the diff cannot be read)
 * ran 45 to 60 minutes for a new binding or two.
 *
 * Each changed line, on either side of the diff, is placed in the statement
 * it belongs to:
 *
 *   - a `use` or a comment: nothing beyond the smoke flow, since the build
 *     checks an import;
 *   - an `.on_action(...)` binding, or an entry of the `switches!` list: the
 *     modules whose scenario names one of its actions or presses the key
 *     keymap.rs binds to it, and those whose covers hold the shell file that
 *     defines a method the binding calls (`this.sync_overleaf`) or the crate
 *     module it calls into (`crate::settings::update`);
 *   - `command_context`: the palette and the menus (`palette`, `shell_lifecycle`),
 *     the modules covering a shell file that builds commands or a sheet from
 *     it, the modules whose steps read it themselves, and what its changed
 *     line calls;
 *   - another handler in the file (`close_tab`): what the bindings that call
 *     it pick, and the modules whose steps call it directly.
 *
 * The smoke flow's own steps (scenario/shell_steps*, scenario/e2e.rs) naming
 * an action count as the smoke flow's, which every gate runs. A statement
 * that picks nothing is a new binding no step can reach when every line of it
 * was added, and needs the whole scenario otherwise; so does any other line.
 */
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SRC = "gpui/crates/lattice-gpui/src/";
export const ACTIONS_FILE = `${SRC}shell/actions.rs`;
const SCENARIO = `${SRC}scenario/`;
const NOT_ACTIONS = new Set(["App", "Context", "Window", "Path", "Self"]);
const ON_ACTION = /^\s*\.on_action\(/;
const SWITCH = /^\s*[A-Z]\w* => \d+,?\s*$/;
// The smoke flow's own steps; parity, mocks and the registry are not it.
const SMOKE = /^gpui\/crates\/lattice-gpui\/src\/scenario\/(shell_steps(\.rs|\/)|e2e\.rs$)/;

function rustFiles(root, dir) {
  const full = path.join(root, dir);
  if (!existsSync(full)) return [];
  return readdirSync(full, { withFileTypes: true, recursive: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
    .map((entry) => path.relative(root, path.join(entry.parentPath, entry.name)));
}

/** What the mapping reads from the tree: the scenario's sources, keymap.rs's keys and the shell's methods. */
export function readContext(root = repo) {
  const read = (file) => readFileSync(path.join(root, file), "utf8");
  const scenario = rustFiles(root, SCENARIO).map((file) => ({ file, text: read(file) }));
  const keys = new Map();
  for (const match of read(`${SRC}shell/commands/keymap.rs`).matchAll(/KeyBinding::new\(\s*(?:m!\("([^"]+)"\)|"([^"]+)")\s*,\s*(?:[a-z_]+::)*([A-Z]\w*)/g)) {
    keys.set(match[3], [...(keys.get(match[3]) ?? []), match[1] ? `cmd-${match[1]}` : match[2]]);
  }
  const methods = new Map();
  const contextReaders = [];
  for (const file of rustFiles(root, `${SRC}shell`)) {
    if (file === ACTIONS_FILE) continue;
    const text = read(file);
    for (const match of text.matchAll(/\bfn (\w+)\b/g)) methods.set(match[1], [...(methods.get(match[1]) ?? []), file]);
    if (/\bcommand_context\(|\bCommandContext\b/.test(text)) contextReaders.push(file);
  }
  return { scenario, keys, methods, contextReaders };
}

/** The new and old line numbers a `git diff -U0` changes. */
export function changedLines(diff) {
  const added = [];
  const removed = [];
  for (const match of diff.matchAll(/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/gm)) {
    const [oldStart, oldCount, newStart, newCount] = [match[1], match[2] ?? "1", match[3], match[4] ?? "1"].map(Number);
    for (let line = 0; line < oldCount; line += 1) removed.push(oldStart + line);
    for (let line = 0; line < newCount; line += 1) added.push(newStart + line);
  }
  return { added, removed };
}

/** The statement line `number` (1-based) of actions.rs is in: its kind and its text. */
export function statementAt(lines, number) {
  const index = number - 1;
  const text = lines[index] ?? "";
  if (/^\s*(\/\/|$)/.test(text)) return { kind: "comment" };
  let fnStart = -1;
  let fn = null;
  for (let at = index; at >= 0 && fnStart < 0; at -= 1) {
    const found = /^ {4}(?:pub(?:\([\w:]+\) )?)?fn (\w+)/.exec(lines[at]);
    if (found) [fnStart, fn] = [at, found[1]];
    else if (/^impl\b/.test(lines[at])) break;
  }
  if (!fn) {
    for (let at = index; at >= 0; at -= 1) {
      if (/^(pub )?use /.test(lines[at])) return { kind: "use" };
      if (at < index && /;\s*$/.test(lines[at])) break;
    }
    return { kind: "other" };
  }
  if (fn === "command_context") return { kind: "context", text };
  if (fn !== "register_commands") return { kind: "handler", fn };
  if (SWITCH.test(text)) return { kind: "binding", text, lines: [number, number] };
  let start = index;
  while (start > fnStart && !ON_ACTION.test(lines[start])) start -= 1;
  if (start === fnStart) return { kind: "other" };
  const indent = (line) => /^\s*/.exec(line)[0].length;
  const closes = (line) => /;\s*$/.test(line) && indent(line) <= indent(lines[start]);
  let end = start;
  while (end + 1 < lines.length && !closes(lines[end]) && !ON_ACTION.test(lines[end + 1]) && !/^\s*\/\//.test(lines[end + 1])) end += 1;
  return index <= end ? { kind: "binding", text: lines.slice(start, end + 1).join("\n"), lines: [start + 1, end + 1] } : { kind: "other" };
}

function picks(text, context, covers, scenarioModule) {
  const modules = new Set();
  let smoke = false;
  const actions = [...text.matchAll(/&(?:[a-z_]+::)*([A-Z]\w*)|^\s*([A-Z]\w*) => \d+/gm)].map((match) => match[1] ?? match[2]).filter((name) => !NOT_ACTIONS.has(name));
  for (const action of new Set(actions)) {
    const words = [new RegExp(`\\b${action}\\b`), ...(context.keys.get(action) ?? []).map((key) => new RegExp(`"${key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"`))];
    for (const { file, text: source } of context.scenario) {
      if (!words.some((word) => word.test(source))) continue;
      const module = scenarioModule(file);
      if (module) modules.add(module);
      else if (SMOKE.test(file)) smoke = true;
    }
  }
  const files = [...text.matchAll(/\b(?:this|self)\.(\w+)\(|Self::(\w+)/g)].flatMap((match) => context.methods.get(match[1] ?? match[2]) ?? []);
  files.push(...[...text.matchAll(/crate::((?:[a-z_]+::)+)[a-z_]+\(/g)].map((match) => SRC + match[1].slice(0, -2).replaceAll("::", "/")));
  for (const file of files) {
    for (const [module, prefixes] of covers) {
      if (prefixes.some((prefix) => file.startsWith(prefix) || prefix.startsWith(`${file}/`) || prefix.startsWith(`${file}.rs`))) modules.add(module);
    }
  }
  return { modules, smoke, actions };
}

/**
 * The modules a diff of actions.rs needs, as `{ full: false, modules, why }`,
 * or `{ full: true, why }` when it needs the whole scenario. `before` and
 * `after` are the file's two texts, `diff` their `git diff -U0`.
 */
export function actionsNeeds({ diff, before, after }, covers, context = readContext()) {
  const scenarioModule = (file) => {
    if (!file.startsWith(`${SCENARIO}e2e/`)) return null;
    const stem = file.slice(`${SCENARIO}e2e/`.length).split("/")[0].replace(/\.rs$/, "");
    return covers.has(stem) ? stem : null;
  };
  const { added, removed } = changedLines(diff);
  const afterLines = after.split("\n");
  const callers = (fn) =>
    afterLines
      .map((line, at) => (new RegExp(`\\.${fn}\\(|Self::${fn}\\b`).test(line) ? statementAt(afterLines, at + 1) : null))
      .filter((statement) => statement?.kind === "binding")
      .map((statement) => statement.text)
      .join("\n");
  const modules = new Set();
  const why = [];
  for (const { lines, numbers, side } of [
    { lines: afterLines, numbers: added, side: "+" },
    { lines: before.split("\n"), numbers: removed, side: "-" },
  ]) {
    for (const number of numbers) {
      const statement = statementAt(lines, number);
      if (statement.kind === "comment" || statement.kind === "use") continue;
      if (statement.kind === "other") return { full: true, why: `line ${side}${number} is outside the bindings, command_context and the handlers` };
      const text = statement.kind === "handler" ? callers(statement.fn) : statement.text;
      const found = picks(text, context, covers, scenarioModule);
      // Steps that call the handler themselves (run_trellis from a step).
      if (statement.kind === "handler") for (const { file, text: source } of context.scenario) if (new RegExp(`\\.${statement.fn}\\(`).test(source) && scenarioModule(file)) found.modules.add(scenarioModule(file));
      // command_context: the palette and the menus, the shell files that build
      // commands or sheets from it, and the steps that read it themselves.
      if (statement.kind === "context") {
        for (const module of ["palette", "shell_lifecycle"]) if (covers.has(module)) found.modules.add(module);
        for (const file of context.contextReaders) for (const [module, prefixes] of covers) if (prefixes.some((prefix) => file.startsWith(prefix))) found.modules.add(module);
        for (const { file, text: source } of context.scenario) if (/\bcommand_context\(|commands::commands\(/.test(source) && scenarioModule(file)) found.modules.add(scenarioModule(file));
      }
      const addedSet = new Set(added);
      const addedWhole = side === "+" && statement.kind === "binding" && Array.from({ length: statement.lines[1] - statement.lines[0] + 1 }, (_, at) => statement.lines[0] + at).every((line) => addedSet.has(line));
      if (!found.modules.size && !found.smoke && !addedWhole) return { full: true, why: `line ${side}${number} maps to no scenario step` };
      for (const module of found.modules) modules.add(module);
      why.push(`${side}${number} ${found.actions.join(", ") || statement.kind}: ${[...found.modules].join(", ") || "smoke"}`);
    }
  }
  return { full: false, modules: [...modules].sort(), why };
}

/** actions.rs against `base`, for actionsNeeds; null when git cannot say. */
export function actionsDiff(base, root = repo) {
  try {
    const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
    const file = path.join(root, ACTIONS_FILE);
    return { diff: git("diff", "-U0", "--no-color", base, "--", ACTIONS_FILE), before: git("show", `${base}:${ACTIONS_FILE}`), after: existsSync(file) ? readFileSync(file, "utf8") : "" };
  } catch {
    return null;
  }
}
