/**
 * The one table that ties Lattice's syntax roles (the `--syntax-*`
 * properties every highlighter paints with) to TextMate scopes, the names
 * VS Code themes and grammars use. It is read two ways:
 *
 * - `resolveRoleColors` reads a VS Code theme (as @shikijs/themes ships it)
 *   the way VS Code and Shiki do, scope selector by scope selector, and
 *   returns the colour each role's first scope gets. code-themes.ts holds
 *   those colours for the themes taken from @shikijs/themes, and its test
 *   checks they still match.
 * - `cssVariablesTheme` turns the whole table into a Shiki theme whose every
 *   colour is the role's `--syntax-*` property, so the visual editor's code blocks,
 *   tokenized by Shiki's TextMate grammars, wear whichever code theme the
 *   writer chose, exactly as the source editors and GPUI's tree-sitter
 *   captures (their roles are the same names) do.
 *
 * A grammar scope takes the role of the most specific selector it matches,
 * as in VS Code: `keyword.operator.logical.python` (`not`, `in`) is a
 * keyword, not an operator, because VS Code's themes say so.
 */
import { syntaxProperty, type SyntaxRole } from "./code-themes";

/* eslint-disable lingui/no-unlocalized-strings -- TextMate scope names */
export const ROLE_SCOPES: Record<SyntaxRole, readonly string[]> = {
  keyword: [
    "keyword", "storage", "storage.type", "storage.modifier", "keyword.control", "meta.preprocessor",
    "keyword.operator.new", "keyword.operator.expression", "keyword.operator.instanceof",
    "keyword.operator.logical.python", "keyword.operator.wordlike",
    // LaTeX: a command is a keyword in every Lattice surface (latex-editor.ts,
    // the tree-sitter query), where TextMate's LaTeX grammar calls it a
    // support function.
    "text.tex support.function", "text.tex punctuation.definition.function",
  ],
  operator: ["keyword.operator"],
  // A call's name is a function to tree-sitter, so Python's generic calls
  // (`parts.split()`) are too; VS Code paints them so with semantic tokens.
  function: ["entity.name.function", "support.function", "meta.function-call.generic"],
  type: [
    "entity.name.type", "entity.name.class", "entity.name.namespace", "entity.name.tag", "support.type", "support.class",
    "entity.other.inherited-class",
  ],
  variable: ["variable", "meta.definition.variable.name", "entity.name.variable"],
  "variable-special": ["variable.language", "support.variable"],
  property: [
    "variable.other.property", "variable.other.object.property", "meta.object-literal.key", "support.type.property-name",
    "meta.attribute.python",
  ],
  attribute: ["entity.other.attribute-name"],
  // An escape stays in its string's colour, as tree-sitter's string node has it.
  // So do its quotes, which a bare `punctuation` rule would take for brackets.
  string: ["string", "constant.character.escape", "punctuation.definition.string"],
  comment: ["comment", "punctuation.definition.comment"],
  // Python's docstrings are documentation comments to tree-sitter
  // (`@comment.doc`), so they are here too.
  "comment-doc": ["comment.block.documentation", "string.quoted.docstring"],
  number: ["constant.numeric"],
  constant: [
    "constant.language", "constant", "support.constant", "variable.other.constant",
    "variable.other.enummember",
  ],
  bracket: ["punctuation"],
  citation: ["constant.other.reference.citation.latex"],
  reference: ["constant.other.reference.label.latex", "variable.parameter.definition.label.latex"],
};
/* eslint-enable lingui/no-unlocalized-strings */

/** The fields of a VS Code theme this reads. */
export type VscodeTheme = {
  colors?: Record<string, string>;
  tokenColors?: readonly { scope?: string | readonly string[]; settings: { foreground?: string; fontStyle?: string } }[];
};

/**
 * The foreground `theme` gives `scope`, as VS Code resolves it for a token
 * whose only scope is `scope`: of the selectors that are `scope` or one of
 * its dot-prefixes, the longest wins, and a later rule wins a tie. Selectors
 * with a parent (`source.css variable`) need a parent scope and never apply.
 */
export function resolveScopeColor(theme: VscodeTheme, scope: string): string | undefined {
  let best: { depth: number; color: string } | undefined;
  for (const rule of theme.tokenColors ?? []) {
    const color = rule.settings.foreground;
    if (!color || !rule.scope) continue;
    const selectors = typeof rule.scope === "string" ? rule.scope.split(",") : rule.scope;
    for (const raw of selectors) {
      const selector = raw.trim();
      if (!selector || selector.includes(" ")) continue;
      if (scope !== selector && !scope.startsWith(`${selector}.`)) continue;
      const depth = selector.split(".").length;
      if (!best || depth >= best.depth) best = { depth, color };
    }
  }
  return best && normalizeHex(best.color);
}

/** `#RGB`, `#RRGGBB` or `#RRGGBBFF` as lower-case `#rrggbb`. */
function normalizeHex(color: string): string {
  const hex = color.toLowerCase();
  if (hex.length === 4) return `#${[...hex.slice(1)].map((digit) => digit + digit).join("")}`;
  return hex.length === 9 && hex.endsWith("ff") ? hex.slice(0, 7) : hex;
}

/** Each role's colour in `theme`: its first scope's, else the editor's ink. */
export function resolveRoleColors(theme: VscodeTheme): Record<SyntaxRole, string> {
  const ink = normalizeHex(theme.colors?.["editor.foreground"] ?? "#000000");
  return Object.fromEntries(Object.entries(ROLE_SCOPES).map(([role, scopes]) => (
    [role, resolveScopeColor(theme, scopes[0]) ?? ink]
  ))) as Record<SyntaxRole, string>;
}

/**
 * A Shiki theme whose colours are the roles' custom properties: a token is
 * painted with the `--syntax-*` property of the role its scopes match, and comments
 * are italic as every Lattice surface sets them.
 */
export function cssVariablesTheme() {
  return {
    name: "lattice-syntax",
    type: "light" as const,
    colors: { "editor.foreground": "var(--code-ink)", "editor.background": "transparent" },
    tokenColors: Object.entries(ROLE_SCOPES).map(([role, scopes]) => ({
      scope: [...scopes],
      settings: {
        foreground: `var(${syntaxProperty(role as SyntaxRole)})`,
        ...(role === "comment" || role === "comment-doc" ? { fontStyle: "italic" } : {}),
      },
    })),
  };
}
