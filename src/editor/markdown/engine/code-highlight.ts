/**
 * Syntax highlighting for the visual engine's code blocks: Shiki's tokens
 * (VS Code's TextMate grammars, shiki-code.ts) as ProseMirror inline
 * decorations over the code text (spec R-BLK-7), each painted with the
 * `--syntax-*` role its scopes map to, so a block wears the chosen code
 * theme as the source editors do.
 *
 * Shiki and each grammar load on demand: a block whose grammar has not
 * loaded yet shows plain, and every editor is highlighted again once it has.
 * A decorated span never contains a line ending: WebKit cannot move the caret
 * across a newline that sits inside an inline element, and Shiki tokenizes
 * line by line. Only the code blocks a transaction touched are highlighted
 * again; the rest of the decorations are mapped.
 *
 * Clean implementation for Lattice; spec: docs/visual-editor-spec.md.
 */
import { Extension } from "@tiptap/core";
import type { Node as PmNode } from "@tiptap/pm/model";
import { Plugin, PluginKey, type Transaction } from "@tiptap/pm/state";
import { Decoration, DecorationSet, type EditorView } from "@tiptap/pm/view";
import { resolveCodeLanguage } from "./code-languages";
import type * as ShikiCode from "./shiki-code";

type Shiki = typeof ShikiCode;

let shiki: Shiki | null = null;
let shikiLoad: Promise<Shiki> | null = null;
const requested = new Set<string>();
/** Editors to highlight again when a grammar arrives. */
const views = new Set<EditorView>();

const key = new PluginKey<DecorationSet>("latticeCodeHighlight");

/** Keywords and functions are set at medium weight, as in the source editors (latex-editor.ts, the Zed theme). */
const MEDIUM = new Set(["var(--syntax-keyword)", "var(--syntax-function)"]);

/** Start loading `grammar` (and Shiki) unless it is on its way. */
function request(grammar: string) {
  if (requested.has(grammar)) return;
  requested.add(grammar);
  shikiLoad ??= import("./shiki-code").then((module) => (shiki = module));
  void shikiLoad
    .then((module) => (module.hasGrammar(grammar) ? module.loadGrammar(grammar) : undefined))
    .then(() => {
      for (const view of views) view.dispatch(view.state.tr.setMeta(key, "grammar"));
    })
    .catch(() => {
      // A grammar that fails to load leaves its blocks plain until the next
      // edit asks for it again (an offline chunk, a stale build).
      requested.delete(grammar);
      if (!shiki) shikiLoad = null;
    });
}

function blockDecorations(node: PmNode, position: number): Decoration[] {
  const grammar = resolveCodeLanguage(node.attrs.language as string | null)?.grammar;
  if (!grammar || !node.textContent) return [];
  const tokens = shiki?.tokenize(node.textContent, grammar);
  if (!tokens) {
    request(grammar);
    return [];
  }
  return tokens.map((token) => Decoration.inline(position + 1 + token.from, position + 1 + token.to, {
    style: `color: ${token.color}${MEDIUM.has(token.color) ? "; font-weight: 500" : ""}${token.italic ? "; font-style: italic" : ""}`,
  }));
}

function highlightAll(doc: PmNode): DecorationSet {
  const decorations: Decoration[] = [];
  doc.descendants((node, position) => {
    if (node.type.name === "codeBlock") {
      decorations.push(...blockDecorations(node, position));
      return false;
    }
    return !node.isTextblock;
  });
  return DecorationSet.create(doc, decorations);
}

/** Map the old decorations and re-highlight only the code blocks the transaction changed. */
function highlightChanges(set: DecorationSet, transaction: Transaction): DecorationSet {
  let next = set.map(transaction.mapping, transaction.doc);
  const touched = new Map<number, PmNode>();
  transaction.mapping.maps.forEach((map, index) => {
    map.forEach((_oldStart, _oldEnd, newStart, newEnd) => {
      const rest = transaction.mapping.slice(index + 1);
      const from = Math.min(rest.map(newStart, -1), transaction.doc.content.size);
      const to = Math.min(rest.map(newEnd, 1), transaction.doc.content.size);
      transaction.doc.nodesBetween(from, Math.max(from, to), (node, position) => {
        if (node.type.name === "codeBlock") {
          touched.set(position, node);
          return false;
        }
        return !node.isTextblock;
      });
    });
  });
  // Attribute-only changes (a language switch) map no ranges; find their blocks too.
  transaction.steps.forEach((step, index) => {
    const position = (step as unknown as { pos?: number }).pos;
    if (typeof position !== "number") return;
    const mapped = transaction.mapping.slice(index + 1).map(position);
    const node = transaction.doc.nodeAt(mapped);
    if (node?.type.name === "codeBlock") touched.set(mapped, node);
  });
  for (const [position, node] of touched) {
    next = next.remove(next.find(position, position + node.nodeSize));
    next = next.add(transaction.doc, blockDecorations(node, position));
  }
  return next;
}

export const CodeHighlight = Extension.create({
  name: "latticeCodeHighlight",
  addProseMirrorPlugins: () => [new Plugin<DecorationSet>({
    key,
    state: {
      init: (_config, state) => highlightAll(state.doc),
      apply: (transaction, set) => {
        if (transaction.getMeta(key) === "grammar") return highlightAll(transaction.doc);
        return transaction.docChanged ? highlightChanges(set, transaction) : set;
      },
    },
    view: (view) => {
      views.add(view);
      return { destroy: () => views.delete(view) };
    },
    props: { decorations: (state) => key.getState(state) },
  })],
});
