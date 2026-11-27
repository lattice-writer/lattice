//! The e2e checks of local annotations (`crate::annotations`,
//! `editor_panel/annotations.rs`, `shell/annotations_flow.rs`): a passage of
//! the chapter selected and annotated from the palette's Annotate selection,
//! the note typed with native keys and saved with ⌘↵ into
//! `.research/annotations.json`; its passage marked, its pin drawn in the
//! gutter and the status bar counting it; the titlebar's Annotations button
//! opening the panel, which lists it; a second note typed and closed with
//! Esc offered back as "Note discarded"; the note's words in its card,
//! opened under the pointer, selected with a native drag and copied with
//! ⌘C, with nothing offering Overleaf in this unlinked project; the first
//! resolved from the panel,
//! leaving the counter at none; and the project's annotations put back as
//! they were.

use std::{path::Path, time::Duration};

use anyhow::Result;
use editor::SelectionEffects;
use gpui::{AsyncApp, Entity, Pixels, Point, px};
use serde_json::json;

use super::Steps;
use crate::{
    annotations::{card::PaintedNote, store},
    editor_panel::EditorPanel,
    native,
    scenario::Run,
};

const QUOTE: &str = "introduces the document";
const SECOND: &str = "second line";
const NOTE: &str = "Say what it introduces";

impl Run {
    pub(super) async fn annotations(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        root: &Path,
    ) -> Result<()> {
        let file = root.join(".research/annotations.json");
        let original = std::fs::read(&file).ok();
        let editor = self.open(cx, &root.join("chapters/intro.tex"), Some(1))?;
        let loaded = self
            .wait_for(cx, Duration::from_secs(10), |_, cx| {
                store::try_store(cx).is_some_and(|store| store.read(cx).loaded(root))
            })
            .await;
        let select = |run: &Run, cx: &mut AsyncApp, quote: &str| -> Result<bool> {
            let text = run.editor_text(cx, &editor);
            let Some(start) = text.find(quote) else {
                return Ok(false);
            };
            let zed = cx.update(|cx| editor.read(cx).editor().clone());
            run.with_window(cx, |_, window, cx| {
                zed.update(cx, |zed, cx| {
                    zed.change_selections(SelectionEffects::default(), window, cx, |selections| {
                        selections.select_ranges([editor::MultiBufferOffset(start)
                            ..editor::MultiBufferOffset(start + quote.len())])
                    })
                })
            })?;
            Ok(true)
        };

        // The palette's Annotate selection opens the composer on it.
        let selected = select(self, cx, QUOTE)?;
        self.dispatch_to_shell(cx, &crate::shell::AnnotateSelection)?;
        let composing = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                editor.read(cx).composing_annotation()
            })
            .await;
        steps.check(
            "Annotate selection opens the note's composer on the selected passage",
            loaded && selected && composing,
            json!({ "loaded": loaded, "selected": selected, "composing": composing }),
        );

        // Typed with native keys, saved with ⌘↵.
        {
            let _front = self.focused(cx, "annotation composer").await?;
            // The composer's field is drawn, and has the keys, before typing.
            self.pause(cx, Duration::from_millis(300)).await;
            self.type_text(cx, NOTE).await?;
            self.press(cx, '\n', true).await?;
        }
        let saved = self
            .wait_for(cx, Duration::from_secs(10), |_, _| {
                std::fs::read_to_string(&file)
                    .is_ok_and(|text| text.contains(NOTE) && text.contains(QUOTE))
            })
            .await;
        let closed = cx.update(|cx| !editor.read(cx).composing_annotation());
        self.adapter_shot(cx, "annotation-saved").await;
        steps.check(
            "⌘↵ saves the typed note as a numbered annotation in .research/annotations.json",
            saved && closed,
            json!({ "saved": saved, "closed": closed }),
        );

        // Its passage marked, its pin in the gutter, and the counter.
        let placed = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                editor
                    .read(cx)
                    .placed_annotations()
                    .iter()
                    .any(|(_, number, _)| *number >= 1)
            })
            .await;
        let pin = {
            let _front = self.foreground(cx, "annotation pin").await?;
            self.wait_for(cx, Duration::from_secs(5), |_, cx| {
                editor
                    .read(cx)
                    .lattice_editor()
                    .read(cx)
                    .painted()
                    .gutter
                    .iter()
                    .any(|(layer, row, _)| {
                        *layer == crate::editor_panel::ANNOTATION_PINS && *row == 1
                    })
            })
            .await
        };
        let counted = self
            .wait_for(cx, Duration::from_secs(10), |_, cx| {
                crate::status_counters::counts(root, "chapters/intro.tex", None, cx).annotations
                    == 1
            })
            .await;
        steps.check(
            "the note's passage is marked, its pin drawn beside its line, and the status bar counts it",
            placed && pin && counted,
            json!({ "placed": placed, "pin": pin, "counted": counted }),
        );

        // Its card, opened under the pointer: the note's words selected with
        // a native drag, and ⌘C copies them, though the editor keeps the
        // keyboard. Nothing here offers Overleaf, the project not linked.
        let id = cx.update(|cx| {
            store::try_store(cx).and_then(|store| {
                store
                    .read(cx)
                    .annotations(root)
                    .into_iter()
                    .find(|comment| comment.quote == QUOTE)
                    .map(|comment| comment.id.clone())
            })
        });
        let (copied, drags, selected) = match &id {
            Some(id) => self.copy_note_words(cx, &editor, QUOTE, id).await?,
            None => (None, 0, String::new()),
        };
        let shared = id.as_ref().and_then(|id| {
            cx.update(|cx| editor.read(cx).annotation_card_parts())
                .into_iter()
                .find(|note| &note.id == id)
                .map(|note| note.share.is_some())
        });
        select(self, cx, QUOTE)?;
        let zed = cx.update(|cx| editor.read(cx).editor().clone());
        self.with_window(cx, |_, window, cx| {
            use gpui::Focusable as _;
            window.focus(&zed.focus_handle(cx), cx)
        })?;
        let toolbar_shown = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                editor.read(cx).selection_toolbar_shown()
            })
            .await;
        self.pause(cx, Duration::from_millis(100)).await;
        let toolbar_offers =
            !toolbar_shown || cx.update(|cx| editor.read(cx).toolbar_overleaf_button().is_some());
        steps.check(
            "a note's words in its card are selected with a drag and copied with ⌘C; unlinked, neither the selection toolbar nor the card offers Overleaf",
            copied.as_deref() == Some(NOTE) && shared == Some(false) && !toolbar_offers,
            json!({ "id": id, "copied": copied, "drags": drags, "selected": selected, "card_shares": shared, "toolbar_offers": toolbar_offers }),
        );

        // The titlebar's button opens the panel, which lists it.
        self.dispatch_to_shell(cx, &crate::shell::ShowAnnotations)?;
        let listed = self
            .wait_for(cx, Duration::from_secs(5), |shell, cx| {
                let shell = shell.read(cx);
                shell.view_on_screen("comments", cx)
                    && shell.annotations_panel().read(cx).listed(cx).len() == 1
            })
            .await;
        self.adapter_shot(cx, "annotations-panel").await;
        steps.check(
            "Show annotations opens the Annotations panel, which lists the note",
            listed,
            json!({ "listed": listed }),
        );

        // A second note closed with Esc is offered back.
        let stack = cx.update(crate::notices::install);
        let reselected = select(self, cx, SECOND)?;
        self.with_window(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| editor.annotate_selection(window, cx))
        })?;
        {
            let _front = self.focused(cx, "annotation discard").await?;
            self.pause(cx, Duration::from_millis(300)).await;
            self.type_text(cx, "Drop this").await?;
            self.press(cx, '\u{1b}', false).await?;
        }
        let discarded = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                !editor.read(cx).composing_annotation()
                    && stack
                        .read(cx)
                        .titles()
                        .iter()
                        .any(|title| &**title == lattice_i18n::t!("Note discarded"))
            })
            .await;
        steps.check(
            "Esc closes a note with words in it and offers it back (Note discarded)",
            reselected && discarded,
            json!({ "reselected": reselected, "discarded": discarded }),
        );

        // Resolved from the panel: the counter goes back to none.
        cx.update(|cx| {
            let store = store::store(cx);
            let id = store
                .read(cx)
                .annotations(root)
                .first()
                .map(|comment| comment.id.clone());
            if let Some(id) = id {
                store.update(cx, |store, cx| store.set_resolved(root, &id, true, cx));
            }
        });
        let resolved = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                crate::status_counters::counts(root, "chapters/intro.tex", None, cx).annotations
                    == 0
                    && shell
                        .read(cx)
                        .annotations_panel()
                        .read(cx)
                        .listed(cx)
                        .is_empty()
            })
            .await;
        steps.check(
            "resolving the note takes it from the open list and the counter",
            resolved,
            json!({ "resolved": resolved }),
        );

        // Put the project's annotations back as they were.
        match original {
            Some(bytes) => std::fs::write(&file, bytes)?,
            None => {
                let _ = std::fs::remove_file(&file);
            }
        }
        cx.update(|cx| crate::status_counters::refresh(root.to_path_buf(), cx));
        Ok(())
    }
}

impl Run {
    /// Open the card of the note on `quote` under the pointer, natively,
    /// the caret moved off the passage first so the selection toolbar is
    /// not over it; what the card drew of the note `id` once it shows.
    pub(super) async fn open_note_card(
        &self,
        cx: &mut AsyncApp,
        editor: &Entity<EditorPanel>,
        quote: &str,
        id: &str,
    ) -> Result<Option<PaintedNote>> {
        let text = self.editor_text(cx, editor);
        let after = text
            .find(quote)
            .map(|start| start + quote.len())
            .and_then(|end| text.get(end..))
            .and_then(|rest| rest.lines().next())
            .unwrap_or_default()
            .to_string();
        if !after.is_empty() {
            self.reveal(cx, editor, &after).await?;
        }
        let lattice = cx.update(|cx| editor.read(cx).lattice_editor().clone());
        let (_, at) = self
            .text_point(cx, editor, &lattice, quote, quote.len() / 2)
            .await?;
        self.hover_at(cx, at.center()).await?;
        if self
            .wait_for_card(cx, &lattice, "annotation")
            .await
            .is_none()
        {
            return Ok(None);
        }
        let watched = editor.clone();
        let wanted = id.to_string();
        self.wait_for(cx, Duration::from_secs(3), move |_, cx| {
            watched
                .read(cx)
                .annotation_card_parts()
                .iter()
                .any(|note| note.id == wanted)
        })
        .await;
        Ok(cx.update(|cx| {
            editor
                .read(cx)
                .annotation_card_parts()
                .into_iter()
                .find(|note| note.id == id)
        }))
    }

    /// The note `id`'s words selected in its card with a native drag across
    /// them, and what ⌘C put on the clipboard, with how many drags it took
    /// and what the window's selection held before ⌘C.
    async fn copy_note_words(
        &self,
        cx: &mut AsyncApp,
        editor: &Entity<EditorPanel>,
        quote: &str,
        id: &str,
    ) -> Result<(Option<String>, u32, String)> {
        // The pasteboard is read and written inside a foreground section.
        let _front = self.foreground(cx, "annotation card copy").await?;
        let Some(note) = self.open_note_card(cx, editor, quote, id).await? else {
            return Ok((None, 0, String::new()));
        };
        cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
        // From the first line's start to past the last line's end.
        let line: Pixels = px(8.);
        let from: Point<Pixels> = gpui::point(note.text.left() + px(1.), note.text.top() + line);
        let to = gpui::point(note.text.right() - px(1.), note.text.bottom() - line);
        let native = self.native(cx)?;
        // ⌘C copies the window's selection only once the drag has made one.
        // On GitHub's macOS runner a drag can end with none, and ⌘C then falls
        // through to the editor's own Copy of its current line, so the drag is
        // tried again, up to three times, until the selection holds words.
        let mut drags = 0;
        let mut selected = String::new();
        while drags < 3 && selected.is_empty() {
            drags += 1;
            native::send_mouse(native, native::Mouse::Down, from)?;
            for step in 1..=12 {
                let t = step as f32 / 12.;
                let at = gpui::point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
                self.pause(cx, Duration::from_millis(16)).await;
                native::send_mouse(native, native::Mouse::Dragged, at)?;
            }
            native::send_mouse(native, native::Mouse::Up, to)?;
            let started = std::time::Instant::now();
            while selected.is_empty() && started.elapsed() < Duration::from_secs(2) {
                self.pause(cx, Duration::from_millis(100)).await;
                selected = self.with_window(cx, |_, window, cx| {
                    gpui_kit::base::TextSelection::selected_text(window, cx)
                        .trim()
                        .to_string()
                })?;
            }
        }
        self.press(cx, 'c', true).await?;
        self.pause(cx, Duration::from_millis(200)).await;
        let copied = cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text()));
        Ok((copied, drags, selected))
    }
}

/// This file's registration as the e2e module `annotations` (`registry.rs`).
pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Build,
    rank: 40,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-gpui/src/annotations",
        "gpui/crates/lattice-gpui/src/editor_panel/annotations.rs",
        "gpui/crates/lattice-gpui/src/editor_panel/annotations",
        "gpui/crates/lattice-gpui/src/shell/annotations_flow.rs",
    ],
    run: |run, cx, steps, fixture| Box::pin(run.annotations(cx, steps, &fixture.root)),
};
