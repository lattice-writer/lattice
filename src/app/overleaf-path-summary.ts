import { t } from "@lingui/core/macro";

/** Show every path inline for small batches; beyond this, summarize or list one per line. */
export const MAX_INLINE_PATHS = 6;
const SHOWN_WHEN_TRUNCATED = 5;

/**
 * Format a list of project-relative paths for dialogs and notifications without
 * letting a bulk folder operation produce an unbounded paragraph.
 */
export function summarizeOverleafPaths(paths: readonly string[]): string {
  if (paths.length <= MAX_INLINE_PATHS) return paths.join(", ");
  const names = paths.slice(0, SHOWN_WHEN_TRUNCATED).join(", ");
  const count = paths.length - SHOWN_WHEN_TRUNCATED;
  return t({ message: `${names} and ${count} more files` });
}
