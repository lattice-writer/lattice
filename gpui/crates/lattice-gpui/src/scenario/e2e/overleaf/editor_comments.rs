//! The e2e checks of commenting on Overleaf from the source editor, with
//! main.tex live against the stand-in (`scenario::overleaf_mock`):
//!
//! - the selection toolbar offers Comment on Overleaf; a native click on it opens the
//!   composer beside the selection, as Annotate does for a local note
//!   (#1396); the comment typed and sent with ⌘↵ goes out as one comment
//!   op, is marked on its passage here, and leaves the keyboard in the
//!   editor on the passage;
//! - a local note saved on another passage offers Share to Overleaf in its
//!   card; a native click posts it on the same passage, and the local note
//!   is resolved once Overleaf has it.

use std::time::Duration;

use anyhow::Result;
use editor::SelectionEffects;
use gpui::{AsyncApp, Entity, Focusable as _};
use serde_json::json;

use super::super::Steps;
use crate::{
    annotations::store,
    editor_panel::EditorPanel,
    native,
    scenario::{Run, overleaf_mock::MockOverleaf},
};

/// The passage Comment on Overleaf comments on, and the comment.
const COMMENTED: &str = "paper";
const COMMENT: &str = "Which paper?";
/// The passage a local note is written on and shared from, and the note.
const SHARED: &str = "written";
const NOTE: &str = "Say where.";

/// Is a thread anchored on `quote` in main.tex saying `said`?
fn anchored(mock: &MockOverleaf, quote: &str, said: &str) -> bool {
    mock.comments("main.tex").iter().any(|range| {
        range.quote == quote
            && mock
                .thread(&range.thread_id)
                .is_some_and(|(messages, _)| messages.iter().any(|m| m == said))
    })
}

impl Run {
    pub(super) async fn overleaf_editor_comments(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
        editor: &Entity<EditorPanel>,
    ) -> Result<()> {
        self.overleaf_toolbar_comment(cx, steps, mock, editor)
            .await?;
        self.overleaf_share_note(cx, steps, mock, editor).await
    }

    /// Select `quote` in the editor, with the keyboard in it.
    pub(super) fn select_quote(
        &self,
        cx: &mut AsyncApp,
        editor: &Entity<EditorPanel>,
        quote: &str,
    ) -> Result<bool> {
        let text = self.editor_text(cx, editor);
        let Some(start) = text.find(quote) else {
            return Ok(false);
        };
        let zed = cx.update(|cx| editor.read(cx).editor().clone());
        self.with_window(cx, |_, window, cx| {
            window.focus(&zed.focus_handle(cx), cx);
            zed.update(cx, |zed, cx| {
                zed.change_selections(SelectionEffects::default(), window, cx, |selections| {
                    selections.select_ranges([editor::MultiBufferOffset(start)
                        ..editor::MultiBufferOffset(start + quote.len())])
                })
            })
        })?;
        Ok(true)
    }

    async fn overleaf_toolbar_comment(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
        editor: &Entity<EditorPanel>,
    ) -> Result<()> {
        let selected = self.select_quote(cx, editor, COMMENTED)?;
        let shown = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                editor.read(cx).selection_toolbar_shown()
            })
            .await;
        let watched = editor.clone();
        self.wait_for(cx, Duration::from_secs(3), move |_, cx| {
            watched.read(cx).toolbar_overleaf_button().is_some()
        })
        .await;
        let button = cx.update(|cx| editor.read(cx).toolbar_overleaf_button());
        self.adapter_shot(cx, "overleaf-toolbar-comment").await;
        // A native click on the toolbar's Comment on Overleaf.
        if let Some(button) = button {
            let _front = self.focused(cx, "overleaf toolbar button").await?;
            let native = self.native(cx)?;
            native::send_mouse(native, native::Mouse::Down, button.center())?;
            self.pause(cx, Duration::from_millis(40)).await;
            native::send_mouse(native, native::Mouse::Up, button.center())?;
        }
        let offered = button.is_some();
        let watched = editor.clone();
        let composing = self
            .wait_for(cx, Duration::from_secs(5), move |_, cx| {
                watched.read(cx).composing_overleaf_comment()
            })
            .await;
        let in_panel = cx.update(|cx| {
            self.shell
                .read(cx)
                .overleaf_collab
                .read(cx)
                .composing_comment()
        });
        if composing {
            let _front = self.focused(cx, "overleaf toolbar comment").await?;
            self.pause(cx, Duration::from_millis(300)).await;
            self.type_text(cx, COMMENT).await?;
            if std::env::var_os("LATTICE_E2E_SHOTS").is_some() {
                for (appearance, theme) in [
                    (lattice_ui::Appearance::Light, "light"),
                    (lattice_ui::Appearance::Dark, "dark"),
                ] {
                    cx.update(|cx| lattice_ui::LatticeTheme::set_appearance(appearance, cx));
                    self.adapter_shot(cx, &format!("overleaf-comment-popover-{theme}"))
                        .await;
                }
                cx.update(|cx| {
                    lattice_ui::LatticeTheme::set_appearance(lattice_ui::Appearance::Light, cx)
                });
            }
            self.press(cx, '\n', true).await?;
        }
        let anchored = self
            .wait_for(cx, Duration::from_secs(15), |_, _| {
                anchored(mock, COMMENTED, COMMENT)
            })
            .await;
        let thread = mock
            .comments("main.tex")
            .into_iter()
            .find(|range| range.quote == COMMENTED)
            .map(|range| (range.thread_id, range.position));
        // One comment op, on the passage.
        let ops = mock
            .updates()
            .iter()
            .flat_map(|update| update["op"].as_array().cloned().unwrap_or_default())
            .filter(|op| op["c"].as_str() == Some(COMMENTED))
            .count();
        let watched = editor.clone();
        let wanted = thread.clone();
        let marked = self
            .wait_for(cx, Duration::from_secs(10), move |_, cx| {
                let editor = watched.read(cx);
                !editor.composing_overleaf_comment()
                    && wanted.as_ref().is_some_and(|(id, position)| {
                        editor
                            .overleaf_comment_marks()
                            .iter()
                            .any(|(mark, from, len)| {
                                mark == id
                                    && from == position
                                    && *len == COMMENTED.encode_utf16().count() as i64
                            })
                    })
            })
            .await;
        let zed = cx.update(|cx| editor.read(cx).editor().clone());
        let (focused, selection) = self.with_window(cx, |_, window, cx| {
            (
                zed.focus_handle(cx).is_focused(window),
                editor.read(cx).overleaf_selection(cx),
            )
        })?;
        let caret = focused
            && selection.as_ref().is_some_and(|(from, quote)| {
                quote == COMMENTED && thread.as_ref().is_some_and(|(_, at)| at == from)
            });
        steps.check(
            "in a linked project the selection toolbar's Comment on Overleaf opens the composer beside the selection; sent with ⌘↵, it goes out as one comment op, is marked on the passage, and the keyboard is back in the editor on it",
            selected && shown && offered && composing && !in_panel && anchored && ops == 1 && marked && caret && mock.rejected().is_empty(),
            json!({ "selected": selected, "shown": shown, "button": button.map(|b| format!("{b:?}")), "composing": composing, "in_panel": in_panel, "anchored": anchored, "ops": ops, "marked": marked, "focused": focused, "selection": selection, "comments": mock.comments("main.tex") }),
        );
        Ok(())
    }

    async fn overleaf_share_note(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
        editor: &Entity<EditorPanel>,
    ) -> Result<()> {
        let root = self.root(cx).await?;
        // A local note on the passage, as the toolbar's Annotate saves one.
        let selected = self.select_quote(cx, editor, SHARED)?;
        let composing = self.with_window(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| editor.annotate_selection(window, cx))
        })?;
        if composing {
            let _front = self.focused(cx, "overleaf local note").await?;
            self.pause(cx, Duration::from_millis(300)).await;
            self.type_text(cx, NOTE).await?;
            self.press(cx, '\n', true).await?;
        }
        let find = |cx: &gpui::App| {
            store::try_store(cx).and_then(|store| {
                store
                    .read(cx)
                    .annotations(&root)
                    .into_iter()
                    .find(|note| note.quote == SHARED && note.body == NOTE)
                    .map(|note| note.id.clone())
            })
        };
        self.wait_for(cx, Duration::from_secs(10), |_, cx| find(cx).is_some())
            .await;
        let id = cx.update(|cx| find(cx));
        // Its card, under the pointer, offers Share to Overleaf: a native
        // click on it.
        let mut share = None;
        if let Some(id) = &id {
            let _front = self.focused(cx, "overleaf share note").await?;
            let card = self.open_note_card(cx, editor, SHARED, id).await?;
            share = card.and_then(|note| note.share);
            self.adapter_shot(cx, "overleaf-share-note-card").await;
            if let Some(button) = share {
                let native = self.native(cx)?;
                native::send_mouse(native, native::Mouse::Down, button.center())?;
                self.pause(cx, Duration::from_millis(40)).await;
                native::send_mouse(native, native::Mouse::Up, button.center())?;
            }
        }
        let anchored = self
            .wait_for(cx, Duration::from_secs(15), |_, _| {
                anchored(mock, SHARED, NOTE)
            })
            .await;
        let resolved = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                id.as_ref().is_some_and(|id| {
                    store::try_store(cx)
                        .and_then(|store| store.read(cx).get(&root, id).map(|note| note.resolved))
                        .unwrap_or(false)
                })
            })
            .await;
        steps.check(
            "a local note in a linked project offers Share to Overleaf in its card; a click posts it on the same passage and resolves it here",
            selected && composing && share.is_some() && anchored && resolved && mock.rejected().is_empty(),
            json!({ "selected": selected, "composing": composing, "id": id, "share": share.map(|b| format!("{b:?}")), "anchored": anchored, "resolved": resolved, "comments": mock.comments("main.tex") }),
        );
        Ok(())
    }
}
