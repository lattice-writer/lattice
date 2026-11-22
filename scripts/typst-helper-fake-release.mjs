#!/usr/bin/env node
/**
 * `node scripts/typst-helper-fake-release.mjs HELPER DIR`: a local stand-in
 * for the release's Typst helper, for the GPUI e2e (gpui/script/e2e), which
 * downloads and installs it as the native app does on first use with no
 * network: DIR gets the archive (the helper at its root under its own file
 * name, `lattice-typst` as gpui-bundle.mjs packs it or `lattice-typst.exe`
 * on Windows), its .sig and the public key that verifies it,
 * from a key pair made for this run alone (scenario/e2e/typst.rs reads them).
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { throwawayMinisign } from "./lib/throwaway-minisign.mjs";
import { isMain } from "./lib/util.mjs";

export const ARCHIVE = "lattice-typst.tar.gz";

export function fakeTypstRelease(helper, directory) {
  mkdirSync(directory, { recursive: true });
  const stage = mkdtempSync(path.join(os.tmpdir(), "typst-fake-release-"));
  try {
    const name = path.basename(helper);
    copyFileSync(helper, path.join(stage, name));
    const archive = path.join(directory, ARCHIVE);
    execFileSync("tar", ["-czf", archive, "-C", stage, name], { env: { ...process.env, COPYFILE_DISABLE: "1" } });
    const key = throwawayMinisign();
    writeFileSync(`${archive}.sig`, key.signFile(readFileSync(archive)));
    writeFileSync(path.join(directory, "key.pub"), key.publicKey);
    return archive;
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}

if (isMain(import.meta.url)) {
  const [helper, directory] = process.argv.slice(2);
  if (!helper || !directory) throw new Error("Usage: node scripts/typst-helper-fake-release.mjs HELPER DIR");
  console.log(fakeTypstRelease(path.resolve(helper), path.resolve(directory)));
}
