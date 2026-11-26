# Typst documents

Lattice also writes and builds [Typst](https://typst.app) documents (`.typ` files). A `.typ` file opens with syntax highlighting straight away. The Typst compiler is a separate one-time download, so the app stays small for writers who never use Typst.

## Download Typst support

The first time you build a Typst document, or it starts compiling as you type, Lattice asks to download Typst support, a one-time download of about 22 MB. **Download** fetches it, checks its signature and runs the build. **Not now** leaves the file as it was, with highlighting only; typing doesn't ask again until you restart Lattice, and the next build asks again. After an app update, Lattice downloads the matching version on its own.

**Settings › Editor & builds › Typst support** shows whether it is installed, with **Download**, or **Download again** and **Remove**. Typst support is published for Macs with Apple silicon and for Intel Macs.

## Start one

- Choose **Blank Typst document** as the venue template when you create a project, or **New Typst file** in the Project panel's **+** menu.
- A folder you open that has a `main.typ` (or another `.typ` that includes the rest) and no LaTeX document builds that file.
- With no Typst main document set, saving a part that another `.typ` file includes builds the document that includes it, not the part alone.
- **In a LaTeX project**, a Typst document you build compiles on its own, and the LaTeX paper stays the document other builds compile.

## Write and build

- **Build and preview:** `⌘S`, the build button and automatic builds work as they do for LaTeX. Typst also compiles as you type: a moment after you stop, the PDF shows your edit, unsaved, and keeps its page and scroll position.
- **Errors:** the build button and the problems card show Typst's errors and warnings with their file and line. Click one to jump to it.
- **Source and PDF:** double-click a place in the PDF to put the cursor on that line of its source, even in a file you `#include`, or press `⌘⇧J` to mark the cursor's line in the PDF. Edits you haven't saved count too; build the document once first.
- **Fonts and packages:** your system fonts are available, plus the fonts Typst bundles (Libertinus Serif, New Computer Modern, DejaVu Sans Mono). `#import "@preview/…"` downloads the package from Typst Universe the first time; after that it works offline.

Overleaf sync and the Visual editor are LaTeX-only for now.
