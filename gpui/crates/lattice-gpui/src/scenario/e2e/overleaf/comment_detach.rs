//! The e2e checks of a commented passage cut from the live document
//! (#1399), against the stand-in, whose ranges move as Overleaf's ranges
//! tracker moves them:
//!
//! - cutting the whole passage detaches its thread on both sides, and its
//!   marks never move onto the text that followed it;
//! - Undo, or pasting the passage back where it was, re-attaches the thread
//!   on Overleaf with one comment op, as Overleaf's own editor does;
//! - the same text pasted anywhere else leaves it detached;
//! - pasted back in place after that near miss, it is re-attached;
//! - pasted back after the document was joined again in between, it stays
//!   detached.

use std::time::Duration;

use anyhow::{Context as _, Result};
use editor::SelectionEffects;
use gpui::{App, AsyncApp, Entity, Focusable as _};
use serde_json::json;

use super::super::Steps;
use crate::{
    editor_panel::EditorPanel,
    scenario::{Run, overleaf_mock::MockOverleaf},
    shell::Shell,
};

/// The commented passage, and the text after it on its line.
const PASSAGE: &str = "Cut this passage. ";
const AFTER: &str = "Keep this text.";

/// Is thread `thread` drawn anywhere in `editor`: anchored in main.tex
/// here, or a pip in its margin?
fn drawn(shell: &Entity<Shell>, editor: &Entity<EditorPanel>, thread: &str, cx: &App) -> bool {
    let anchored = shell
        .read(cx)
        .overleaf
        .read(cx)
        .comments_in("main.tex")
        .iter()
        .any(|(range, _)| range.thread_id == thread);
    let key = format!("comment:{thread}");
    anchored
        || editor
            .read(cx)
            .margin_painted()
            .iter()
            .any(|(painted, _)| *painted == key)
}

/// Is `thread` anchored on the stand-in at `position` on `quote`?
fn anchored_at(mock: &MockOverleaf, thread: &str, position: i64, quote: &str) -> bool {
    mock.comments("main.tex").iter().any(|range| {
        range.thread_id == thread && range.position == position && range.quote == quote
    })
}

/// The comment ops the app sent for `thread`.
fn anchors_sent(mock: &MockOverleaf, thread: &str) -> usize {
    mock.updates()
        .iter()
        .flat_map(|update| update["op"].as_array().cloned().unwrap_or_default())
        .filter(|op| op["t"].as_str() == Some(thread) && op.get("c").is_some())
        .count()
}

impl Run {
    pub(super) async fn overleaf_comment_cut(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
        editor: &Entity<EditorPanel>,
    ) -> Result<()> {
        let root = self.root(cx).await?;
        let line = format!("{PASSAGE}{AFTER}\n");
        let typed = mock.peer_typed(
            "main.tex",
            vec![lattice_core::overleaf_rt::ot::insert(0, &line)],
        );
        let thread = mock.peer_commented("main.tex", 0, PASSAGE, "Is this needed?");
        let shown = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                drawn(shell, editor, &thread, cx)
                    && editor.read(cx).overleaf_text(cx).starts_with(&line)
            })
            .await;
        let settled =
            |shell: &Entity<Shell>, cx: &mut App| !shell.read(cx).overleaf.read(cx).live_sending();

        // Cut, as the writer does: the passage selected, then ⌘X.
        let selected = self.select_quote(cx, editor, PASSAGE)?;
        self.cut_keys(cx, &[('x', true)]).await?;
        let cut = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                mock.file("main.tex")
                    .is_some_and(|text| text.starts_with(AFTER))
                    && anchored_at(mock, &thread, 0, "")
                    && settled(shell, cx)
                    && !drawn(shell, editor, &thread, cx)
            })
            .await;
        self.adapter_shot(cx, "overleaf-comment-cut").await;
        steps.check(
            "cutting a whole commented passage detaches its thread on both sides, and its marks never move onto the text after it",
            typed && shown && selected && cut && mock.rejected().is_empty(),
            json!({ "typed": typed, "shown": shown, "selected": selected, "cut": cut, "comments": mock.comments("main.tex"), "overleaf": mock.file("main.tex"), "refused": mock.rejected() }),
        );

        // Undo brings the passage back where it was: one comment op
        // re-attaches the thread.
        self.cut_keys(cx, &[('z', true)]).await?;
        let undone = self.reattached(cx, mock, editor, &thread, &line).await;
        let after_undo = anchors_sent(mock, &thread);
        // Cut again, then paste it back in place.
        self.select_quote(cx, editor, PASSAGE)?;
        self.cut_keys(cx, &[('x', true)]).await?;
        let cut_again = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                anchored_at(mock, &thread, 0, "") && settled(shell, cx)
            })
            .await;
        self.cut_keys(cx, &[('v', true)]).await?;
        let pasted_back = self.reattached(cx, mock, editor, &thread, &line).await;
        let after_paste = anchors_sent(mock, &thread);
        steps.check(
            "Undo, or pasting the passage back where it was, re-attaches the thread on Overleaf with one comment op each time, and its marks are on the passage again",
            undone && after_undo == 1 && cut_again && pasted_back && after_paste == 2 && mock.rejected().is_empty(),
            json!({ "undone": undone, "after_undo": after_undo, "cut_again": cut_again, "pasted_back": pasted_back, "after_paste": after_paste, "comments": mock.comments("main.tex"), "overleaf": mock.file("main.tex") }),
        );

        // A near miss: cut, and paste the same text after the line's other
        // text. Overleaf keeps it detached, and so does this.
        self.select_quote(cx, editor, PASSAGE)?;
        self.cut_keys(cx, &[('x', true)]).await?;
        self.wait_for(cx, Duration::from_secs(15), |shell, cx| {
            anchored_at(mock, &thread, 0, "") && settled(shell, cx)
        })
        .await;
        let caret = self.caret_to(cx, editor, AFTER.len())?;
        self.cut_keys(cx, &[('v', true)]).await?;
        let elsewhere = format!("{AFTER}{PASSAGE}\n");
        let landed = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                mock.file("main.tex")
                    .is_some_and(|text| text.starts_with(&elsewhere))
                    && settled(shell, cx)
            })
            .await;
        // Long enough for an anchor that should not go out to have gone.
        self.pause(cx, Duration::from_millis(1500)).await;
        let missed = anchored_at(mock, &thread, 0, "")
            && anchors_sent(mock, &thread) == 2
            && cx.update(|cx| !drawn(&self.shell, editor, &thread, cx));
        steps.check(
            "the cut passage pasted anywhere else leaves its thread detached, with no comment op",
            caret && landed && missed,
            json!({ "caret": caret, "landed": landed, "missed": missed, "sent": anchors_sent(mock, &thread), "comments": mock.comments("main.tex"), "overleaf": mock.file("main.tex") }),
        );
        // Undo the stray paste, then paste the passage back in place.
        self.cut_keys(cx, &[('z', true)]).await?;
        let unpasted = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                mock.file("main.tex")
                    .is_some_and(|text| text.starts_with(&format!("{AFTER}\n")))
                    && settled(shell, cx)
            })
            .await;
        let home = self.caret_to(cx, editor, 0)?;
        self.cut_keys(cx, &[('v', true)]).await?;
        let restored = self.reattached(cx, mock, editor, &thread, &line).await;
        steps.check(
            "after the near miss, pasting the passage back where it was re-attaches the thread, with one comment op",
            unpasted && home && restored && anchors_sent(mock, &thread) == 3,
            json!({ "unpasted": unpasted, "home": home, "restored": restored, "sent": anchors_sent(mock, &thread), "comments": mock.comments("main.tex"), "overleaf": mock.file("main.tex") }),
        );

        // A join in between: cut, leave main.tex for another file and come
        // back, then paste the passage back where it was. What the first
        // join detached is forgotten.
        self.select_quote(cx, editor, PASSAGE)?;
        self.cut_keys(cx, &[('x', true)]).await?;
        let detached = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                anchored_at(mock, &thread, 0, "") && settled(shell, cx)
            })
            .await;
        let rejoined = self.rejoin_main(cx, &root).await?;
        // Pasted back where it was: the cut is still on the clipboard.
        let home = self.caret_to(cx, editor, 0)?;
        self.cut_keys(cx, &[('v', true)]).await?;
        let back = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                mock.file("main.tex")
                    .is_some_and(|text| text.starts_with(&line))
                    && settled(shell, cx)
            })
            .await;
        self.pause(cx, Duration::from_millis(1500)).await;
        let stayed = mock
            .comments("main.tex")
            .iter()
            .any(|range| range.thread_id == thread && range.quote.is_empty())
            && anchors_sent(mock, &thread) == 3
            && cx.update(|cx| !drawn(&self.shell, editor, &thread, cx));
        steps.check(
            "the passage pasted back after the document was joined again in between leaves the thread detached, with no comment op",
            detached && rejoined && home && back && stayed && mock.rejected().is_empty(),
            json!({ "detached": detached, "rejoined": rejoined, "home": home, "back": back, "stayed": stayed, "sent": anchors_sent(mock, &thread), "comments": mock.comments("main.tex"), "overleaf": mock.file("main.tex") }),
        );

        // Leave main.tex and the threads as the next steps expect them.
        let shell = self.shell.clone();
        cx.update(|cx| {
            shell
                .read(cx)
                .overleaf
                .clone()
                .update(cx, |bridge, cx| bridge.delete_thread(&thread, cx))
        });
        mock.peer_typed(
            "main.tex",
            vec![lattice_core::overleaf_rt::ot::delete(0, &line)],
        );
        let cleared = self
            .wait_for(cx, Duration::from_secs(15), |_, cx| {
                mock.thread(&thread).is_none()
                    && !editor.read(cx).overleaf_text(cx).contains(PASSAGE)
            })
            .await;
        anyhow::ensure!(cleared, "the cut passage's thread and line did not go");
        Ok(())
    }

    /// Press `keys` (each a character and whether ⌘ is held) in the window.
    async fn cut_keys(&self, cx: &mut AsyncApp, keys: &[(char, bool)]) -> Result<()> {
        let _front = self.focused(cx, "overleaf comment cut").await?;
        for (key, command) in keys {
            self.press(cx, *key, *command).await?;
        }
        Ok(())
    }

    /// The passage is back at the line's start on both sides, its thread
    /// anchored on it on Overleaf and drawn here.
    async fn reattached(
        &self,
        cx: &mut AsyncApp,
        mock: &MockOverleaf,
        editor: &Entity<EditorPanel>,
        thread: &str,
        line: &str,
    ) -> bool {
        self.wait_for(cx, Duration::from_secs(15), |shell, cx| {
            mock.file("main.tex")
                .is_some_and(|text| text.starts_with(line))
                && anchored_at(mock, thread, 0, PASSAGE)
                && !shell.read(cx).overleaf.read(cx).live_sending()
                && drawn(shell, editor, thread, cx)
        })
        .await
    }

    /// The caret at byte `at` of the editor's text, with the keyboard in it.
    fn caret_to(&self, cx: &mut AsyncApp, editor: &Entity<EditorPanel>, at: usize) -> Result<bool> {
        let zed = cx.update(|cx| editor.read(cx).editor().clone());
        self.with_window(cx, |_, window, cx| {
            window.focus(&zed.focus_handle(cx), cx);
            zed.update(cx, |zed, cx| {
                zed.change_selections(SelectionEffects::default(), window, cx, |selections| {
                    selections.select_ranges([
                        editor::MultiBufferOffset(at)..editor::MultiBufferOffset(at)
                    ])
                })
            })
        })?;
        Ok(true)
    }

    /// Leave main.tex for refs.bib and come back to it, each joined live.
    /// (Not notes.md: the Markdown checks expect to open it first.)
    async fn rejoin_main(&self, cx: &mut AsyncApp, root: &std::path::Path) -> Result<bool> {
        let mut joined = true;
        for path in ["refs.bib", "main.tex"] {
            let file = root.join(path);
            self.with_window(cx, |shell, window, cx| {
                shell.update(cx, |shell, cx| {
                    shell.open_path(file.clone(), None, window, cx)
                })
            })?;
            joined &= self
                .wait_for(cx, Duration::from_secs(20), |shell, cx| {
                    let bridge = shell.read(cx).overleaf.read(cx);
                    bridge.live_file() == Some(path) && !bridge.live_sending()
                })
                .await;
        }
        cx.update(|cx| {
            self.shell.update(cx, |shell, cx| {
                shell.open_editor(&root.join("main.tex"), cx)
            })
        })
        .context("main.tex is open")?;
        Ok(joined)
    }
}
