; TypeScript's own tokens, appended to the JavaScript query (TypeScript's
; grammar extends JavaScript's): a type a type, its keywords keywords.
; Adapted from tree-sitter-typescript's own query (MIT).

[
  (type_identifier)
  (predefined_type)
] @type

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

(interface_declaration
  name: (type_identifier) @type)

(decorator) @attribute

(decorator
  (identifier) @attribute)

(decorator
  (call_expression
    function: (identifier) @attribute))

[
  "abstract"
  "declare"
  "enum"
  "implements"
  "interface"
  "keyof"
  "namespace"
  "private"
  "protected"
  "public"
  "readonly"
  "override"
  "satisfies"
  "type"
] @keyword
