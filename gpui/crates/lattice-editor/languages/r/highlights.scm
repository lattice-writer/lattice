; R in the code theme's names, later patterns winning as in the JavaScript
; query: a function assigned or called a function, a named argument a
; property, a package's name special. Adapted from tree-sitter-r (MIT).

(identifier) @variable

(binary_operator
  lhs: (identifier) @function
  operator: "<-"
  rhs: (function_definition))

(binary_operator
  lhs: (identifier) @function
  operator: "="
  rhs: (function_definition))

(call
  function: (identifier) @function)

(call
  function: (namespace_operator
    rhs: (identifier) @function))

(namespace_operator
  lhs: (identifier) @variable.special)

(arguments
  (argument
    name: (identifier) @property))

(parameters
  (parameter
    name: (identifier) @variable))

[
  (integer)
  (float)
  (complex)
  (true)
  (false)
] @number

[
  (null)
  (inf)
  (nan)
  (na)
] @constant

(string) @string

(comment) @comment

[
  "if"
  "else"
  "for"
  "while"
  "repeat"
  "in"
  "function"
  (next)
  (break)
] @keyword

((call
  function: (identifier) @keyword)
  (#eq? @keyword "return"))

[
  "?"
  ":="
  "="
  "<-"
  "<<-"
  "->"
  "->>"
  "~"
  "|>"
  "||"
  "|"
  "&&"
  "&"
  "<"
  "<="
  ">"
  ">="
  "=="
  "!="
  "+"
  "-"
  "*"
  "/"
  "::"
  ":::"
  "**"
  "^"
  "$"
  "@"
  ":"
  "!"
  "special"
] @operator

[
  "("
  ")"
  "{"
  "}"
  "["
  "]"
  "[["
  "]]"
] @punctuation.bracket
