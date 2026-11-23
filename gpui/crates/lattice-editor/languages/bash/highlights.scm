; Shell scripts (bash, and zsh read as bash) in the code theme's names: a
; command a function, a variable a variable, an option a constant, strings
; strings, keywords keywords. Adapted from tree-sitter-bash (MIT).

[
  (string)
  (raw_string)
  (ansi_c_string)
  (heredoc_body)
  (heredoc_start)
  (heredoc_end)
] @string

(command_name) @function

(function_definition
  name: (word) @function)

(variable_name) @variable

(special_variable_name) @variable.special

((command
  argument: (word) @constant)
  (#match? @constant "^-"))

(file_descriptor) @number

(number) @number

(comment) @comment

[
  "case"
  "declare"
  "do"
  "done"
  "elif"
  "else"
  "esac"
  "export"
  "fi"
  "for"
  "function"
  "if"
  "in"
  "local"
  "readonly"
  "select"
  "then"
  "typeset"
  "unset"
  "unsetenv"
  "until"
  "while"
] @keyword

[
  "$"
  "&&"
  "||"
  ">"
  ">>"
  "<"
  "|"
  "="
  "+="
  "=="
  "!="
] @operator

[
  "("
  ")"
  "(("
  "))"
  "["
  "]"
  "[["
  "]]"
  "{"
  "}"
  "${"
] @punctuation.bracket
