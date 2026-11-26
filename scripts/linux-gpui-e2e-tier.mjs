/**
 * How much of the GPUI e2e scenario gpui.yml's Linux job runs for a change:
 * the whole scenario (`full`), or the smoke flow alone (`smoke`, as on
 * Windows: scripts/windows-e2e-tier.mjs). The job always runs the release
 * build and every unit test first.
 *
 * Under Xvfb on llvmpipe the whole scenario takes about 35 minutes of a
 * 70-minute job, so a change runs it only when it touches what can behave
 * differently on Linux:
 *
 *   - the vendored GPUI crates and their patch diffs (gpui/patches/,
 *     gpui/compat/): the X11 platform, the renderer, gpui-webview's
 *     WebKitGTK. A file only macOS or Windows compiles (by its name or its
 *     first cfg attribute) is left out, and a patch diff counts by the
 *     files it touches;
 *   - Linux-only code anywhere under gpui/: a file or directory named for
 *     Linux (linux.rs, linux_*.rs, *_linux.rs, linux/), or a source whose
 *     first attribute is #![cfg(target_os = "linux")];
 *   - the scenario's harness and fixtures: scenario.rs and scenario/ apart
 *     from the e2e modules themselves (e2e.rs and e2e/registry.rs count as
 *     harness), gpui/script/e2e and gpui/fixtures/;
 *   - the web modules the islands are built from (gpui/islands/, and the
 *     src/ files GPUI reads): on Linux they render in WebKitGTK, a WebKit
 *     port of the distribution's own version that the macOS run (WKWebView)
 *     does not stand for;
 *   - the GPUI manifests and toolchain, the workspace's and each crate's
 *     (a crate's Linux-only dependency table moves no line in Cargo.lock);
 *   - the Linux job and what it runs or judges with: gpui.yml, ci-tier.mjs,
 *     this file and windows-e2e-tier.mjs it builds on, the verdict script
 *     and its main baseline, the known-failure list, the e2e module map and
 *     the stand-in Typst release.
 *
 * Two kinds of change stay on the smoke flow though Linux may see them
 * differently: an edit inside an item-level #[cfg(target_os = "linux")]
 * in a file that is not Linux-only, and a changed e2e module (its steps run
 * on macOS, and on Linux once main does).
 *
 * Anything else, platform-neutral app code included, takes the smoke flow on
 * a branch: the macOS job (or the CI-local trailer that stands for it) and
 * Windows run what the diff picks there. Every tag and full run takes the
 * whole Linux scenario.
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { SMOKE, WHOLE, smokeReason } from "./windows-e2e-tier.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const VENDORED = /^gpui\/(patches|compat)\//;
const WINDOWS_ONLY =
  /^gpui\/patches\/gpui-fast\/crates\/gpui_windows\/|(^|\/)(windows|win32)(\/|\.rs$)|(^|\/)(windows|win32)_[^/]*\.rs$|_(windows|win32)\.rs$|\.ps1$/;
const WINDOWS_CFG =
  /^(?:[ \t]*(?:\/\/[^\n]*)?\n)*[ \t]*#!\[cfg\((?:windows|target_os\s*=\s*"windows")\)\]/;
const LINUX_NAMED =
  /^gpui\/(?:.*\/)?(linux(\/|\.rs$)|linux_[^/]*\.rs$|[^/]*_linux\.rs$)/;
const LINUX_CFG =
  /^(?:[ \t]*(?:\/\/[^\n]*)?\n)*[ \t]*#!\[cfg\(target_os\s*=\s*"linux"\)\]/;
const HARNESS =
  /^gpui\/(crates\/lattice-gpui\/src\/scenario(\.rs$|\/(?!e2e\/)|\/e2e\/registry\.rs$)|script\/e2e$|fixtures\/)/;
const MANIFESTS =
  /^gpui\/(Cargo\.(toml|lock)|rust-toolchain\.toml|\.cargo\/|crates\/[^/]+\/(Cargo\.toml|build\.rs)$)/;
const WEB = /^(gpui\/islands\/|src\/)/;
const JOB =
  /^(\.github\/workflows\/gpui\.yml|scripts\/(gpui-e2e-verdict\.mjs|gpui-e2e-known-failures\.json|ci-tier\.mjs|linux-gpui-e2e-tier\.mjs|windows-e2e-tier\.mjs|gpui-main-baseline\.mjs|gpui-e2e-modules\.mjs|typst-helper-fake-release\.mjs))$/;

const readRepoFile = (file) => {
  try {
    return readFileSync(path.join(repo, file), "utf8");
  } catch {
    return null;
  }
};

const otherPlatform = (file, read) =>
  WINDOWS_ONLY.test(file) ||
  (file.endsWith(".rs") && WINDOWS_CFG.test(read(file) ?? "")) ||
  Boolean(smokeReason(file, read)?.startsWith("macOS only"));

/**
 * Why a file needs the whole Linux scenario, or null when the smoke flow
 * covers it. `read` returns a file's text at the commit (null once it is
 * gone), for the cfg attribute and a patch diff's targets.
 */
export function wholeReason(file, read = readRepoFile) {
  if (file.endsWith(".md")) return null;
  if (JOB.test(file)) return "the Linux job or what judges it";
  if (smokeReason(file, read) === "read by no e2e step") return null;
  if (HARNESS.test(file)) return "the scenario's harness";
  if (MANIFESTS.test(file)) return "the GPUI manifests";
  if (WEB.test(file) && !/\.(test|spec)\.[cm]?[jt]sx?$/.test(file))
    return "web code the islands render in WebKitGTK";
  if (/^gpui\/patches\/[^/]+\.diff$/.test(file)) {
    const targets = [
      ...(read(file) ?? "").matchAll(/^(?:---|\+\+\+) [ab]\/(\S+)/gm),
    ].map((match) => `gpui/patches/${match[1]}`);
    return !targets.length ||
      targets.some((target) => wholeReason(target, read))
      ? "a patch to vendored code Linux builds"
      : null;
  }
  if (VENDORED.test(file))
    return otherPlatform(file, read) ? null : "vendored code Linux builds";
  if (
    LINUX_NAMED.test(file) ||
    (/^gpui\/.*\.rs$/.test(file) && LINUX_CFG.test(read(file) ?? ""))
  )
    return "Linux-only code";
  return null;
}

/**
 * The Linux e2e for these changed GPUI inputs (ciTier's gpuiFiles):
 * { e2e: "full" | "smoke", why }, where why names the first files that need
 * the whole scenario and their reasons.
 */
export function linuxE2e(
  gpuiFiles,
  { full = false, read = readRepoFile } = {},
) {
  if (full) return { e2e: WHOLE, why: ["full run"] };
  if (!gpuiFiles.length)
    return { e2e: WHOLE, why: ["no changed GPUI input to judge"] };
  const whole = gpuiFiles.flatMap((file) => {
    const reason = wholeReason(file, read);
    return reason ? [`${file}: ${reason}`] : [];
  });
  if (whole.length) return { e2e: WHOLE, why: whole.slice(0, 5) };
  return {
    e2e: SMOKE,
    why: ["nothing it touches behaves differently on Linux"],
  };
}
