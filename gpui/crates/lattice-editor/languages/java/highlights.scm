; Java in the code theme's names, later patterns winning as in the
; JavaScript query: an annotation an attribute, `this` and `super` special.
; Adapted from tree-sitter-java's own query (MIT).

(identifier) @variable

((identifier) @type
  (#match? @type "^[A-Z]"))

((identifier) @constant
  (#match? @constant "^_*[A-Z][A-Z0-9_]+$"))

(field_access
  field: (identifier) @property)

[
  (type_identifier)
  (boolean_type)
  (integral_type)
  (floating_point_type)
  (void_type)
] @type

(class_declaration
  name: (identifier) @type)

(interface_declaration
  name: (identifier) @type)

(enum_declaration
  name: (identifier) @type)

(record_declaration
  name: (identifier) @type)

(constructor_declaration
  name: (identifier) @type)

(method_declaration
  name: (identifier) @function)

(method_invocation
  name: (identifier) @function)

(annotation
  "@" @attribute
  name: (identifier) @attribute)

(marker_annotation
  "@" @attribute
  name: (identifier) @attribute)

[
  (this)
  (super)
] @variable.special

[
  (character_literal)
  (string_literal)
] @string

[
  (hex_integer_literal)
  (decimal_integer_literal)
  (octal_integer_literal)
  (binary_integer_literal)
  (decimal_floating_point_literal)
  (hex_floating_point_literal)
  (true)
  (false)
] @number

(null_literal) @constant

(line_comment) @comment

(block_comment) @comment

((block_comment) @comment.doc
  (#match? @comment.doc "^/\\*\\*"))

[
  "abstract"
  "assert"
  "break"
  "case"
  "catch"
  "class"
  "continue"
  "default"
  "do"
  "else"
  "enum"
  "exports"
  "extends"
  "final"
  "finally"
  "for"
  "if"
  "implements"
  "import"
  "instanceof"
  "interface"
  "module"
  "native"
  "new"
  "non-sealed"
  "open"
  "opens"
  "package"
  "permits"
  "private"
  "protected"
  "provides"
  "public"
  "record"
  "requires"
  "return"
  "sealed"
  "static"
  "strictfp"
  "switch"
  "synchronized"
  "throw"
  "throws"
  "to"
  "transient"
  "transitive"
  "try"
  "uses"
  "volatile"
  "when"
  "while"
  "with"
  "yield"
] @keyword

[
  "+"
  "-"
  "*"
  "/"
  "%"
  "="
  "=="
  "!="
  "!"
  "<"
  ">"
  "<="
  ">="
  "&&"
  "||"
  "++"
  "--"
  "+="
  "-="
  "->"
  "?"
  "::"
] @operator

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket
