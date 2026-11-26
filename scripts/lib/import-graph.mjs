/**
 * The tests near a change: each test whose static imports reach a changed
 * file in at most `depth` steps (the file's own tests at 1, its importers'
 * tests at 2), read from the sources' import, export-from, dynamic import and
 * vi.mock specifiers, relative or under `@/`, as vite resolves them. Type-only
 * imports are left out, as vite strips them. With no depth limit it finds what
 * `vitest related` finds (102 tests for src/lib/pointer-hold.ts, both ways).
 *
 * `pnpm gate:changed` runs these instead of `vitest related`'s whole
 * closure: every suite that mounts the complete app imports nearly every
 * module, so a one-line change ran a hundred files one at a time
 * (vitest.config.ts's maxWorkers: 1), about 6 minutes, where its importers'
 * tests take one and a half. The deeper suites run in release CI.
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";

const SOURCE = /\.(tsx?|mts|jsx?|mjs)$/;
const TEST = /\.test\.(tsx?|mts|jsx?|mjs)$/;
const IMPORT = /^(?!\s*(?:import|export)\s+type\b).*?(?:from\s+|import\s*\(\s*|import\s+|vi\.mock\(\s*)["']([^"']+)["']/gm;
const ENDINGS = ["", ".ts", ".tsx", ".mts", ".js", ".jsx", ".mjs", "/index.ts", "/index.tsx", "/index.js"];

/** Every source file under src, scripts and tools, mapped to the files that import it. */
export function importers(cwd, read = (file) => readFileSync(path.join(cwd, file), "utf8")) {
  const files = execFileSync("git", ["ls-files", "src", "scripts", "tools"], { cwd, encoding: "utf8" }).split("\n").filter((file) => SOURCE.test(file));
  const known = new Set(files);
  const resolve = (from, spec) => {
    const bare = spec.replace(/[?#].*$/, "");
    if (!bare.startsWith(".") && !bare.startsWith("@/")) return null;
    const base = (bare.startsWith("@/") ? path.posix.join("src", bare.slice(2)) : path.posix.join(path.posix.dirname(from), bare)).replace(/\.js$/, "");
    for (const ending of [...ENDINGS, ".js"]) if (known.has(base + ending)) return base + ending;
    return null;
  };
  const graph = new Map(files.map((file) => [file, new Set()]));
  for (const file of files) {
    for (const match of read(file).matchAll(IMPORT)) {
      const target = resolve(file, match[1]);
      if (target && target !== file) graph.get(target).add(file);
    }
  }
  return graph;
}

/** The tests within `depth` import steps of `changed` (a changed test is its own), and the changed files the graph does not hold. */
export function nearTests(changed, graph, depth = 2) {
  const unknown = changed.filter((file) => !graph.has(file));
  const seen = new Set(changed.filter((file) => graph.has(file)));
  let frontier = [...seen];
  for (let step = 1; step <= depth && frontier.length; step += 1) {
    frontier = frontier.flatMap((file) => [...graph.get(file)].filter((importer) => !seen.has(importer)));
    for (const file of frontier) seen.add(file);
  }
  return { tests: [...seen].filter((file) => TEST.test(file)).sort(), unknown };
}
