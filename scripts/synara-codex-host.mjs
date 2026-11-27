// The fork carries Lattice's Codex host fixes in its source (codexAppServerManager.ts):
// Codex inherits the host's bibliography sandbox when LATTICE_BIBLIOGRAPHY_SANDBOX=1
// instead of running a second sandbox-exec, and classified stderr and every app-server
// exit are logged even after teardown began. Since Synara 1.6.8 the server bundle is
// minified, so staging no longer patches it; it only fails closed when a pin lost them.
// String literals survive minification, so each fix is found by the literal it adds.
// Do not remove the standalone bibliography guard.
const CODEX_HOST_MARKERS = [
  "LATTICE_BIBLIOGRAPHY_SANDBOX",
  "codex app-server stderr",
  "codex app-server exit",
];

export function verifyCodexHostProcess(source) {
  const missing = CODEX_HOST_MARKERS.filter((marker) => !source.includes(marker));
  if (missing.length > 0) {
    throw new Error(
      `The pinned Synara bundle lacks Lattice's Codex host fixes (${missing.join(", ")}); re-check the fork's codexAppServerManager.ts`,
    );
  }
}
