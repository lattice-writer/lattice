; Rust in the code theme's names, later patterns winning as in the
; JavaScript query: a macro a function, an attribute an attribute, a lifetime
; a label, `self` special. Adapted from tree-sitter-rust's own query (MIT).

(identifier) @variable

((identifier) @type
  (#match? @type "^[A-Z]"))

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z0-9_]+$"))

[
  (type_identifier)
  (primitive_type)
] @type

(field_identifier) @property

(shorthand_field_identifier) @property

(scoped_identifier
  path: (identifier) @type
  (#match? @type "^[A-Z]"))

(call_expression
  function: (identifier) @function)

(call_expression
  function: (field_expression
    field: (field_identifier) @function))

(call_expression
  function: (scoped_identifier
    name: (identifier) @function))

(generic_function
  function: (identifier) @function)

(generic_function
  function: (scoped_identifier
    name: (identifier) @function))

(generic_function
  function: (field_expression
    field: (field_identifier) @function))

(function_item
  name: (identifier) @function)

(function_signature_item
  name: (identifier) @function)

(lifetime
  (identifier) @label)

(self) @variable.special

[
  (char_literal)
  (string_literal)
  (raw_string_literal)
] @string

[
  (integer_literal)
  (float_literal)
  (boolean_literal)
] @number

[
  (line_comment)
  (block_comment)
] @comment

[
  (line_comment
    (doc_comment))
  (block_comment
    (doc_comment))
] @comment.doc

[
  (attribute_item)
  (inner_attribute_item)
] @attribute

[
  "as"
  "async"
  "await"
  "break"
  "const"
  "continue"
  "default"
  "dyn"
  "else"
  "enum"
  "extern"
  "fn"
  "for"
  "if"
  "impl"
  "in"
  "let"
  "loop"
  "macro_rules!"
  "match"
  "mod"
  "move"
  "pub"
  "ref"
  "return"
  "static"
  "struct"
  "trait"
  "type"
  "union"
  "unsafe"
  "use"
  "where"
  "while"
  "yield"
  (crate)
  (mutable_specifier)
  (super)
] @keyword

[
  "*"
  "&"
  "-"
  "+"
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
  "+="
  "-="
  "->"
  "=>"
  ".."
  "..="
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

(type_arguments
  [
    "<"
    ">"
  ] @punctuation.bracket)

(type_parameters
  [
    "<"
    ">"
  ] @punctuation.bracket)

; The same tokens as an operator above, where they mean something else: a
; macro's `!` and a doc comment's third `/` or `!`.
(macro_invocation
  macro: (identifier) @function
  "!" @function)

(line_comment
  [
    (outer_doc_comment_marker)
    (inner_doc_comment_marker)
  ] @comment.doc)

(block_comment
  [
    (outer_doc_comment_marker)
    (inner_doc_comment_marker)
  ] @comment.doc)
