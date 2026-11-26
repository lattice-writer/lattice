//! The e2e check of pausing and resuming a linked project's sync
//! (`OverleafBridge::set_paused`, the palette's Pause and Resume Overleaf
//! sync), against the stand-in:
//!
//! - pausing with typing not yet answered waits for it, then closes the
//!   live channel and records the pause in the project's sync state, so it
//!   holds across restarts, and the palette then offers Resume;
//! - edits made here while paused stay on this Mac, and a co-author's
//!   typing meanwhile stays on Overleaf;
//! - resuming runs a sync at once, which merges both sides line by line:
//!   each edit reaches the other side exactly once, nothing is marked up
//!   and nothing is lost;
//! - in manual mode, resuming opens Review changes on what was kept here,
//!   and applying it sends it up;
//! - resumed from Settings › Overleaf's button, Review changes is the layer
//!   the writer meets: Settings closes, and a click on Review's Apply
//!   button reaches it.

use std::time::Duration;

use anyhow::Result;
use gpui::AsyncApp;
use lattice_core::overleaf_rt::ot::insert;
use serde_json::json;

use super::super::Steps;
use crate::{
    overleaf::{bridge::ChannelStatus, sync},
    scenario::{Run, overleaf_mock::MockOverleaf},
    settings,
    shell::overleaf_flow::{PauseOverleafSync, ResumeOverleafSync},
};
use lattice_settings::{OverleafSyncMode, SettingsTab};

impl Run {
    pub(super) async fn overleaf_pause_resume(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
    ) -> Result<()> {
        const UNSENT: &str = "% Typed just before the pause\n";
        const OURS: &str = "% Written here while sync was paused\n";
        const THEIRS: &str = "% Grace typed this on Overleaf while we were paused\n";
        let root = self.root(cx).await?;
        let main = root.join("main.tex");
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_path(main.clone(), Some(1), window, cx)
            })
        })?;
        let ready = self
            .wait_for(cx, Duration::from_secs(20), |shell, cx| {
                let bridge = shell.read(cx).overleaf.read(cx);
                bridge.live_file() == Some("main.tex")
                    && !bridge.live_sending()
                    && !bridge.syncing()
            })
            .await;
        // Everything here goes at the end of the file, and the co-author's
        // at its top: lines apart, so the merge takes both.
        self.with_window(cx, |shell, window, cx| {
            if let Some(editor) = shell.update(cx, |shell, cx| shell.open_editor(&main, cx)) {
                let zed = editor.read(cx).editor().clone();
                zed.update(cx, |zed, cx| {
                    zed.move_to_end(&editor::actions::MoveToEnd, window, cx)
                });
            }
        })?;
        // Paused at once, while this typing is still going out.
        self.type_text(cx, UNSENT).await?;
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
        let once = |text: &str| {
            [UNSENT, OURS, THEIRS]
                .iter()
                .all(|line| text.matches(line).count() == 1)
        };
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
            ready
                && paused
                && persisted
                && offered
                && theirs_typed
                && saved_here
                && held
                && merged
                && unpaused
                && live,
            json!({
                "ready": ready, "paused": paused, "persisted": persisted, "offered": offered,
                "theirs_typed": theirs_typed, "saved_here": saved_here, "held": held,
                "merged": merged, "unpaused": unpaused, "live": live,
                "overleaf": mock.file("main.tex"),
                "here": std::fs::read_to_string(&main).ok(),
            }),
        );

        // Manual mode: resuming opens Review changes on what was kept here.
        const KEPT: &str = "% Kept here while paused in manual mode\n";
        cx.update(|cx| settings::update(cx, |s| s.overleaf_sync_mode = OverleafSyncMode::Manual));
        self.with_window(cx, |_, window, cx| {
            window.dispatch_action(Box::new(PauseOverleafSync), cx)
        })?;
        let paused = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                shell.read(cx).overleaf.read(cx).paused()
            })
            .await;
        self.type_text(cx, KEPT).await?;
        let saved = self
            .wait_for(cx, Duration::from_secs(10), |_, _| {
                std::fs::read_to_string(&main).is_ok_and(|text| text.contains(KEPT))
            })
            .await;
        self.with_window(cx, |_, window, cx| {
            window.dispatch_action(Box::new(ResumeOverleafSync), cx)
        })?;
        let opened = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                shell.read(cx).overleaf_review().is_some()
            })
            .await;
        let review = cx.update(|cx| self.shell.read(cx).overleaf_review());
        if let Some(review) = &review {
            self.wait_for(cx, Duration::from_secs(30), |_, cx| {
                review.read(cx).scenario_summary().is_some()
            })
            .await;
            cx.update(|cx| review.update(cx, |review, cx| review.scenario_apply(cx)));
        }
        let sent = self
            .wait_for(cx, Duration::from_secs(40), |_, _| {
                mock.file("main.tex")
                    .is_some_and(|text| text.matches(KEPT).count() == 1)
            })
            .await;
        cx.update(|cx| settings::update(cx, |s| s.overleaf_sync_mode = OverleafSyncMode::Live));
        steps.check(
            "in manual mode, resuming opens Review changes on what was kept here, and applying it sends it up",
            paused && saved && opened && sent,
            json!({
                "paused": paused, "saved": saved, "opened": opened, "sent": sent,
                "overleaf": mock.file("main.tex"),
            }),
        );
        Ok(())
    }

    /// Resume a paused, manual-mode project from Settings › Overleaf's
    /// button, as the writer does, and check the layers they meet. Settings
    /// and Review changes are both layers of this window, and Settings was
    /// left open with Review (lattice#1322), to stand over the window again
    /// once Review closed; it closes first now. Which layer is on top is
    /// what the pointer finds: the click on Apply below is a window event
    /// at where Apply is drawn, so it reaches whichever layer covers that
    /// spot, as the writer's would. Focus tells nothing here, since each
    /// modal layer takes the keyboard as it first draws, under another or
    /// not.
    pub(super) async fn overleaf_resume_from_settings(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
    ) -> Result<()> {
        const KEPT: &str = "% Kept here while paused, resumed from Settings\n";
        let root = self.root(cx).await?;
        let main = root.join("main.tex");
        // From a settled bridge, whatever the steps before left running.
        let settled = self
            .wait_for(cx, Duration::from_secs(30), |shell, cx| {
                let shell = shell.read(cx);
                let bridge = shell.overleaf.read(cx);
                !bridge.syncing()
                    && !bridge.paused()
                    && !bridge.live_sending()
                    && shell.overleaf_review().is_none()
                    && !shell.settings_open()
            })
            .await;
        lattice_ui::parity::enable();
        lattice_ui::parity::forget("overleaf/settings/pause");
        lattice_ui::parity::forget("overleaf-review/");
        cx.update(|cx| settings::update(cx, |s| s.overleaf_sync_mode = OverleafSyncMode::Manual));
        self.with_window(cx, |_, window, cx| {
            window.dispatch_action(Box::new(PauseOverleafSync), cx)
        })?;
        // Settings reads the pause back from the project's sync state.
        let paused = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                shell.read(cx).overleaf.read(cx).paused()
                    && sync::link(&root)
                        .ok()
                        .flatten()
                        .is_some_and(|link| link.paused)
            })
            .await;
        // Saved here while paused, so Review has a change to apply.
        let text = std::fs::read_to_string(&main)?;
        std::fs::write(&main, format!("{text}{KEPT}"))?;

        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_settings(Some(SettingsTab::Overleaf), window, cx)
            })
        })?;
        let offered = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                let shows_paused = shell
                    .read(cx)
                    .settings
                    .as_ref()
                    .and_then(|settings| settings.read(cx).overleaf_connection())
                    .and_then(|connection| connection.read(cx).scenario_state())
                    .is_some_and(|(_, link)| link.is_some_and(|link| link.paused));
                shows_paused && drawn("overleaf/settings/pause").is_some()
            })
            .await;
        // Past Settings' entry, so the press lands where the button rests.
        self.pause(cx, Duration::from_millis(300)).await;
        let resume = drawn("overleaf/settings/pause");
        if let Some(at) = resume {
            self.click_at(cx, at)?;
        }
        let review = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                shell
                    .read(cx)
                    .overleaf_review()
                    .is_some_and(|review| review.read(cx).scenario_summary().is_some())
                    && drawn("overleaf-review/apply").is_some()
            })
            .await
            .then(|| cx.update(|cx| self.shell.read(cx).overleaf_review()))
            .flatten();
        // Review's entry, likewise.
        self.pause(cx, Duration::from_millis(300)).await;
        self.snapshot(cx, "overleaf-resume-from-settings").await;
        let settings_closed = cx.update(|cx| !self.shell.read(cx).settings_open());
        let apply = drawn("overleaf-review/apply");
        if let Some(at) = apply.filter(|_| review.is_some()) {
            self.click_at(cx, at)?;
        }
        let sent_once = |text: &str| text.matches(KEPT).count() == 1;
        let clicked_apply = self
            .wait_for(cx, Duration::from_secs(20), |_, _| {
                mock.file("main.tex").is_some_and(|text| sent_once(&text))
            })
            .await;
        // Left as the steps after this one expect it, had the click missed.
        if !clicked_apply {
            self.with_window(cx, |shell, _, cx| {
                shell.update(cx, |shell, cx| shell.close_settings(cx))
            })?;
            if let Some(review) = &review {
                cx.update(|cx| review.update(cx, |review, cx| review.scenario_apply(cx)));
            }
            self.wait_for(cx, Duration::from_secs(30), |_, _| {
                mock.file("main.tex").is_some_and(|text| sent_once(&text))
            })
            .await;
        }
        cx.update(|cx| settings::update(cx, |s| s.overleaf_sync_mode = OverleafSyncMode::Live));
        steps.check(
            "in manual mode, resuming from Settings › Overleaf closes Settings and leaves Review changes in front, where its Apply takes the click",
            settled
                && paused
                && offered
                && resume.is_some()
                && review.is_some()
                && settings_closed
                && clicked_apply,
            json!({
                "settled": settled, "paused": paused, "offered": offered,
                "resume_at": format!("{resume:?}"), "review": review.is_some(),
                "settings_closed": settings_closed,
                "apply_at": format!("{apply:?}"), "clicked_apply": clicked_apply,
                "overleaf": mock.file("main.tex"),
            }),
        );
        Ok(())
    }
}

/// The centre of the button drawn as region `name`.
fn drawn(name: &str) -> Option<gpui::Point<gpui::Pixels>> {
    lattice_ui::parity::regions()
        .get(name)
        .and_then(|region| region.bounds)
        .map(|bounds| bounds.center())
}
