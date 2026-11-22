use std::collections::HashMap;

use super::*;

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
