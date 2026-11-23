; C in the code theme's names, later patterns winning as in the JavaScript
; query. Adapted from tree-sitter-c's own query (MIT).

(identifier) @variable

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z0-9_]*$"))

(field_identifier) @property

(statement_identifier) @label

[
  (type_identifier)
  (primitive_type)
  (sized_type_specifier)
] @type

(call_expression
  function: (identifier) @function)

(call_expression
  function: (field_expression
    field: (field_identifier) @function))

(function_declarator
  declarator: (identifier) @function)

(preproc_function_def
  name: (identifier) @function)

[
  (string_literal)
  (system_lib_string)
  (char_literal)
] @string

[
  (number_literal)
  (true)
  (false)
] @number

(null) @constant

(comment) @comment

[
  "#define"
  "#elif"
  "#else"
  "#endif"
  "#if"
  "#ifdef"
  "#ifndef"
  "#include"
  (preproc_directive)
] @keyword

[
  "break"
  "case"
  "const"
  "continue"
  "default"
  "do"
  "else"
  "enum"
  "extern"
  "for"
  "goto"
  "if"
  "inline"
  "return"
  "sizeof"
  "static"
  "struct"
  "switch"
  "typedef"
  "union"
  "volatile"
  "while"
] @keyword

[
  "--"
  "-"
  "-="
  "->"
  "="
  "!"
  "!="
  "*"
  "*="
  "/"
  "/="
  "%"
  "&"
  "&&"
  "|"
  "||"
  "^"
  "~"
  "+"
  "++"
  "+="
  "<"
  "<<"
  "<="
  "=="
  ">"
  ">>"
  ">="
  "?"
] @operator

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket
