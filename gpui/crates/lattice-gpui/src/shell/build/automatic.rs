//! When an automatic build runs: now, or once typing pauses while the
//! live preview paints the edits (`quiet-builds.ts`), and Typst's live
//! build after each pause in typing.

use std::time::Duration;

use gpui::{App, Context, Window};

use super::BuildRequest;
use crate::shell::Shell;

/// How long typing in a `.typ` file pauses before its live build runs.
const TYPST_LIVE_PAUSE: Duration = Duration::from_millis(150);

impl Shell {
    /// An automatic build asks to run: false when it should run now, as it
    /// does without a live preview; otherwise it runs once typing pauses,
    /// quietly if the live preview is still live (`quiet-builds.ts`'s
    /// `defer`).
    pub(in crate::shell) fn defer_automatic_build(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.live_on_tex(cx) {
            log::info!(
                "automatic build runs now: live {} in {:?}",
                self.live_preview.read(cx).is_live(),
                self.active_editor()
                    .map(|editor| editor.read(cx).relative().to_string())
            );
            self.quiet_builds.cancel();
            return false;
        }
        log::info!("automatic build waits for typing to pause");
        let now = std::time::Instant::now();
        self.quiet_builds.saved(now);
        if self.quiet_builds.due(now) {
            self.fire_quiet_build(window, cx);
        } else {
            self.wait_for_typing_pause(window, cx);
        }
        true
    }

    /// rtex draws .tex files only: a .bib or a figure the writer saves is
    /// seen in the next build alone.
    pub(super) fn live_on_tex(&self, cx: &App) -> bool {
        self.live_preview.read(cx).is_live()
            && self.active_editor().is_some_and(|editor| {
                editor
                    .read(cx)
                    .relative()
                    .to_ascii_lowercase()
                    .ends_with(".tex")
            })
    }

    fn wait_for_typing_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let timer = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(crate::live_preview::quiet_builds::IDLE)
                .await;
            this.update_in(cx, |this, window, cx| {
                // Typed since the save: another save restarts the wait when
                // they pause. Checked anyway, as a completion menu holds the
                // save off.
                let typing = this
                    .active_editor()
                    .is_some_and(|editor| editor.read(cx).is_dirty(cx));
                let due = this.quiet_builds.due(std::time::Instant::now());
                if typing && this.live_on_tex(cx) && !due {
                    this.wait_for_typing_pause(window, cx);
                } else {
                    this.fire_quiet_build(window, cx);
                }
            })
            .ok();
        });
        self.quiet_builds.set_timer(timer);
    }

    /// An edit to a `.typ` file: its live build runs once typing pauses for
    /// [`TYPST_LIVE_PAUSE`]; each edit before then restarts the wait.
    pub(in crate::shell) fn schedule_typst_live(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.typst_live = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(TYPST_LIVE_PAUSE).await;
            this.update_in(cx, |this, window, cx| {
                this.typst_live = None;
                this.run_build(BuildRequest::typst_live(), window, cx);
            })
            .ok();
        }));
    }

    fn fire_quiet_build(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let quiet = self.live_on_tex(cx);
        log::info!("the waiting build runs, quiet: {quiet}");
        self.quiet_builds.cancel();
        let request = BuildRequest {
            quiet,
            ..BuildRequest::automatic()
        };
        self.run_build(request, window, cx);
    }
}
