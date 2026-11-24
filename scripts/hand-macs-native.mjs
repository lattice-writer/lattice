import { readFileSync, writeFileSync } from "node:fs";
import { ARCHES, FLAVORS } from "./gpui-bundle.mjs";
import { isMain } from "./lib/util.mjs";

// From v0.20.0 every Mac runs the native app. latest.json is the classic
// (Tauri) app's feed, read by every classic copy, 0.19.x and older included,
// and its updater installs whatever signed .app.tar.gz a darwin entry names
// over Lattice.app and relaunches the executable the new Info.plist names.
// So this hands each darwin entry of latest.json the native app's entry for
// the same key from the release's latest-native.json: release.yml for
// darwin-aarch64, macos-intel.yml for darwin-x86_64. Windows and Linux
// entries, and the manifest's version, notes and date, stay as they are.
//
// It checks only the manifests: the native entries' signatures were verified
// against the updater key before latest-native.json was written
// (check-native-feed.mjs, add-updater-platform.mjs). It refuses a native
// manifest for another version, a Mac key it lacks, and an entry that is not
// that arch's Lattice-native archive, since a wrong entry here reaches every
// classic copy on that Mac.

const archive = (version, platform) => {
  const arch = Object.values(ARCHES).find((candidate) => platform === candidate.platform || platform === `${candidate.platform}-app`);
  if (!arch) throw new Error(`${platform} is no Mac the native app ships for`);
  return `${FLAVORS.main.artifact}_${version}_${arch.suffix}.app.tar.gz`;
};

/**
 * `classic` (latest.json) with every darwin entry it lists, and every one in
 * `platforms`, replaced by `native`'s (latest-native.json) for that key.
 */
export function handMacsNative(classic, native, platforms = []) {
  if (native.version !== classic.version) {
    throw new Error(`${FLAVORS.main.manifest} is for ${native.version}, latest.json for ${classic.version}`);
  }
  for (const platform of platforms) {
    if (!platform.startsWith("darwin-")) throw new Error(`${platform} is not a Mac`);
  }
  const keys = [...new Set([...Object.keys(classic.platforms ?? {}).filter((key) => key.startsWith("darwin-")), ...platforms])];
  if (keys.length === 0) throw new Error("latest.json lists no Mac and none was asked for");
  const handed = { ...classic.platforms };
  for (const key of keys) {
    const entry = native.platforms?.[key];
    if (!entry) throw new Error(`${FLAVORS.main.manifest} lists no ${key}`);
    const name = archive(native.version, key);
    const url = new URL(entry.url);
    if (url.origin !== "https://github.com" || !url.pathname.endsWith(`/releases/download/v${native.version}/${name}`)) {
      throw new Error(`${FLAVORS.main.manifest}'s ${key} is not v${native.version}'s ${name}`);
    }
    if (!entry.signature?.trim()) throw new Error(`${FLAVORS.main.manifest}'s ${key} has no signature`);
    handed[key] = { signature: entry.signature.trim(), url: entry.url };
  }
  return { ...classic, platforms: handed };
}

if (isMain(import.meta.url)) {
  const [, , classicPath, nativePath, ...platforms] = process.argv;
  if (!classicPath || !nativePath) {
    throw new Error("Usage: node scripts/hand-macs-native.mjs <latest.json> <latest-native.json> [darwin-platform...]");
  }
  const classic = JSON.parse(readFileSync(classicPath, "utf8"));
  const native = JSON.parse(readFileSync(nativePath, "utf8"));
  const result = handMacsNative(classic, native, platforms);
  writeFileSync(classicPath, `${JSON.stringify(result, null, 2)}\n`);
  const macs = Object.keys(result.platforms).filter((key) => key.startsWith("darwin-"));
  console.log(`latest.json ${result.version} now hands ${macs.join(", ")} the native app.`);
}
