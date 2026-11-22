// A line comment
/* A block comment */
#set page(paper: "a4", margin: 2cm)
#set text(font: "New Computer Modern", size: 11pt)
#show heading: it => block(above: 1em, it.body)
#let title = [Lattice and Typst]
#let double(x) = 2 * x

= Introduction <intro>
This is *strong* and _emphasised_ text, see @intro and https://typst.app.

- An item
+ A numbered item
/ Term: its description

The area is $A = pi r^2$, and in display:
$ sum_(k=1)^n k = (n (n + 1)) / 2 $

#if double(2) == 4 [four] else [not four]
#for x in (1, 2, 3) { x }
#figure(image("plot.png", width: 80%), caption: [A plot]) <fig>
#import "@preview/cetz:0.3.4": canvas
#include "chapter.typ"

Raw `inline` code, and a block:
```rust
fn main() {}
```
Done: #(true, none, auto), \#escaped.
