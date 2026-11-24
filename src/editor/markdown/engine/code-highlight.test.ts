/**
 * The highlight plugin's lifecycle: Shiki and each grammar load on demand, so
 * a block starts plain, every live editor is highlighted again when its
 * grammar arrives, a destroyed one is left alone, and a failed load is tried
 * again on the next edit.
 */
import { Editor } from "@tiptap/core";
import CodeBlock from "@tiptap/extension-code-block";
import Document from "@tiptap/extension-document";
import Text from "@tiptap/extension-text";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const editors: Editor[] = [];

async function editorWith(html: string) {
  const { CodeHighlight } = await import("./code-highlight");
  const editor = new Editor({ extensions: [Document, Text, CodeBlock, CodeHighlight], content: html });
  editors.push(editor);
  return editor;
}

const painted = (editor: Editor) => editor.view.dom.querySelectorAll('span[style*="--syntax-"]');
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => vi.resetModules());
afterEach(() => {
  for (const editor of editors.splice(0)) if (!editor.isDestroyed) editor.destroy();
  vi.doUnmock("./shiki-code");
});

describe("code block highlighting", () => {
  it("shows a block plain until its grammar loads, then highlights it", async () => {
    const editor = await editorWith('<pre><code class="language-python">def solve(): return 1</code></pre>');
    expect(painted(editor)).toHaveLength(0);
    await vi.waitFor(() => expect(painted(editor).length).toBeGreaterThan(0), { timeout: 10_000 });
    expect([...painted(editor)].map((span) => span.textContent)).toContain("def");
  });

  it("leaves a block in a language it has no grammar for plain", async () => {
    const editor = await editorWith('<pre><code class="language-brainfuck">+[-->-[>>+>-----&lt;&lt;]</code></pre>');
    await settle();
    expect(painted(editor)).toHaveLength(0);
  });

  it("does not touch an editor destroyed before the grammar arrives", async () => {
    let arrive!: () => void;
    vi.doMock("./shiki-code", () => ({
      hasGrammar: () => true,
      loadGrammar: () => new Promise<void>((resolve) => (arrive = resolve)),
      tokenize: () => null,
    }));
    const editor = await editorWith('<pre><code class="language-python">x = 1</code></pre>');
    await vi.waitFor(() => expect(arrive).toBeTypeOf("function"));
    const dispatch = vi.spyOn(editor.view, "dispatch");
    editor.destroy();
    arrive();
    await settle();
    expect(dispatch).not.toHaveBeenCalled();
  });

  it("asks again for a grammar that failed to load", async () => {
    const loadGrammar = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(undefined);
    vi.doMock("./shiki-code", () => ({ hasGrammar: () => true, loadGrammar, tokenize: () => null }));
    const editor = await editorWith('<pre><code class="language-python">x = 1</code></pre>');
    await vi.waitFor(() => expect(loadGrammar).toHaveBeenCalledTimes(1));
    await settle();
    editor.commands.insertContentAt(2, "y");
    await vi.waitFor(() => expect(loadGrammar).toHaveBeenCalledTimes(2));
  });
});
