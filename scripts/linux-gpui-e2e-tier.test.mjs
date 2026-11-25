// @vitest-environment node
import { describe, expect, it } from "vitest";
import { ciTier } from "./ci-tier.mjs";
import { linuxE2e, wholeReason } from "./linux-gpui-e2e-tier.mjs";
import { SMOKE, WHOLE } from "./windows-e2e-tier.mjs";

const scan = { testReads: [], rustTargets: [], gpuiTargets: [], harness: [] };
const files = {
  "gpui/crates/lattice-gpui/src/linux_only.rs":
    '#![cfg(target_os = "linux")]\n',
  "gpui/patches/gpui-webview/src/windows_cfg.rs": "#![cfg(windows)]\n",
  "gpui/patches/gpui-webview-windows-overlays.diff":
    "--- a/gpui-webview/src/win32.rs\n+++ b/gpui-webview/src/win32.rs\n+++ b/gpui-webview/README.md\n",
  "gpui/patches/gpui-fast-overlay-bounds.diff":
    "+++ b/gpui-fast/crates/gpui/src/fast/overlay_bounds.rs\n",
  "gpui/patches/gpui-fast-lab-occlusion.diff":
    "--- a/gpui-fast/crates/gpui_macos/src/window.rs\n+++ b/gpui-fast/crates/gpui_macos/src/window.rs\n",
};
const read = (file) => files[file] ?? null;
const e2eFor = (changed, options = {}) =>
  linuxE2e(ciTier(changed, { scan }).gpuiFiles, { read, ...options }).e2e;

describe("linux-gpui-e2e-tier", () => {
  it("runs the smoke flow for platform-neutral app code and other platforms' code", () => {
    for (const file of [
      "gpui/crates/lattice-gpui/src/pdf/view/notes.rs",
      "gpui/crates/lattice-gpui/src/app.rs",
      "gpui/crates/lattice-gpui/src/scenario/e2e/island_overlays.rs",
      "gpui/crates/lattice-ui/src/button.rs",
      "src-tauri/lattice-core/src/lib.rs",
      "gpui/patches/gpui-fast/crates/gpui_macos/src/window.rs",
      "gpui/patches/gpui-fast/crates/gpui_windows/src/window.rs",
      "gpui/patches/gpui-webview/src/win32.rs",
      "gpui/patches/gpui-webview/src/windows_cfg.rs",
      "gpui/patches/gpui-webview-windows-overlays.diff",
      "gpui/patches/gpui-fast-lab-occlusion.diff",
      "gpui/script/fetch-pdfium.ps1",
      "gpui/README.md",
    ]) {
      expect(wholeReason(file, read), file).toBeNull();
      expect(e2eFor([file]), file).toBe(SMOKE);
    }
  });

  it("runs the whole scenario for anything that can behave differently on Linux", () => {
    for (const file of [
      "gpui/patches/gpui-fast/crates/gpui_linux/src/linux/x11/client.rs",
      "gpui/patches/gpui-fast/crates/gpui/src/window.rs",
      "gpui/patches/gpui-webview/src/region.rs",
      "gpui/patches/gpui-fast-overlay-bounds.diff",
      "gpui/compat/gpui-fast/Cargo.toml",
      "gpui/crates/lattice-gpui/src/native_linux.rs",
      "gpui/crates/lattice-gpui/src/linux_only.rs",
      "gpui/crates/lattice-gpui/src/scenario.rs",
      "gpui/crates/lattice-gpui/src/scenario/shot.rs",
      "gpui/crates/lattice-gpui/src/scenario/e2e.rs",
      "gpui/crates/lattice-gpui/src/scenario/e2e/registry.rs",
      "gpui/script/e2e",
      "gpui/fixtures/e2e/main.tex",
      "gpui/Cargo.lock",
      "gpui/crates/lattice-gpui/Cargo.toml",
      "gpui/islands/markdown/main.tsx",
      "src/editor/spreadsheet/grid.ts",
      "scripts/windows-e2e-tier.mjs",
      "scripts/typst-helper-fake-release.mjs",
      ".github/workflows/gpui.yml",
      "scripts/gpui-e2e-known-failures.json",
      "scripts/linux-gpui-e2e-tier.mjs",
    ]) {
      expect(wholeReason(file, read), file).not.toBeNull();
      expect(e2eFor([file]), file).toBe(WHOLE);
    }
  });

  it("names the files that need the whole scenario", () => {
    expect(
      linuxE2e(
        [
          "gpui/crates/lattice-gpui/src/app.rs",
          "gpui/crates/lattice-gpui/src/native_linux.rs",
        ],
        { read },
      ),
    ).toEqual({
      e2e: WHOLE,
      why: ["gpui/crates/lattice-gpui/src/native_linux.rs: Linux-only code"],
    });
  });

  it("runs the whole scenario for a full run and when there is nothing to judge", () => {
    expect(linuxE2e(["gpui/README.md"], { full: true, read }).e2e).toBe(WHOLE);
    expect(linuxE2e([], { read }).e2e).toBe(WHOLE);
  });

  it("reads the checked-in sources and patches", () => {
    expect(wholeReason("gpui/crates/lattice-gpui/src/native_linux.rs")).toBe(
      "Linux-only code",
    );
    expect(wholeReason("gpui/patches/gpui-webview-windows-overlays.diff")).toBe(
      "a patch to vendored code Linux builds",
    );
  });
});
