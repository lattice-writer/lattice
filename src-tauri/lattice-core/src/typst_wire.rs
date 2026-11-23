//! What the native app and its Typst helper say to each other.
//!
//! The Typst compiler, its library's data and its bundled fonts weigh about
//! 40 MB, so the native app downloads them on first use instead of shipping
//! them: `lattice-typst`, a helper the app starts once and keeps running
//! (`gpui/crates/lattice-typst`). It keeps its compiler session between
//! builds, so a live compile after a keystroke re-lays out only what changed.
//! The two speak JSON, one object per line: the helper says [`Hello`] when it
//! starts, then answers each [`Request`] on stdin with one line on stdout: a
//! [`Reply`] to a compile, a [`Jump`] to a jump between source and PDF.
//! The helper ships with the app's version and is replaced with it, so
//! [`PROTOCOL`] guards a helper from another build in development, and one
//! left in this version's folder by a build before the jumps (protocol 1):
//! the app takes it as outdated and downloads its own again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::latex::{BuildResult, BuildStep, PdfSyncTarget};
use crate::models::Diagnostic;

/// The wire format's version; a helper that says another one is not used.
pub const PROTOCOL: u32 = 2;

/// The helper's first line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    pub version: String,
}

impl Hello {
    pub fn current() -> Self {
        Self { protocol: PROTOCOL, version: crate::VERSION.to_string() }
    }
}

/// What the app asks of the helper, one per line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Request {
    Compile(Compile),
    /// From a point in the PDF to its source: answered with a
    /// [`Jump`] of a [`SourceSpot`].
    JumpFromClick(ClickJump),
    /// From a caret in a source to the PDF: answered with a [`Jump`] of
    /// every place the text there was laid out, first to last page.
    JumpFromCursor(CursorJump),
}

/// Compile `document` (relative to `root`), reading `overlay`'s text
/// (project-relative path → the editor's buffer) in place of those files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Compile {
    pub root: PathBuf,
    pub document: String,
    #[serde(default)]
    pub overlay: HashMap<String, String>,
}

/// A click at (`x`, `y`) PDF points from the top-left corner of the
/// 1-based `page` of `document`'s PDF. The jumps look in the last document
/// the helper compiled successfully, which may be older than the text now
/// on screen: `overlay` is the editor's unsaved text, as a compile takes it,
/// and the place found in the compiled text is carried onto it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClickJump {
    pub root: PathBuf,
    pub document: String,
    #[serde(default)]
    pub overlay: HashMap<String, String>,
    pub page: u32,
    pub x: f64,
    pub y: f64,
}

/// The caret at the 1-based `line` and `column` (in characters) of `path`
/// (project-relative), as the editor shows it now (`overlay`, as in
/// [`ClickJump`]).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CursorJump {
    pub root: PathBuf,
    pub document: String,
    #[serde(default)]
    pub overlay: HashMap<String, String>,
    pub path: String,
    pub line: u32,
    pub column: u32,
}

/// A place in a project file: the 1-based `line` and `column` (in
/// characters), as the editor counts them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpot {
    pub path: String,
    pub line: u32,
    pub column: u32,
}

/// A jump's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Jump<T> {
    To(T),
    /// The point or the caret has no counterpart: a margin, a line of code,
    /// text in a package, a file the document does not read.
    Nowhere,
    /// The helper holds no compiled document of `root` and `document`: it
    /// started since the PDF was written, or compiled another one since.
    NotCompiled,
}

/// Where the source at the caret is in the PDF.
pub type PdfJump = Jump<Vec<PdfSyncTarget>>;
/// Where the point clicked in the PDF comes from.
pub type SourceJump = Jump<SourceSpot>;

/// One diagnostic as the helper reports it: a [`Diagnostic`] without the
/// catalog code and parameters, which Typst's messages never carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireDiagnostic {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
    pub level: String,
    pub message: String,
}

/// A compile's outcome, which [`Reply::into_build`] turns back into the
/// [`BuildResult`] every build hands the window.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub success: bool,
    pub has_pdf: bool,
    pub log: String,
    pub duration_ms: u64,
    pub pages: Option<u32>,
    pub outputs: Vec<String>,
    pub diagnostics: Vec<WireDiagnostic>,
}

/// The tool name the build card shows for the one pass.
pub const TOOL: &str = "typst";

impl Reply {
    /// `result`, as the helper sends it.
    pub fn from_build(result: &BuildResult) -> Self {
        Self {
            success: result.success,
            has_pdf: result.has_pdf,
            log: result.log.clone(),
            duration_ms: u64::try_from(result.duration_ms).unwrap_or(u64::MAX),
            pages: result.pages,
            outputs: result.outputs.clone(),
            diagnostics: result
                .diagnostics
                .iter()
                .map(|diagnostic| WireDiagnostic {
                    file: diagnostic.file.clone(),
                    line: diagnostic.line,
                    column: diagnostic.column,
                    end_line: diagnostic.end_line,
                    end_column: diagnostic.end_column,
                    level: diagnostic.level.clone(),
                    message: diagnostic.message.clone(),
                })
                .collect(),
        }
    }

    /// A build that failed before Typst ran (the helper could not start, or
    /// stopped), with `message` as its one error.
    pub fn failed(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            log: message.clone(),
            diagnostics: vec![WireDiagnostic {
                file: None,
                line: None,
                column: None,
                end_line: None,
                end_column: None,
                level: "error".into(),
                message,
            }],
            ..Self::default()
        }
    }

    /// The build `document` (relative to `root`) made, as a [`BuildResult`].
    pub fn into_build(self, root: &Path, document: &str, one_off: bool) -> BuildResult {
        BuildResult {
            success: self.success,
            has_pdf: self.has_pdf || root.join(document).with_extension("pdf").is_file(),
            log: self.log,
            duration_ms: u128::from(self.duration_ms),
            diagnostics: self
                .diagnostics
                .into_iter()
                .map(|diagnostic| Diagnostic {
                    file: diagnostic.file,
                    line: diagnostic.line,
                    column: diagnostic.column,
                    end_line: diagnostic.end_line,
                    end_column: diagnostic.end_column,
                    level: diagnostic.level,
                    message: diagnostic.message,
                    code: None,
                    params: Default::default(),
                })
                .collect(),
            root_document: document.to_string(),
            one_off,
            venue: None,
            steps: vec![BuildStep {
                tool: TOOL.into(),
                run: 1,
                draft: false,
                started_ms: 0,
                duration_ms: Some(self.duration_ms),
            }],
            pages: self.pages,
            // Typst writes no SyncTeX map, and the card has no row to show for it.
            synctex: None,
            outputs: self.outputs,
            unchanged: false,
            aux_folder: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_crosses_the_wire_and_comes_back_as_the_build_it_was() {
        let reply = Reply {
            success: false,
            has_pdf: false,
            log: "error: unknown variable".into(),
            duration_ms: 26,
            pages: None,
            outputs: Vec::new(),
            diagnostics: vec![WireDiagnostic {
                file: Some("notes.typ".into()),
                line: Some(4),
                column: Some(2),
                end_line: None,
                end_column: None,
                level: "error".into(),
                message: "unknown variable: nonexistent-call".into(),
            }],
        };
        let line = serde_json::to_string(&reply).unwrap();
        assert!(!line.contains('\n'));
        let back: Reply = serde_json::from_str(&line).unwrap();
        let result = back.into_build(Path::new("/nonexistent"), "notes.typ", true);
        assert!(!result.success && result.one_off);
        assert_eq!(result.root_document, "notes.typ");
        assert_eq!(result.steps[0].tool, TOOL);
        assert_eq!(result.diagnostics[0].file.as_deref(), Some("notes.typ"));
        assert_eq!(result.diagnostics[0].line, Some(4));
        assert_eq!(Reply::from_build(&result).diagnostics, reply.diagnostics);
    }

    #[test]
    fn requests_and_jumps_cross_the_wire_tagged_by_what_they_ask() {
        let click = Request::JumpFromClick(ClickJump {
            root: "/p".into(),
            document: "main.typ".into(),
            overlay: HashMap::from([("one.typ".to_string(), "Text.".to_string())]),
            page: 2,
            x: 72.5,
            y: 100.0,
        });
        let line = serde_json::to_string(&click).unwrap();
        assert!(line.starts_with(r#"{"jumpFromClick":{"#), "{line}");
        let Request::JumpFromClick(back) = serde_json::from_str(&line).unwrap() else {
            panic!("{line}");
        };
        assert_eq!((back.page, back.x, back.y), (2, 72.5, 100.0));
        assert_eq!(back.overlay["one.typ"], "Text.");

        let compile: Request =
            serde_json::from_str(r#"{"compile":{"root":"/p","document":"main.typ"}}"#).unwrap();
        assert!(matches!(compile, Request::Compile(Compile { overlay, .. }) if overlay.is_empty()));
        // A request of protocol 1 (the bare compile) is not one of these.
        assert!(serde_json::from_str::<Request>(r#"{"root":"/p","document":"main.typ"}"#).is_err());

        let jump =
            SourceJump::To(SourceSpot { path: "chapters/one.typ".into(), line: 3, column: 5 });
        let line = serde_json::to_string(&jump).unwrap();
        assert_eq!(serde_json::from_str::<SourceJump>(&line).unwrap(), jump);
        let places =
            PdfJump::To(vec![PdfSyncTarget { page: 1, x: 1.0, y: 2.0, width: 3.0, height: 4.0 }]);
        let line = serde_json::to_string(&places).unwrap();
        assert!(line.contains(r#""page":1"#), "{line}");
        assert!(
            matches!(serde_json::from_str::<PdfJump>(&line).unwrap(), Jump::To(places) if places.len() == 1)
        );
        for nothing in [SourceJump::Nowhere, SourceJump::NotCompiled] {
            let line = serde_json::to_string(&nothing).unwrap();
            assert_eq!(serde_json::from_str::<SourceJump>(&line).unwrap(), nothing);
        }
    }

    #[test]
    fn a_failed_reply_is_one_error_and_no_pdf() {
        let result = Reply::failed("The Typst helper stopped").into_build(
            Path::new("/nonexistent"),
            "a.typ",
            false,
        );
        assert!(!result.success && !result.has_pdf);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].level, "error");
    }
}
