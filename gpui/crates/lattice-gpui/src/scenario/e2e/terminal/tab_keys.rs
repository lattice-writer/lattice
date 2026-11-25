//! Tab and ⇧Tab in the Terminal (lattice#1272): the shell's keys, as
//! xterm.js hands them to it in the Tauri app, not the window root's walk
//! over its tab stops, which took the keyboard out of the terminal.

use std::time::Duration;

use anyhow::Result;
use gpui::AsyncApp;
use serde_json::json;

use super::{super::super::Run, super::Steps, tail, terminal_text};
use crate::native;

impl Run {
    /// Tab completes the file name and ⇧Tab reaches the program as CSI Z,
    /// the keyboard staying in the terminal. The completed name is proven by
    /// running it, since fish draws the same name as a suggestion before
    /// any Tab.
    pub(super) async fn terminal_tab_keys(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
    ) -> Result<()> {
        self.type_text(cx, "cat -v refe").await?;
        self.pause(cx, Duration::from_millis(300)).await;
        native::send_chord(self.native(cx)?, '\t', false, false, false)?;
        self.pump_frame(cx);
        self.pause(cx, Duration::from_millis(500)).await;
        let tab_kept = self.terminal_has_keyboard(cx)?;
        self.focus_active_terminal(cx)?;
        self.type_text(cx, "\n").await?;
        let completed = self
            .wait_for(cx, Duration::from_secs(5), |shell, cx| {
                terminal_text(shell, cx).contains("@book{knuth1984")
            })
            .await;
        let completion = tail(&self.terminal_content(cx));
        self.terminal_shot(cx, "tab-completion").await?;
        self.type_text(cx, "cat -v\n").await?;
        self.pause(cx, Duration::from_millis(300)).await;
        native::send_chord(self.native(cx)?, '\t', false, false, true)?;
        self.pump_frame(cx);
        self.type_text(cx, "\n").await?;
        let back_tab = self
            .wait_for(cx, Duration::from_secs(5), |shell, cx| {
                terminal_text(shell, cx).contains("^[[Z")
            })
            .await;
        let shift_tab_kept = self.terminal_has_keyboard(cx)?;
        let back_tab_tail = tail(&self.terminal_content(cx));
        self.focus_active_terminal(cx)?;
        self.terminal_input(cx, "\u{3}");
        self.pause(cx, Duration::from_millis(300)).await;
        steps.check(
            "Tab completes in the shell and ⇧Tab reaches it, the keyboard staying in the terminal",
            completed && tab_kept && back_tab && shift_tab_kept,
            json!({
                "completed": completed, "tab_kept": tab_kept, "completion": completion,
                "back_tab": back_tab, "shift_tab_kept": shift_tab_kept, "back_tab_tail": back_tab_tail,
            }),
        );
        Ok(())
    }

    /// Bytes written to the active shell as if typed, for the keys the
    /// scenario's native keyboard has no code for (⌃C).
    fn terminal_input(&self, cx: &mut AsyncApp, text: &str) {
        cx.update(|cx| {
            if let Some(terminal) = self.active_terminal(cx) {
                terminal.update(cx, |terminal, _| terminal.input(text.as_bytes().to_vec()));
            }
        });
    }

    fn terminal_has_keyboard(&self, cx: &mut AsyncApp) -> Result<bool> {
        self.with_window(cx, |shell, window, cx| {
            shell
                .read(cx)
                .terminal()
                .read(cx)
                .active_screen_focus(cx)
                .is_some_and(|focus| focus.is_focused(window))
        })
    }
}
