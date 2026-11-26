/**
 * The move to the native Lattice on a Mac.
 *
 * From 0.20.0 the native app (GPUI, drawn by the Mac instead of a web page) is
 * Lattice on every Mac. The classic feed (latest.json) already hands Macs the
 * native archive, so an update from an older version lands there directly; a
 * copy of this version that still starts on a Mac (installed from its disk
 * image) moves itself at launch, without a choice: native-app-migration.tsx
 * shows a short notice while it downloads the native app in place of this one
 * and restarts into it. The backend finds the release (check_native_app,
 * native_app.rs) and installs it through the updater's own claim, install and
 * restart.
 *
 * Whether this launch moves at all is the backend's to say: only a release
 * build on a Mac, and only its first window to ask (take_native_move).
 */
import { invoke } from "@tauri-apps/api/core";

/** A native release a check found, held by the backend under `rid` until installed or released. */
export type NativeAppRelease = { version: string; rid: number };

/** The native release to move to; null off a Mac, without one, or when the check failed. */
function checkNativeApp(): Promise<NativeAppRelease | null> {
  return invoke<NativeAppRelease | null>("check_native_app").catch(() => null);
}

/** Let the backend drop a release that will not be installed. */
export function releaseNativeApp(release: NativeAppRelease) {
  void invoke("plugin:resources|close", { rid: release.rid }).catch(() => {});
}

/**
 * The release this launch moves to: null unless the backend says this window
 * moves (a release build on a Mac, its first window to ask) and the check finds
 * one. A launch that finds none (offline, or no native build for this Mac yet)
 * says nothing and the next launch tries again.
 */
export async function nativeMigrationRelease(): Promise<NativeAppRelease | null> {
  const due = await invoke<boolean>("take_native_move").catch(() => false);
  return due ? checkNativeApp() : null;
}
