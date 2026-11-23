import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { checkNativeManifest, checkTypstHelper, nativeFiles } from "./check-native-feed.mjs";
import { FLAVORS, typstHelperFiles, updaterManifest } from "./gpui-bundle.mjs";
import { throwawayMinisign } from "./lib/throwaway-minisign.mjs";

// The manifest gpui-bundle.mjs writes for the native app is the one
// release.yml checks before it publishes it as latest-native.json.
const tag = "v1.2.3";
const files = nativeFiles("1.2.3");
const manifest = updaterManifest({
  version: "1.2.3",
  signature: "native",
  url: `${FLAVORS.main.feed("1.2.3", "leo1oel/lattice")}/${files.archive}`,
  pubDate: "2026-11-20T00:00:00Z",
});
const check = { tag, version: "1.2.3", signature: "native\n" };

describe("the native feed", () => {
  it("names the native app's files apart from tauri-action's, with its own manifest", () => {
    expect(files).toEqual({
      dmg: "Lattice-native_1.2.3_aarch64.dmg",
      archive: "Lattice-native_1.2.3_aarch64.app.tar.gz",
      signature: "Lattice-native_1.2.3_aarch64.app.tar.gz.sig",
      manifest: "latest-native.json",
    });
  });

  it("accepts the manifest gpui-bundle writes for this tag's archive", () => {
    expect(() => checkNativeManifest(manifest, check)).not.toThrow();
    expect(manifest.platforms["darwin-aarch64-app"].url).toBe(
      "https://github.com/leo1oel/lattice/releases/download/v1.2.3/Lattice-native_1.2.3_aarch64.app.tar.gz",
    );
  });

  it("accepts a rehearsal tag for the version's own files", () => {
    expect(() => checkNativeManifest(manifest, { ...check, tag: "v1.2.3-rc.1" })).not.toThrow();
  });

  it("refuses another version or tag, a missing key, another archive and another signature", () => {
    expect(() => checkNativeManifest({ ...manifest, version: "1.2.4" }, check)).toThrow(/not v1.2.3/);
    expect(() => checkNativeManifest(manifest, { ...check, tag: "v1.2.4" })).toThrow(/not v1.2.4/);
    expect(() => checkNativeManifest(manifest, { ...check, tag: "v1.2.30" })).toThrow(/not v1.2.30/);
    const only = { ...manifest, platforms: { "darwin-aarch64": manifest.platforms["darwin-aarch64"] } };
    expect(() => checkNativeManifest(only, check)).toThrow(/lists no darwin-aarch64-app/);
    const tauri = updaterManifest({ version: "1.2.3", signature: "native", url: `${FLAVORS.main.feed("1.2.3")}/Lattice_1.2.3_aarch64.app.tar.gz` });
    expect(() => checkNativeManifest(tauri, check)).toThrow(/is not v1.2.3's/);
    const elsewhere = updaterManifest({ version: "1.2.3", signature: "native", url: `https://example.com/releases/download/v1.2.3/${files.archive}` });
    expect(() => checkNativeManifest(elsewhere, check)).toThrow(/is not v1.2.3's/);
    expect(() => checkNativeManifest(manifest, { ...check, signature: "other" })).toThrow(/another signature/);
    expect(() => checkNativeManifest(manifest, { ...check, signature: " " })).toThrow(/Empty/);
  });
});

describe("the Typst helper's files", () => {
  it("are this version's archive and signature, verified against the updater key", () => {
    const directory = mkdtempSync(path.join(os.tmpdir(), "typst-feed-"));
    try {
      const key = throwawayMinisign();
      const { archive, signature } = typstHelperFiles("1.2.3");
      const helper = Buffer.from("the helper");
      expect(() => checkTypstHelper(directory, "1.2.3", key.publicKey)).toThrow(/missing or empty/);
      writeFileSync(path.join(directory, archive), helper);
      writeFileSync(path.join(directory, signature), key.signFile(helper));
      expect(checkTypstHelper(directory, "1.2.3", key.publicKey)).toEqual([path.join(directory, archive), path.join(directory, signature)]);
      expect(() => checkTypstHelper(directory, "1.2.3", throwawayMinisign(Buffer.from("fedcba9876543210", "hex")).publicKey)).toThrow();
      writeFileSync(path.join(directory, signature), key.signFile(Buffer.from("another helper")));
      expect(() => checkTypstHelper(directory, "1.2.3", key.publicKey)).toThrow(/does not match/);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});
