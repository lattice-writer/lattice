; YAML in the code theme's names: a key a property, scalars strings,
; numbers and booleans numbers, null a constant, an anchor or alias a label,
; a tag a type, a directive a keyword. Adapted from tree-sitter-yaml (MIT).

[
  (double_quote_scalar)
  (single_quote_scalar)
  (block_scalar)
  (string_scalar)
] @string

[
  (integer_scalar)
  (float_scalar)
  (boolean_scalar)
] @number

(null_scalar) @constant

(comment) @comment

[
  (anchor_name)
  (alias_name)
] @label

(tag) @type

[
  (yaml_directive)
  (tag_directive)
  (reserved_directive)
] @keyword

(block_mapping_pair
  key: (flow_node
    [
      (double_quote_scalar)
      (single_quote_scalar)
    ] @property))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @property)))

(flow_mapping
  (_
    key: (flow_node
      [
        (double_quote_scalar)
        (single_quote_scalar)
      ] @property)))

(flow_mapping
  (_
    key: (flow_node
      (plain_scalar
        (string_scalar) @property))))

[
  "*"
  "&"
  "---"
  "..."
] @operator

[
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket
