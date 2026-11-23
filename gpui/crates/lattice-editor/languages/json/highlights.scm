; JSON in the code theme's names: a key a property, strings strings,
; numbers and booleans numbers, null a constant.

(string) @string

(pair
  key: (string) @property)

[
  (number)
  (true)
  (false)
] @number

(null) @constant

(comment) @comment

[
  "{"
  "}"
  "["
  "]"
] @punctuation.bracket
