//! The shell step on closing (`../shell_steps.rs`): a panel goes with its
//! last tab, in the update that closed it, and no frame draws an "Empty"
//! panel in its place.

use anyhow::{Context as _, Result};
use gpui::AsyncApp;
use serde_json::json;
use trellis::model::LayoutDocument;

use super::{Step, panel_with};
use crate::scenario::Run;
use crate::shell::commands::ReopenClosedTab;

/// The panels of `doc` drawn "Empty": every view in them a document slot.
fn empty_panels(doc: &LayoutDocument) -> Vec<String> {
    trellis::model::panels_of(doc.root.as_ref())
        .into_iter()
        .filter(|panel| {
            !panel.views.is_empty()
                && panel
                    .views
                    .iter()
                    .all(|id| doc.views.get(id).is_some_and(|view| view.kind == "slot"))
        })
        .map(|panel| panel.id.clone())
        .collect()
}

impl Run {
    /// A document split beside the others, then closed by its tab; the
    /// PDF's tab closed: each panel goes with its last tab, in the update
    /// that closed it and on the frame after, with no "Empty" panel left.
    pub(super) async fn closing_a_panels_last_tab_leaves_no_empty_panel(
        &self,
        cx: &mut AsyncApp,
    ) -> Result<Step> {
        self.reset_layout(cx)?;
        self.settle_workspace(cx).await?;
        let documents = self.documents_panel(cx)?;
        let bib = self.root(cx).await?.join("references.bib");
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.open_path(bib, None, window, cx))
        })?;
        self.settle_workspace(cx).await?;
        let view = cx
            .update(|cx| self.shell.read(cx).view_of_document("references.bib", cx))
            .context("references.bib did not open")?;
        cx.update(|cx| {
            self.shell.update(cx, |shell, cx| {
                shell.engine_do(cx, |engine| {
                    engine.dock(
                        &view,
                        Some(trellis::model::DockTarget::Beside {
                            beside: documents.clone(),
                            edge: trellis::model::Edge::Right,
                            share: Some(0.5),
                        }),
                    );
                })
            })
        });
        self.settle_workspace(cx).await?;
        let split = panel_with(&self.document(cx), &view).filter(|panel| *panel != documents);

        let close = |cx: &mut AsyncApp, view: String| {
            cx.update(|cx| {
                let workspace = self.shell.read(cx).workspace().clone();
                workspace.update(cx, |workspace, cx| workspace.close(&[view], cx));
                empty_panels(&self.shell.read(cx).document(cx))
            })
        };
        let in_update = close(cx, view.clone());
        self.frame_after_input(cx).await?;
        let doc = self.document(cx);
        let after_frame = empty_panels(&doc);
        let split_gone = split.as_deref().is_some_and(|split| {
            !trellis::model::panels_of(doc.root.as_ref())
                .iter()
                .any(|panel| panel.id == split)
        });

        let pdf_panel = panel_with(&doc, "pdf");
        let pdf_in_update = close(cx, "pdf".to_string());
        self.frame_after_input(cx).await?;
        let doc = self.document(cx);
        let pdf_after_frame = empty_panels(&doc);
        let pdf_gone = pdf_panel.as_deref().is_some_and(|pdf| {
            !trellis::model::panels_of(doc.root.as_ref())
                .iter()
                .any(|panel| panel.id == pdf)
        });
        // Every document closed, Project and Papers still there: no Empty
        // panel stands for them.
        let files: Vec<String> = doc
            .views
            .iter()
            .filter(|(_, view)| view.kind == "file")
            .map(|(id, _)| id.clone())
            .collect();
        let mut docs_in_update = Vec::new();
        for file in files.clone() {
            docs_in_update.extend(close(cx, file));
        }
        self.frame_after_input(cx).await?;
        let docs_after_frame = empty_panels(&self.document(cx));
        self.with_window(cx, |_, window, cx| {
            for _ in &files {
                window.dispatch_action(Box::new(ReopenClosedTab), cx);
            }
        })?;
        self.reset_layout(cx)?;
        self.settle_workspace(cx).await?;
        Ok((
            "a panel whose last tab closes (a split document, the PDF, the last document) goes at once, with no Empty panel drawn",
            split.is_some()
                && split_gone
                && in_update.is_empty()
                && after_frame.is_empty()
                && pdf_gone
                && pdf_in_update.is_empty()
                && pdf_after_frame.is_empty()
                && !files.is_empty()
                && docs_in_update.is_empty()
                && docs_after_frame.is_empty(),
            json!({
                "documents_closed": files.len(),
                "docs_empty_in_update": docs_in_update,
                "docs_empty_after_frame": docs_after_frame,
                "split": split,
                "split_gone": split_gone,
                "empty_in_update": in_update,
                "empty_after_frame": after_frame,
                "pdf_gone": pdf_gone,
                "pdf_empty_in_update": pdf_in_update,
                "pdf_empty_after_frame": pdf_after_frame,
            }),
        ))
    }
}
