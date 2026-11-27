//! The e2e check of a commented passage cut from the live document
//! (#1399), against the stand-in, whose ranges move as Overleaf's ranges
//! tracker moves them: the thread is detached on both sides, and neither its
//! marks nor its margin pip land on the text that followed the passage,
//! before Undo brings the passage back or after.

use std::time::Duration;

use anyhow::Result;
use gpui::{App, AsyncApp, Entity};
use lattice_core::overleaf_rt::ot::{delete, insert};
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

impl Run {
    pub(super) async fn overleaf_comment_cut(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
        editor: &Entity<EditorPanel>,
    ) -> Result<()> {
        let line = format!("{PASSAGE}{AFTER}\n");
        let typed = mock.peer_typed("main.tex", vec![insert(0, &line)]);
        let thread = mock.peer_commented("main.tex", 0, PASSAGE, "Is this needed?");
        let shown = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                drawn(shell, editor, &thread, cx)
                    && editor.read(cx).overleaf_text(cx).starts_with(&line)
            })
            .await;
        // Cut, as the writer does: the passage selected, then ⌘X.
        let selected = self.select_quote(cx, editor, PASSAGE)?;
        {
            let _front = self.focused(cx, "overleaf comment cut").await?;
            self.press(cx, 'x', true).await?;
        }
        let detached = |quote: &str| {
            mock.comments("main.tex")
                .iter()
                .any(|range| range.thread_id == thread && range.quote == quote)
        };
        let cut = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                mock.file("main.tex")
                    .is_some_and(|text| text.starts_with(AFTER))
                    && detached("")
                    && !shell.read(cx).overleaf.read(cx).live_sending()
                    && !drawn(shell, editor, &thread, cx)
            })
            .await;
        self.adapter_shot(cx, "overleaf-comment-cut").await;
        // Undo brings the passage back; the thread stays off the text after.
        {
            let _front = self.focused(cx, "overleaf comment cut undo").await?;
            self.press(cx, 'z', true).await?;
        }
        let undone = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                mock.file("main.tex")
                    .is_some_and(|text| text.starts_with(&line))
                    && !shell.read(cx).overleaf.read(cx).live_sending()
            })
            .await;
        // The margin is painted after the editor's review is refreshed: give
        // a pip that should not come back the time to show.
        self.pause(cx, Duration::from_millis(800)).await;
        let off_after = cx.update(|cx| !drawn(&self.shell, editor, &thread, cx));
        steps.check(
            "cutting a whole commented passage detaches its thread on both sides, and its marks never move onto the text after it, also once Undo brings the passage back",
            typed && shown && selected && cut && undone && off_after && mock.rejected().is_empty(),
            json!({ "typed": typed, "shown": shown, "selected": selected, "cut": cut, "undone": undone, "off_after": off_after, "comments": mock.comments("main.tex"), "overleaf": mock.file("main.tex"), "refused": mock.rejected() }),
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
        mock.peer_typed("main.tex", vec![delete(0, &line)]);
        let cleared = self
            .wait_for(cx, Duration::from_secs(15), |_, cx| {
                mock.thread(&thread).is_none()
                    && !editor.read(cx).overleaf_text(cx).contains(PASSAGE)
            })
            .await;
        anyhow::ensure!(cleared, "the cut passage's thread and line did not go");
        Ok(())
    }
}
