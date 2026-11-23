// @vitest-environment node
import { describe, expect, it } from "vitest";
import { ciTier } from "./ci-tier.mjs";
import { SMOKE, WHOLE, smokeReason, windowsE2e } from "./windows-e2e-tier.mjs";

const scan = { testReads: [], rustTargets: [], gpuiTargets: [], harness: [] };
const files = {
  "gpui/crates/lattice-gpui/src/mac_only.rs": '//! Only on a Mac.\n#![cfg(target_os = "macos")]\nuse objc2;\n',
  "gpui/crates/lattice-gpui/src/windows_only.rs": "#![cfg(windows)]\n",
  "gpui/patches/gpui-fast-lab-occlusion.diff": "--- a/gpui-fast/crates/gpui_macos/src/window.rs\n+++ b/gpui-fast/crates/gpui_macos/src/window.rs\n",
  "gpui/patches/gpui-fast-ime-bindings.diff": "+++ b/gpui-fast/crates/gpui/src/platform.rs\n+++ b/gpui-fast/crates/gpui_macos/src/events.rs\n",
  "gpui/patches/gpui-fast-drop-neutral.diff": "--- a/gpui-fast/crates/gpui/src/platform.rs\n+++ /dev/null\n--- a/gpui-fast/crates/gpui_macos/src/events.rs\n+++ b/gpui-fast/crates/gpui_macos/src/events.rs\n",
  "gpui/patches/gpui-fast-move-into-macos.diff": "--- a/gpui-fast/crates/gpui/src/fast/cursor.rs\n+++ b/gpui-fast/crates/gpui_macos/src/fast/cursor.rs\n",
  "gpui/patches/gpui-fast-drop-macos.diff": "--- a/gpui-fast/crates/gpui_macos/src/fast/old.rs\n+++ /dev/null\n",
};
const read = (file) => files[file] ?? null;
const e2eFor = (changed, options = {}) => windowsE2e(ciTier(changed, { scan }).gpuiFiles, { read, ...options }).e2e;

describe("windows-e2e-tier", () => {
  it("runs the smoke flow for macOS-only code", () => {
    for (const file of [
      "gpui/patches/gpui-fast/crates/gpui_macos/src/window.rs",
      "gpui/patches/gpui-fast/crates/gpui_apple/src/text_system.rs",
      "gpui/patches/gpui-component/src/native_menu/macos.rs",
      "gpui/patches/gpui-base/src/macos_accessibility.rs",
      "gpui/patches/gpui-webview/src/macos.rs",
      "gpui/script/fetch-pdfium",
      "gpui/parity/run.mjs",
      "gpui/crates/lattice-gpui/src/mac_only.rs",
      "gpui/patches/gpui-fast-lab-occlusion.diff",
      "gpui/patches/gpui-fast-drop-macos.diff",
    ]) {
      expect(smokeReason(file, read), file).toMatch(/^macOS only/);
      expect(e2eFor([file]), file).toBe(SMOKE);
    }
  });

  it("runs the smoke flow for files no step reads", () => {
    expect(e2eFor(["gpui/README.md", "gpui/crates/lattice-editor/tests/motion.rs", "gpui/crates/lattice-gpui/src/pdf/tests.rs"])).toBe(SMOKE);
  });

  it("runs the whole scenario for anything that can behave differently on Windows", () => {
    for (const file of [
      "gpui/crates/lattice-gpui/src/pdf/view/notes.rs",
      "gpui/crates/lattice-gpui/src/windows_only.rs",
      "gpui/crates/lattice-gpui/src/shell/windows.rs",
      "gpui/patches/gpui-component/src/native_menu/windows.rs",
      "gpui/patches/gpui-fast/crates/gpui/src/window.rs",
      "gpui/patches/gpui-fast-ime-bindings.diff",
      "gpui/patches/gpui-fast-drop-neutral.diff",
      "gpui/patches/gpui-fast-move-into-macos.diff",
      "src/editor/spreadsheet/grid.ts",
      "src/i18n/catalog.ts",
      "gpui/islands/markdown/main.tsx",
      "gpui/script/fetch-pdfium.ps1",
      "gpui/script/e2e",
      "gpui/fixtures/e2e/main.tex",
      "gpui/crates/lattice-gpui/src/scenario/e2e/documents.rs",
      "gpui/Cargo.lock",
      "src-tauri/lattice-core/src/lib.rs",
      "design-system/tokens.json",
      ".github/workflows/gpui.yml",
      "scripts/windows-e2e-tier.mjs",
      "scripts/ci-tier.mjs",
    ]) {
      expect(e2eFor([file]), file).toBe(WHOLE);
    }
  });

  it("runs the whole scenario when any one file needs it, and names it", () => {
    const tier = windowsE2e(["gpui/patches/gpui-fast/crates/gpui_macos/src/window.rs", "gpui/crates/lattice-gpui/src/app.rs"], { read });
    expect(tier).toEqual({ e2e: WHOLE, why: ["gpui/crates/lattice-gpui/src/app.rs"] });
  });

  it("runs the whole scenario for a full run and when there is nothing to judge", () => {
    expect(windowsE2e(["gpui/README.md"], { full: true, read }).e2e).toBe(WHOLE);
    expect(windowsE2e([], { read }).e2e).toBe(WHOLE);
  });

  it("reads the checked-in sources and patches", () => {
    expect(smokeReason("gpui/patches/gpui-fast-lab-occlusion.diff")).toMatch(/^macOS only/);
    expect(smokeReason("gpui/patches/gpui-fast-native-cursor.diff")).toBeNull();
  });
});
