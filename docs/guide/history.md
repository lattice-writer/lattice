# History and Compare versions

Lattice keeps a history of every change to your project, whoever made it: you, the Agent, or a co-author on Overleaf. Open it with the **Project history** button in the title bar (the clock), or **Open project history** in the command palette (`⌘K`).

## Undo a change

History has three views, four for a project linked to Overleaf:

- **Everything** lists every change newest first: your edits, the Agent's, saved versions and Overleaf updates.
- **Activity** lists Lattice's own edits and the Agent's turns, filtered by **All**, **You**, **Agent** or **Citations**. An Agent turn can be undone as a whole: **Undo this Agent turn's file changes**.
- **Versions** lists the versions you saved (below).
- **Overleaf** lists the updates from Overleaf, for a linked project.

Click a change to see its diff, then **Restore this file** to put the file back as it was before that change. A restore is itself a new entry, so it can be undone too.

![History: an edit to main.tex opened to its diff, with Restore this file](images/en-history.webp)

## Save versions

Named versions use Git. The first time, **Versions** offers **Enable version tracking**, which makes the project a Git repository (an existing repository is used as it is). Then:

- **Save version** records the project now, with a name you give it.
- Each version has **Compare with now** and **Restore project to this version**.

## Compare versions

Choose **Compare…** in the History toolbar, then pick a **From** and a **To** version (**Now** is the project as it is on disk). Lattice lists every changed file. **Prose** shows changed words, which is easier to read for text; **Source** shows the line diff.

**Export PDF with changes…** writes a PDF with the differences marked, through `latexdiff` from your TeX installation. With BasicTeX you may need `sudo tlmgr install latexdiff` first.

![Compare versions: from Initialize version tracking to Now, with one changed word in main.tex](images/en-compare.webp)

## Track changes

The **Track changes** button in the title bar turns on review mode: your edits from then on are marked in the source as you type (the button then reads **Stop tracking changes**). The review bar steps through them with **Previous change** and **Next change**, and has **Accept**, **Reject**, **Accept all**, **Reject all** and **Show changes in the PDF**. It works without Overleaf.
