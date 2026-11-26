/**
 * A source file's code with its comments and the spacing between tokens taken
 * out, for telling a change of comments and whitespace from one of code
 * (commentOnlyEdit in scripts/ci-local-select.mjs). Two texts with the same
 * stream differ only in comments and in where the spaces and line breaks
 * between tokens go; text inside a string, a template or a raw string is
 * kept as written, so a comment-looking line or a run of spaces there counts.
 *
 *   JavaScript and TypeScript  the TypeScript parser's tokens, each marked
 *                              when a line break comes before it, since
 *                              automatic semicolons read line breaks
 *   Rust                       a lexer for line, nested block and doc
 *                              comments, strings, raw strings, chars and
 *                              lifetimes
 *   CSS                        block comments and strings (CSS has no `//`)
 *
 * null for any other file.
 */
import { createRequire } from "node:module";

const SCRIPT = /\.([cm]?[jt]sx?)$/;
let ts = null;

/** The stream for `text` as the source of `file`, or null when its language is none of these. */
export function codeStream(file, text) {
  if (SCRIPT.test(file)) return scriptStream(file, text);
  if (file.endsWith(".rs")) return lexed(text, rustToken);
  if (file.endsWith(".css")) return lexed(text, cssToken);
  return null;
}

/** Whether codeStream reads this kind of file. */
export const streamable = (file) => SCRIPT.test(file) || file.endsWith(".rs") || file.endsWith(".css");

function scriptStream(file, text) {
  ts ??= createRequire(import.meta.url)("typescript");
  const kind = /x$/.test(file) ? ts.ScriptKind.TSX : /\.[cm]?js$/.test(file) ? ts.ScriptKind.JS : ts.ScriptKind.TS;
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, kind);
  const tokens = [];
  const walk = (node) => {
    // JSDoc comments come back as children of the node they document.
    if (node.kind >= ts.SyntaxKind.FirstJSDocNode && node.kind <= ts.SyntaxKind.LastJSDocNode) return;
    const children = node.getChildren(source);
    if (children.length) return children.forEach(walk);
    const start = node.getStart(source);
    const breaks = text.slice(node.getFullStart(), start).includes("\n");
    tokens.push(`${breaks ? "\n" : " "}${text.slice(start, node.getEnd())}`);
  };
  walk(source);
  return tokens.join("");
}

/** Runs `token` over `text`: each call returns [what it adds, where it ends]; comments and spacing add one space. */
function lexed(text, token) {
  let out = "";
  for (let at = 0; at < text.length; ) {
    const [piece, end] = token(text, at);
    out += piece === " " && out.endsWith(" ") ? "" : piece;
    at = end;
  }
  return out.trim();
}

const SPACE = /\s/;

function blockComment(text, at, nested) {
  let depth = 0;
  for (let index = at; index < text.length; index += 1) {
    if (text.startsWith("/*", index) && (nested || depth === 0)) {
      depth += 1;
      index += 1;
    } else if (text.startsWith("*/", index)) {
      depth -= 1;
      index += 1;
      if (depth === 0) return index + 1;
    }
  }
  return text.length;
}

function quoted(text, at, quote) {
  for (let index = at + 1; index < text.length; index += 1) {
    if (text[index] === "\\") index += 1;
    else if (text[index] === quote) return index + 1;
  }
  return text.length;
}

function rustToken(text, at) {
  const char = text[at];
  if (SPACE.test(char)) return [" ", at + 1];
  if (text.startsWith("//", at)) {
    const end = text.indexOf("\n", at);
    return [" ", end < 0 ? text.length : end];
  }
  if (text.startsWith("/*", at)) return [" ", blockComment(text, at, true)];
  const identifierBefore = at > 0 && /[A-Za-z0-9_]/.test(text[at - 1]);
  const raw = !identifierBefore && /^(?:b|c)?r(#*)"/.exec(text.slice(at, at + 260));
  if (raw) {
    const close = text.indexOf(`"${raw[1]}`, at + raw[0].length);
    const end = close < 0 ? text.length : close + 1 + raw[1].length;
    return [text.slice(at, end), end];
  }
  if (char === '"') {
    const end = quoted(text, at, '"');
    return [text.slice(at, end), end];
  }
  if (char === "'") {
    const literal = /^'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'\n])'/.exec(text.slice(at, at + 16));
    if (literal) return [literal[0], at + literal[0].length];
  }
  return [char, at + 1];
}

function cssToken(text, at) {
  const char = text[at];
  if (SPACE.test(char)) return [" ", at + 1];
  if (text.startsWith("/*", at)) return [" ", blockComment(text, at, false)];
  if (char === '"' || char === "'") {
    const end = quoted(text, at, char);
    return [text.slice(at, end), end];
  }
  return [char, at + 1];
}
