# Building and fixing errors

## Build

Press `⌘S` to save and build. By default Lattice also builds on its own after you stop typing; **Settings › Editor & builds › Automatic build** switches that to **Manual only**, which leaves building to `⌘S` and the build button. The build button above the editor shows the result and how long it took; click it to build again, or `⇧`-click it for a clean rebuild.

From the command palette (`⌘K`):

- **Clean rebuild** deletes the build's intermediate files and builds from scratch. Try it when a build behaves strangely.
- **Clean aux files** only deletes them.
- **Stop build** stops a build that is running.

The ⋯ menu above the editor has **Clean rebuild** and **Stop the build** too.

With a project open, **Settings › Editor & builds › Builds** also has the **Compile engine** (pdfLaTeX, XeLaTeX or LuaLaTeX; a `latexmkrc` in the project overrides it), **Allow shell escape**, and **Stop on first error** (off by default, so TeX carries on past errors as Overleaf does). A LuaLaTeX project gets **Live preview** too, which shows your edits in the PDF as you type; it downloads its engine the first time.

## The PDF beside your source

Each build updates the PDF panel in place. To move between the two:

- In the PDF, **double-click** a sentence to jump to the source line that wrote it.
- In the source, press `⌘⇧J` (**Jump to PDF**) to show the current line in the PDF.

![After a double-click in the PDF, the editor shows the paragraph that produced it](images/en-synctex.webp)

The PDF can also move to a window of its own for a second screen: **Open the PDF in a separate window** in the command palette (`⌘K`), and **Return the PDF to the workspace** to bring it back.

## Auxiliary files

A LaTeX build leaves `.aux`, `.log`, SyncTeX and other files beside your document. The Project panel hides them either way (**Show hidden files** shows them).

To keep them out of the folder itself, turn on **Settings › Editor & builds › Hide auxiliary files**. From the next build, they go to the project's hidden `.research/build` folder and only the PDF stays beside the document. A document that fails there, because a package ignores TeX's output directory (minted 2.x, for example), builds beside itself instead. **Auxiliary files › Clean** on the same page deletes them.

## Read and fix a build problem

When a build fails, the build button turns red and a card opens over the PDF. Each problem names its file and line, shows the source around it, and says in plain words what LaTeX objected to. The card can float over the page or dock below it.

![A failed build: Unknown command \undefinedmacro on line 22, with the source excerpt and Ask the agent](images/en-build-error.webp)

- Click a problem to jump to its line. `F8` and `⇧F8` step through problems from the keyboard.
- **Messages** lists the problems; **Log** shows the raw LaTeX log, with **Copy log**.
- **Ask the agent** hands one problem to the Agent; **Fix all** asks it to fix every problem.
- Some problems come with a fix of their own: **Install** for a missing package, **Switch to pdfLaTeX**, or **Update TeX packages**.

## Export

- **Export for arXiv…** (project menu or **File**) writes one ZIP that builds on its own.
- **Export ZIP** writes the whole project, which Overleaf can import.
- The PDF toolbar's menu has **Save PDF as…**, and **Export review PDF…**, which writes the PDF with the annotations you pick as ordinary PDF comments.
