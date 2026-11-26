//! The e2e check of pausing and resuming a linked project's sync
//! (`OverleafBridge::set_paused`, the palette's Pause and Resume Overleaf
//! sync), against the stand-in:
//!
//! - pausing closes the live channel and records the pause in the
//!   project's sync state, so it holds across restarts, and the palette
//!   then offers Resume;
//! - edits made here while paused stay on this Mac, and a co-author's
//!   typing meanwhile stays on Overleaf;
//! - resuming runs a sync at once, which merges both sides line by line:
//!   each edit reaches the other side exactly once, nothing is marked up
//!   and nothing is lost.

use std::time::Duration;

use anyhow::Result;
use gpui::AsyncApp;
use lattice_core::overleaf_rt::ot::insert;
use serde_json::json;

use super::super::Steps;
use crate::{
    overleaf::{bridge::ChannelStatus, sync},
    scenario::{Run, overleaf_mock::MockOverleaf},
    shell::overleaf_flow::{PauseOverleafSync, ResumeOverleafSync},
};

impl Run {
    pub(super) async fn overleaf_pause_resume(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
    ) -> Result<()> {
        const OURS: &str = "% Written here while sync was paused\n";
        const THEIRS: &str = "% Grace typed this on Overleaf while we were paused\n";
        let root = self.root(cx).await?;
        let main = root.join("main.tex");
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_path(main.clone(), Some(1), window, cx)
            })
        })?;
        self.wait_for(cx, Duration::from_secs(20), |shell, cx| {
            let bridge = shell.read(cx).overleaf.read(cx);
            bridge.live_file() == Some("main.tex") && !bridge.live_sending() && !bridge.syncing()
        })
        .await;

        self.with_window(cx, |_, window, cx| {
            window.dispatch_action(Box::new(PauseOverleafSync), cx)
        })?;
        let paused = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                let bridge = shell.read(cx).overleaf.read(cx);
                bridge.paused() && bridge.channel() == ChannelStatus::Off
            })
            .await;
        let persisted = sync::link(&root)
            .ok()
            .flatten()
            .is_some_and(|link| link.paused);
        let offered =
            cx.update(|cx| self.shell.read(cx).command_context(cx).overleaf_paused);

        // Ours at the end of the file, theirs at its top: lines apart, so
        // the merge takes both.
        self.with_window(cx, |shell, window, cx| {
            if let Some(editor) = shell.update(cx, |shell, cx| shell.open_editor(&main, cx)) {
                let zed = editor.read(cx).editor().clone();
                zed.update(cx, |zed, cx| {
                    zed.move_to_end(&editor::actions::MoveToEnd, window, cx)
                });
            }
        })?;
        self.type_text(cx, OURS).await?;
        let theirs_typed = mock.peer_typed("main.tex", vec![insert(0, THEIRS)]);
        let saved_here = self
            .wait_for(cx, Duration::from_secs(10), |_, _| {
                std::fs::read_to_string(&main).is_ok_and(|text| text.contains(OURS))
            })
            .await;
        // Long enough for live mode's push and poll to have gone, had they
        // still been running.
        self.pause(cx, Duration::from_secs(5)).await;
        let held = mock.file("main.tex").is_some_and(|text| !text.contains(OURS))
            && std::fs::read_to_string(&main).is_ok_and(|text| !text.contains(THEIRS));

        self.with_window(cx, |_, window, cx| {
            window.dispatch_action(Box::new(ResumeOverleafSync), cx)
        })?;
        let once = |text: &str| text.matches(OURS).count() == 1 && text.matches(THEIRS).count() == 1;
        let merged = self
            .wait_for(cx, Duration::from_secs(45), |shell, cx| {
                let bridge = shell.read(cx).overleaf.read(cx);
                !bridge.paused()
                    && !bridge.syncing()
                    && mock.file("main.tex").is_some_and(|text| once(&text))
                    && std::fs::read_to_string(&main)
                        .is_ok_and(|text| once(&text) && !text.contains("<<<<<<<"))
            })
            .await;
        let unpaused = sync::link(&root)
            .ok()
            .flatten()
            .is_some_and(|link| !link.paused);
        // The channel comes back for the checks after this one.
        let live = self
            .wait_for(cx, Duration::from_secs(40), |shell, cx| {
                shell.read(cx).overleaf.read(cx).channel() == ChannelStatus::Live
            })
            .await;
        steps.check(
            "pausing Overleaf sync keeps each side's edits apart, and resuming merges each once",
            paused
                && persisted
                && offered
                && theirs_typed
                && saved_here
                && held
                && merged
                && unpaused
                && live,
            json!({
                "paused": paused, "persisted": persisted, "offered": offered,
                "theirs_typed": theirs_typed, "saved_here": saved_here, "held": held,
                "merged": merged, "unpaused": unpaused, "live": live,
                "overleaf": mock.file("main.tex"),
                "here": std::fs::read_to_string(&main).ok(),
            }),
        );
        Ok(())
    }
}
