//! Typst's jumps between a source and its PDF, which Typst writes no
//! SyncTeX map for: the helper answers them from the document it last
//! compiled (`crate::typst::helper`), so they go there instead of to
//! lattice-core's SyncTeX lookups (`synctex.rs`), from the same places: a
//! double-click (or ⌘-double-click) in the PDF, Jump to PDF, the toolbar's
//! Reveal cursor in PDF and the Outline.
//!
//! The helper compiles the editor's unsaved text, so nothing is saved
//! first, and the text on screen may be newer than the compile: the helper
//! carries the place across the edits. A helper that has compiled nothing
//! of the document yet (it started after the PDF was written) gets one
//! build of the live PDF's document before the jump says so.

use std::collections::HashMap;

use editor::{SelectionEffects, scroll::Autoscroll};
use gpui::{App, AppContext as _, AsyncWindowContext, Context, Entity, WeakEntity, Window};
use language::Point;
use lattice_core::{
    project::is_typst_path,
    typst_wire::{ClickJump, CursorJump, Jump},
};
use lattice_i18n::t;

use super::{Shell, build::BuildRequest};
use crate::{editor_panel::EditorPanel, notices, typst::helper::Failure};

impl Shell {
    /// The live PDF's document, when it is a Typst one.
    pub(super) fn typst_live_document(&self) -> Option<String> {
        self.live_document
            .clone()
            .filter(|document| is_typst_path(document))
    }

    /// The Typst sources open in the editor, as a compile reads them.
    fn typst_overlay(&mut self, cx: &App) -> HashMap<String, String> {
        self.open_editors()
            .iter()
            .map(|editor| editor.read(cx))
            .filter(|editor| is_typst_path(editor.relative()))
            .map(|editor| {
                (
                    editor.relative().to_string(),
                    editor.editor().read(cx).text(cx),
                )
            })
            .collect()
    }

    /// From the caret in a .typ to its place in the live PDF (`tab`: `None`)
    /// or in a root document's tab (`tab`, that document).
    pub(super) fn forward_sync_typst(
        &mut self,
        tab: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(editor), Some(root)) = (self.active_editor(), self.root(cx)) else {
            return;
        };
        let Some(binary) = crate::typst::binary(cx) else {
            crate::typst::require(false, |_| {}, cx);
            return;
        };
        let (relative, position) = {
            let editor = editor.read(cx);
            (editor.relative().to_string(), editor.cursor())
        };
        if tab.is_none() && self.parts.get::<super::pdf_sync::PdfSync>().locating {
            return;
        }
        let overlay = self.typst_overlay(cx);
        let generation = tab.is_none().then(|| self.set_locating(None, cx));
        cx.spawn_in(window, async move |this, cx| {
            let answer = typst_jump(&this, cx, tab.clone(), {
                let relative = relative.clone();
                move |document| {
                    crate::typst::helper::jump_from_cursor(
                        &binary,
                        CursorJump {
                            root: root.clone(),
                            document,
                            overlay: overlay.clone(),
                            path: relative.clone(),
                            line: position.0,
                            column: position.1,
                        },
                    )
                }
            })
            .await;
            this.update_in(cx, |this, window, cx| {
                if let Some(generation) = generation {
                    this.set_locating(Some(generation), cx);
                }
                // The writer moved on while the jump was made.
                let unmoved = this.active_editor().is_some_and(|editor| {
                    let editor = editor.read(cx);
                    editor.relative() == relative && editor.cursor() == position
                });
                if !unmoved {
                    return;
                }
                match answer {
                    Ok(Jump::To(places)) => {
                        let Some(place) = places.first() else {
                            return;
                        };
                        let panel = match &tab {
                            None => Some(this.pdf.clone()),
                            Some(document) => this
                                .root_pdfs
                                .iter()
                                .find(|(open, _)| open == document)
                                .map(|(_, panel)| panel.clone()),
                        };
                        let Some(panel) = panel else {
                            return;
                        };
                        crate::get_started::progress::mark(crate::get_started::Step::Jump, cx);
                        panel.update(cx, |panel, cx| panel.show_sync_target(place, cx));
                        if tab.is_none() && !this.pdf.read(cx).in_window() {
                            this.show_view("pdf", false, window, cx);
                        }
                    }
                    Ok(Jump::Nowhere) => notices::warning(
                        t!("This source line has no matching position in the PDF."),
                        cx,
                    ),
                    Ok(Jump::NotCompiled) => not_compiled(cx),
                    Err(failure) => failed(failure, cx),
                }
            })
            .ok();
        })
        .detach();
    }

    /// From a double-click at (`x`, `y`) on `page` of `document`'s PDF to
    /// its Typst source: the live PDF's (`tab` false) or a root document's
    /// tab's. Only the newest double-click (`generation`) lands.
    #[allow(
        clippy::too_many_arguments,
        reason = "each argument is a separate piece of the click's lookup"
    )]
    pub(super) fn inverse_sync_typst(
        &mut self,
        generation: u64,
        tab: Option<String>,
        page: u32,
        x: f64,
        y: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root(cx) else {
            return;
        };
        let Some(binary) = crate::typst::binary(cx) else {
            crate::typst::require(false, |_| {}, cx);
            return;
        };
        let overlay = self.typst_overlay(cx);
        cx.spawn_in(window, async move |this, cx| {
            let answer = typst_jump(&this, cx, tab, {
                let root = root.clone();
                move |document| {
                    crate::typst::helper::jump_from_click(
                        &binary,
                        ClickJump {
                            root: root.clone(),
                            document,
                            overlay: overlay.clone(),
                            page,
                            x,
                            y,
                        },
                    )
                }
            })
            .await;
            this.update_in(cx, |this, window, cx| {
                if this.root(cx).as_deref() != Some(root.as_path())
                    || !this.newest_inverse(generation)
                {
                    return;
                }
                match answer {
                    Ok(Jump::To(spot)) => {
                        crate::get_started::progress::mark(crate::get_started::Step::Jump, cx);
                        if let Some(editor) =
                            this.open_path(root.join(&spot.path), Some(spot.line), window, cx)
                        {
                            land_at(&editor, spot.line, spot.column, window, cx);
                        }
                        window.activate_window();
                    }
                    Ok(Jump::Nowhere) => notices::warning(
                        t!("No Typst source was found for this place in the PDF."),
                        cx,
                    ),
                    Ok(Jump::NotCompiled) => not_compiled(cx),
                    Err(failure) => failed(failure, cx),
                }
            })
            .ok();
        })
        .detach();
    }
}

/// Ask `jump` of the document in the live PDF (or `tab`'s). A helper that
/// holds no compile of the live PDF's document builds it once, then is
/// asked again: Typst compiles in moments, and it is the PDF on screen.
async fn typst_jump<T: Send + 'static>(
    this: &WeakEntity<Shell>,
    cx: &mut AsyncWindowContext,
    tab: Option<String>,
    jump: impl Fn(String) -> Result<Jump<T>, Failure> + Clone + Send + 'static,
) -> Result<Jump<T>, Failure> {
    for attempt in 0..2 {
        let Ok(document) = this.update(cx, |this, _| {
            tab.clone().or_else(|| this.typst_live_document())
        }) else {
            return Ok(Jump::NotCompiled);
        };
        let answer = match document {
            Some(document) => {
                let jump = jump.clone();
                cx.background_spawn(async move { jump(document) }).await?
            }
            None => Jump::NotCompiled,
        };
        if !matches!(answer, Jump::NotCompiled) || tab.is_some() || attempt > 0 {
            return Ok(answer);
        }
        let Ok(settled) = this.update_in(cx, |this, window, cx| {
            if this.build_task.is_none() {
                this.run_build(BuildRequest::typst_live(), window, cx);
            }
            this.builds_settled(cx)
        }) else {
            return Ok(Jump::NotCompiled);
        };
        settled.await.ok();
    }
    Ok(Jump::NotCompiled)
}

/// The helper has compiled nothing of the document, and building did not
/// change that (the build failed, or the PDF is a root document's tab).
fn not_compiled(cx: &mut App) {
    notices::warning(
        t!("Build the Typst document first, so Lattice can match the PDF to its source."),
        cx,
    );
}

fn failed(failure: Failure, cx: &mut App) {
    if failure.outdated {
        crate::typst::require(false, |_| {}, cx);
        return;
    }
    notices::error(t!("Typst support"), failure.message, cx);
}

/// Put the caret at the 1-based `line` and `column` (in characters) of
/// `editor`, the line already scrolled to by `open_path`.
fn land_at(
    editor: &Entity<EditorPanel>,
    line: u32,
    column: u32,
    window: &mut Window,
    cx: &mut App,
) {
    let zed = editor.read(cx).editor().clone();
    let text = zed.read(cx).text(cx);
    let Some(source) = text.split('\n').nth(line.saturating_sub(1) as usize) else {
        return;
    };
    let byte = source
        .char_indices()
        .nth(column.saturating_sub(1) as usize)
        .map_or(source.len(), |(at, _)| at);
    let point = Point::new(line.saturating_sub(1), byte as u32);
    zed.update(cx, |zed, cx| {
        zed.change_selections(
            SelectionEffects::scroll(Autoscroll::center()),
            window,
            cx,
            |selections| selections.select_ranges([point..point]),
        );
    });
}
