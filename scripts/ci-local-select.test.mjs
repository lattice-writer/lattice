// @vitest-environment node
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { FULL, TEXT_ONLY, coveredJobs, docsGate, e2eNeeds, findGate, formatTrailer, fullCover, macosNeeds, mainMergeRegate, parseTrailer, resolveCover } from "./ci-local-select.mjs";
import { ciTier } from "./ci-tier.mjs";

const covers = new Map([
  ["pdf", ["gpui/crates/lattice-gpui/src/pdf"]],
  ["pdf_notes", ["gpui/crates/lattice-gpui/src/pdf/view/notes.rs"]],
  ["tree", ["gpui/crates/lattice-gpui/src/file_tree"]],
]);
const scan = { testReads: [], rustTargets: [], gpuiTargets: [], harness: [] };
const needsFor = (files) => macosNeeds(ciTier(files, { scan }), files);

describe("ci-local-select: the e2e a change needs", () => {
  it("runs the modules whose covers or scenario file a change touches", () => {
    const need = e2eNeeds(["gpui/crates/lattice-gpui/src/pdf/view/notes.rs", "gpui/crates/lattice-gpui/src/scenario/e2e/tree.rs"], covers);
    expect(need).toMatchObject({ full: false, modules: ["pdf", "pdf_notes", "tree"] });
    expect(need.why.tree).toEqual(["gpui/crates/lattice-gpui/src/scenario/e2e/tree.rs"]);
  });

  it("runs the smoke flow alone for files no scenario step reads", () => {
    expect(e2eNeeds(["gpui/README.md", "gpui/crates/lattice-editor/tests/motion.rs", "gpui/crates/lattice-gpui/src/pdf/tests.rs"], new Map())).toMatchObject({ full: false, modules: [] });
  });

  it("runs the whole scenario for its harness, shared code and anything the table does not map", () => {
    for (const file of ["gpui/crates/lattice-gpui/src/scenario/e2e/registry.rs", "gpui/patches/gpui-fast/crates/gpui/src/window.rs", "gpui/Cargo.lock", "src-tauri/lattice-core/src/lib.rs", "gpui/crates/lattice-gpui/src/shell.rs"]) {
      expect(e2eNeeds([file], covers), file).toMatchObject({ full: true, fullBecause: [file] });
    }
  });

  it("runs the whole scenario for the vendored crates under gpui/compat/, gpui-fast among them", () => {
    for (const file of ["gpui/compat/gpui-fast/gpui-fast/src/window.rs", "gpui/compat/gpui-fast/gpui-fast-platform/Cargo.toml", "gpui/compat/gpui-pre/gpui-pre/src/lib.rs", "gpui/compat/sqlite/libsqlite3-sys/build.rs"]) {
      expect(e2eNeeds([file], covers), file).toMatchObject({ full: true, fullBecause: [file] });
    }
    expect(e2eNeeds(["gpui/compat/gpui-fast/README.md"], covers)).toMatchObject({ full: false, modules: [] });
  });

  it("runs the whole scenario for what every window is built from, even where a module's covers name it", () => {
    const real = [
      "gpui/patches/gpui-base/src/window.rs",
      "gpui/patches/gpui-fast/crates/gpui_macos/src/window.rs",
      "gpui/crates/lattice-ui/src/fonts.rs",
      "gpui/crates/lattice-gpui/src/main.rs",
      "gpui/crates/lattice-gpui/src/app.rs",
      "gpui/crates/lattice-editor/Cargo.toml",
      "gpui/crates/lattice-gpui/src/scenario/shell_steps.rs",
      "src-tauri/lattice-core/src/plugins/mod.rs",
    ];
    for (const file of real) expect(e2eNeeds([file]), file).toMatchObject({ full: true, fullBecause: [file] });
    expect(e2eNeeds(["gpui/crates/lattice-gpui/src/scenario/e2e/input_method.rs"])).toMatchObject({ full: false, modules: ["input_method"] });
  });

  it("maps the Tauri modules the islands are built from to the modules that open an island", () => {
    const need = e2eNeeds(["src/editor/board/board.tsx"]);
    expect(need.full).toBe(false);
    expect(need.modules).toContain("islands");
    expect(need.modules).not.toContain("pdf");
  });
});

describe("ci-local-select: stages by diff", () => {
  it("leaves the GPUI stages out of a Tauri or web change, and the WebKit benchmark out of a GPUI one", () => {
    expect(needsFor(["src-tauri/src/lib.rs"])).toMatchObject({ rust: true, gpui: false, e2e: null, webkit: false });
    expect(needsFor(["src/App.tsx"])).toMatchObject({ rust: false, gpui: false, webkit: true });
    expect(needsFor(["gpui/crates/lattice-gpui/src/pdf/view/notes.rs"])).toMatchObject({ rust: false, gpui: true, webkit: false, e2e: { full: false } });
  });

  it("needs nothing for documentation", () => {
    expect(needsFor(["docs/agent-validation.md", "gpui/docs/updater.md"])).toMatchObject({ rust: false, gpui: false, webkit: false, e2e: null });
  });

  it("marks release paths, and a full tier needs the whole e2e", () => {
    expect(needsFor(["src-tauri/tauri.conf.json", "gpui/crates/lattice-gpui/src/updater/install.rs"]).release).toHaveLength(2);
    const release = ["src-tauri/Info.plist", "src-tauri/icons/StoreLogo.png", "src-tauri/macos/x", "gpui/script/fetch-pdfium", "scripts/windows-package.mjs", "scripts/lib/windows-package.mjs", "scripts/release-notes.mjs", "scripts/release-macos-job.sh", "scripts/lib/node-runtime.mjs", "scripts/lib/runtime-prune.mjs"];
    expect(needsFor(release).release).toEqual(release);
    expect(needsFor(["scripts/release-notes.test.mjs", "src-tauri/src/lib.rs"]).release).toEqual([]);
    const full = macosNeeds(ciTier(["README.md"], { full: true, scan }), ["README.md"]);
    expect(full.e2e.full).toBe(true);
  });
});

describe("ci-local-select: the trailer", () => {
  it("records the stages that ran and reads them back", () => {
    const line = formatTrailer("0123456789ab", { rust: false, gpui: true, e2e: ["pdf", "pdf_notes"], webkit: false });
    expect(line).toBe("CI-local: macos 0123456789ab gpui e2e=smoke+pdf+pdf_notes");
    expect(parseTrailer(`fix: x\n\n${line}\n`)).toMatchObject({ tree: "0123456789ab", gpui: true, rust: false, e2e: ["pdf", "pdf_notes"], webkit: false, regate: null });
    expect(parseTrailer(formatTrailer("0123456789ab", { rust: true, gpui: true, e2e: FULL, webkit: true, regate: "abcdef123" }))).toMatchObject({ rust: true, e2e: FULL, webkit: true, regate: "abcdef123" });
  });

  it("reads a bare trailer, from before it recorded stages, as every macOS job", () => {
    expect(parseTrailer("fix: x\n\nCI-local: macos 0123456789ab\n")).toMatchObject({ rust: true, gpui: true, e2e: FULL, webkit: true, legacy: true });
    expect(parseTrailer("fix: x")).toBeNull();
    const two = parseTrailer("fix: x\n\nCI-local: macos 0123456789ab rust\nCI-local: macos fedcba987654 gpui e2e=full\n");
    expect(two).toMatchObject({ tree: "fedcba987654", line: "CI-local: macos fedcba987654 gpui e2e=full", gpui: true, rust: false });
  });

  it("lets GitHub skip only the groups whose recorded stages cover the diff", () => {
    const needs = needsFor(["gpui/crates/lattice-gpui/src/pdf/view/notes.rs", "src-tauri/src/lib.rs"]);
    const cover = parseTrailer("CI-local: macos 0123456789ab rust gpui e2e=smoke+pdf+pdf_notes");
    expect(coveredJobs(cover, needs)).toEqual({ rust: { covered: true }, gpui: { covered: true }, webkit: { covered: true } });
    const partial = coveredJobs(parseTrailer("CI-local: macos 0123456789ab gpui e2e=smoke+pdf"), needs);
    expect(partial.rust).toEqual({ covered: false, reason: "the trailer records no rust stage" });
    expect(partial.gpui.reason).toMatch(/lacks pdf_notes/);
  });

  it("does not take a module e2e for a change that needs the whole scenario", () => {
    const shared = needsFor(["gpui/crates/lattice-gpui/src/shell.rs"]);
    expect(coveredJobs(parseTrailer("CI-local: macos 0123456789ab gpui e2e=smoke+pdf"), shared).gpui.covered).toBe(false);
    expect(coveredJobs(parseTrailer("CI-local: macos 0123456789ab gpui e2e=full"), shared).gpui.covered).toBe(true);
  });

  it("takes only the full gate's trailer for a release path", () => {
    const release = needsFor(["gpui/crates/lattice-gpui/src/updater/install.rs", "src-tauri/tauri.conf.json"]);
    expect(coveredJobs(parseTrailer("CI-local: macos 0123456789ab"), release).gpui.reason).toMatch(/release paths .* full gate/);
    expect(coveredJobs(parseTrailer("CI-local: macos 0123456789ab rust gpui e2e=full"), release).rust.reason).toMatch(/release paths/);
    expect(coveredJobs(parseTrailer("CI-local: macos 0123456789ab rust gpui e2e=smoke+pdf webkit"), release).gpui.reason).toMatch(/release paths/);
    const full = parseTrailer(formatTrailer("0123456789ab", { rust: true, gpui: true, e2e: FULL, webkit: true }));
    expect(fullCover(full)).toBe(true);
    expect(coveredJobs(full, release)).toEqual({ rust: { covered: true }, gpui: { covered: true }, webkit: { covered: true } });
    const workflow = needsFor([".github/workflows/release.yml"]);
    expect(workflow.release).toEqual([".github/workflows/release.yml"]);
    expect(coveredJobs(null, workflow)).toEqual({ rust: { covered: true }, gpui: { covered: true }, webkit: { covered: true } });
  });
});

describe.skipIf(process.platform === "win32")("ci-local-select: the quick re-gate after a main merge", () => {
  let dir;
  const git = (...args) => execFileSync("git", args, { cwd: dir, encoding: "utf8", env: { ...process.env, GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@t", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@t" } }).trim();
  const commit = (file, text, message) => {
    writeFileSync(path.join(dir, file), text);
    git("add", file);
    git("commit", "-q", "-m", message);
  };
  const gated = () => {
    dir = mkdtempSync(path.join(tmpdir(), "ci-local-select-"));
    git("init", "-q", "-b", "main");
    commit("shared.txt", "one\n", "base");
    git("checkout", "-q", "-b", "topic");
    commit("branch.txt", "branch\n", "branch change");
    git("commit", "-q", "--amend", "--no-edit", "--trailer", `CI-local: macos ${git("rev-parse", "HEAD^{tree}").slice(0, 12)} gpui e2e=smoke`);
    git("checkout", "-q", "main");
  };
  const merge = () => {
    git("checkout", "-q", "topic");
    git("merge", "-q", "--no-edit", "main");
  };
  afterEach(() => rmSync(dir, { recursive: true, force: true }));

  it("re-certifies a clean merge of main that touches none of the branch's files", () => {
    gated();
    commit("main.txt", "main\n", "main change");
    merge();
    const check = mainMergeRegate({ base: "main", cwd: dir });
    expect(check).toMatchObject({ ok: true, gate: git("rev-parse", "HEAD^1"), mainCommit: git("rev-parse", "main"), branchFiles: ["branch.txt"] });
    expect(check.cover).toMatchObject({ gpui: true, e2e: [] });
  });

  it("re-certifies two merges of main in a row", () => {
    gated();
    commit("main.txt", "main\n", "main change");
    merge();
    git("checkout", "-q", "main");
    commit("main2.txt", "more\n", "main again");
    merge();
    expect(mainMergeRegate({ base: "main", cwd: dir })).toMatchObject({ ok: true, gate: git("rev-parse", "HEAD^1^1") });
  });

  it("runs the full gate when main changed a file the branch changes", () => {
    gated();
    commit("branch.txt", "main's own\n", "main edits the branch's file");
    git("checkout", "-q", "topic");
    try {
      git("merge", "-q", "--no-edit", "main");
    } catch {
      git("checkout", "--theirs", "branch.txt");
      git("commit", "-q", "-am", "merge main");
    }
    expect(mainMergeRegate({ base: "main", cwd: dir })).toMatchObject({ ok: false, reason: expect.stringMatching(/branch\.txt/) });
  });

  it("runs the full gate when the merge commit carries a hand edit, or a branch commit follows the gate", () => {
    gated();
    commit("main.txt", "main\n", "main change");
    merge();
    writeFileSync(path.join(dir, "branch.txt"), "edited in the merge\n");
    git("commit", "-q", "-a", "--amend", "--no-edit");
    expect(mainMergeRegate({ base: "main", cwd: dir }).reason).toMatch(/not the clean merge/);

    rmSync(dir, { recursive: true, force: true });
    gated();
    git("checkout", "-q", "topic");
    commit("branch.txt", "more\n", "another branch commit");
    expect(mainMergeRegate({ base: "main", cwd: dir }).reason).toBe("HEAD is not a merge");
  });
});

describe("ci-local-select: what a docs-only pass may change", () => {
  it("takes the docs, the release notes, the user guide and the i18n catalogs", () => {
    for (const file of ["docs/agent-validation.md", "docs/guide/zh-CN/editing.md", "docs/guide/images/pdf.png", "gpui/docs/updater.md", ".github/release-notes/v0.20.0.md", "src/release-notes/0.20.0.json", "src/locales/en/messages.json", "src/locales/zh-CN/messages.po", "gpui/crates/lattice-i18n/locales/zh-CN/app.po", "README.md", "CLAUDE.md"]) {
      expect(TEXT_ONLY.test(file), file).toBe(true);
    }
  });

  it("fails closed: code, config, scripts, workflows and anything unnamed are not text", () => {
    for (const file of [
      "src/App.tsx",
      "src/release-notes/index.ts",
      "src/locales/index.ts",
      "src/locales/en/nested/messages.json",
      "gpui/crates/lattice-i18n/locales/unlocalized.txt",
      "gpui/crates/lattice-i18n/src/lib.rs",
      "gpui/crates/lattice-i18n/build.rs",
      "THIRD_PARTY_NOTICES.md",
      "src-tauri/skills/x/SKILL.md",
      "scripts/release-notes.mjs",
      "scripts/i18n-unlocalized-baseline.txt",
      ".github/workflows/ci.yml",
      ".github/release-notes/v0.20.0.json",
      "package.json",
      "docs",
      "gpui/compat/gpui-fast/gpui-fast/src/window.rs",
    ]) {
      expect(TEXT_ONLY.test(file), file).toBe(false);
    }
  });

  it("writes and reads the docs-only trailer, which records no stage of its own", () => {
    const line = formatTrailer("0123456789ab", { docs: true, from: "fedcba987654" });
    expect(line).toBe("CI-local: macos 0123456789ab docs from=fedcba987654");
    const cover = parseTrailer(`docs: x\n\n${line}\n`);
    expect(cover).toMatchObject({ tree: "0123456789ab", docs: true, from: "fedcba987654", rust: false, gpui: false, webkit: false, e2e: null });
    // Left unresolved it stands for nothing.
    expect(coveredJobs(cover, needsFor(["gpui/crates/lattice-gpui/src/pdf/view/notes.rs"])).gpui.covered).toBe(false);
  });
});

describe.skipIf(process.platform === "win32")("ci-local-select: the docs-only pass on top of a gate", () => {
  let dir;
  const git = (...args) => execFileSync("git", args, { cwd: dir, encoding: "utf8", env: { ...process.env, GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@t", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@t" } }).trim();
  const commit = (file, text, message = `change ${file}`) => {
    mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
    writeFileSync(path.join(dir, file), text);
    git("add", file);
    git("commit", "-q", "-m", message);
    return git("rev-parse", "HEAD");
  };
  const trailer = (stages) => {
    git("commit", "-q", "--amend", "--no-edit", "--trailer", `CI-local: macos ${git("rev-parse", "HEAD^{tree}").slice(0, 12)} ${stages}`);
    return git("rev-parse", "HEAD");
  };
  const docsTrailer = (gate) => {
    git("commit", "-q", "--amend", "--no-edit", "--trailer", formatTrailer(git("rev-parse", "HEAD^{tree}").slice(0, 12), { docs: true, from: gate.slice(0, 12) }));
    return parseTrailer(git("log", "-1", "--format=%B"));
  };
  const gated = (stages = "rust gpui e2e=full webkit") => {
    dir = mkdtempSync(path.join(tmpdir(), "ci-local-docs-"));
    git("init", "-q", "-b", "main");
    commit("base.txt", "base\n");
    git("checkout", "-q", "-b", "topic");
    commit("src-tauri/tauri.conf.json", "{}\n");
    return trailer(stages);
  };
  afterEach(() => rmSync(dir, { recursive: true, force: true }));

  it("stands on the gate when everything since it is text, and covers a release path with the gate's full trailer", () => {
    const gate = gated();
    commit("docs/guide/editing.md", "words\n");
    commit("src/release-notes/0.20.0.json", "{}\n");
    expect(findGate({ cwd: dir })).toBe(gate);
    const resolved = resolveCover(docsTrailer(gate), { cwd: dir });
    expect(resolved).toMatchObject({ reason: null, cover: { rust: true, gpui: true, e2e: FULL, webkit: true, from: gate }, docsFiles: ["docs/guide/editing.md", "src/release-notes/0.20.0.json"] });
    const rest = ["src-tauri/tauri.conf.json"];
    expect(coveredJobs(resolved.cover, needsFor(rest))).toEqual({ rust: { covered: true }, gpui: { covered: true }, webkit: { covered: true } });
  });

  it("does not stand for a release path whose gate recorded less than the full gate", () => {
    const gate = gated("rust");
    commit("docs/x.md", "x\n");
    const resolved = resolveCover(docsTrailer(gate), { cwd: dir });
    expect(coveredJobs(resolved.cover, needsFor(["src-tauri/tauri.conf.json"])).rust.reason).toMatch(/release paths/);
  });

  it("fails closed on any file since the gate that is not text", () => {
    const gate = gated();
    commit("docs/x.md", "x\n");
    commit("src/App.tsx", "x\n");
    const resolved = resolveCover(docsTrailer(gate), { cwd: dir });
    expect(resolved).toMatchObject({ cover: null, reason: expect.stringMatching(/src\/App\.tsx/) });
    expect(docsGate(gate, { cwd: dir })).toMatchObject({ ok: false });
  });

  it("counts uncommitted and untracked files in a working-tree check", () => {
    const gate = gated();
    commit("docs/x.md", "x\n");
    expect(docsGate(gate, { cwd: dir, worktree: true })).toMatchObject({ ok: true, files: ["docs/x.md"] });
    writeFileSync(path.join(dir, "loose.ts"), "x\n");
    expect(docsGate(gate, { cwd: dir, worktree: true })).toMatchObject({ ok: false, reason: expect.stringMatching(/loose\.ts/) });
  });

  it("refuses a gate that is not an ancestor, not a commit, has no trailer for its own tree, or is itself a docs-only pass", () => {
    const gate = gated();
    git("checkout", "-q", "main");
    commit("docs/y.md", "y\n");
    expect(resolveCover(docsTrailer(gate), { cwd: dir }).reason).toMatch(/not an ancestor/);

    git("checkout", "-q", "topic");
    commit("docs/x.md", "x\n");
    expect(resolveCover(parseTrailer("CI-local: macos 0123456789ab docs from=deadbeefdead"), { cwd: dir }).reason).toMatch(/not a commit/);
    expect(resolveCover(parseTrailer("CI-local: macos 0123456789ab docs"), { cwd: dir }).reason).toMatch(/names no gate/);
    const plain = commit("docs/z.md", "z\n");
    commit("docs/v.md", "v\n");
    expect(resolveCover(docsTrailer(plain), { cwd: dir }).reason).toMatch(/no CI-local trailer/);

    const docsPass = git("rev-parse", "HEAD");
    commit("docs/w.md", "w\n");
    expect(resolveCover(docsTrailer(docsPass), { cwd: dir }).reason).toMatch(/no CI-local trailer that names its own tree and records its stages/);
    // ci:local still finds the real gate behind the docs-only passes.
    expect(findGate({ cwd: dir })).toBe(gate);
  });

  it("refuses a gate whose trailer names another tree", () => {
    const gate = gated();
    commit("docs/x.md", "x\n");
    git("checkout", "-q", gate);
    git("commit", "-q", "--amend", "--allow-empty", "-m", "moved\n\nCI-local: macos 0123456789ab rust gpui e2e=full webkit");
    const forged = git("rev-parse", "HEAD");
    git("checkout", "-q", "topic");
    expect(docsGate(forged, { cwd: dir }).ok).toBe(false);
  });
});
