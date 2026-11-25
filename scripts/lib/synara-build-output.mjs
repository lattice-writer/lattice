// Refuses to stage Synara build output this run did not produce.
import { existsSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const SERVER_BUNDLE = "apps/server/dist/index.mjs";
const SERVER_CLIENT = "apps/server/dist/client/index.html";
const WEB_ARTIFACT = "apps/web/dist/index.html";

/**
 * Staging stale bytes under a fresh revision is far worse than failing, so
 * even with a working build command this checks that the output is current.
 *
 * The server bundle is always rebuilt, so it must be newer than the build
 * start. The client is not checked that way: the server build copies
 * `apps/web/dist` into `dist/client`, and a copy on Windows (CopyFileW, under
 * Node's `fs.cp`) keeps the source's last-write time. When the web build is
 * reused, its file predates this run, so the copy looked stale on Windows
 * although it was current. The client must match the web build's bytes
 * instead; the web build is current because it was just rebuilt (checked
 * here) or because its cache key matched this source.
 */
export function assertFreshSynaraBuild(sourceRoot, { buildStartedAt, webBuildReused }) {
  const path = (artifact) => join(sourceRoot, artifact);
  for (const artifact of [SERVER_BUNDLE, SERVER_CLIENT, WEB_ARTIFACT]) {
    if (!existsSync(path(artifact))) throw new Error(`The Synara build did not produce ${artifact}.`);
  }
  const rewritten = [SERVER_BUNDLE, ...(webBuildReused ? [] : [WEB_ARTIFACT])];
  for (const artifact of rewritten) {
    if (statSync(path(artifact)).mtimeMs < buildStartedAt) throw stale(artifact);
  }
  if (!readFileSync(path(SERVER_CLIENT)).equals(readFileSync(path(WEB_ARTIFACT)))) throw stale(SERVER_CLIENT);
}

function stale(artifact) {
  return new Error(
    `${artifact} was not rewritten by the build, so the staged runtime would ship stale code. ` +
      `Check that the workspace build command actually ran.`,
  );
}
