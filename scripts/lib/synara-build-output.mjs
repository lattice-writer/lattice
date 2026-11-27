// Refuses to stage Synara build output this run did not produce.
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const SERVER_BUNDLE = "apps/server/dist/index.mjs";
const SERVER_CLIENT = "apps/server/dist/client/index.html";
const WEB_ARTIFACT = "apps/web/dist/index.html";
const SERVER_CLIENT_DIR = "apps/server/dist/client";
const WEB_DIST = "apps/web/dist";

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
 *
 * Comparing `index.html` alone is enough to tell builds apart because Vite
 * names every chunk it loads by a hash of the chunk's contents, so a client
 * from another build names other files. It does not catch a copy that
 * stopped part way, so every file of `apps/web/dist` must also be in
 * `dist/client`. Only their presence is checked: the server build replaces
 * the favicons in `dist/client` with development icons after the copy.
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
  const missing = filesUnder(path(WEB_DIST)).filter((file) => !existsSync(join(path(SERVER_CLIENT_DIR), file)));
  if (missing.length > 0) {
    throw new Error(
      `${SERVER_CLIENT_DIR} is missing ${missing.length} of the files in ${WEB_DIST}, such as ${missing[0]}, ` +
        `so the copy into the server build did not finish.`,
    );
  }
}

/** Every file under `root`, by its path from `root` with `/` between parts. */
function filesUnder(root) {
  return readdirSync(root, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => relative(root, join(entry.parentPath, entry.name)).split("\\").join("/"));
}

function stale(artifact) {
  return new Error(
    `${artifact} was not rewritten by the build, so the staged runtime would ship stale code. ` +
      `Check that the workspace build command actually ran.`,
  );
}
