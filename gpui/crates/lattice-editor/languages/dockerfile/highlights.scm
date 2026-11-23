; Dockerfiles in the code theme's names: an instruction a keyword, a
; variable a variable, a key a property, an image a type. Adapted from
; tree-sitter-containerfile's own query (MIT).

[
  "FROM"
  "AS"
  "RUN"
  "CMD"
  "LABEL"
  "EXPOSE"
  "ENV"
  "ADD"
  "COPY"
  "ENTRYPOINT"
  "VOLUME"
  "USER"
  "WORKDIR"
  "ARG"
  "ONBUILD"
  "STOPSIGNAL"
  "HEALTHCHECK"
  "SHELL"
  "MAINTAINER"
  "CROSS_BUILD"
] @keyword

(comment) @comment

(image_spec
  (image_name) @type)

[
  (double_quoted_string)
  (single_quoted_string)
  (json_string)
  (heredoc_block)
] @string

[
  (heredoc_marker)
  (heredoc_end)
] @label

(expansion
  [
    "$"
    "{"
    "}"
  ] @operator)

(variable) @variable

(arg_pair
  name: (unquoted_string) @property)

(env_pair
  name: (unquoted_string) @property)

(label_pair
  key: (_) @property)

(param
  name: (_) @property)

(expose_port) @number

[
  ":"
  "@"
] @operator
