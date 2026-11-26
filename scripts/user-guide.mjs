#!/usr/bin/env node
/**
 * The user guide (docs/guide/): English pages at its root, the same pages in
 * Simplified Chinese under zh-CN/, and the screenshots both use in images/.
 * The Markdown reads as it is on GitHub; this script also renders it to the
 * static pages the website serves at https://lattice-writer.github.io/lattice/guide/
 * (published by .github/workflows/user-guide.yml), which Help ▸ User Guide
 * opens in both apps.
 *
 * Usage:
 *   node scripts/user-guide.mjs --check
 *       Every English page has its Chinese page and the other way round, and
 *       every link and image a page names exists. scripts/user-guide.test.ts
 *       runs the same check.
 *   node scripts/user-guide.mjs --out DIR
 *       Check, then render the site into DIR: README.md becomes index.html,
 *       every other page NAME.md becomes NAME.html, each language gets its
 *       search-index.js, and search.js and images/ are copied beside them.
 */
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, posix, relative, sep } from "node:path";
import { Marked } from "marked";
import { isMain, projectRoot } from "./lib/util.mjs";

export const GUIDE_DIR = join(projectRoot, "docs/guide");
const LANGUAGES = [
  { code: "en", dir: "", name: "English", title: "Lattice User Guide", search: "Search the guide", noResults: "No pages match.", contents: "Contents", previous: "Previous", next: "Next" },
  { code: "zh-CN", dir: "zh-CN", name: "简体中文", title: "Lattice 使用指南", search: "搜索指南", noResults: "没有匹配的页面。", contents: "目录", previous: "上一页", next: "下一页" },
];
const EXTERNAL = /^[a-z][a-z0-9+.-]*:/i;

const pagesIn = (dir) => readdirSync(join(GUIDE_DIR, dir)).filter((name) => name.endsWith(".md")).sort();

/** The file a page path becomes on the website: README.md is the folder's index. */
const htmlName = (path) => path.replace(/(^|\/)README\.md$/, "$1index.html").replace(/\.md$/, ".html");

/** Relative link and image targets as written, anchors dropped. */
function localTargets(markdown) {
  const targets = [];
  for (const match of markdown.matchAll(/!?\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g)) {
    const target = match[1];
    if (EXTERNAL.test(target) || target.startsWith("#")) continue;
    targets.push(target.split("#")[0]);
  }
  return targets;
}

/** What would break the guide on GitHub or on the website; empty when nothing would. */
export function checkGuide() {
  const problems = [];
  const [english, chinese] = LANGUAGES.map((language) => pagesIn(language.dir));
  for (const page of english) if (!chinese.includes(page)) problems.push(`zh-CN/${page} is missing (the Chinese page of ${page})`);
  for (const page of chinese) if (!english.includes(page)) problems.push(`${page} is missing (the English page of zh-CN/${page})`);
  const usedImages = new Set();
  for (const language of LANGUAGES) {
    for (const page of pagesIn(language.dir)) {
      const path = posix.join(language.dir, page);
      for (const target of localTargets(readFileSync(join(GUIDE_DIR, path), "utf8"))) {
        const resolved = join(GUIDE_DIR, language.dir, target);
        const inGuide = relative(GUIDE_DIR, resolved).split(sep).join("/");
        if (inGuide.startsWith("..")) problems.push(`${path} links to ${target}, outside the guide, which the website does not carry`);
        else if (!existsSync(resolved)) problems.push(`${path} links to ${target}, which does not exist`);
        else if (inGuide.startsWith("images/")) usedImages.add(inGuide);
      }
    }
  }
  for (const image of readdirSync(join(GUIDE_DIR, "images"))) {
    if (!usedImages.has(`images/${image}`)) problems.push(`images/${image} is not used by any page`);
  }
  return problems;
}

const escapeHtml = (text) => text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

/** GitHub's heading anchors, so a link to page.md#section works in both places. */
const slug = (html) => html.replace(/<[^>]+>/g, "").toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, "").trim().replace(/\s/g, "-");

function renderer() {
  const marked = new Marked({ gfm: true });
  marked.use({
    walkTokens(token) {
      if ((token.type === "link" || token.type === "image") && !EXTERNAL.test(token.href) && !token.href.startsWith("#")) {
        const [path, hash] = token.href.split("#");
        token.href = htmlName(path) + (hash ? `#${hash}` : "");
      }
    },
    renderer: {
      heading({ tokens, depth }) {
        const inner = this.parser.parseInline(tokens);
        return `<h${depth} id="${slug(inner)}">${inner}</h${depth}>\n`;
      },
    },
  });
  return marked;
}

/**
 * The contents page's links in order, grouped by the README's `## ` headings
 * (Getting started, Writing, …): the sidebar every page in that language
 * shows. Links above the first heading form a group with no title.
 */
function contents(language) {
  const readme = readFileSync(join(GUIDE_DIR, language.dir, "README.md"), "utf8");
  const groups = [{ title: null, pages: [] }];
  for (const line of readme.split("\n")) {
    const heading = line.match(/^##\s+(.+?)\s*$/);
    if (heading) {
      groups.push({ title: heading[1], pages: [] });
      continue;
    }
    const link = line.match(/^\s*(?:\d+\.|-)\s+\[([^\]]+)\]\(([^)#\s]+\.md)\)/);
    if (link) groups.at(-1).pages.push({ title: link[1], page: link[2] });
  }
  return groups.filter((group) => group.pages.length > 0);
}

const stripTags = (html) =>
  html
    .replace(/<\/?(?:a|code|em|kbd|strong)\b[^>]*>/g, "")
    .replace(/<[^>]+>/g, " ")
    .replace(/&(amp|lt|gt|quot|#39);/g, (_, name) => ({ amp: "&", lt: "<", gt: ">", quot: '"', "#39": "'" })[name])
    .replace(/\s+/g, " ")
    .trim();

/**
 * A page's sections for the search: one entry per h1–h3 with the text up to
 * the next heading, so a hit links to its section rather than the page top.
 */
function sections(page, pageTitle, body) {
  const entries = [];
  let current = { href: htmlName(page), title: pageTitle, text: "" };
  for (const part of body.split(/(<h[1-3] id="[^"]*">[\s\S]*?<\/h[1-3]>)/)) {
    const heading = part.match(/^<h([1-3]) id="([^"]*)">([\s\S]*?)<\/h[1-3]>$/);
    if (!heading) {
      current.text += ` ${stripTags(part)}`;
      continue;
    }
    entries.push(current);
    const title = stripTags(heading[3]);
    current = heading[1] === "1"
      ? { href: htmlName(page), title, text: "" }
      : { href: `${htmlName(page)}#${heading[2]}`, title: `${pageTitle} › ${title}`, text: "" };
  }
  entries.push(current);
  return entries.filter((entry) => entry.text.trim()).map((entry) => ({ ...entry, text: entry.text.trim() }));
}

function pageHtml({ language, page, body, groups }) {
  const heading = body.match(/<h1[^>]*>(.*?)<\/h1>/)?.[1].replace(/<[^>]+>/g, "") ?? language.title;
  const title = heading === language.title ? heading : `${heading} · ${language.title}`;
  const other = LANGUAGES.find((candidate) => candidate !== language);
  const toRoot = language.dir ? "../" : "";
  const otherHref = `${toRoot}${other.dir ? `${other.dir}/` : ""}${htmlName(page)}`;
  const link = (entry) => {
    const current = entry.page === page ? ' aria-current="page"' : "";
    return `<li><a href="${htmlName(entry.page)}"${current}>${escapeHtml(entry.title)}</a></li>`;
  };
  const nav = groups
    .map((group) => (group.title ? `<h2>${escapeHtml(group.title)}</h2>` : "") + `<ol>${group.pages.map(link).join("")}</ol>`)
    .join("");
  // Previous and next follow the sidebar, the contents page first.
  const order = [{ title: language.title, page: "README.md" }, ...groups.flatMap((group) => group.pages)];
  const at = order.findIndex((entry) => entry.page === page);
  const step = (entry, rel, label) =>
    entry ? `<a rel="${rel}" href="${htmlName(entry.page)}"><small>${label}</small>${escapeHtml(entry.title)}</a>` : "<span></span>";
  const pager = at < 0 ? "" : `<footer class="pager">${step(order[at - 1], "prev", language.previous)}${step(order[at + 1], "next", language.next)}</footer>\n`;
  return `<!doctype html>
<html lang="${language.code}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(title)}</title>
<link rel="alternate" hreflang="${other.code}" href="${otherHref}">
<style>${STYLE}</style>
<script src="search-index.js" defer></script>
<script src="${toRoot}search.js" defer></script>
</head>
<body>
<header>
<a class="home" href="index.html">${escapeHtml(language.title)}</a>
<div class="search" role="search">
<input type="search" id="search" placeholder="${escapeHtml(language.search)}" aria-label="${escapeHtml(language.search)}" autocomplete="off" spellcheck="false" data-empty="${escapeHtml(language.noResults)}">
<ol id="results" hidden></ol>
</div>
<a class="language" href="${otherHref}" hreflang="${other.code}" lang="${other.code}">${other.name}</a>
</header>
<div class="layout">
<nav aria-label="${escapeHtml(language.contents)}">${nav}</nav>
<main>
${body}${pager}</main>
</div>
</body>
</html>
`;
}

/**
 * The search box. Every term must appear in a section's title or text, as
 * substrings, so Chinese needs no word splitting; title hits rank first. It
 * reads the index from a script rather than fetch(), so the guide also
 * searches when it is opened from disk (file://).
 */
const SEARCH_SCRIPT = `(() => {
  const input = document.getElementById("search");
  const list = document.getElementById("results");
  const index = window.LATTICE_GUIDE_INDEX || [];
  let active = -1;
  const lower = (text) => text.toLocaleLowerCase();
  const snippet = (text, term) => {
    const from = Math.max(0, lower(text).indexOf(term) - 50);
    return (from > 0 ? "…" : "") + text.slice(from, from + 160) + (from + 160 < text.length ? "…" : "");
  };
  const mark = (parent, text, terms) => {
    const haystack = lower(text);
    let from = 0;
    while (from < text.length) {
      let at = -1;
      let length = 0;
      for (const term of terms) {
        const found = haystack.indexOf(term, from);
        if (found >= 0 && (at < 0 || found < at)) [at, length] = [found, term.length];
      }
      if (at < 0) break;
      parent.append(text.slice(from, at));
      const highlight = document.createElement("mark");
      highlight.textContent = text.slice(at, at + length);
      parent.append(highlight);
      from = at + length;
    }
    parent.append(text.slice(from));
  };
  const select = (next) => {
    const items = list.querySelectorAll("a");
    if (!items.length) return;
    active = (next + items.length) % items.length;
    items.forEach((item, i) => item.toggleAttribute("aria-selected", i === active));
    items[active].scrollIntoView({ block: "nearest" });
  };
  const show = () => {
    const terms = lower(input.value).split(/\\s+/).filter(Boolean);
    list.replaceChildren();
    active = -1;
    if (!terms.length) {
      list.hidden = true;
      return;
    }
    const hits = [];
    for (const entry of index) {
      const title = lower(entry.title);
      const text = lower(entry.text);
      let score = 0;
      for (const term of terms) {
        if (title.includes(term)) score += 10;
        else if (text.includes(term)) score += 1;
        else {
          score = 0;
          break;
        }
      }
      if (score) hits.push([score, entry]);
    }
    hits.sort((a, b) => b[0] - a[0]);
    for (const [, entry] of hits.slice(0, 12)) {
      const item = document.createElement("li");
      const link = document.createElement("a");
      link.href = entry.href;
      const title = document.createElement("strong");
      mark(title, entry.title, terms);
      const text = document.createElement("span");
      mark(text, snippet(entry.text, terms[0]), terms);
      link.append(title, text);
      item.append(link);
      list.append(item);
    }
    if (!hits.length) {
      const item = document.createElement("li");
      item.className = "empty";
      item.textContent = input.dataset.empty;
      list.append(item);
    }
    list.hidden = false;
  };
  input.addEventListener("input", show);
  input.addEventListener("focus", () => input.value && show());
  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      select(active + 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      select(active - 1);
    } else if (event.key === "Enter") {
      const target = list.querySelectorAll("a")[Math.max(active, 0)];
      if (target) location.href = target.href;
    } else if (event.key === "Escape") {
      input.value = "";
      show();
      input.blur();
    }
  });
  document.addEventListener("keydown", (event) => {
    const typing = /^(INPUT|TEXTAREA)$/.test(document.activeElement?.tagName ?? "");
    if ((event.key === "/" && !typing) || (event.key === "k" && (event.metaKey || event.ctrlKey))) {
      event.preventDefault();
      input.focus();
      input.select();
    }
  });
  document.addEventListener("click", (event) => {
    if (!event.target.closest(".search")) list.hidden = true;
  });
})();
`;

const STYLE = `
:root { color-scheme: light dark; --text: #1f2328; --muted: #59636e; --line: #d8dbd2; --surface: #f4f5f0; --accent: #556324; --background: #fff; --mark: #e7ecc9; }
@media (prefers-color-scheme: dark) { :root { --text: #e6e6e3; --muted: #9a9a95; --line: #3a3a37; --surface: #252523; --accent: #b5c47a; --background: #181817; --mark: #3d4423; } }
* { box-sizing: border-box; }
body { margin: 0; background: var(--background); color: var(--text); font: 16px/1.65 -apple-system, BlinkMacSystemFont, "PingFang SC", "Helvetica Neue", sans-serif; }
header { position: sticky; top: 0; z-index: 2; display: flex; gap: 20px; align-items: center; padding: 10px 28px; border-bottom: 1px solid var(--line); background: var(--background); }
header a { color: var(--text); text-decoration: none; }
header .home { font-weight: 650; white-space: nowrap; }
header .language { margin-left: auto; color: var(--muted); white-space: nowrap; }
.search { position: relative; flex: 0 1 360px; }
.search input { width: 100%; font: inherit; font-size: 14px; padding: 5px 10px; border: 1px solid var(--line); border-radius: 7px; background: var(--surface); color: var(--text); }
.search input:focus { outline: 2px solid var(--accent); outline-offset: -1px; }
#results { position: absolute; top: calc(100% + 6px); left: 0; width: min(560px, 90vw); max-height: 70vh; overflow-y: auto; margin: 0; padding: 6px; list-style: none; background: var(--background); border: 1px solid var(--line); border-radius: 10px; box-shadow: 0 12px 32px rgb(0 0 0 / 0.14); }
#results a { display: block; padding: 7px 10px; border-radius: 6px; color: var(--text); text-decoration: none; }
#results a:hover, #results a[aria-selected] { background: var(--surface); }
#results strong { display: block; font-size: 14px; }
#results span { display: block; font-size: 13px; color: var(--muted); line-height: 1.45; }
#results mark { background: var(--mark); color: inherit; border-radius: 2px; }
#results .empty { padding: 8px 10px; color: var(--muted); font-size: 14px; }
.layout { display: grid; grid-template-columns: 230px minmax(0, 800px); gap: 44px; padding: 28px; }
nav { position: sticky; top: 76px; align-self: start; max-height: calc(100vh - 96px); overflow-y: auto; }
nav h2 { margin: 18px 0 4px; padding: 0 10px; font-size: 12px; font-weight: 650; letter-spacing: 0.04em; text-transform: uppercase; color: var(--muted); }
nav h2:first-child { margin-top: 0; }
nav ol { list-style: none; margin: 0; padding: 0; }
nav a { display: block; padding: 3px 10px; border-radius: 6px; color: var(--muted); text-decoration: none; font-size: 15px; }
nav a:hover { color: var(--text); }
nav a[aria-current] { background: var(--surface); color: var(--text); font-weight: 600; }
main h1 { font-size: 30px; line-height: 1.25; margin: 0 0 18px; }
main h2 { font-size: 21px; margin: 36px 0 10px; padding-top: 10px; border-top: 1px solid var(--line); scroll-margin-top: 64px; }
main h3 { font-size: 17px; margin: 26px 0 8px; scroll-margin-top: 64px; }
main a { color: var(--accent); }
main img { max-width: 100%; border: 1px solid var(--line); border-radius: 10px; margin: 8px 0; }
main code, main kbd { font: 13.5px ui-monospace, SFMono-Regular, Menlo, monospace; background: var(--surface); border-radius: 4px; padding: 1px 5px; }
main pre { background: var(--surface); padding: 12px 14px; border-radius: 8px; overflow-x: auto; }
main pre code { padding: 0; }
main table { border-collapse: collapse; margin: 12px 0; }
main th, main td { border: 1px solid var(--line); padding: 6px 12px; text-align: left; vertical-align: top; }
main blockquote { margin: 12px 0; padding: 2px 16px; border-left: 3px solid var(--accent); color: var(--muted); }
.pager { display: flex; justify-content: space-between; gap: 16px; margin-top: 48px; padding-top: 20px; border-top: 1px solid var(--line); }
.pager a { display: flex; flex-direction: column; padding: 10px 14px; border: 1px solid var(--line); border-radius: 8px; text-decoration: none; }
.pager a[rel="next"] { margin-left: auto; text-align: right; }
.pager small { color: var(--muted); font-size: 12px; }
@media (max-width: 860px) { header { flex-wrap: wrap; padding: 10px 16px; } .search { order: 3; flex-basis: 100%; } .layout { grid-template-columns: 1fr; gap: 20px; padding: 16px; } nav { position: static; max-height: none; } }
`;

/**
 * Render the guide into `out`, replacing whatever is there: each language's
 * pages and search index (search-index.js), the search script and images/.
 */
export function renderGuide(out) {
  rmSync(out, { recursive: true, force: true });
  const marked = renderer();
  for (const language of LANGUAGES) {
    const groups = contents(language);
    const titles = new Map(groups.flatMap((group) => group.pages).map((entry) => [entry.page, entry.title]));
    const index = [];
    mkdirSync(join(out, language.dir), { recursive: true });
    for (const page of pagesIn(language.dir)) {
      const body = marked.parse(readFileSync(join(GUIDE_DIR, language.dir, page), "utf8"));
      writeFileSync(join(out, language.dir, htmlName(page)), pageHtml({ language, page, body, groups }));
      index.push(...sections(page, titles.get(page) ?? language.title, body));
    }
    writeFileSync(join(out, language.dir, "search-index.js"), `window.LATTICE_GUIDE_INDEX = ${JSON.stringify(index)};\n`);
  }
  writeFileSync(join(out, "search.js"), SEARCH_SCRIPT);
  cpSync(join(GUIDE_DIR, "images"), join(out, "images"), { recursive: true });
}

if (isMain(import.meta.url)) {
  const args = process.argv.slice(2);
  const outIndex = args.indexOf("--out");
  if (outIndex < 0 && !args.includes("--check")) {
    console.error("Usage: node scripts/user-guide.mjs --check | --out DIR");
    process.exit(2);
  }
  const problems = checkGuide();
  if (problems.length > 0) {
    console.error(`The user guide has ${problems.length} problem(s):\n${problems.map((problem) => `  - ${problem}`).join("\n")}`);
    process.exit(1);
  }
  if (outIndex >= 0) {
    const out = args[outIndex + 1];
    if (!out) throw new Error("--out needs a directory");
    renderGuide(out);
    console.log(`Rendered the user guide into ${out}.`);
  } else {
    console.log("The user guide's pages, translations, links and images check out.");
  }
}
