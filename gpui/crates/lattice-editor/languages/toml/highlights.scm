; TOML in the code theme's names: a table's name a type, a key a
; property, strings strings, numbers and booleans numbers, dates constants.
; Adapted from tree-sitter-toml (MIT).

(bare_key) @type

(quoted_key) @string

(pair
  (bare_key) @property)

(pair
  (quoted_key) @property)

(pair
  (dotted_key
    (bare_key) @property))

(comment) @comment

(string) @string

[
  (integer)
  (float)
  (boolean)
] @number

[
  (offset_date_time)
  (local_date_time)
  (local_date)
  (local_time)
] @constant

"=" @operator

[
  "["
  "]"
  "[["
  "]]"
  "{"
  "}"
] @punctuation.bracket
