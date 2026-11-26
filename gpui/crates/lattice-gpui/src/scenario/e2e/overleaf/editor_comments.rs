//! The e2e checks of commenting on Overleaf from the source editor, with
//! main.tex live against the stand-in (`scenario::overleaf_mock`):
//!
//! - the selection toolbar offers Comment on Overleaf, which opens the
//!   collaboration panel's composer on the selection, as the palette's
//!   Annotate selection does; the comment typed and sent with Enter is
//!   posted and anchored on Overleaf;
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
        self.overleaf_toolbar_comment(cx, steps, mock, editor).await?;
        self.overleaf_share_note(cx, steps, mock, editor).await
    }

    /// Select `quote` in the editor, with the keyboard in it.
    fn select_quote(
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
        let offered = cx.update(|cx| editor.read(cx).toolbar_offers_overleaf_comment());
        self.adapter_shot(cx, "overleaf-toolbar-comment").await;
        self.with_window(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| {
                editor.scripted_toolbar("comment-overleaf", window, cx)
            })
        })?;
        let composing = self
            .wait_for(cx, Duration::from_secs(5), |shell, cx| {
                shell.read(cx).overleaf_collab.read(cx).composing_comment()
            })
            .await;
        if composing {
            let _front = self.focused(cx, "overleaf toolbar comment").await?;
            self.pause(cx, Duration::from_millis(300)).await;
            self.type_text(cx, COMMENT).await?;
            self.press(cx, '\n', false).await?;
        }
        let anchored = self
            .wait_for(cx, Duration::from_secs(15), |_, _| {
                anchored(mock, COMMENTED, COMMENT)
            })
            .await;
        steps.check(
            "in a linked project the selection toolbar offers Comment on Overleaf, which opens the comment on the selection; sent with Enter, it is anchored on Overleaf",
            selected && shown && offered && composing && anchored && mock.rejected().is_empty(),
            json!({ "selected": selected, "shown": shown, "offered": offered, "composing": composing, "anchored": anchored, "comments": mock.comments("main.tex") }),
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
            .wait_for(cx, Duration::from_secs(15), |_, _| anchored(mock, SHARED, NOTE))
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
