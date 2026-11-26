//! Export review PDF placing a note made in a .tex file on the PDF that
//! lattice-core built from it, read through PDFium and SyncTeX as the sheet
//! reads it.

use std::{path::Path, process::Command};

use lattice_core::latex::{self, AuxFiles, BuildSteps};

use super::*;
use crate::{
    annotations::model,
    pdf::{engine::tests::engine, review_place::ReviewPlacement},
};

fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The e2e fixture as a project built by lattice-core with its build files
/// `layout`, in a scratch folder; `None` without latexmk.
fn built_fixture(layout: AuxFiles) -> Option<tempfile::TempDir> {
    built_fixture_with(layout, None)
}

/// The fixture built with `open` in the editor (`None`: main.tex), after
/// `open`'s own file is written with `text`.
fn built_fixture_with(layout: AuxFiles, open: Option<(&str, &str)>) -> Option<tempfile::TempDir> {
    Command::new("latexmk").arg("-v").output().ok()?;
    let scratch = tempfile::tempdir().unwrap();
    copy_folder(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/e2e"),
        scratch.path(),
    );
    lattice_core::project::open(scratch.path()).unwrap();
    if let Some((path, text)) = open {
        std::fs::write(scratch.path().join(path), text).unwrap();
    }
    let built = latex::build(
        scratch.path(),
        true,
        true,
        &Default::default(),
        open.map(|(path, _)| path),
        &BuildSteps::new(|_, _| {}),
        layout,
    )
    .unwrap();
    assert!(built.success, "{}", built.log);
    Some(scratch)
}

// A note on words of an \input file has no PDF anchor: SyncTeX finds its
// line on the PDF and the words are looked for there.
/// The note on the introduction's words, and `pdf` of `root` read as the
/// sheet reads the live PDF.
fn note_and_source(root: &Path, pdf: &str) -> (EditorComment, ReviewSource) {
    let engine = engine().clone();
    let doc = engine.new_document();
    let info =
        futures::executor::block_on(engine.open(doc, std::fs::read(root.join(pdf)).unwrap()))
            .unwrap()
            .unwrap();
    let path = "chapters/intro.tex";
    let text = std::fs::read_to_string(root.join(path)).unwrap();
    let words = "paragraph introduces the document";
    let start = text.find(words).unwrap();
    let author = model::Author {
        id: "me".into(),
        name: "Ada".into(),
    };
    let mut note = model::new_note(
        path,
        &text,
        start,
        start + words.len(),
        "Why?",
        None,
        &author,
    )
    .unwrap();
    note.number = 1;
    let source = ReviewSource {
        engine,
        doc,
        generation: info.generation,
        sizes: info.page_sizes.clone(),
        root: root.to_path_buf(),
        document: None,
        path: root.join(pdf),
        built: None,
        digest: String::new(),
    };
    (note, source)
}

// On another root's PDF the note has no place, and the sheet names the root
// that typesets it, to build instead.
#[test]
fn names_the_root_of_a_note_another_roots_pdf_leaves_unplaced() {
    let other = "\\documentclass{article}\n\\begin{document}\nAnother document.\n\\end{document}\n";
    let Some(scratch) = built_fixture_with(AuxFiles::Beside, Some(("other.tex", other))) else {
        eprintln!("latexmk is not installed; skipped");
        return;
    };
    let (note, source) = note_and_source(scratch.path(), "other.pdf");
    let placed = source.place(std::slice::from_ref(&note));
    assert_eq!(placed, vec![ReviewPlacement::Unplaced]);
    assert!(source.shows("other.tex") && source.is_live());
    assert_eq!(
        source.elsewhere(&[note], &placed),
        vec![("main.tex".to_string(), 1)]
    );
    source.engine.close(source.doc);
}

#[test]
fn places_a_note_on_words_of_an_input_file_as_a_highlight() {
    for layout in [AuxFiles::Beside, AuxFiles::Hidden] {
        let Some(scratch) = built_fixture(layout) else {
            eprintln!("latexmk is not installed; skipped");
            return;
        };
        let (note, source) = note_and_source(scratch.path(), "main.pdf");
        let placed = source.place(std::slice::from_ref(&note));
        source.engine.close(source.doc);
        assert!(
            matches!(
                placed.as_slice(),
                [ReviewPlacement::Highlight { page: 0, quads }] if quads.len() == 1
            ),
            "{layout:?}: {placed:?}"
        );
        assert!(source.elsewhere(&[note], &placed).is_empty());
    }
}
