// @vitest-environment node
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { CI_CONTROL, CI_CONTROL_TESTS, GPUI_INPUTS, JOBS, ciTier, knownFailuresInert, localMacosPass, macosExcusesKept, trailerHonoured, wantsFull } from "./ci-tier.mjs";

const repo = path.resolve(import.meta.dirname, "..");

const scan = {
  testReads: [
    { test: "src/platform/host-error-catalog.test.ts", read: "src-tauri/src" },
    { test: "src/editor/markdown/spec.test.ts", read: "docs/visual-editor-spec.md" },
  ],
  rustTargets: [{ target: "src/styles/titlebar.css", prefix: false }],
  gpuiTargets: ["THIRD_PARTY_NOTICES.md"],
  harness: ["scripts/perf-bench/smoke.test.mjs"],
};
const running = (files) => {
  const tier = ciTier(files, { scan });
  return { on: JOBS.filter((job) => tier.jobs[job]), files: tier.vitestFiles, docsOnly: tier.docsOnly };
};

describe("ci-tier", () => {
  it("runs nothing for documentation", () => {
    expect(running(["docs/agent-validation.md", "README.md", "gpui/docs/design-system.md", ".github/ISSUE_TEMPLATE/bug.yml", "LICENSE"])).toEqual({ on: [], files: [], docsOnly: true });
  });

  it("runs the tests that name a document, and nothing else", () => {
    expect(running(["docs/visual-editor-spec.md"])).toEqual({ on: ["vitest"], files: ["src/editor/markdown/spec.test.ts"], docsOnly: false });
  });

  it("runs the Rust jobs and only the tests that read Rust sources for a Rust change", () => {
    expect(running(["src-tauri/src/lib.rs"])).toEqual({ on: ["vitest", "rust"], files: ["src/platform/host-error-catalog.test.ts"], docsOnly: false });
    expect(running(["src-tauri/Cargo.lock"]).on).toEqual(["lint-build", "rust"]);
  });

  it("runs only gpui.yml for a GPUI change, and both apps for lattice-core", () => {
    expect(running(["gpui/crates/lattice-gpui/src/main.rs"]).on).toEqual(["gpui"]);
    expect(running(["src-tauri/lattice-core/src/lib.rs"]).on).toEqual(["rust", "gpui"]);
  });

  it("runs gpui.yml for every crate outside gpui/ the GPUI workspace builds", () => {
    const crates = new Set();
    const visit = (dir) => {
      const manifest = path.join(repo, dir, "Cargo.toml");
      if (crates.has(dir) || !existsSync(manifest)) return;
      crates.add(dir);
      for (const [, dependency] of readFileSync(manifest, "utf8").matchAll(/path\s*=\s*"([^"]+)"/g)) {
        visit(path.relative(repo, path.join(repo, dir, dependency)).split(path.sep).join("/"));
      }
    };
    visit("gpui");
    for (const crate of readdirSync(path.join(repo, "gpui", "crates"))) visit(`gpui/crates/${crate}`);
    const outside = [...crates].filter((dir) => !dir.startsWith("gpui"));
    expect(outside).toContain("src-tauri/lattice-core");
    expect(outside.filter((dir) => !GPUI_INPUTS.test(`${dir}/src/lib.rs`))).toEqual([]);
  });

  it("runs the frontend jobs for anything else, and every test file", () => {
    expect(running(["src/App.tsx"])).toEqual({ on: ["vitest", "lint-build", "perf-bench"], files: [], docsOnly: false });
    expect(running(["scripts/lint.mjs"]).on).toEqual(["vitest", "lint-build"]);
    expect(running(["src/editor/board/board.tsx"]).on).toEqual(["vitest", "lint-build", "perf-bench", "gpui"]);
    expect(running(["src/styles/titlebar.css"]).on).toEqual(["vitest", "lint-build", "perf-bench", "rust"]);
  });

  it("keeps the self-contained projects to their own jobs", () => {
    expect(running(["literature-worker/src/index.ts"]).on).toEqual(["literature-worker"]);
    expect(running(["research-env/lattice_env/cli.py"]).on).toEqual(["research-env"]);
    expect(running(["tools/open-slide-runtime/server.mjs"]).on).toEqual(["vitest", "lint-build", "open-slide-runtime"]);
    expect(running(["scripts/synara-runtime.json"]).on).toEqual(["vitest", "lint-build", "synara-pin"]);
  });

  it("counts THIRD_PARTY_NOTICES.md and docs/performance.md as inputs, not documentation", () => {
    expect(running(["THIRD_PARTY_NOTICES.md"]).on).toEqual(["vitest", "lint-build", "gpui"]);
    expect(running(["docs/performance.md"]).on).toEqual(["lint-build"]);
  });

  it("runs a workflow's every job when it changes, and everything for the shared pieces", () => {
    expect(running([".github/workflows/gpui.yml"]).on).toEqual(["gpui"]);
    expect(running([".github/workflows/ci.yml"]).on).toEqual(JOBS.filter((job) => job !== "gpui"));
    expect(running([".github/actions/setup-pnpm/action.yml"]).on).toEqual(JOBS);
    // The selector alone runs its own tests; with any other file, everything.
    for (const file of ["scripts/ci-tier.mjs", "scripts/ci-local.mjs", "scripts/ci-local-select.mjs", "scripts/gpui-e2e-modules.mjs", "scripts/windows-e2e-tier.mjs", "scripts/linux-gpui-e2e-tier.mjs"]) {
      const tier = ciTier([file, "scripts/ci-tier.test.mjs", "docs/agent-validation.md"], { scan });
      expect(JOBS.filter((job) => tier.jobs[job]), file).toEqual(["vitest", "lint-build"]);
      expect(tier.vitestFiles, file).toContain("scripts/windows-e2e-tier.test.mjs");
    }
    const reads = ciTier(["scripts/ci-tier.mjs", "docs/spec.md"], { scan: { ...scan, testReads: [{ read: "docs/spec.md", test: "src/spec.test.ts" }] } });
    expect(reads.vitestFiles).toContain("src/spec.test.ts");
    for (const other of ["gpui/crates/lattice-gpui/src/shell.rs", "src/App.tsx", "scripts/inapp-lock", "src-tauri/src/lib.rs"]) {
      expect(running(["scripts/ci-local-select.mjs", other]).on, other).toEqual(JOBS);
    }
    expect(ciTier(["README.md"], { full: true, scan }).jobs).toEqual(Object.fromEntries(JOBS.map((job) => [job, true])));
  });

  it("lists every test that imports a CI-selection script as its own", () => {
    const scripts = path.resolve(import.meta.dirname);
    const imports = (file, seen = new Set()) => {
      if (seen.has(file) || !existsSync(file)) return seen;
      seen.add(file);
      for (const [, spec] of readFileSync(file, "utf8").matchAll(/(?:from|import\(?)\s*["'](\.{1,2}\/[^"']+)["']/g)) imports(path.resolve(path.dirname(file), spec), seen);
      return seen;
    };
    const control = [...CI_CONTROL].map((file) => path.resolve(scripts, "..", file));
    const reaching = readdirSync(scripts)
      .filter((name) => /\.test\.(mjs|ts)$/.test(name))
      .filter((name) => [...imports(path.join(scripts, name))].some((file) => control.includes(file)))
      .map((name) => "scripts/" + name);
    expect(reaching.sort()).toEqual([...CI_CONTROL_TESTS].sort());
    // selectorListsEdited reads the lists line by line.
    const source = readFileSync(path.join(scripts, "ci-tier.mjs"), "utf8");
    for (const name of ["CI_CONTROL_TESTS", "CI_CONTROL", "DOCS"]) expect(source, name).toMatch(new RegExp(`^(export )?const ${name} = .*;$`, "m"));
    expect(JOBS.filter((job) => ciTier(["scripts/ci-tier.mjs"], { scan, selector: false }).jobs[job])).toEqual(JOBS);
  });

  it("takes only the verdict's tests for a known-failures edit no macOS run can show", () => {
    const file = "scripts/gpui-e2e-known-failures.json";
    const inert = ciTier([file], { scan, inertKnown: true });
    expect(JOBS.filter((job) => inert.jobs[job])).toEqual(["vitest"]);
    expect(inert.vitestFiles).toEqual(["scripts/gpui-e2e-verdict.test.mjs"]);
    expect(inert.gpuiFiles).toEqual([file]);
    expect(ciTier([file], { scan }).jobs.gpui).toBe(true);
    expect(ciTier([file, "gpui/crates/lattice-gpui/src/main.rs"], { scan, inertKnown: true }).jobs.gpui).toBe(true);

    const list = (...steps) => JSON.stringify({ steps: steps.map(([step, platforms, tier]) => ({ step, platforms, reason: "r", followup: "LEO-14", ...(tier && { tier }) })) });
    const before = list(["a", ["macos", "linux"]], ["b", ["macos"], "flaky"], ["c", ["windows"]]);
    expect(macosExcusesKept(before, list(["a", ["macos", "linux"]], ["c", ["windows"]]))).toBe(true);
    expect(macosExcusesKept(before, list(["a", ["macos"]], ["b", ["macos"], "flaky"], ["c", ["windows", "linux"]], ["d", ["linux"]]))).toBe(true);
    expect(macosExcusesKept(before, list(["a", ["macos", "linux"]], ["b", ["macos"], "flaky"], ["c", ["windows", "macos"]]))).toBe(false);
    expect(macosExcusesKept(before, list(["a", ["macos", "linux"]], ["b", ["macos"]], ["c", ["windows"]]))).toBe(false);
    expect(macosExcusesKept(before, list(["a", ["macos", "linux"]], ["b", ["macos"], "flaky"], ["e", ["macos"]]))).toBe(false);
    expect(knownFailuresInert("HEAD", [file])).toBe(true);
    expect(knownFailuresInert("HEAD", ["src/App.tsx"])).toBe(false);
    expect(knownFailuresInert("no-such-ref", [file])).toBe(false);
  });

  it("reads the CI: full trailer and the ci:full label", () => {
    expect(wantsFull("fix: x\n\nCI: full\n")).toBe(true);
    expect(wantsFull("fix: CI: full is mentioned in prose")).toBe(false);
    expect(wantsFull("fix: x", ["ci:full"])).toBe(true);
  });

  it("takes a CI-local trailer only for the tree it names", () => {
    const tree = "0123456789abcdef0123456789abcdef01234567";
    expect(localMacosPass("fix: x\n\nCI-local: macos 0123456789ab\n", tree)).toBe(true);
    expect(localMacosPass("fix: x\n\nCI-local: macos fedcba987654\n", tree)).toBe(false);
    expect(localMacosPass("fix: x", tree)).toBe(false);
  });

  it("honours a CI-local trailer on a branch's dispatch only, never on a tag or a full run", () => {
    expect(trailerHonoured({ full: false, ref: "refs/heads/capy/topic" })).toBe(true);
    expect(trailerHonoured({ full: false, ref: "refs/tags/v0.19.3" })).toBe(false);
    expect(trailerHonoured({ full: true, ref: "refs/heads/capy/topic" })).toBe(false);
  });

  it("runs only one platform's Rust and GPUI jobs, in full, for a platform dispatch", () => {
    const outputs = (platform) => {
      const file = path.join(mkdtempSync(path.join(tmpdir(), "ci-tier-")), "out");
      const env = { PATH: process.env.PATH, HOME: process.env.HOME, GITHUB_EVENT_NAME: "workflow_dispatch", GITHUB_REF: "refs/heads/capy/topic", GITHUB_OUTPUT: file, INPUT_PLATFORM: platform };
      execFileSync(process.execPath, [path.join(repo, "scripts/ci-tier.mjs"), "--github"], { cwd: repo, env, stdio: "ignore" });
      return Object.fromEntries(readFileSync(file, "utf8").trim().split("\n").map((line) => line.split(/=(.*)/s).slice(0, 2)));
    };
    const windows = outputs("windows");
    expect(JOBS.filter((job) => windows[job] === "true")).toEqual(["rust", "gpui"]);
    expect(windows).toMatchObject({ full: "true", macos: "false", linux: "false", windows: "true", "all-platforms": "false", "windows-e2e": "full", "macos-local-rust": "false", "macos-local-gpui": "false" });
    expect(outputs("")).toMatchObject({ macos: "true", linux: "true", windows: "true", "all-platforms": "true" });
  });

  it("runs ci.yml and gpui.yml for tags and dispatches only", () => {
    for (const file of ["ci.yml", "gpui.yml"]) {
      const text = readFileSync(path.join(repo, ".github/workflows", file), "utf8");
      const lines = text.slice(text.indexOf("\non:\n") + 5).split("\n");
      const on = lines.slice(0, lines.findIndex((line) => /^\S/.test(line)));
      expect(on.filter((line) => /^ {2}\S/.test(line)), file).toEqual(["  push:", "  workflow_dispatch:"]);
      expect(on.filter((line) => /^ {4}\S/.test(line))[0], file).toBe('    tags: ["v*"]');
      expect(text, file).toMatch(/\non:\n {2}push:\n {4}tags: \["v\*"\]\n {2}workflow_dispatch:/);
      expect(text, file).toMatch(/\n {6}platform:\n[\s\S]*?options: \[all, macos, linux, windows\]/);
    }
  });
});
