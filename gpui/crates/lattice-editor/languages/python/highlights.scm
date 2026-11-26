; Python in the code theme's names, as the JavaScript query names its
; tokens: later patterns win, so the general ones come first. Adapted from
; tree-sitter-python's own query (MIT). The captures follow the scopes VS
; Code's Python grammar gives the same tokens (src/settings/syntax-scopes.ts
; maps both onto one role), so a .py file and a Markdown code block, which
; Shiki highlights with that grammar, look the same: a plain name stays in
; the ink, a builtin type is a type, `True` and `None` are constants.

((identifier) @type
  (#match? @type "^[A-Z]"))

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z_0-9]*$"))

; Parameters come after the capitalised-name patterns, so `def f(Model)`
; keeps its parameter a parameter, and before `self` and `cls`, which stay
; special.
(parameters
  (identifier) @variable)

(lambda_parameters
  (identifier) @variable)

(default_parameter
  name: (identifier) @variable)

(typed_parameter
  (identifier) @variable)

(typed_default_parameter
  name: (identifier) @variable)

(list_splat_pattern
  (identifier) @variable)

(dictionary_splat_pattern
  (identifier) @variable)

((identifier) @variable.special
  (#any-of? @variable.special "self" "cls"))

(attribute
  attribute: (identifier) @property)

(type
  (identifier) @type)

(call
  function: (identifier) @function)

((identifier) @type
  (#any-of? @type
    "bool" "bytearray" "bytes" "complex" "dict" "float" "frozenset" "int" "list"
    "memoryview" "object" "set" "str" "tuple" "type"))

((identifier) @variable.special
  (#match? @variable.special "^__[a-z_]+__$"))

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
] @number

[
  (true)
  (false)
  (none)
] @constant

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
