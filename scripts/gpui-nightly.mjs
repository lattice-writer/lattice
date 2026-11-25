#!/usr/bin/env node
/**
 * `node scripts/gpui-nightly.mjs run|report`: the nightly full gate on main
 * (.github/workflows/nightly-e2e.yml, on the Mac mini through runner-up's
 * `lattice-mac` pool, docs/release-process.md).
 *
 * A branch's gate runs the smoke flow and the e2e modules its diff picks
 * (scripts/ci-local-select.mjs), so the rest of the scenario runs here, once
 * a night, and a failure it finds is fixed the next day.
 *
 *   run     records HEAD with scripts/gpui-main-baseline.mjs (the gate, the
 *           release build and the whole e2e scenario, which also leaves this
 *           Mac's main baseline for that commit), judges the e2e as gpui.yml
 *           judges main (scripts/gpui-e2e-known-failures.json, the flaky tier
 *           counted, one rerun of a new failure's module on a fresh fixture),
 *           and writes nightly@<sha>.json beside the baseline. The scenario
 *           and the rerun open windows: run it under scripts/inapp-lock.
 *   report  reads that record and keeps one GitHub issue, labelled
 *           `nightly-e2e`, in step (planIssue): it opens one for the first
 *           failures, rewrites its body every night they persist and comments
 *           when a failure is new since the night before, and comments and
 *           closes it on the first green night. A missing record (the run
 *           died or never got the lock) is a failure too.
 *
 * Options:
 *   report --dry-run   print what it would do to the issue, and do nothing
 */
import { execFileSync, spawnSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync, statfsSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { judge, knownFailures, rerun, rerunModules, settle } from "./gpui-e2e-verdict.mjs";
import { baselineDir } from "./gpui-main-baseline.mjs";
import { isMain } from "./lib/util.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const git = (...args) => execFileSync("git", args, { cwd: repo, encoding: "utf8" }).trim();
export const LABEL = "nightly-e2e";
// The system disk floor release builds keep on the Mac mini (CREW.md).
const MIN_FREE_GB = Number(process.env.LATTICE_NIGHTLY_MIN_FREE_GB ?? 20);

/** What failed tonight, one line each: gate stages, the build, and the e2e steps the verdict counts. */
export function failuresOf(record, verdict) {
  if (!record) return ["the nightly run wrote no baseline record"];
  const failures = (record.gate?.failing ?? []).map((stage) => `gate stage ${stage}`);
  if (record.build?.status !== "ok") failures.push(`release build: ${record.build?.status ?? "not run"}`);
  else if (!verdict) failures.push(`e2e: ${record.e2e?.status ?? "not run"}${record.e2e?.reason ? ` (${record.e2e.reason})` : ""}`);
  if (verdict?.error) failures.push(`e2e did not finish: ${verdict.error}`);
  for (const step of verdict?.fresh ?? []) failures.push(`e2e step ${step}`);
  for (const step of verdict?.quarantined ?? []) failures.push(`e2e step ${step} (flaky tier, failed twice)`);
  if (record.e2e?.after_quit) failures.push("e2e: quitting did not save the typed text");
  return failures;
}

const MARKER = /<!-- nightly-failures (\[.*?\]) -->/s;

/** The failures an issue body lists, from its marker; [] when it has none. */
export function listedFailures(body) {
  try {
    const listed = JSON.parse(MARKER.exec(body ?? "")?.[1] ?? "[]");
    return Array.isArray(listed) ? listed.map(String) : [];
  } catch {
    return [];
  }
}

export function issueBody({ sha, subject, failures, flaky = [], run }) {
  return [
    `The nightly full gate on main failed at ${sha.slice(0, 9)} (${subject}).`,
    "",
    ...failures.map((failure) => `- ${failure}`),
    ...(flaky.length ? ["", "Flaky (failed, then passed on a rerun of the module):", ...flaky.map((step) => `- ${step}`)] : []),
    "",
    run ? `Run: ${run}` : "",
    "The known failures (scripts/gpui-e2e-known-failures.json) are not listed. This issue is rewritten every night and closed by the first green one (scripts/gpui-nightly.mjs).",
    "",
    `<!-- nightly-failures ${JSON.stringify(failures)} -->`,
  ].join("\n");
}

/**
 * What to do to the nightly issue: `[{ kind: "create" | "edit" | "comment" |
 * "close", ... }]`. Nothing failed and no issue is open: nothing. Something
 * failed: open the issue, or rewrite the open one's body and comment the
 * failures it did not list. Nothing failed with one open: comment and close.
 */
export function planIssue({ issue, failures, sha, subject, flaky = [], run = "" }) {
  const body = issueBody({ sha, subject, failures, flaky, run });
  if (!failures.length) return issue ? [{ kind: "comment", number: issue.number, body: `Green at ${sha.slice(0, 9)}${run ? ` (${run})` : ""}: closing.` }, { kind: "close", number: issue.number }] : [];
  if (!issue) return [{ kind: "create", title: `Nightly e2e on main fails (${failures.length})`, body }];
  const listed = new Set(listedFailures(issue.body));
  const fresh = failures.filter((failure) => !listed.has(failure));
  return [
    { kind: "edit", number: issue.number, title: `Nightly e2e on main fails (${failures.length})`, body },
    ...(fresh.length ? [{ kind: "comment", number: issue.number, body: [`New since the last night, at ${sha.slice(0, 9)}:`, "", ...fresh.map((failure) => `- ${failure}`), ...(run ? ["", run] : [])].join("\n") }] : []),
  ];
}

const recordFile = (sha) => path.join(baselineDir(), `nightly@${sha}.json`);

function runNightly() {
  const sha = git("rev-parse", "HEAD");
  const { bavail, bsize } = statfsSync("/");
  const freeGb = (bavail * bsize) / 1e9;
  if (freeGb < MIN_FREE_GB) throw new Error(`the system disk has ${freeGb.toFixed(1)} GB free, under the ${MIN_FREE_GB} GB floor`);
  spawnSync("node", [path.join(repo, "scripts", "gpui-main-baseline.mjs")], { cwd: repo, stdio: "inherit" });
  const dir = baselineDir();
  const baselineFile = path.join(dir, `known-failures@${sha}.json`);
  const record = existsSync(baselineFile) ? JSON.parse(readFileSync(baselineFile, "utf8")) : null;
  let verdict = null;
  if (record?.e2e?.result) {
    let result = null;
    try {
      result = JSON.parse(readFileSync(path.join(dir, record.e2e.result), "utf8"));
    } catch {
      // judge() reports the missing result.
    }
    verdict = judge(result, knownFailures(), "macos", { strict: true });
    const retry = [...verdict.fresh, ...verdict.quarantined];
    if (retry.length && !verdict.error) verdict = settle(verdict, rerun(path.join(repo, "gpui", "target", "release", "lattice-gpui"), path.join(repo, "gpui", "fixtures", "e2e"), rerunModules(result, retry)));
  }
  const nightly = { sha, subject: git("log", "-1", "--format=%s", sha), finished: new Date().toISOString(), baseline: record ? path.basename(baselineFile) : null, failures: failuresOf(record, verdict), flaky: verdict?.flaky ?? [], fixed: verdict?.fixed ?? [] };
  writeFileSync(recordFile(sha), `${JSON.stringify(nightly, null, 2)}\n`);
  const lines = [`Nightly full gate at ${sha.slice(0, 9)}: ${nightly.failures.length ? `${nightly.failures.length} failure(s)` : "green"}`, ...nightly.failures.map((failure) => `- ${failure}`), ...nightly.flaky.map((step) => `- flaky: ${step}`), ...nightly.fixed.map((step) => `- known failure that passed: ${step}`)];
  console.log(lines.join("\n"));
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## ${lines[0]}\n\n${lines.slice(1).join("\n")}\n`);
  process.exitCode = nightly.failures.length ? 1 : 0;
}

function report({ dryRun }) {
  const sha = git("rev-parse", "HEAD");
  const file = recordFile(sha);
  const nightly = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : { sha, subject: git("log", "-1", "--format=%s", sha), failures: failuresOf(null), flaky: [] };
  const run = process.env.GITHUB_RUN_ID ? `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}` : "";
  const gh = (...args) => execFileSync("gh", args, { cwd: repo, encoding: "utf8" }).trim();
  const [issue = null] = dryRun ? [] : JSON.parse(gh("issue", "list", "--label", LABEL, "--state", "open", "--limit", "1", "--json", "number,body") || "[]");
  for (const action of planIssue({ issue, failures: nightly.failures, sha, subject: nightly.subject, flaky: nightly.flaky, run })) {
    if (dryRun) {
      console.log(JSON.stringify(action, null, 2));
      continue;
    }
    if (action.kind === "create") {
      gh("label", "create", LABEL, "--color", "B60205", "--description", "The nightly full gate on main (scripts/gpui-nightly.mjs)", "--force");
      console.log(gh("issue", "create", "--title", action.title, "--body", action.body, "--label", LABEL));
    } else if (action.kind === "edit") gh("issue", "edit", String(action.number), "--title", action.title, "--body", action.body);
    else if (action.kind === "comment") gh("issue", "comment", String(action.number), "--body", action.body);
    else if (action.kind === "close") gh("issue", "close", String(action.number));
  }
}

if (isMain(import.meta.url)) {
  const [command, ...rest] = process.argv.slice(2);
  try {
    if (command === "run") runNightly();
    else if (command === "report") report({ dryRun: rest.includes("--dry-run") });
    else throw new Error("usage: gpui-nightly.mjs run | report [--dry-run]");
  } catch (error) {
    console.error(`gpui-nightly: ${error.message}`);
    process.exit(1);
  }
}
