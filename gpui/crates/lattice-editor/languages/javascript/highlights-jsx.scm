; JSX, appended to the JavaScript query for .js, .jsx and .tsx files: a tag
; a type and an attribute an attribute, as the HTML query names them.

(jsx_opening_element
  name: (identifier) @type)

(jsx_closing_element
  name: (identifier) @type)

(jsx_self_closing_element
  name: (identifier) @type)

(jsx_opening_element
  name: (member_expression) @type)

(jsx_attribute
  (property_identifier) @attribute)

(jsx_opening_element
  [
    "<"
    ">"
  ] @punctuation.bracket)

(jsx_closing_element
  [
    "</"
    ">"
  ] @punctuation.bracket)

(jsx_self_closing_element
  [
    "<"
    "/>"
  ] @punctuation.bracket)
