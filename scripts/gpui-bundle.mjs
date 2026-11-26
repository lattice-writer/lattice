#!/usr/bin/env node
/**
 * `node scripts/gpui-bundle.mjs`: packages the GPUI app for macOS as the
 * release workflow packages the Tauri app (.github/workflows/release.yml), so
 * gpui-release.yml and a developer's Mac make the same bundle.
 *
 *   1. the release binary: built through scripts/gpui-build.mjs (the shared
 *      build tree, without the `offscreen-shots` test feature), or the one
 *      `--binary` names (CI builds it in gpui/ itself, with its Rust cache);
 *   2. what it loads from its bundle (gpui/crates/lattice-gpui/src/structured/
 *      resources.rs, pdf/engine.rs): PDFium in Contents/Frameworks, and in
 *      Contents/Resources the island pages (gpui/script/build-islands), the
 *      Agent's Synara runtime and the Open Slide runtime staged as the Tauri
 *      build stages them (prepare-synara-sidecar.mjs needs SYNARA_SOURCE_DIR,
 *      docs/synara-runtime.md; prepare-presentation-runtime.mjs), the bundled
 *      skills, the icon and the Chinese InfoPlist.strings;
 *   3. an Info.plist with the Tauri bundle's keys (src-tauri/Info.plist, the
 *      version and minimum macOS from tauri.conf.json) under the flavor's
 *      name and identifier (FLAVORS below): Lattice Beta, which installs
 *      beside Lattice.app, or Lattice, the Tauri app's own identity, which a
 *      writer who chooses the native app installs in its place;
 *   4. signatures, inside out, with the hardened runtime and
 *      src-tauri/Entitlements.plist: the Developer ID in
 *      APPLE_SIGNING_IDENTITY (as release.yml exports it), else ad hoc;
 *   5. notarization of the app and then the disk image, each stapled, when
 *      APPLE_API_KEY, APPLE_API_ISSUER and APPLE_API_KEY_PATH are set (the
 *      App Store Connect key release.yml uses) and the identity is a real one;
 *   6. the outputs in --out: the .dmg (hdiutil, which opens no Finder
 *      window, unlike Tauri's dmg bundler), a .zip, and the updater archive
 *      (.app.tar.gz) with its minisign .sig (TAURI_SIGNING_PRIVATE_KEY and
 *      _PASSWORD, the Tauri updater's key, through `tauri signer sign`) and the
 *      flavor's manifest for it (latest.json or latest-native.json, read by
 *      crates/lattice-gpui/src/updater.rs).
 *
 * Without keys every step still runs: the app is signed ad hoc, nothing is
 * notarized, and the archive has no .sig (so no manifest). It publishes
 * nothing; gpui/docs/updater.md says how the channel is published.
 *
 * Usage:
 *   node scripts/gpui-bundle.mjs [--flavor beta|main] [--arch aarch64|x86_64]
 *     [--binary PATH] [--out DIR] [--skip-islands] [--skip-runtimes] [--no-dmg]
 *     [--feed URL] [--typst-helper PATH]
 *     --flavor         beta (default): Lattice Beta; main: Lattice
 *     --arch           aarch64 (default): Apple silicon; x86_64: Intel (ARCHES
 *                      below). The binary and the staged runtimes must be
 *                      built for it: CI builds x86_64 on an Intel runner, so
 *                      prepare-synara-sidecar.mjs stages the host's Node
 *     --skip-islands   reuse gpui/islands/dist
 *     --skip-runtimes  reuse the staged src-tauri/synara-runtime and
 *                      presentation-runtime as they are
 *     --feed URL       where the manifest and the archive are published
 *                      (default: the flavor's, FLAVORS below)
 *     --typst-helper PATH  also package the Typst helper (gpui/target/release/
 *                      lattice-typst): signed and notarized as the app is,
 *                      packed as Lattice-typst_<version>_<suffix>.tar.gz with
 *                      its minisign .sig beside the app's files
 */
import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { codesign, signMachOTree } from "./lib/codesign.mjs";

/**
 * The repository whose releases the archive is published to: the workflow's
 * own (`GITHUB_REPOSITORY`), so a release cut after the move to the
 * lattice-writer organization names its own address and never leans on
 * GitHub's redirect from the old one.
 */
export const releaseRepository = (env = process.env) => env.GITHUB_REPOSITORY || "lattice-writer/lattice";

/**
 * The apps the GPUI binary ships as; it reads which one it is from the
 * bundle's identifier (gpui/crates/lattice-gpui/src/flavor.rs). `artifact`
 * names the files (<artifact>_<version>_<suffix>.*), `manifest` the updater
 * manifest written beside them, and `feed` the folder the archive is
 * downloaded from, which the manifest's URL names.
 */
export const FLAVORS = {
  // Installed beside the Tauri Lattice, with its own data and its own
  // channel: the lattice-beta prerelease (gpui/docs/updater.md).
  beta: {
    product: "Lattice Beta",
    identifier: "app.leo1oel.lattice-beta",
    artifact: "Lattice-Beta",
    manifest: "latest.json",
    feed: (_version, repository = releaseRepository()) => `https://github.com/${repository}/releases/download/lattice-beta`,
  },
  // The native app: the Tauri app's name and identifier, so whichever
  // updater installs it puts it in place of Lattice.app and it keeps the
  // app's privacy grants, Keychain items and Dock place. Every release
  // carries it beside the Tauri app, under names of its own so it never
  // replaces a Tauri asset, with its own manifest (latest-native.json): the
  // native app updates from it, and from 0.20.0 latest.json hands Macs it too
  // (.github/workflows/release.yml, gpui/docs/updater.md). The bundle in the
  // dmg and the archive is still Lattice.app.
  main: {
    product: "Lattice",
    identifier: "app.leo1oel.researchwriter",
    artifact: "Lattice-native",
    manifest: "latest-native.json",
    feed: (version, repository = releaseRepository()) => `https://github.com/${repository}/releases/download/v${version}`,
  },
};
export const EXECUTABLE = "lattice-gpui";

/**
 * The Macs the GPUI app ships for. `suffix` ends the file names as
 * tauri-action ends the Tauri app's (Lattice_<version>_aarch64.dmg,
 * Lattice_<version>_x64.dmg), `platform` is the updater key Tauri and
 * crates/lattice-gpui/src/updater/manifest.rs read (darwin-<rust arch>), and
 * `pdfium` the folder gpui/script/fetch-pdfium unpacks PDFium into.
 */
export const ARCHES = {
  aarch64: { suffix: "aarch64", platform: "darwin-aarch64", pdfium: "mac-arm64" },
  x86_64: { suffix: "x64", platform: "darwin-x86_64", pdfium: "mac-x64" },
};

/** `name` as an ARCHES key, or a thrown error naming the choices. */
export function archOf(name) {
  const arch = ARCHES[name];
  if (!arch) throw new Error(`--arch is one of ${Object.keys(ARCHES).join(", ")}, not ${name}`);
  return arch;
}

/**
 * The Typst helper the native app downloads on first use
 * (gpui/crates/lattice-gpui/src/typst/install.rs names it the same way):
 * the archive holds the one binary at its root, and its .sig is the Tauri
 * updater key's, which the app checks it against.
 */
export const TYPST_HELPER = "lattice-typst";
export function typstHelperFiles(version, arch = "aarch64") {
  const archive = `Lattice-typst_${version}_${archOf(arch).suffix}.tar.gz`;
  return { archive, signature: `${archive}.sig` };
}

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauri = path.join(repo, "src-tauri");
const entitlements = path.join(tauri, "Entitlements.plist");

function run(command, args, options = {}) {
  console.log(`gpui-bundle: ${command} ${args.join(" ")}`);
  execFileSync(command, args, { stdio: "inherit", cwd: repo, ...options });
}

function capture(command, args, options = {}) {
  return execFileSync(command, args, { encoding: "utf8", cwd: repo, ...options });
}

/** The Info.plist keys: the Tauri bundle's own (src-tauri/Info.plist) under the flavor's identity. */
export function infoPlist({ flavor, version, minimumSystemVersion, extra }) {
  return {
    CFBundleDevelopmentRegion: "en",
    CFBundleDisplayName: flavor.product,
    CFBundleExecutable: EXECUTABLE,
    CFBundleIconFile: "icon.icns",
    CFBundleIdentifier: flavor.identifier,
    CFBundleInfoDictionaryVersion: "6.0",
    CFBundleName: flavor.product,
    CFBundlePackageType: "APPL",
    CFBundleShortVersionString: version,
    CFBundleVersion: version,
    LSApplicationCategoryType: "public.app-category.productivity",
    LSMinimumSystemVersion: minimumSystemVersion,
    NSHighResolutionCapable: true,
    ...extra,
  };
}

/**
 * The flavor's manifest, in the shape tauri-action writes latest.json
 * (crates/lattice-gpui/src/updater/manifest.rs reads it). It lists the
 * archive under both keys tauri-action writes for the arch (darwin-aarch64
 * and darwin-aarch64-app, or darwin-x86_64 and darwin-x86_64-app), since the
 * Tauri app's updater looks for the -app key first.
 */
export function updaterManifest({ version, signature, url, arch = "aarch64", pubDate = new Date().toISOString(), notes = "" }) {
  const entry = { signature, url };
  const { platform } = archOf(arch);
  return { version, notes, pub_date: pubDate, platforms: { [platform]: entry, [`${platform}-app`]: { ...entry } } };
}

/**
 * The Typst helper as the app downloads it: signed with the hardened runtime
 * and notarized like the app (a bare binary has no ticket to staple; Gatekeeper
 * looks the notarization up online), then packed and signed with the updater
 * key. Returns the files made.
 */
function packageTypstHelper({ binary, out, version, arch, identity, key }) {
  if (!existsSync(binary)) throw new Error(`no Typst helper at ${binary}`);
  const { archive, signature } = typstHelperFiles(version, arch);
  const stage = mkdtempSync(path.join(os.tmpdir(), "gpui-typst-"));
  try {
    const helper = path.join(stage, TYPST_HELPER);
    cpSync(binary, helper);
    codesign(helper, { identity });
    run("codesign", ["--verify", "--strict", "--verbose=2", helper]);
    if (key) {
      const upload = path.join(stage, "lattice-typst.notarize.zip");
      run("ditto", ["-c", "-k", helper, upload]);
      notarize(upload, key);
      rmSync(upload);
    }
    const target = path.join(out, archive);
    run("tar", ["-czf", target, "-C", stage, TYPST_HELPER], { env: { ...process.env, COPYFILE_DISABLE: "1" } });
    const outputs = [target];
    if (process.env.TAURI_SIGNING_PRIVATE_KEY) {
      rmSync(path.join(out, signature), { force: true });
      run(path.join(repo, "node_modules", ".bin", "tauri"), ["signer", "sign", target]);
      outputs.push(path.join(out, signature));
    }
    return outputs;
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}

function notarize(file, key) {
  run("xcrun", ["notarytool", "submit", file, "--key", key.path, "--key-id", key.id, "--issuer", key.issuer, "--wait"]);
}

async function main() {
  const { values } = parseArgs({
    options: {
      binary: { type: "string" },
      out: { type: "string" },
      flavor: { type: "string", default: "beta" },
      arch: { type: "string", default: "aarch64" },
      feed: { type: "string" },
      "skip-islands": { type: "boolean", default: false },
      "skip-runtimes": { type: "boolean", default: false },
      "no-dmg": { type: "boolean", default: false },
      "typst-helper": { type: "string" },
    },
  });
  if (process.platform !== "darwin") throw new Error("the GPUI app bundle is built on macOS only");
  const flavor = FLAVORS[values.flavor];
  if (!flavor) throw new Error(`--flavor is one of ${Object.keys(FLAVORS).join(", ")}, not ${values.flavor}`);
  const arch = archOf(values.arch);

  const config = JSON.parse(readFileSync(path.join(tauri, "tauri.conf.json"), "utf8"));
  const version = config.version;
  const feed = values.feed ?? flavor.feed(version);
  const minimumSystemVersion = config.bundle.macOS.minimumSystemVersion;
  const identityEnv = process.env.APPLE_SIGNING_IDENTITY?.trim();
  const identity = identityEnv && identityEnv !== "-" ? identityEnv : "-";
  const key = process.env.APPLE_API_KEY && process.env.APPLE_API_ISSUER && process.env.APPLE_API_KEY_PATH
    ? { id: process.env.APPLE_API_KEY, issuer: process.env.APPLE_API_ISSUER, path: process.env.APPLE_API_KEY_PATH }
    : null;
  if (key && identity === "-") throw new Error("notarization needs a Developer ID in APPLE_SIGNING_IDENTITY, not an ad hoc signature");
  const out = path.resolve(values.out ?? path.join(repo, "gpui", "target", "bundle"));
  mkdirSync(out, { recursive: true });

  // 1. The binary.
  let binary = values.binary && path.resolve(values.binary);
  if (!binary) {
    // gpui-build.mjs builds for the host; another arch's binary comes from --binary.
    if (ARCHES[{ arm64: "aarch64", x64: "x86_64" }[process.arch]] !== arch) throw new Error(`--arch ${values.arch} on this ${process.arch} Mac needs --binary`);
    run(process.execPath, [path.join(repo, "scripts", "gpui-build.mjs"), "--", "build", "--release", "--locked", "-p", "lattice-gpui"]);
    binary = path.join(repo, "gpui", "target", "release", EXECUTABLE);
  }
  if (!existsSync(binary)) throw new Error(`no binary at ${binary}`);

  // 2. What it loads from the bundle.
  run(path.join(repo, "gpui", "script", "fetch-pdfium"), [arch.pdfium]);
  const pdfium = path.join(repo, "gpui", "vendor", "pdfium", arch.pdfium, "lib", "libpdfium.dylib");
  if (!values["skip-islands"]) run(path.join(repo, "gpui", "script", "build-islands"), []);
  if (!values["skip-runtimes"]) {
    run(process.execPath, [path.join(repo, "scripts", "prepare-synara-sidecar.mjs")]);
    run(process.execPath, [path.join(repo, "scripts", "prepare-presentation-runtime.mjs")]);
  }
  const resources = {
    islands: path.join(repo, "gpui", "islands", "dist"),
    "synara-runtime": path.join(tauri, "synara-runtime"),
    "presentation-runtime": path.join(tauri, "presentation-runtime"),
    "src/embedded_skills": path.join(tauri, "src", "embedded_skills"),
  };
  const markers = {
    islands: "sheet/index.html",
    "synara-runtime": "server/dist/index.mjs",
    "presentation-runtime": "server.mjs",
    "src/embedded_skills": ".",
  };
  for (const [name, source] of Object.entries(resources)) {
    if (!existsSync(path.join(source, markers[name]))) throw new Error(`${name} is not staged: ${path.join(source, markers[name])} is missing`);
  }

  // 3. The bundle.
  const app = path.join(out, `${flavor.product}.app`);
  rmSync(app, { recursive: true, force: true });
  const contents = path.join(app, "Contents");
  for (const dir of ["MacOS", "Frameworks", "Resources/zh-Hans.lproj"]) mkdirSync(path.join(contents, dir), { recursive: true });
  cpSync(binary, path.join(contents, "MacOS", EXECUTABLE));
  cpSync(pdfium, path.join(contents, "Frameworks", "libpdfium.dylib"));
  // ditto keeps symlinks, modes and extended attributes as they are staged.
  for (const [name, source] of Object.entries(resources)) {
    const target = path.join(contents, "Resources", name);
    mkdirSync(path.dirname(target), { recursive: true });
    run("ditto", [source, target]);
  }
  cpSync(path.join(tauri, "icons", "icon.icns"), path.join(contents, "Resources", "icon.icns"));
  cpSync(path.join(tauri, "macos", "zh-Hans.lproj", "InfoPlist.strings"), path.join(contents, "Resources", "zh-Hans.lproj", "InfoPlist.strings"));
  const extra = JSON.parse(capture("plutil", ["-convert", "json", "-o", "-", path.join(tauri, "Info.plist")]));
  const scratch = mkdtempSync(path.join(os.tmpdir(), "gpui-bundle-"));
  try {
    const json = path.join(scratch, "Info.json");
    writeFileSync(json, JSON.stringify(infoPlist({ flavor, version, minimumSystemVersion, extra })));
    run("plutil", ["-convert", "xml1", "-o", path.join(contents, "Info.plist"), json]);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
  writeFileSync(path.join(contents, "PkgInfo"), "APPL????");

  // 4. Signatures, inside out: the runtimes' Mach-O files and PDFium, then the
  // bundle, whose signature seals the main executable and every resource.
  const signed = signMachOTree(path.join(contents, "Resources"), { identity, entitlements });
  codesign(path.join(contents, "Frameworks", "libpdfium.dylib"), { identity });
  codesign(app, { identity, entitlements });
  run("codesign", ["--verify", "--deep", "--strict", "--verbose=2", app]);
  console.log(`gpui-bundle: signed ${signed + 2} Mach-O files and the bundle ${identity === "-" ? "ad hoc" : `as ${identity}`}`);

  // 5. Notarize the app before it is packed, so every package holds the stapled app.
  const base = `${flavor.artifact}_${version}_${arch.suffix}`;
  if (key) {
    const upload = path.join(out, `${base}.notarize.zip`);
    run("ditto", ["-c", "-k", "--keepParent", app, upload]);
    notarize(upload, key);
    rmSync(upload);
    run("xcrun", ["stapler", "staple", app]);
    run("spctl", ["--assess", "--type", "execute", "--verbose=4", app]);
  } else {
    console.log("gpui-bundle: no App Store Connect key (APPLE_API_KEY, APPLE_API_ISSUER, APPLE_API_KEY_PATH): not notarized");
  }

  // 6. The packages.
  const outputs = [];
  const zip = path.join(out, `${base}.zip`);
  rmSync(zip, { force: true });
  run("ditto", ["-c", "-k", "--sequesterRsrc", "--keepParent", app, zip]);
  outputs.push(zip);

  if (!values["no-dmg"]) {
    const dmg = path.join(out, `${base}.dmg`);
    const stage = mkdtempSync(path.join(os.tmpdir(), "gpui-dmg-"));
    try {
      run("ditto", [app, path.join(stage, `${flavor.product}.app`)]);
      symlinkSync("/Applications", path.join(stage, "Applications"));
      rmSync(dmg, { force: true });
      run("hdiutil", ["create", "-volname", flavor.product, "-srcfolder", stage, "-fs", "HFS+", "-format", "UDZO", "-ov", dmg]);
    } finally {
      rmSync(stage, { recursive: true, force: true });
    }
    if (identity !== "-") run("codesign", ["--force", "--timestamp", "--sign", identity, dmg]);
    if (key) {
      notarize(dmg, key);
      run("xcrun", ["stapler", "staple", dmg]);
      run("xcrun", ["stapler", "validate", dmg]);
    }
    outputs.push(dmg);
  }

  // The updater's archive holds the app at its root, as tauri-plugin-updater's
  // does (crates/lattice-gpui/src/updater/install.rs unpacks it).
  const archive = path.join(out, `${base}.app.tar.gz`);
  run("tar", ["-czf", archive, "-C", out, `${flavor.product}.app`], { env: { ...process.env, COPYFILE_DISABLE: "1" } });
  outputs.push(archive);
  if (process.env.TAURI_SIGNING_PRIVATE_KEY) {
    rmSync(`${archive}.sig`, { force: true });
    run(path.join(repo, "node_modules", ".bin", "tauri"), ["signer", "sign", archive]);
    const signature = readFileSync(`${archive}.sig`, "utf8").trim();
    const manifest = path.join(out, flavor.manifest);
    const url = `${feed.replace(/\/$/, "")}/${path.basename(archive)}`;
    writeFileSync(manifest, `${JSON.stringify(updaterManifest({ version, signature, url, arch: values.arch }), null, 2)}\n`);
    outputs.push(`${archive}.sig`, manifest);
  } else {
    console.log(`gpui-bundle: no TAURI_SIGNING_PRIVATE_KEY: the updater archive is unsigned and no ${flavor.manifest} is written`);
  }

  if (values["typst-helper"]) {
    outputs.push(...packageTypstHelper({ binary: path.resolve(values["typst-helper"]), out, version, arch: values.arch, identity, key }));
  }

  console.log(`gpui-bundle: ${app}`);
  for (const file of outputs) console.log(`gpui-bundle: ${file} (${(statSync(file).size / 1024 / 1024).toFixed(1)} MB)`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main().catch((error) => {
    console.error(`gpui-bundle: ${error.message}`);
    process.exit(1);
  });
}
