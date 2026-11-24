/**
 * Shiki, loaded on demand for the visual engine's code blocks
 * (code-highlight.ts): VS Code's TextMate grammars, one lazy chunk per
 * language, tokenized with Shiki's JavaScript regex engine (no WebAssembly)
 * under a theme whose colours are Lattice's `--syntax-*` roles
 * (settings/syntax-scopes.ts), so a block wears the chosen code theme.
 */
import { createHighlighterCore, type LanguageRegistration } from "@shikijs/core";
import { createJavaScriptRegexEngine } from "@shikijs/engine-javascript";
import { cssVariablesTheme } from "../../../settings/syntax-scopes";

export type CodeToken = { from: number; to: number; color: string; italic: boolean };

/** Lines longer than this (minified code) stay plain, as VS Code leaves them. */
export const MAX_LINE_LENGTH = 1500;
/** Blocks longer than this stay plain. */
export const MAX_BLOCK_LENGTH = 100_000;

type Grammar = () => Promise<{ default: LanguageRegistration[] }>;

/** Every grammar code-languages.ts names, each its own chunk. */
const GRAMMARS: Record<string, Grammar> = {
  bash: () => import("@shikijs/langs/bash"),
  c: () => import("@shikijs/langs/c"),
  cpp: () => import("@shikijs/langs/cpp"),
  csharp: () => import("@shikijs/langs/csharp"),
  css: () => import("@shikijs/langs/css"),
  diff: () => import("@shikijs/langs/diff"),
  go: () => import("@shikijs/langs/go"),
  graphql: () => import("@shikijs/langs/graphql"),
  html: () => import("@shikijs/langs/html"),
  java: () => import("@shikijs/langs/java"),
  javascript: () => import("@shikijs/langs/javascript"),
  json: () => import("@shikijs/langs/json"),
  kotlin: () => import("@shikijs/langs/kotlin"),
  latex: () => import("@shikijs/langs/latex"),
  lua: () => import("@shikijs/langs/lua"),
  makefile: () => import("@shikijs/langs/makefile"),
  markdown: () => import("@shikijs/langs/markdown"),
  php: () => import("@shikijs/langs/php"),
  python: () => import("@shikijs/langs/python"),
  r: () => import("@shikijs/langs/r"),
  ruby: () => import("@shikijs/langs/ruby"),
  rust: () => import("@shikijs/langs/rust"),
  scss: () => import("@shikijs/langs/scss"),
  sql: () => import("@shikijs/langs/sql"),
  swift: () => import("@shikijs/langs/swift"),
  toml: () => import("@shikijs/langs/toml"),
  typescript: () => import("@shikijs/langs/typescript"),
  yaml: () => import("@shikijs/langs/yaml"),
};

export const hasGrammar = (grammar: string) => Object.hasOwn(GRAMMARS, grammar);

const THEME = cssVariablesTheme();
const INK = THEME.colors["editor.foreground"];
const highlighter = createHighlighterCore({ themes: [THEME], langs: [], engine: createJavaScriptRegexEngine({ forgiving: true }) });
const loads = new Map<string, Promise<void>>();
const loaded = new Set<string>();
let ready: Awaited<typeof highlighter> | null = null;

/** Load `grammar` once; resolves when `tokenize` can use it. */
export function loadGrammar(grammar: string): Promise<void> {
  const known = loads.get(grammar);
  if (known) return known;
  const load = Promise.all([highlighter, GRAMMARS[grammar]!()]).then(async ([instance, module]) => {
    await instance.loadLanguage(...module.default);
    ready = instance;
    loaded.add(grammar);
  });
  load.catch(() => loads.delete(grammar));
  loads.set(grammar, load);
  return load;
}

/**
 * Token ranges (relative to the code's start) of `code` in `grammar`, or
 * null until the grammar has loaded. Tokens in the ink colour are left out,
 * and a token never spans a line ending.
 */
export function tokenize(code: string, grammar: string): CodeToken[] | null {
  if (!ready || !loaded.has(grammar)) return null;
  // A block is tokenized again, whole and synchronously, on every edit inside
  // it, so a huge one (a pasted log, a data dump) stays plain.
  if (code.length > MAX_BLOCK_LENGTH) return [];
  const tokens: CodeToken[] = [];
  let lines;
  try {
    lines = ready.codeToTokensBase(code, { lang: grammar, theme: THEME.name, tokenizeMaxLineLength: MAX_LINE_LENGTH });
  } catch {
    return [];
  }
  for (const line of lines) {
    for (const token of line) {
      if (!token.content || !token.color || token.color === INK) continue;
      // Shiki's FontStyle flags: 1 is italic.
      tokens.push({ from: token.offset, to: token.offset + token.content.length, color: token.color, italic: ((token.fontStyle ?? 0) & 1) === 1 });
    }
  }
  return tokens;
}
