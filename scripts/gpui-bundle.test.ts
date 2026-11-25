import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { parse } from "yaml";
import { ARCHES, archOf, EXECUTABLE, FLAVORS, infoPlist, releaseRepository, typstHelperFiles, updaterManifest } from "./gpui-bundle.mjs";

// The GPUI package ships two ways. Lattice Beta never reaches the Tauri app's
// users: it installs under its own identity and its feed is not the Tauri
// feed. Lattice, the native app, takes the Tauri app's identity so either
// updater can install it in place when the writer chooses it, and only
// release.yml publishes it: beside the Tauri app, under its own file names,
// with its own feed (latest-native.json). The Tauri feed stays the Tauri app's.
const repo = path.resolve(__dirname, "..");
const read = (file: string) => readFileSync(path.join(repo, file), "utf8");
const tauri = JSON.parse(read("src-tauri/tauri.conf.json"));
type Step = { name?: string; uses?: string; run?: string; if?: string; with?: Record<string, unknown> };
type Job = { steps?: Step[]; uses?: string; needs?: string[] | string; if?: string; with?: Record<string, unknown> };
const workflow = (file: string) => parse(read(file)) as {
  on: Record<string, unknown>;
  permissions: Record<string, string>;
  jobs: Record<string, Job>;
};
const flavor = read("gpui/crates/lattice-gpui/src/flavor.rs");

describe("the GPUI bundle", () => {
  it("installs Lattice Beta beside Lattice.app under its own identity, with the Tauri bundle's keys", () => {
    const plist = infoPlist({ flavor: FLAVORS.beta, version: tauri.version, minimumSystemVersion: "14.0", extra: { NSMicrophoneUsageDescription: "mic" } });
    expect(FLAVORS.beta.identifier).not.toBe(tauri.identifier);
    expect(FLAVORS.beta.product).not.toBe(tauri.productName);
    expect(plist).toMatchObject({
      CFBundleIdentifier: FLAVORS.beta.identifier,
      CFBundleExecutable: EXECUTABLE,
      CFBundleShortVersionString: tauri.version,
      LSMinimumSystemVersion: "14.0",
      NSMicrophoneUsageDescription: "mic",
    });
  });

  it("makes Lattice under the Tauri app's name and identifier, with file names and a manifest of its own", () => {
    expect(infoPlist({ flavor: FLAVORS.main, version: "1.2.3", minimumSystemVersion: "14.0", extra: {} })).toMatchObject({
      CFBundleIdentifier: tauri.identifier,
      CFBundleName: tauri.productName,
      CFBundleDisplayName: tauri.productName,
    });
    expect(FLAVORS.main.product).toBe(tauri.productName);
    expect(FLAVORS.main.artifact).toBe("Lattice-native");
    expect(FLAVORS.main.manifest).toBe("latest-native.json");
    expect(FLAVORS.main.feed("1.2.3", "leo1oel/lattice")).toBe("https://github.com/leo1oel/lattice/releases/download/v1.2.3");
    expect(FLAVORS.main.feed("1.2.3", "lattice-writer/lattice")).toBe("https://github.com/lattice-writer/lattice/releases/download/v1.2.3");
    // tauri-action names the Tauri app's files <productName>_<version>_*, and
    // the Tauri feed is latest.json: the native app's must never collide.
    expect(FLAVORS.main.artifact.startsWith(`${tauri.productName}_`)).toBe(false);
    for (const endpoint of tauri.plugins.updater.endpoints) expect(endpoint.endsWith(`/${FLAVORS.main.manifest}`)).toBe(false);
  });

  it("names each flavor as the GPUI app names itself", () => {
    for (const { product } of Object.values(FLAVORS)) expect(flavor).toContain(`name: "${product}"`);
    expect(flavor).toContain(`lattice_settings::import::TAURI_BUNDLE_ID`);
    expect(read("gpui/crates/lattice-settings/src/import.rs")).toContain(`"${FLAVORS.main.identifier}"`);
  });

  it("names the releases of the repository the workflow runs in", () => {
    expect(releaseRepository({ GITHUB_REPOSITORY: "lattice-writer/lattice" })).toBe("lattice-writer/lattice");
    expect(releaseRepository({})).toBe("lattice-writer/lattice");
  });

  it("writes Lattice Beta's manifest for a feed apart from the Tauri app's", () => {
    for (const repository of ["lattice-writer/lattice", "leo1oel/lattice"]) {
      expect(flavor).toContain(`"${FLAVORS.beta.feed(undefined, repository)}/latest.json"`);
    }
    const feed = FLAVORS.beta.feed();
    const manifest = updaterManifest({ version: "1.2.3", signature: "sig", url: `${feed}/a.app.tar.gz`, pubDate: "2026-01-01T00:00:00Z" });
    expect(manifest.platforms["darwin-aarch64"]).toEqual({ signature: "sig", url: `${feed}/a.app.tar.gz` });
    expect(manifest.platforms["darwin-aarch64-app"]).toEqual(manifest.platforms["darwin-aarch64"]);
    expect(FLAVORS.beta.manifest).toBe("latest.json");
    for (const endpoint of tauri.plugins.updater.endpoints) expect(endpoint.startsWith(feed)).toBe(false);
  });

  it("names the Intel build as tauri-action names the Tauri app's (_x64) and lists it under darwin-x86_64", () => {
    expect(ARCHES.aarch64).toMatchObject({ suffix: "aarch64", platform: "darwin-aarch64", pdfium: "mac-arm64" });
    expect(ARCHES.x86_64).toMatchObject({ suffix: "x64", platform: "darwin-x86_64", pdfium: "mac-x64" });
    expect(() => archOf("arm64")).toThrow(/--arch is one of aarch64, x86_64/);
    const manifest = updaterManifest({ version: "1.2.3", signature: "sig", url: "u", arch: "x86_64", pubDate: "2026-01-01T00:00:00Z" });
    expect(Object.keys(manifest.platforms)).toEqual(["darwin-x86_64", "darwin-x86_64-app"]);
    expect(manifest.platforms["darwin-x86_64-app"]).toEqual({ signature: "sig", url: "u" });
    // The PDFium folders fetch-pdfium pins and the app looks in (pdf/engine.rs).
    for (const { pdfium } of Object.values(ARCHES)) {
      expect(read("gpui/script/fetch-pdfium")).toContain(`platform=${pdfium};`);
      expect(read("gpui/crates/lattice-gpui/src/pdf/engine.rs")).toContain(`"${pdfium}/lib"`);
    }
  });
});

describe("the GPUI package workflow", () => {
  const package_ = workflow(".github/workflows/gpui-package.yml");
  const steps = Object.values(package_.jobs).flatMap((job) => job.steps ?? []);
  const scripts = steps.map((step) => step.run ?? "").join("\n");

  it("runs by hand or from the release workflow, with read-only access", () => {
    expect(Object.keys(package_.on).sort()).toEqual(["workflow_call", "workflow_dispatch"]);
    expect(package_.permissions).toEqual({ contents: "read" });
    expect(scripts).toContain('--flavor "$FLAVOR"');
    expect(scripts).toContain('--arch "$ARCH"');
  });

  it("builds aarch64 on the Mac mini and x86_64 on GitHub's Intel runner, checking every Intel Mach-O", () => {
    const call = package_.on.workflow_call as { inputs: Record<string, { default?: string }> };
    const dispatch = package_.on.workflow_dispatch as { inputs: Record<string, { default?: string; options?: string[] }> };
    expect(call.inputs.arch.default).toBe("aarch64");
    expect(dispatch.inputs.arch).toMatchObject({ default: "aarch64", options: ["aarch64", "x86_64"] });
    const runsOn = String((package_.jobs.macos as { "runs-on"?: string })["runs-on"]);
    expect(runsOn).toContain("inputs.arch == 'x86_64' && 'macos-15-intel'");
    expect(runsOn).toContain("vars.LATTICE_MACOS_RUNNER || 'lattice-mac'");
    expect(scripts).toContain(".github/scripts/check-macos-arch.sh \"$app_path\" x86_64");
    expect(steps.find((step) => step.uses?.startsWith("actions/checkout"))?.with?.ref).toBe("${{ inputs.ref || github.sha }}");
    const upload = steps.find((step) => step.uses?.startsWith("actions/upload-artifact"));
    expect(upload?.with?.name).toBe("lattice-gpui-macos-${{ inputs.arch }}-${{ inputs.flavor }}");
  });

  it("publishes nothing: no release, tag or upload, only the run's artifact", () => {
    expect(scripts).not.toMatch(/gh release|git push|git tag/);
    expect(steps.some((step) => step.uses?.startsWith("tauri-apps/"))).toBe(false);
    expect(steps.some((step) => step.uses?.startsWith("actions/upload-artifact"))).toBe(true);
    expect(steps.some((step) => step.uses?.startsWith("actions/cache/save") || step.with?.["save-if"] === true)).toBe(false);
  });

  it("packages the Typst helper the cargo build makes, checks its signatures and keeps it in the artifact", () => {
    expect(scripts).toContain("--typst-helper gpui/target/release/lattice-typst");
    expect(scripts).toContain("codesign --verify --strict --verbose=2 \"$helper_dir/lattice-typst\"");
    const upload = steps.find((step) => step.uses?.startsWith("actions/upload-artifact"));
    expect(String(upload?.with?.path)).toContain("gpui-bundle/Lattice-typst_*_${{ env.SUFFIX }}.tar.gz.sig");
    expect(read("gpui/Cargo.toml")).toMatch(/default-members = \[[^\]]*"crates\/lattice-typst"/);
  });

  it("names the Typst helper's archive as the native app downloads it", () => {
    expect(typstHelperFiles("1.2.3")).toEqual({
      archive: "Lattice-typst_1.2.3_aarch64.tar.gz",
      signature: "Lattice-typst_1.2.3_aarch64.tar.gz.sig",
    });
    expect(typstHelperFiles("1.2.3", "x86_64")).toEqual({
      archive: "Lattice-typst_1.2.3_x64.tar.gz",
      signature: "Lattice-typst_1.2.3_x64.tar.gz.sig",
    });
    const install = read("gpui/crates/lattice-gpui/src/typst/install.rs");
    expect(install).toContain('"Lattice-typst_{}_{suffix}.tar.gz"');
    expect(install).toContain('("macos", "aarch64") => "aarch64"');
    expect(install).toContain('("macos", "x86_64") => "x64"');
  });

  it("keeps either flavor's manifest in the artifact (latest.json or latest-native.json)", () => {
    const upload = steps.find((step) => step.uses?.startsWith("actions/upload-artifact"));
    expect(String(upload?.with?.path)).toContain("gpui-bundle/latest*.json");
    for (const { manifest } of Object.values(FLAVORS)) expect(manifest).toMatch(/^latest.*\.json$/);
  });
});

describe("the release workflow", () => {
  const release = workflow(".github/workflows/release.yml");
  const publish = release.jobs.publish.steps ?? [];
  const names = publish.map((step) => step.name ?? step.uses ?? "");
  const at = (name: string) => names.findIndex((candidate) => candidate.startsWith(name));
  const script = (name: string) => publish[at(name)]?.run ?? "";

  it("builds the native app on every release, beside the Tauri app", () => {
    expect(Object.keys(release.jobs).sort()).toEqual(["gpui", "publish", "release"]);
    expect(release.jobs.gpui).toMatchObject({ uses: "./.github/workflows/gpui-package.yml", with: { flavor: "main" } });
    expect(release.jobs.gpui.if).toBeUndefined();
    expect(release.jobs.gpui.needs).toBeUndefined();
  });

  it("publishes only from the last job, and only when both apps built", () => {
    expect(release.jobs.publish.needs).toEqual(["release", "gpui"]);
    expect(release.jobs.publish.if).toBeUndefined();
    expect(names.at(-1)).toBe("Publish verified release");
    for (const job of ["release", "gpui"]) {
      expect((release.jobs[job].steps ?? []).map((step) => step.run ?? "").join("\n")).not.toContain("--draft=false");
    }
  });

  it("keeps latest.json the Tauri app's and uploads the native app's checked files under their own names", () => {
    expect(at("Use public download URLs")).toBeLessThan(at("Upload the updater manifest"));
    expect(at("Upload the native app")).toBeGreaterThan(-1);
    expect(at("Upload the native app")).toBeLessThan(at("Check the release assets"));
    expect(script("Upload the native app")).toContain("node scripts/check-native-feed.mjs");
    expect(script("Upload the native app")).toContain("--clobber");
    expect(publish.find((step) => step.uses?.startsWith("actions/download-artifact"))?.with).toMatchObject({ name: "lattice-gpui-macos-aarch64-main" });
    expect(publish.some((step) => step.if)).toBe(false);
    expect(publish.map((step) => step.run ?? "").join("\n")).not.toMatch(/release-apps|replace-updater-archive/);
  });

  it("uploads the Typst helper under the version's tag once check-native-feed has verified it, before the asset check", () => {
    expect(at("Upload Typst support")).toBeGreaterThan(at("Upload the native app"));
    expect(at("Upload Typst support")).toBeLessThan(at("Check the release assets"));
    expect(script("Upload Typst support")).toContain('node scripts/check-native-feed.mjs --typst "$GITHUB_REF_NAME"');
    expect(script("Upload Typst support")).toContain('test "${#files[@]}" -eq 2');
  });

  it("hands Macs the native app in latest.json once the native files and normalized latest.json are up", () => {
    const hand = at("Hand Macs the native app");
    expect(hand).toBeGreaterThan(at("Use public download URLs"));
    expect(hand).toBeGreaterThan(at("Upload the updater manifest"));
    expect(hand).toBeGreaterThan(at("Upload the native app"));
    expect(hand).toBeGreaterThan(at("Upload Typst support"));
    expect(hand).toBeLessThan(at("Check the release assets"));
    expect(script("Hand Macs the native app")).toContain(
      'node scripts/hand-macs-native.mjs "$MANIFEST_DIR/latest.json" "$RUNNER_TEMP/gpui/latest-native.json" darwin-aarch64 darwin-aarch64-app',
    );
    expect(script("Hand Macs the native app")).toContain('gh release upload "$GITHUB_REF_NAME" "$MANIFEST_DIR/latest.json" --clobber');
  });
});

describe("the macOS Intel workflow", () => {
  const intel = workflow(".github/workflows/macos-intel.yml");
  const attach = intel.jobs.attach.steps ?? [];
  const scripts = attach.map((step) => step.run ?? "").join("\n");

  it("builds the native app for x86_64 from the release's commit", () => {
    expect(intel.jobs.native).toMatchObject({
      uses: "./.github/workflows/gpui-package.yml",
      with: { flavor: "main", arch: "x86_64", ref: "${{ github.event.workflow_run.head_sha || github.sha }}" },
    });
    expect(intel.jobs.native.if).toBe(intel.jobs.build.if);
  });

  it("attaches nothing for Intel unless the native build passed and launched on Intel too", () => {
    expect(intel.jobs.attach.needs).toEqual(["build", "smoke", "native", "native-smoke"]);
    expect(attach.find((step) => step.with?.name === "lattice-gpui-macos-x86_64-main")).toBeTruthy();
  });

  it("checks the native files, then merges darwin-x86_64 into latest-native.json and hands it to latest.json", () => {
    expect(scripts).toContain('node scripts/check-native-feed.mjs --arch x86_64 "$TAG"');
    expect(scripts).toContain('node scripts/check-native-feed.mjs --typst --arch x86_64 "$TAG"');
    const merge = scripts.indexOf("node scripts/add-updater-platform.mjs");
    const hand = scripts.indexOf("node scripts/hand-macs-native.mjs");
    expect(merge).toBeGreaterThan(scripts.indexOf("gh release upload"));
    expect(hand).toBeGreaterThan(merge);
    expect(scripts.slice(merge, hand)).toContain('"$manifest_dir/latest-native.json"');
    expect(scripts.slice(hand)).toContain("darwin-x86_64 darwin-x86_64-app");
    expect(scripts.indexOf('upload "$TAG" "$manifest_dir/latest-native.json"')).toBeLessThan(scripts.indexOf('upload "$TAG" "$manifest_dir/latest.json"'));
  });
});
