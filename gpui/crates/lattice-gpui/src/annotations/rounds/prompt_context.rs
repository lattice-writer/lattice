//! What a revision's prompt reads beyond the annotation itself: the
//! bibliography an "Add a citation" prompt offers, and the rest of the paper
//! around the passage's file (`annotation_revision::paper`).

use std::{collections::HashMap, ops::Range, path::Path};

use gpui::{App, WeakEntity};
use lattice_core::project;
use lattice_editor::annotation_revision as revision;

use crate::editor_panel::EditorPanel;

/// The project's citation keys and titles, for the "Add a citation"
/// intent's prompt.
pub(super) fn citations(root: &Path) -> Vec<revision::Citation> {
    project::citations(root)
        .map(|index| {
            index
                .citations
                .into_iter()
                .map(|citation| revision::Citation {
                    key: citation.key,
                    title: Some(citation.title).filter(|title| !title.is_empty()),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The live text of every file an editor has open, by project-relative
/// path, for [`paper`] to read off the main thread.
pub(super) fn open_sources(
    editors: &[WeakEntity<EditorPanel>],
    cx: &App,
) -> HashMap<String, String> {
    editors
        .iter()
        .filter_map(WeakEntity::upgrade)
        .map(|editor| {
            let editor = editor.read(cx);
            (editor.relative().to_string(), editor.text(cx))
        })
        .collect()
}

/// The paper around `range` of `source`, the passage's file at `path`.
/// Each file reads as the passage's own does: its live text when `open`
/// has it, else what `root` holds on disk.
pub(super) fn paper(
    root: &Path,
    open: &HashMap<String, String>,
    main: Option<&str>,
    path: &str,
    source: &str,
    range: Range<usize>,
) -> revision::Paper {
    revision::paper(main, path, source, range.start, range.end, |file| {
        open.get(file)
            .cloned()
            .or_else(|| std::fs::read_to_string(root.join(file)).ok())
    })
}
