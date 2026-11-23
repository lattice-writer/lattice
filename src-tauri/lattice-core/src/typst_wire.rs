//! What the native app and its Typst helper say to each other.
//!
//! The Typst compiler, its library's data and its bundled fonts weigh about
//! 40 MB, so the native app downloads them on first use instead of shipping
//! them: `lattice-typst`, a helper the app starts once and keeps running
//! (`gpui/crates/lattice-typst`). It keeps its compiler session between
//! builds, so a live compile after a keystroke re-lays out only what changed.
//! The two speak JSON, one object per line: the helper says [`Hello`] when it
//! starts, then answers each [`Request`] on stdin with a [`Reply`] on stdout.
//! The helper ships with the app's version and is replaced with it, so
//! [`PROTOCOL`] only guards a helper from another build in development.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::latex::{BuildResult, BuildStep};
use crate::models::Diagnostic;

/// The wire format's version; a helper that says another one is not used.
pub const PROTOCOL: u32 = 1;

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

/// Compile `document` (relative to `root`), reading `overlay`'s text
/// (project-relative path → the editor's buffer) in place of those files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Request {
    pub root: PathBuf,
    pub document: String,
    #[serde(default)]
    pub overlay: HashMap<String, String>,
}

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
