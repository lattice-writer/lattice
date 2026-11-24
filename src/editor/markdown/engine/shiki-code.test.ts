import { describe, expect, it } from "vitest";
import { loadGrammar, tokenize } from "./shiki-code";

describe("Shiki code tokens", () => {
  it("paints each token with the role its TextMate scopes map to", async () => {
    const code = "def solve(n):\n    x = int(n) if len(n) > 1 else None  # done\n    return \"ok\\n\"\n";
    expect(tokenize(code, "python")).toBeNull();
    await loadGrammar("python");
    const roles = new Map(tokenize(code, "python")!.map((token) => [code.slice(token.from, token.to), token]));
    expect(roles.get("def")?.color).toBe("var(--syntax-keyword)");
    expect(roles.get("solve")?.color).toBe("var(--syntax-function)");
    expect(roles.get("int")?.color).toBe("var(--syntax-type)");
    expect(roles.get("len")?.color).toBe("var(--syntax-function)");
    expect(roles.get("1")?.color).toBe("var(--syntax-number)");
    expect(roles.get("None")?.color).toBe("var(--syntax-constant)");
    // Quotes and escapes keep their string's colour, so Shiki merges them into one token.
    expect(roles.get("\"ok\\n\"")?.color).toBe("var(--syntax-string)");
    expect(roles.get("# done")).toMatchObject({ color: "var(--syntax-comment)", italic: true });
    expect([...roles.keys()]).not.toContain("x");
  });
});
