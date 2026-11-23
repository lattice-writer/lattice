//! Typst's diagnostics as Lattice's: the file and line a click in the error
//! list jumps to, the message with its hints, and a plain log for the Log tab.
//!
//! A diagnostic inside a package (a template's own code) points nowhere the
//! writer can edit, so it takes the place in the project where the package
//! was called from: the innermost call in its trace that is in the project.

use lattice_core::models::Diagnostic;
use typst::{World as _, WorldExt as _, diag::SourceDiagnostic, syntax::DiagSpan};

use crate::world::{Session, project_path};

pub(crate) fn convert(session: &Session, diagnostic: &SourceDiagnostic) -> Diagnostic {
    let place = locate(session, diagnostic.span).or_else(|| {
        diagnostic
            .trace
            .iter()
            .find_map(|point| locate(session, DiagSpan::from_span(point.span, None)))
    });
    let mut message = diagnostic.message.to_string();
    for hint in &diagnostic.hints {
        message.push_str("\nhint: ");
        message.push_str(&hint.v);
    }
    let level = match diagnostic.severity {
        typst::diag::Severity::Error => "error",
        typst::diag::Severity::Warning => "warning",
    };
    let (file, line, column, end_line, end_column) = match place {
        Some(place) => (
            Some(place.file),
            Some(place.line),
            Some(place.column),
            Some(place.end_line),
            Some(place.end_column),
        ),
        None => (None, None, None, None, None),
    };
    Diagnostic {
        file,
        line,
        column,
        end_line,
        end_column,
        level: level.into(),
        message,
        code: None,
        params: Default::default(),
    }
}

/// A diagnostic of Lattice's own, at no place in the sources.
pub(crate) fn note(level: &str, message: String) -> Diagnostic {
    Diagnostic {
        file: None,
        line: None,
        column: None,
        end_line: None,
        end_column: None,
        level: level.into(),
        message,
        code: None,
        params: Default::default(),
    }
}

struct Place {
    file: String,
    /// From 1, as TeX's log and the error list count lines.
    line: u32,
    column: u32,
    end_line: u32,
    end_column: u32,
}

fn locate(session: &Session, span: DiagSpan) -> Option<Place> {
    let id = span.id()?;
    let file = project_path(id)?;
    let range = session.range(span)?;
    let source = session.source(id).ok()?;
    let lines = source.lines();
    let (line, column) = lines.byte_to_line_column(range.start)?;
    let (end_line, end_column) = lines
        .byte_to_line_column(range.end)
        .unwrap_or((line, column));
    let number = |value: usize| u32::try_from(value + 1).unwrap_or(u32::MAX);
    Some(Place {
        file,
        line: number(line),
        column: number(column),
        end_line: number(end_line),
        end_column: number(end_column),
    })
}

/// `main.typ:3:5: error: unknown variable: x`, one per diagnostic, as the
/// `typst` CLI's short format writes them.
pub(crate) fn log(diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let place = match (&diagnostic.file, diagnostic.line, diagnostic.column) {
                (Some(file), Some(line), Some(column)) => format!("{file}:{line}:{column}: "),
                (Some(file), _, _) => format!("{file}: "),
                _ => String::new(),
            };
            format!("{place}{}: {}\n", diagnostic.level, diagnostic.message)
        })
        .collect()
}
