import darkPlus from "@shikijs/themes/dark-plus";
import githubDarkDefault from "@shikijs/themes/github-dark-default";
import githubLightDefault from "@shikijs/themes/github-light-default";
import lightPlus from "@shikijs/themes/light-plus";
import { describe, expect, it } from "vitest";
import { syntaxProperty, THEMES, type SyntaxRole } from "./code-themes";
import { cssVariablesTheme, resolveRoleColors, resolveScopeColor, ROLE_SCOPES, type VscodeTheme } from "./syntax-scopes";

describe("code themes taken from @shikijs/themes", () => {
  const sources: [string, keyof typeof THEMES, "light" | "dark", VscodeTheme][] = [
    ["light-plus", "vscode", "light", lightPlus],
    ["dark-plus", "vscode", "dark", darkPlus],
    ["github-light-default", "github", "light", githubLightDefault],
    ["github-dark-default", "github", "dark", githubDarkDefault],
  ];
  it.each(sources)("%s gives every role the colour code-themes.ts holds", (_name, id, kind, theme) => {
    const palette = THEMES[id][kind]!;
    expect(palette.tokens).toEqual(resolveRoleColors(theme));
    expect(palette.ink).toBe(theme.colors?.["editor.foreground"]?.toLowerCase());
  });
});

describe("resolveScopeColor", () => {
  it("lets the longest matching selector win, as VS Code does", () => {
    expect(resolveScopeColor(lightPlus, "keyword.operator.assignment.python")).toBe("#000000");
    expect(resolveScopeColor(lightPlus, "keyword.operator.logical.python")).toBe("#0000ff");
    expect(resolveScopeColor(lightPlus, "storage.type.function.python")).toBe("#0000ff");
  });

  it("ignores selectors that need a parent scope", () => {
    expect(resolveScopeColor({ tokenColors: [{ scope: "source.css variable", settings: { foreground: "#123456" } }] }, "variable"))
      .toBeUndefined();
  });
});

describe("cssVariablesTheme", () => {
  it("paints every role with its custom property", () => {
    const theme = cssVariablesTheme();
    expect(theme.tokenColors.map((rule) => rule.settings.foreground))
      .toEqual(Object.keys(ROLE_SCOPES).map((role) => `var(${syntaxProperty(role as SyntaxRole)})`));
  });
});
