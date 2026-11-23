; A raw block names its language after the fence (````python`), as a
; Markdown fence does; the registry highlights the ones Lattice bundles.
(raw_blck
  lang: (ident) @injection.language
  (blob) @injection.content)
