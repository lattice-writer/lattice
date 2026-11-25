import { copyFileSync, mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { assertFreshSynaraBuild } from "./synara-build-output.mjs";

let root;
const hourAgo = () => new Date(Date.now() - 3_600_000);
const put = (path, text, time) => {
  mkdirSync(dirname(join(root, path)), { recursive: true });
  writeFileSync(join(root, path), text);
  if (time) utimesSync(join(root, path), time, time);
};
// What Windows' CopyFileW does: the copy keeps the source's last-write time.
const copyKeepingTime = (from, to, time) => {
  mkdirSync(dirname(join(root, to)), { recursive: true });
  copyFileSync(join(root, from), join(root, to));
  utimesSync(join(root, to), time, time);
};

beforeEach(() => { root = mkdtempSync(join(tmpdir(), "synara-build-output-")); });
afterEach(() => rmSync(root, { recursive: true, force: true }));

describe("assertFreshSynaraBuild", () => {
  it("accepts a reused web build whose copy kept its old timestamp", () => {
    const buildStartedAt = Date.now() - 1_000;
    put("apps/web/dist/index.html", "<html>v2</html>", hourAgo());
    copyKeepingTime("apps/web/dist/index.html", "apps/server/dist/client/index.html", hourAgo());
    put("apps/server/dist/index.mjs", "server");
    expect(() => assertFreshSynaraBuild(root, { buildStartedAt, webBuildReused: true })).not.toThrow();
  });

  it("refuses a client that is not the web build's output", () => {
    const buildStartedAt = Date.now() - 1_000;
    put("apps/web/dist/index.html", "<html>v2</html>", hourAgo());
    put("apps/server/dist/client/index.html", "<html>v1</html>");
    put("apps/server/dist/index.mjs", "server");
    expect(() => assertFreshSynaraBuild(root, { buildStartedAt, webBuildReused: true }))
      .toThrow(/dist\/client\/index\.html was not rewritten/);
  });

  it("refuses a web build this run should have rewritten but did not", () => {
    const buildStartedAt = Date.now() - 1_000;
    put("apps/web/dist/index.html", "<html>v1</html>", hourAgo());
    copyKeepingTime("apps/web/dist/index.html", "apps/server/dist/client/index.html", hourAgo());
    put("apps/server/dist/index.mjs", "server");
    expect(() => assertFreshSynaraBuild(root, { buildStartedAt, webBuildReused: false }))
      .toThrow(/apps\/web\/dist\/index\.html was not rewritten/);
  });

  it("refuses a stale or missing server bundle", () => {
    const buildStartedAt = Date.now() - 1_000;
    put("apps/web/dist/index.html", "<html>v2</html>");
    put("apps/server/dist/client/index.html", "<html>v2</html>");
    expect(() => assertFreshSynaraBuild(root, { buildStartedAt, webBuildReused: false }))
      .toThrow(/did not produce apps\/server\/dist\/index\.mjs/);
    put("apps/server/dist/index.mjs", "server", hourAgo());
    expect(() => assertFreshSynaraBuild(root, { buildStartedAt, webBuildReused: false }))
      .toThrow(/dist\/index\.mjs was not rewritten/);
  });
});
