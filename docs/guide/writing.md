# Writing

## The editor

A `.tex` file opens in the source editor, with completion and diagnostics as you type. Above it, switch between:

- **Source**: the LaTeX as written.
- **Visual**: the same file with headings, emphasis, equations, lists and figures drawn in place. You still edit the same text; switch back any time.

![main.tex in Visual mode, with the title and abstract drawn in place](images/en-visual.webp)

Useful while writing:

- **Format keys**: `⌘B` bold, `⌘I` emphasis, `⌘⇧M` inline math, `⌘/` comment a line, `⌘⌥E` wrap in an equation. The full list is in **Keyboard shortcuts** (`⌘?`).
- **Find and replace**: `⌘F` in the file, `⌘⇧F` in the whole project.
- **Go to**: `⌘P` opens any file, `⌘G` a line, `⌘⇧O` a section or label.
- **Outline**: the status bar's section name, or the marks at the right edge of the editor, take you through the document's structure.
- Markdown notes, spreadsheets, HTML pages, slides and [boards](boards.md) live in the same project and open in their own editors.
- **Code files**: in the native Lattice, Python, TypeScript, JavaScript, Rust, Go, C and C++, Java, R, Lua, shell scripts, YAML, TOML, JSON, Makefiles, Dockerfiles and CSS open with syntax highlighting. A fenced code block in a Markdown note is highlighted in the same languages and in the same colors as a file of that language, from the [code theme](plugins-and-themes.md#themes).
- **Markdown tables**: in the native Lattice, drag a column border in the visual editor to resize the column, or double-click it to fit the column to its content. Drag a row's bottom border to make the row taller, or double-click it to give the row its content's height again. Widths and heights are remembered for the next time you open the file, and the `.md` file itself stays plain Markdown.
- **Spreadsheets**: Control-click or right-click opens a cell's menu; `⌘`-click adds another range to the selection.

Lattice saves as you type. **Saved** in the status bar shows when.

## The PDF beside your source

Every save builds the PDF (see [Building](building.md)) and the PDF panel updates in place. To move between them:

- In the PDF, **double-click** a sentence to jump to the source line that wrote it.
- In the source, press `⌘⇧J` (**Jump to PDF**) to show the current line in the PDF.

![After a double-click in the PDF, the editor shows the paragraph that produced it](images/en-synctex.webp)

The PDF can also move to a window of its own for a second screen: **Open the PDF in a separate window** in the command palette (`⌘K`).
