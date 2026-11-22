# Building and fixing errors

## Build

Press `⌘S` to save and build. By default Lattice also builds on its own after you stop typing (**Settings › Editor & builds › Automatic build**). The build button above the editor shows the result and how long it took; click it to build again.

From the command palette (`⌘K`):

- **Clean rebuild** deletes the build's intermediate files and builds from scratch. Try it when a build behaves strangely.
- **Clean aux files** only deletes them.
- **Stop build** stops a build that is running.

The compile engine (pdfLaTeX, XeLaTeX or LuaLaTeX), shell escape and where intermediate files go are in **Settings › Editor & builds › Builds**. A `latexmkrc` in the project overrides the engine.

## Read and fix a build problem

When a build fails, the build button turns red and a panel opens over the PDF. Each problem names its file and line, shows the source around it, and says in plain words what LaTeX objected to.

![A failed build: Unknown command \undefinedmacro on line 22, with the source excerpt and Ask the agent](images/en-build-error.webp)

- Click a problem to jump to its line. `F8` and `⇧F8` step through problems from the keyboard.
- **Log** shows the raw LaTeX log.
- **Ask the agent** hands one problem to the Agent; **Fix all** asks it to fix every problem.
- A missing package can be installed from the problem itself.

## Export

- **Export for arXiv…** (project menu or **File**) writes one ZIP that builds on its own.
- **Export ZIP…** writes the whole project, which Overleaf can import.
- The PDF panel's menu saves the compiled PDF.

## Typst documents

The native Lattice app also writes and builds [Typst](https://typst.app) documents (`.typ` files). Nothing needs installing: the Typst compiler is part of the app.

- **Start one:** choose **Blank Typst document** when you create a project, or **New Typst file** in the Project panel's `+` menu. A folder you open that has a `main.typ` (or another `.typ` that includes the rest) and no LaTeX document builds that file.
- **Build and preview:** `⌘S`, the build button and automatic builds work as they do for LaTeX. Typst also compiles as you type: a moment after you stop, the PDF shows your edit, unsaved, and keeps its page and scroll position.
- **Errors:** the build button and the problems panel show Typst's errors and warnings with their file and line. Click one to jump to it.
- **Fonts and packages:** your system fonts are available, plus the fonts Typst bundles (Libertinus Serif, New Computer Modern, DejaVu Sans Mono). `#import "@preview/…"` downloads the package from Typst Universe the first time; after that it works offline.
- **In a LaTeX project**, a `.typ` file you build compiles on its own, and the LaTeX paper stays the document other builds compile.

Clicking between the source and the PDF (SyncTeX), Overleaf sync and the Visual editor are LaTeX-only for now.

