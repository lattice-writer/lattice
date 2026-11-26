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
    Command::new("latexmk").arg("-v").output().ok()?;
    let scratch = tempfile::tempdir().unwrap();
    copy_folder(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/e2e"),
        scratch.path(),
    );
    lattice_core::project::open(scratch.path()).unwrap();
    let built = latex::build(
        scratch.path(),
        true,
        true,
        &Default::default(),
        None,
        &BuildSteps::new(|_, _| {}),
        layout,
    )
    .unwrap();
    assert!(built.success, "{}", built.log);
    Some(scratch)
}

// A note on words of an \input file has no PDF anchor: SyncTeX finds its
// line on the PDF and the words are looked for there.
#[test]
fn places_a_note_on_words_of_an_input_file_as_a_highlight() {
    for layout in [AuxFiles::Beside, AuxFiles::Hidden] {
        let Some(scratch) = built_fixture(layout) else {
            eprintln!("latexmk is not installed; skipped");
            return;
        };
        let root = scratch.path().to_path_buf();
        let engine = engine().clone();
        let doc = engine.new_document();
        let info = futures::executor::block_on(
            engine.open(doc, std::fs::read(root.join("main.pdf")).unwrap()),
        )
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
            engine: engine.clone(),
            doc,
            generation: info.generation,
            sizes: info.page_sizes.clone(),
            root: root.clone(),
            document: None,
            path: root.join("main.pdf"),
            built: None,
            digest: String::new(),
        };
        let placed = source.place(&[note]);
        engine.close(doc);
        assert!(
            matches!(
                placed.as_slice(),
                [ReviewPlacement::Highlight { page: 0, quads }] if quads.len() == 1
            ),
            "{layout:?}: {placed:?}"
        );
    }
}
