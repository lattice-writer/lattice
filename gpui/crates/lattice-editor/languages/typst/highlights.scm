; Typst highlighting for uben0's tree-sitter grammar (codebook-tree-sitter-typst),
; in the code theme's names, so every code theme colours a .typ file in light
; and dark as it colours LaTeX and Markdown:
;
;   keyword          #let, #set, #show, #import, control flow, `#` itself
;   function         a called function (`#figure(..)`, `text(..)`)
;   property         a named argument or a field (`fill:`, `.body`)
;   variable         any other identifier in code, and maths between its
;                    dollars, as LaTeX's maths is
;   string           a string
;   number           a number; constant: true, false, none, auto, escapes
;   label            a label (`<intro>`) or reference (`@intro`)
;   markup.heading   a heading; markup.meta its markers, list marks, `$`
;   markup.raw       a raw span or block; markup.link a URL
;   emphasis         _emphasis_ and *strong*
;
; Later patterns win over earlier ones on the same node, so the general
; rules come first.

(ident) @variable

(math) @variable

(comment) @comment

(string) @string
(number) @number
[
  (bool)
  (none)
  (auto)
] @constant
[
  (escape)
  (shorthand)
  (linebreak)
] @constant

(call
  item: (ident) @function)
(call
  item: (field
    field: (ident) @function))
(field
  field: (ident) @property)
(tagged
  field: (ident) @property)

[
  "let"
  "set"
  "show"
  "import"
  "include"
  "as"
  "if"
  "else"
  "for"
  "in"
  "while"
  "return"
  "break"
  "continue"
  "context"
  "and"
  "or"
  "not"
] @keyword

"#" @keyword

; Operators by the node they belong to: the same characters are markup's
; marks elsewhere (`*strong*`, `_emphasis_`, `<label>`).
(sign ["+" "-"] @operator)
(add "+" @operator)
(sub "-" @operator)
(mul "*" @operator)
(div "/" @operator)
(cmp ["==" "!=" "<" "<=" ">" ">="] @operator)
(fraction "/" @operator)
(fac "!" @operator)
(attach ["^" "_"] @operator)
(lambda "=>" @operator)
"assign" @operator
(wildcard) @operator
(align) @operator

[
  "("
  ")"
  "{"
  "}"
  "["
  "]"
] @punctuation.bracket

(heading) @markup.heading
(heading
  [
    "="
    "=="
    "==="
    "===="
    "====="
    "======"
  ] @markup.meta)
(item
  [
    "-"
    "+"
  ] @markup.meta)
(term
  [
    "/"
    ":"
  ] @markup.meta)
(math
  "$" @markup.meta)

(emph) @emphasis
(strong) @emphasis.strong
(emph "_" @markup.meta)
(strong "*" @markup.meta)

(url) @markup.link

[
  (raw_span)
  (raw_blck)
] @markup.raw
(raw_blck
  lang: (ident) @markup.raw)

[
  (label)
  (ref)
] @label

; Inside a string or comment nothing else is coloured.
(string) @string
(comment) @comment
