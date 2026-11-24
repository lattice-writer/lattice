import { existsSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";
import { verifyUpdaterSignature } from "./add-updater-platform.mjs";
import { archOf, FLAVORS, typstHelperFiles } from "./gpui-bundle.mjs";
import { isMain, projectRoot } from "./lib/util.mjs";

// Checks the native app's files (scripts/gpui-bundle.mjs --flavor main, as
// gpui-package.yml keeps them) before release.yml (Apple silicon) or
// macos-intel.yml (Intel) uploads them beside the Tauri app's.
// latest-native.json is the feed every native copy reads to update, and its
// entries are what latest.json hands every Mac (scripts/hand-macs-native.mjs),
// so this refuses a manifest for another version or tag, one whose archive is
// not this release's Lattice-native archive for the arch, and a signature
// that does not verify against the updater key both apps pin
// (tauri.conf.json).

/** The native app's file names for `version` and `arch`, from gpui-bundle's FLAVORS.main and ARCHES. */
export function nativeFiles(version, arch = "aarch64") {
  const base = `${FLAVORS.main.artifact}_${version}_${archOf(arch).suffix}`;
  return { dmg: `${base}.dmg`, archive: `${base}.app.tar.gz`, signature: `${base}.app.tar.gz.sig`, manifest: FLAVORS.main.manifest };
}

/**
 * Throws unless `manifest` offers `version` as `tag`'s own Lattice-native
 * archive for `arch`, under both of the arch's keys (darwin-aarch64 and
 * darwin-aarch64-app, or darwin-x86_64 and darwin-x86_64-app), with
 * `signature` (the .sig file's contents).
 */
export function checkNativeManifest(manifest, { tag, version, signature, arch = "aarch64" }) {
  const name = FLAVORS.main.manifest;
  // A rehearsal tag (v1.2.3-rc.1) checks its version's files; the manifest
  // names the archive under the version's own tag either way.
  if ((tag !== `v${version}` && !tag.startsWith(`v${version}-`)) || manifest.version !== version) {
    throw new Error(`${name} is for ${manifest.version}, not ${tag}`);
  }
  const { archive } = nativeFiles(version, arch);
  const trimmed = signature.trim();
  if (!trimmed) throw new Error("Empty updater signature");
  const { platform } = archOf(arch);
  for (const key of [platform, `${platform}-app`]) {
    const entry = manifest.platforms?.[key];
    if (!entry) throw new Error(`${name} lists no ${key}`);
    const url = new URL(entry.url);
    if (url.origin !== "https://github.com" || !url.pathname.endsWith(`/releases/download/v${version}/${archive}`)) {
      throw new Error(`${name}'s ${key} is not v${version}'s ${archive}`);
    }
    if (entry.signature?.trim() !== trimmed) throw new Error(`${name}'s ${key} carries another signature`);
  }
}

/**
 * The Typst helper's archive and .sig for `version` and `arch` in
 * `directory`, checked: both there, the signature verifying against the
 * updater key the native app checks the download with
 * (gpui/crates/lattice-gpui/src/typst/install.rs). Returns their paths, the
 * archive first.
 */
export function checkTypstHelper(directory, version, pubkey, arch = "aarch64") {
  const { archive, signature } = typstHelperFiles(version, arch);
  const paths = [join(directory, archive), join(directory, signature)];
  for (const file of paths) {
    if (!existsSync(file) || statSync(file).size === 0) throw new Error(`The Typst helper's ${file} is missing or empty`);
  }
  verifyUpdaterSignature(readFileSync(paths[0]), readFileSync(paths[1], "utf8"), pubkey);
  return paths;
}

if (isMain(import.meta.url)) {
  // --typst: the Typst helper's files instead of the app's. --arch: aarch64
  // (default, release.yml) or x86_64 (macos-intel.yml).
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: { typst: { type: "boolean", default: false }, arch: { type: "string", default: "aarch64" } },
  });
  const [tag, directory] = positionals;
  if (!tag || !directory) throw new Error("Usage: node scripts/check-native-feed.mjs [--typst] [--arch aarch64|x86_64] <tag> <directory>");
  const { arch } = values;
  archOf(arch);
  const config = JSON.parse(readFileSync(join(projectRoot, "src-tauri/tauri.conf.json"), "utf8"));
  if (values.typst) {
    process.stdout.write(`${checkTypstHelper(directory, config.version, config.plugins.updater.pubkey, arch).join("\n")}\n`);
    process.exit(0);
  }
  const paths = Object.fromEntries(Object.entries(nativeFiles(config.version, arch)).map(([kind, name]) => [kind, join(directory, name)]));
  for (const file of Object.values(paths)) {
    if (!existsSync(file) || statSync(file).size === 0) throw new Error(`The native app's ${file} is missing or empty`);
  }
  const signature = readFileSync(paths.signature, "utf8");
  verifyUpdaterSignature(readFileSync(paths.archive), signature, config.plugins.updater.pubkey);
  checkNativeManifest(JSON.parse(readFileSync(paths.manifest, "utf8")), { tag, version: config.version, signature, arch });
  // The files to upload, the manifest last, so a check never reads a
  // manifest whose archive is not there yet.
  process.stdout.write(`${[paths.dmg, paths.archive, paths.signature, paths.manifest].join("\n")}\n`);
}
