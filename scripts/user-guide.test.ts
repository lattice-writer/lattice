// @vitest-environment node
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { checkGuide, renderGuide } from "./user-guide.mjs";

describe("the user guide", () => {
  it("has every page in both languages, and every link and image it names", () => {
    expect(checkGuide()).toEqual([]);
  });

  it("renders the website's pages with links between them", () => {
    const out = mkdtempSync(join(tmpdir(), "lattice-guide-"));
    try {
      renderGuide(out);
      const index = readFileSync(join(out, "index.html"), "utf8");
      expect(index).toContain('href="getting-started.html"');
      expect(index).toContain('href="zh-CN/index.html"');
      expect(readFileSync(join(out, "zh-CN/index.html"), "utf8")).toContain('lang="zh-CN"');
      expect(index).toMatch(/<nav[^>]*><h2>[^<]+<\/h2><ol><li><a href="getting-started.html">/);
      expect(readFileSync(join(out, "writing.html"), "utf8")).toMatch(/<footer class="pager"><a rel="prev" href="[^"]+\.html">/);
    } finally {
      rmSync(out, { recursive: true, force: true });
    }
  });

  it("indexes every page's sections for the search box, in each language", () => {
    const out = mkdtempSync(join(tmpdir(), "lattice-guide-"));
    try {
      renderGuide(out);
      const load = (path: string) => {
        const script = readFileSync(join(out, path), "utf8");
        return JSON.parse(script.slice(script.indexOf("=") + 1).trim().replace(/;$/, "")) as { href: string; title: string; text: string }[];
      };
      const english = load("search-index.js");
      expect(english.some((entry) => entry.href.startsWith("overleaf.html#") && entry.title.startsWith("Overleaf"))).toBe(true);
      expect(english.every((entry) => entry.text.length > 0)).toBe(true);
      expect(load("zh-CN/search-index.js").some((entry) => entry.text.includes("Overleaf"))).toBe(true);
      const search = readFileSync(join(out, "search.js"), "utf8");
      expect(search).toContain("LATTICE_GUIDE_INDEX");
      expect(() => new Function(search)).not.toThrow();
    } finally {
      rmSync(out, { recursive: true, force: true });
    }
  });
});
