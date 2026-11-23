; C++ in the code theme's names: C's patterns (C++'s grammar extends C's),
; then C++'s own. Adapted from tree-sitter-c and tree-sitter-cpp (MIT).

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

(namespace_identifier) @type

(auto) @type

(this) @variable.special

(call_expression
  function: (qualified_identifier
    name: (identifier) @function))

(template_function
  name: (identifier) @function)

(template_method
  name: (field_identifier) @function)

(function_declarator
  declarator: (qualified_identifier
    name: (identifier) @function))

(function_declarator
  declarator: (field_identifier) @function)

(raw_string_literal) @string

(null
  "nullptr" @constant)

[
  "catch"
  "class"
  "co_await"
  "co_return"
  "co_yield"
  "concept"
  "consteval"
  "constexpr"
  "constinit"
  "delete"
  "explicit"
  "final"
  "friend"
  "mutable"
  "namespace"
  "new"
  "noexcept"
  "override"
  "private"
  "protected"
  "public"
  "requires"
  "template"
  "throw"
  "try"
  "typename"
  "using"
  "virtual"
] @keyword

"::" @operator
