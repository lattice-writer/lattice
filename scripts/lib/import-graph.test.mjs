// @vitest-environment node
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { importers, nearTests } from "./import-graph.mjs";

describe("import-graph", () => {
  it("finds a change's own tests and its importers' tests, not the suites further out", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "import-graph-"));
    const files = {
      "src/lib/hold.ts": "export const hold = 1;\n",
      "src/lib/hold.test.ts": 'import { hold } from "./hold";\n',
      "src/editor/select.ts": 'import { hold } from "@/lib/hold.js";\nexport { hold };\n',
      "src/editor/select.test.ts": 'vi.mock("./select");\nimport "./select";\n',
      "src/editor/types.test.ts": 'import type { hold } from "../lib/hold";\n',
      "src/editor/lazy.test.ts": 'await import("../lib/hold?raw");\n',
      "src/app/App.tsx": 'export { hold } from "../editor/select";\n',
      "src/app/app.test.tsx": 'import "./App";\n',
      "src/app/style.css": "a {}\n",
      "src/lib/far.ts": "export const far = 1;\n",
      "src/lib/barrel.ts": 'export * from "./far";\n',
      "src/lib/barrel.test.ts": 'import {\n  far,\n} from "./barrel";\n',
      "src/platform/test-setup.ts": 'import "../lib/parked";\n',
      "src/lib/parked.ts": "export {};\n",
      "vitest.config.ts": 'export default { test: { setupFiles: ["./src/platform/test-setup.ts"] } };\n',
    };
    try {
      for (const [file, text] of Object.entries(files)) {
        mkdirSync(path.join(dir, path.dirname(file)), { recursive: true });
        writeFileSync(path.join(dir, file), text);
      }
      execFileSync("git", ["init", "-q"], { cwd: dir });
      execFileSync("git", ["add", "-A"], { cwd: dir });
      const graph = importers(dir);
      expect(nearTests(["src/lib/hold.ts"], graph)).toEqual({ tests: ["src/editor/lazy.test.ts", "src/editor/select.test.ts", "src/lib/hold.test.ts"], unknown: [] });
      expect(nearTests(["src/lib/hold.ts"], graph, 99).tests).toContain("src/app/app.test.tsx");
      expect(nearTests(["src/editor/select.test.ts", "src/app/style.css", "src/gone.ts"], graph)).toEqual({ tests: ["src/editor/select.test.ts"], unknown: ["src/app/style.css", "src/gone.ts"] });
      expect(nearTests(["src/lib/far.ts"], graph).tests).toEqual(["src/lib/barrel.test.ts"]);
      // Every test loads the setup file, and so what it imports.
      const all = Object.keys(files).filter((file) => file.includes(".test.")).sort();
      expect(nearTests(["src/platform/test-setup.ts"], graph).tests).toEqual(all);
      expect(nearTests(["src/lib/parked.ts"], graph).tests).toEqual(all);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("reads this repository's setup file from vitest.config.ts, so a change to it runs every test", () => {
    const repo = path.resolve(import.meta.dirname, "../..");
    const graph = importers(repo);
    const tests = [...graph.keys()].filter((file) => /\.test\./.test(file));
    expect(nearTests(["src/platform/test-setup.ts"], graph).tests).toEqual(tests.sort());
  });
});
