/**
 * The name of the action that shows a file in the system's file manager
 * (`revealItemInDir`), one row per host: Finder on a Mac, File Explorer on
 * Windows, and on Linux whichever file manager the desktop uses.
 */
import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { hostPlatform, type HostPlatform } from "./host-platform";

const SHOW_IN_FILE_MANAGER: Record<HostPlatform, MessageDescriptor> = {
  mac: msg`Show in Finder`,
  windows: msg`Show in File Explorer`,
  linux: msg`Show in File Manager`,
};

/** Translate with `i18n._(…)` or `useLingui`'s `t(…)`. */
export function showInFileManager(): MessageDescriptor {
  return SHOW_IN_FILE_MANAGER[hostPlatform()];
}
