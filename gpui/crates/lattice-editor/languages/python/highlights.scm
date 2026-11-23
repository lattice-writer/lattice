; Python in the code theme's names, as the JavaScript query names its
; tokens: later patterns win, so the general ones come first. Adapted from
; tree-sitter-python's own query (MIT).

(identifier) @variable

((identifier) @type
  (#match? @type "^[A-Z]"))

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z_0-9]*$"))

((identifier) @variable.special
  (#any-of? @variable.special "self" "cls"))

(attribute
  attribute: (identifier) @property)

(type
  (identifier) @type)

(call
  function: (identifier) @function)

(call
  function: (attribute
    attribute: (identifier) @function))

(function_definition
  name: (identifier) @function)

(class_definition
  name: (identifier) @type)

(decorator) @attribute

(decorator
  (identifier) @attribute)

(keyword_argument
  name: (identifier) @property)

[
  (integer)
  (float)
  (true)
  (false)
] @number

(none) @constant

(string) @string

(interpolation
  "{" @punctuation.bracket
  (_) @variable
  "}" @punctuation.bracket)

(comment) @comment

(expression_statement
  (string) @comment.doc)

[
  "-"
  "-="
  "!="
  "*"
  "**"
  "**="
  "*="
  "/"
  "//"
  "//="
  "/="
  "&"
  "&="
  "%"
  "%="
  "^"
  "^="
  "+"
  "->"
  "+="
  "<"
  "<<"
  "<<="
  "<="
  "<>"
  "="
  ":="
  "=="
  ">"
  ">="
  ">>"
  ">>="
  "|"
  "|="
  "~"
  "@="
] @operator

[
  "and"
  "as"
  "assert"
  "async"
  "await"
  "break"
  "case"
  "class"
  "continue"
  "def"
  "del"
  "elif"
  "else"
  "except"
  "exec"
  "finally"
  "for"
  "from"
  "global"
  "if"
  "import"
  "in"
  "is"
  "lambda"
  "match"
  "nonlocal"
  "not"
  "or"
  "pass"
  "print"
  "raise"
  "return"
  "try"
  "type"
  "while"
  "with"
  "yield"
] @keyword

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket
