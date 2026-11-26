import { describe, expect, it } from "vitest";
import { nativeFiles } from "./check-native-feed.mjs";
import { FLAVORS, updaterManifest } from "./gpui-bundle.mjs";
import { handMacsNative } from "./hand-macs-native.mjs";

// latest.json is what every classic copy reads, 0.19.x included; from this
// release its Mac entries name the native archive from latest-native.json.
const feed = FLAVORS.main.feed("1.2.3", "lattice-writer/lattice");
const classicUrl = (suffix: string) => `${feed}/Lattice_1.2.3_${suffix}.app.tar.gz`;
const arm = { signature: "native-arm", url: `${feed}/${nativeFiles("1.2.3").archive}` };
const intel = { signature: "native-intel", url: `${feed}/${nativeFiles("1.2.3", "x86_64").archive}` };
const windows = { signature: "windows", url: `${feed}/Lattice_1.2.3_x64-setup.nsis.zip` };
const classic = {
  version: "1.2.3",
  notes: "notes",
  pub_date: "2026-11-24T00:00:00Z",
  platforms: {
    "darwin-aarch64": { signature: "classic-arm", url: classicUrl("aarch64") },
    "darwin-aarch64-app": { signature: "classic-arm", url: classicUrl("aarch64") },
    "windows-x86_64": windows,
  },
};
const nativeArm = updaterManifest({ version: "1.2.3", signature: "native-arm", url: arm.url });
const nativeBoth = { ...nativeArm, platforms: { ...nativeArm.platforms, "darwin-x86_64": intel, "darwin-x86_64-app": intel } };

describe("handing Macs the native app in latest.json", () => {
  it("replaces each Apple silicon entry with the native one and leaves the rest", () => {
    const result = handMacsNative(classic, nativeArm);
    expect(result).toEqual({
      ...classic,
      platforms: { "darwin-aarch64": arm, "darwin-aarch64-app": arm, "windows-x86_64": windows },
    });
    expect(result.platforms["windows-x86_64"]).toBe(windows);
    expect(classic.platforms["darwin-aarch64"].signature).toBe("classic-arm");
    // A re-run hands out the same entries again.
    expect(handMacsNative(result, nativeArm)).toEqual(result);
  });

  it("adds the Intel keys it is asked for, keeping the Apple silicon ones native", () => {
    const armed = handMacsNative(classic, nativeArm);
    const result = handMacsNative(armed, nativeBoth, ["darwin-x86_64", "darwin-x86_64-app"]);
    expect(result.platforms).toEqual({
      "darwin-aarch64": arm,
      "darwin-aarch64-app": arm,
      "darwin-x86_64": intel,
      "darwin-x86_64-app": intel,
      "windows-x86_64": windows,
    });
  });

  it("replaces a classic Intel entry too", () => {
    const withIntel = { ...classic, platforms: { ...classic.platforms, "darwin-x86_64": { signature: "classic-intel", url: classicUrl("x64") } } };
    expect(handMacsNative(withIntel, nativeBoth).platforms["darwin-x86_64"]).toEqual(intel);
  });

  it.each([
    ["a native manifest for another version", classic, { ...nativeArm, version: "1.2.4" }, [], /for 1.2.4, latest.json for 1.2.3/],
    ["a Mac the native manifest lacks", classic, nativeArm, ["darwin-x86_64"], /lists no darwin-x86_64/],
    ["a classic Mac entry the native manifest lacks", { ...classic, platforms: { ...classic.platforms, "darwin-x86_64": intel } }, nativeArm, [], /lists no darwin-x86_64/],
    ["a key that is not a Mac", classic, nativeArm, ["windows-x86_64"], /not a Mac/],
    ["a Mac the native app does not ship for", classic, { ...nativeArm, platforms: { ...nativeArm.platforms, "darwin-riscv64": arm } }, ["darwin-riscv64"], /no Mac the native app ships for/],
    ["the classic archive in the native manifest", classic, updaterManifest({ version: "1.2.3", signature: "s", url: classicUrl("aarch64") }), [], /is not v1.2.3's Lattice-native_1.2.3_aarch64/],
    ["the other arch's archive", classic, { ...nativeArm, platforms: { "darwin-aarch64": intel, "darwin-aarch64-app": intel } }, [], /is not v1.2.3's Lattice-native_1.2.3_aarch64/],
    ["an archive off GitHub", classic, updaterManifest({ version: "1.2.3", signature: "s", url: `https://example.com/releases/download/v1.2.3/${nativeFiles("1.2.3").archive}` }), [], /is not v1.2.3's/],
    ["an unsigned entry", classic, updaterManifest({ version: "1.2.3", signature: " ", url: arm.url }), [], /no signature/],
    ["a feed with no Mac and none asked for", { ...classic, platforms: { "windows-x86_64": windows } }, nativeArm, [], /lists no Mac/],
  ])("refuses %s", (_name, latest, native, platforms, error) => {
    expect(() => handMacsNative(latest, native, platforms as string[])).toThrow(error);
  });
});
