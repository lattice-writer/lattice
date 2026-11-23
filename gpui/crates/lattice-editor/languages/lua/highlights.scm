; Lua in the code theme's names, later patterns winning as in the
; JavaScript query: `self` special, a field a property. Adapted from
; tree-sitter-lua's own query (MIT).

(identifier) @variable

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z_0-9]*$"))

((identifier) @variable.special
  (#eq? @variable.special "self"))

(field
  name: (identifier) @property)

(dot_index_expression
  field: (identifier) @property)

(function_declaration
  name: [
    (identifier) @function
    (dot_index_expression
      field: (identifier) @function)
    (method_index_expression
      method: (identifier) @function)
  ])

(function_call
  name: [
    (identifier) @function
    (dot_index_expression
      field: (identifier) @function)
    (method_index_expression
      method: (identifier) @function)
  ])

(label_statement) @label

[
  (number)
  (true)
  (false)
] @number

[
  (nil)
  (vararg_expression)
] @constant

(string) @string

(comment) @comment

(hash_bang_line) @comment

[
  "and"
  "do"
  "else"
  "elseif"
  "end"
  "for"
  "function"
  "goto"
  "if"
  "in"
  "local"
  "not"
  "or"
  "repeat"
  "return"
  "then"
  "until"
  "while"
] @keyword

(break_statement) @keyword

(binary_expression
  operator: _ @operator)

(unary_expression
  operator: _ @operator)

"=" @operator

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket
