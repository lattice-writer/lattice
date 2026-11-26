/**
 * How much of the GPUI e2e scenario gpui.yml's Windows job runs for a
 * change: the whole scenario (`full`), or the smoke flow alone (`smoke`:
 * open, build, SyncTeX both ways, edits and save, autosave, a second window,
 * the quit that saves). The job always runs the release build and every unit
 * test first.
 *
 * The whole scenario takes 25 to 40 minutes on the hosted Windows runner, so
 * it runs only for a change that can behave differently there. A change
 * whose every GPUI input is one of these takes the smoke flow:
 *
 *   - compiled or run on macOS alone: the gpui_macos and gpui_apple crates
 *     under gpui/patches/gpui-fast/, a file or directory named for macOS
 *     (macos.rs, macos_*.rs, *_macos.rs, macos/), a source whose first
 *     attribute is #![cfg(target_os = "macos")], a patch diff that touches
 *     only such files, the parity report (gpui/parity/ and the
 *     design-system screens it stages) and the macOS-only scripts
 *     (fetch-pdfium, make-fonts);
 *   - read by no scenario step on any platform: the rows of E2E_TABLE
 *     (scripts/ci-local-select.mjs) that need no module, such as Markdown,
 *     unit tests and benches.
 *
 * The web modules the islands are built from (src/, gpui/islands/) need the
 * whole scenario: on Windows they render in WebView2, which the macOS e2e
 * (WKWebView) does not stand for. A patch diff counts by every file it
 * touches, the old side of a removed or renamed file included.
 *
 * Anything else the GPUI job reads needs the whole scenario: the
 * platform-neutral crates, cfg(windows) code, Windows packaging, the
 * scenario's harness and fixtures, the workflows, lattice-core and the
 * design-system tokens. So does every run with no diff to judge: a tag, the
 * schedule, a full dispatch, and every push to main (scripts/ci-tier.mjs).
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { E2E_TABLE } from "./ci-local-select.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export const WHOLE = "full";
export const SMOKE = "smoke";

const MACOS_ONLY =
  /^(gpui\/patches\/gpui-fast\/crates\/(gpui_macos|gpui_apple)\/|gpui\/parity\/|design-system\/screens\/|gpui\/script\/(fetch-pdfium|make-fonts)$)|(^|\/)macos(\/|\.rs$)|(^|\/)macos_[^/]*\.rs$|_macos\.rs$/;
const NO_STEP = E2E_TABLE.filter(([, need]) => Array.isArray(need) && need.length === 0).map(([pattern]) => pattern);
const MACOS_CFG = /^(?:[ \t]*(?:\/\/[^\n]*)?\n)*[ \t]*#!\[cfg\(target_os\s*=\s*"macos"\)\]/;

const readRepoFile = (file) => {
  try {
    return readFileSync(path.join(repo, file), "utf8");
  } catch {
    return null;
  }
};

/**
 * Why a file does not need the whole Windows scenario, or null when it does.
 * `read` returns a file's text at the commit (null once it is gone), for
 * the cfg attribute and a patch diff's targets.
 */
export function smokeReason(file, read = readRepoFile) {
  if (MACOS_ONLY.test(file)) return "macOS only";
  if (NO_STEP.some((pattern) => pattern.test(file))) return "read by no e2e step";
  if (/^gpui\/.*\.rs$/.test(file) && MACOS_CFG.test(read(file) ?? "")) return 'macOS only (#![cfg(target_os = "macos")])';
  if (/^gpui\/patches\/[^/]+\.diff$/.test(file)) {
    const targets = [...(read(file) ?? "").matchAll(/^(?:---|\+\+\+) [ab]\/(\S+)/gm)].map((match) => `gpui/patches/${match[1]}`);
    if (targets.length && targets.every((target) => target.endsWith(".md") || smokeReason(target, read)?.startsWith("macOS only"))) return "macOS only (a patch to macOS-only files)";
  }
  return null;
}

/**
 * The Windows e2e for these changed GPUI inputs (ciTier's gpuiFiles):
 * { e2e: "full" | "smoke", why }, where why names the first files that need
 * the whole scenario, or each file's reason for the smoke flow.
 */
export function windowsE2e(gpuiFiles, { full = false, read = readRepoFile } = {}) {
  if (full) return { e2e: WHOLE, why: ["full run"] };
  const whole = [];
  const smoke = [];
  for (const file of gpuiFiles) {
    const reason = smokeReason(file, read);
    if (reason) smoke.push(`${file}: ${reason}`);
    else whole.push(file);
  }
  if (!gpuiFiles.length) return { e2e: WHOLE, why: ["no changed GPUI input to judge"] };
  if (whole.length) return { e2e: WHOLE, why: whole.slice(0, 5) };
  return { e2e: SMOKE, why: smoke.slice(0, 5) };
}
