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

The native Lattice app also writes and builds [Typst](https://typst.app) documents (`.typ` files). A `.typ` file opens with syntax highlighting straight away. The Typst compiler is a separate one-time download, so the app stays small for writers who never use Typst.

- **Download Typst support:** the first time you build a Typst document, or it starts compiling as you type, Lattice asks to download Typst support, a one-time download of about 22 MB. **Download** fetches it, checks its signature and runs the build. **Not now** leaves the file as it was, with highlighting only; typing doesn't ask again until you restart Lattice, and the next build asks again. After an app update, Lattice downloads the matching version on its own. **Settings › Editor & builds › Typst support** downloads it, removes it, or downloads it again.

- **Start one:** choose **Blank Typst document** when you create a project, or **New Typst file** in the Project panel's `+` menu. A folder you open that has a `main.typ` (or another `.typ` that includes the rest) and no LaTeX document builds that file.
- **Build and preview:** `⌘S`, the build button and automatic builds work as they do for LaTeX. Typst also compiles as you type: a moment after you stop, the PDF shows your edit, unsaved, and keeps its page and scroll position.
- **Errors:** the build button and the problems panel show Typst's errors and warnings with their file and line. Click one to jump to it.
- **Source and PDF:** double-click a place in the PDF to put the cursor on that line of its source, even in a file you `#include`, or press `⌘⇧J` to mark the cursor's line in the PDF. Edits you haven't saved count too; build the document once first.
- **Fonts and packages:** your system fonts are available, plus the fonts Typst bundles (Libertinus Serif, New Computer Modern, DejaVu Sans Mono). `#import "@preview/…"` downloads the package from Typst Universe the first time; after that it works offline.
- **In a LaTeX project**, a `.typ` file you build compiles on its own, and the LaTeX paper stays the document other builds compile.

Overleaf sync and the Visual editor are LaTeX-only for now.

