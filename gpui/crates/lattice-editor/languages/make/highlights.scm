; Makefiles in the code theme's names: a target a function, a variable a
; variable, directives and functions keywords, an automatic variable special.
; Adapted from tree-sitter-make's own query (MIT).

[
  (text)
  (string)
  (raw_text)
] @string

(targets
  (word) @function)

(variable_assignment
  name: (word) @variable)

(variable_reference
  (word) @variable)

(automatic_variable) @variable.special

(comment) @comment

[
  "ifeq"
  "ifneq"
  "ifdef"
  "ifndef"
  "else"
  "endif"
  "define"
  "endef"
  "vpath"
  "undefine"
  "export"
  "unexport"
  "override"
  "private"
  "include"
  "sinclude"
  "-include"
  "foreach"
  "subst"
  "patsubst"
  "strip"
  "findstring"
  "filter"
  "filter-out"
  "sort"
  "word"
  "words"
  "wordlist"
  "firstword"
  "lastword"
  "dir"
  "notdir"
  "suffix"
  "basename"
  "addsuffix"
  "addprefix"
  "join"
  "wildcard"
  "realpath"
  "abspath"
  "call"
  "eval"
  "file"
  "value"
  "shell"
  "error"
  "warning"
  "info"
] @keyword

[
  "="
  ":="
  "::="
  "?="
  "+="
  "!="
  "$"
  "$$"
] @operator

[
  "("
  ")"
  "{"
  "}"
] @punctuation.bracket
