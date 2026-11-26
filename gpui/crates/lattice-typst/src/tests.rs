use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use lattice_core::typst_wire::{ClickJump, CursorJump, SourceSpot};

use super::*;

/// The compiler session is one per process: tests that build, then read
/// what the build kept, take turns.
fn serial() -> MutexGuard<'static, ()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

const PAPER: &str = "#set page(width: 10cm, height: 8cm)\n= Hello\n\nSome text, $x^2$.\n#pagebreak()\nSecond page.\n";

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a scratch project");
    for (path, text) in files {
        let path = root.path().join(path);
        std::fs::create_dir_all(path.parent().expect("inside the project")).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

#[test]
fn a_document_builds_its_pdf_beside_it() {
    let _serial = serial();
    let root = project(&[("main.typ", PAPER)]);
    let result = build(root.path(), "main.typ", &HashMap::new());
    assert!(result.success, "{:?}", result.diagnostics);
    assert!(result.has_pdf);
    assert_eq!(result.pages, Some(2));
    assert_eq!(result.outputs, vec!["main.pdf".to_string()]);
    let pdf = std::fs::read(root.path().join("main.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(result.steps.len(), 1);
    assert_eq!(result.steps[0].tool, TOOL);
}

#[test]
fn an_error_points_at_its_file_and_line_and_keeps_the_last_pdf() {
    let _serial = serial();
    let root = project(&[
        ("main.typ", "= Title\n#include \"chapters/one.typ\"\n"),
        ("chapters/one.typ", "Fine.\n\nStill fine.\n#unknown-thing\n"),
    ]);
    std::fs::write(root.path().join("main.pdf"), b"%PDF-old").unwrap();
    let result = build(root.path(), "main.typ", &HashMap::new());
    assert!(!result.success);
    assert!(result.has_pdf, "the last PDF stays on screen");
    assert_eq!(
        std::fs::read(root.path().join("main.pdf")).unwrap(),
        b"%PDF-old"
    );
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.level == "error")
        .expect("an error");
    assert_eq!(error.file.as_deref(), Some("chapters/one.typ"));
    assert_eq!(error.line, Some(4));
    assert_eq!(error.column, Some(2));
    assert!(error.message.contains("unknown"), "{}", error.message);
    assert!(
        result.log.contains("chapters/one.typ:4:2: error:"),
        "{}",
        result.log
    );
}

#[test]
fn the_overlay_compiles_unsaved_text() {
    let _serial = serial();
    let root = project(&[("main.typ", "#broken(\n")]);
    let overlay = HashMap::from([("main.typ".to_string(), PAPER.to_string())]);
    let result = build(root.path(), "main.typ", &overlay);
    assert!(result.success, "{:?}", result.diagnostics);
    // Back on disk's text, the same session reports the error again.
    let result = build(root.path(), "main.typ", &HashMap::new());
    assert!(!result.success);
    assert_eq!(result.diagnostics[0].line, Some(1));
}

#[test]
fn a_rebuild_after_a_small_edit_reuses_the_session() {
    let _serial = serial();
    let body: String = (0..40)
        .map(|i| {
            format!(
                "== Section {i}\n{}\n\n",
                "Lorem ipsum dolor sit amet. ".repeat(30)
            )
        })
        .collect();
    let root = project(&[("main.typ", &body)]);
    let first = build(root.path(), "main.typ", &HashMap::new());
    assert!(first.success, "{:?}", first.diagnostics);
    let edited = HashMap::from([("main.typ".to_string(), body.replacen("Lorem", "Lorum", 1))]);
    let second = build(root.path(), "main.typ", &edited);
    assert!(second.success);
    eprintln!(
        "typst: first build {} ms, incremental {} ms, {} pages",
        first.duration_ms,
        second.duration_ms,
        second.pages.unwrap_or(0)
    );
    assert!(second.duration_ms <= first.duration_ms);
}

/// A book of a main file and a chapter it includes, the chapter's second
/// line on the second page.
const BOOK: &str = "#set page(width: 10cm, height: 8cm)\n= Book\nThe opening words.\n#pagebreak()\n#include \"chapters/one.typ\"\n";
const CHAPTER: &str = "== One\nThe chapter's own sentence sits here.\nAnd a last line.\n";

fn book() -> tempfile::TempDir {
    project(&[("main.typ", BOOK), ("chapters/one.typ", CHAPTER)])
}

fn cursor(
    root: &Path,
    path: &str,
    line: u32,
    column: u32,
    overlay: &HashMap<String, String>,
) -> PdfJump {
    jump_from_cursor(&CursorJump {
        root: root.to_path_buf(),
        document: "main.typ".into(),
        overlay: overlay.clone(),
        path: path.into(),
        line,
        column,
    })
}

fn click(
    root: &Path,
    page: u32,
    (x, y): (f64, f64),
    overlay: &HashMap<String, String>,
) -> SourceJump {
    jump_from_click(&ClickJump {
        root: root.to_path_buf(),
        document: "main.typ".into(),
        overlay: overlay.clone(),
        page,
        x,
        y,
    })
}

/// The middle of a jump's first place: where a double-click on its mark lands.
fn center(jump: &PdfJump) -> (u32, (f64, f64)) {
    let Jump::To(places) = jump else {
        panic!("no place: {jump:?}");
    };
    let place = &places[0];
    (
        place.page,
        (place.x + place.width / 2.0, place.y + place.height / 2.0),
    )
}

#[test]
fn a_caret_in_an_included_file_finds_its_line_in_the_pdf_and_a_click_there_comes_back() {
    let _serial = serial();
    let root = book();
    let none = HashMap::new();
    assert!(build(root.path(), "main.typ", &none).success);

    let places = cursor(root.path(), "chapters/one.typ", 2, 10, &none);
    let (page, (x, y)) = center(&places);
    assert_eq!(page, 2, "{places:?}");
    let Jump::To(found) = &places else {
        unreachable!()
    };
    let mark = &found[0];
    // The line's box: below the heading, as wide as the sentence, a line high.
    assert!(
        mark.y > 20.0 && mark.width > 100.0 && (5.0..30.0).contains(&mark.height),
        "{mark:?}"
    );

    let back = click(root.path(), page, (x, y), &none);
    assert!(
        matches!(&back, Jump::To(SourceSpot { path, line: 2, column }) if path == "chapters/one.typ" && (1..=40).contains(column)),
        "{back:?}"
    );
    // The main file's own text goes to the main file.
    let (page, point) = center(&cursor(root.path(), "main.typ", 3, 5, &none));
    assert_eq!(page, 1);
    assert!(
        matches!(click(root.path(), page, point, &none), Jump::To(SourceSpot { ref path, line: 3, .. }) if path == "main.typ")
    );
    // A margin goes to the nearest text, as SyncTeX takes the nearest line.
    assert!(matches!(
        click(root.path(), 2, (2.0, y), &none),
        Jump::To(SourceSpot { line: 2, .. })
    ));
}

#[test]
fn jumps_carry_onto_text_typed_since_the_compile_and_survive_a_failed_one() {
    let _serial = serial();
    let root = book();
    let none = HashMap::new();
    assert!(build(root.path(), "main.typ", &none).success);
    let (page, point) = center(&cursor(root.path(), "chapters/one.typ", 2, 10, &none));

    // Two lines typed at the top of the chapter, not compiled yet: the same
    // point in the PDF is two lines further down in the text on screen.
    let typed = HashMap::from([(
        "chapters/one.typ".to_string(),
        format!("New line.\nAnother.\n{CHAPTER}"),
    )]);
    assert!(matches!(
        click(root.path(), page, point, &typed),
        Jump::To(SourceSpot { line: 4, .. })
    ));
    // And the caret on that line now finds the same place as before.
    assert_eq!(
        center(&cursor(root.path(), "chapters/one.typ", 4, 10, &typed)),
        (page, point)
    );

    // A compile that fails keeps the PDF and the document it was made of.
    let broken = HashMap::from([(
        "chapters/one.typ".to_string(),
        format!("#broken(\n{CHAPTER}"),
    )]);
    assert!(!build(root.path(), "main.typ", &broken).success);
    assert!(matches!(
        click(root.path(), page, point, &none),
        Jump::To(SourceSpot { line: 2, .. })
    ));

    // Unsaved text that compiles is what the jumps then read.
    assert!(build(root.path(), "main.typ", &typed).success);
    assert_eq!(
        center(&cursor(root.path(), "chapters/one.typ", 4, 10, &typed)).0,
        page
    );
    assert!(matches!(
        click(root.path(), page, point, &typed),
        Jump::To(SourceSpot { line: 4, .. })
    ));
}

#[test]
fn a_jump_before_any_compile_of_that_document_says_so() {
    let _serial = serial();
    let root = book();
    let none = HashMap::new();
    let fresh = project(&[("main.typ", PAPER)]);
    // Only another project was compiled.
    assert!(build(fresh.path(), "main.typ", &none).success);
    assert_eq!(
        click(root.path(), 1, (10.0, 10.0), &none),
        Jump::NotCompiled
    );
    assert_eq!(
        cursor(root.path(), "main.typ", 2, 1, &none),
        Jump::NotCompiled
    );
    // A file the document does not read, and a page it does not have.
    assert!(build(root.path(), "main.typ", &none).success);
    std::fs::write(root.path().join("loose.typ"), "Loose.\n").unwrap();
    assert_eq!(cursor(root.path(), "loose.typ", 1, 1, &none), Jump::Nowhere);
    assert_eq!(click(root.path(), 9, (10.0, 10.0), &none), Jump::Nowhere);
}
