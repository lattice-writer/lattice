//! What a revision's prompt reads beyond the annotation itself: the
//! bibliography an "Add a citation" prompt offers.

use std::path::Path;

use lattice_core::project;
use lattice_editor::annotation_revision as revision;

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
