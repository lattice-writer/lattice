// @vitest-environment node
/**
 * A selection across a code block's lines is painted from the code's text
 * column, never across the block's padding (LATA-86). Selection painting is
 * WebKit's own (the app's WKWebView), so this renders the real stylesheet in
 * Playwright's WebKit and reads the pixels; it runs wherever that browser is
 * installed (`pnpm exec playwright-core install webkit`), the local macOS gate
 * among them.
 */
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { webkit } from "playwright-core";

const css = readFileSync(new URL("./lattice-visual-blocks.css", import.meta.url), "utf8");
const installed = (() => {
  try {
    return existsSync(webkit.executablePath());
  } catch {
    return false;
  }
})();


describe("code block selection", () => {
  it.skipIf(!installed)("paints a selected empty line and line start from the text column", async () => {
    const browser = await webkit.launch();
    try {
      const page = await browser.newPage({ deviceScaleFactor: 1 });
      await page.setContent(`<!doctype html><style>
        :root { --space-1: 4px; --space-2: 8px; --space-3: 12px; --space-5: 20px; --space-6: 24px; --space-8: 32px; }
        body { margin: 20px; font: 16px Menlo, monospace; background: #fff; }
        .lx-md-editor { outline: none; }
        .lx-md-code { background: rgb(240, 240, 240) !important; border-color: #ccc !important; }
        ::selection { background: rgb(80, 120, 255); }
        ${css}
      </style>
      <div class="lx-md-editor" contenteditable="true"><div class="lx-md-surface">
        <p>before</p>
        <div class="lx-md-code"><pre class="lx-md-code-pre"><code><div id="code">def solve():\n    out.append(x)\n    sys.stdout.write(x)\n\nif __name__:\n    solve()</div></code></pre></div>
        <p>after</p>
      </div></div>`);
      const rows = await page.evaluate(() => {
        const text = document.getElementById("code")!.firstChild as Text;
        const start = text.data.indexOf("    sys.stdout");
        const range = document.createRange();
        range.setStart(text, start);
        range.setEnd(text, text.data.indexOf("__name__:") + "__name__:".length);
        (document.querySelector(".lx-md-editor") as HTMLElement).focus();
        getSelection()!.removeAllRanges();
        getSelection()!.addRange(range);
        // The empty line and the "if" line, one character wide at the line start.
        const lineStart = (offset: number) => {
          const probe = document.createRange();
          probe.setStart(text, offset);
          probe.setEnd(text, offset + 1);
          return probe.getBoundingClientRect();
        };
        const code = document.querySelector(".lx-md-code")!.getBoundingClientRect();
        const ifLine = lineStart(text.data.indexOf("if __name__"));
        return { blockLeft: code.left, textLeft: ifLine.left, ifMiddle: ifLine.top + ifLine.height / 2, emptyMiddle: ifLine.top - ifLine.height / 2 };
      });
      // A 4×4 patch in the padding, and one just inside the text column, on
      // the empty line and on the "if" line: the padding must look the same
      // selected or not, the text column must not.
      const patch = (x: number, y: number) => ({ x: Math.round(x) - 2, y: Math.round(y) - 2, width: 4, height: 4 });
      const padding = (rows.blockLeft + rows.textLeft) / 2;
      const patches = [rows.emptyMiddle, rows.ifMiddle].flatMap((y) => [patch(padding, y), patch(rows.textLeft + 3, y)]);
      const shoot = () => Promise.all(patches.map((clip) => page.screenshot({ clip })));
      // WebKit paints a new selection a moment after it is set.
      await page.waitForTimeout(200);
      const selected = await shoot();
      await page.evaluate(() => getSelection()!.removeAllRanges());
      await page.waitForTimeout(200);
      const plain = await shoot();
      expect(rows.textLeft - rows.blockLeft).toBeGreaterThan(16);
      expect(patches.map((_patch, index) => selected[index]!.equals(plain[index]!)))
        .toEqual([true, false, true, false]);
    } finally {
      await browser.close();
    }
  }, 30_000);
});
