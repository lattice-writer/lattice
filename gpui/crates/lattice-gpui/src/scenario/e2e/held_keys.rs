//! The e2e checks of keys held down, as the Tauri app runs them
//! (`src/app/use-app-commands.ts`; here `shell/commands/held_keys.rs`): a
//! key the keyboard repeats runs an app command once a press, and again only
//! a command that asks to (history, the build's problems, the interface
//! size). Held for about a second, ⌘K opens the palette once and it stays
//! open, ⌘W closes one tab, and ⌘= steps the interface size more than once.
//! A webview island's forwarded repeats follow the same rule (`islands.rs`).

use std::{path::Path, time::Duration};

use anyhow::Result;
use gpui::AsyncApp;
use serde_json::json;

use super::Steps;
use crate::scenario::Run;

/// The keyboard's default delay before a held key repeats, and the time
/// between repeats (macOS's, a little quicker than the slowest setting).
const REPEAT_DELAY: Duration = Duration::from_millis(250);
const REPEAT_EVERY: Duration = Duration::from_millis(60);
/// About a second of holding.
const REPEATS: usize = 12;

impl Run {
    /// `character` with ⌘ / Ctrl held down for about a second: one press,
    /// then the keyboard's repeats, then the key let go. `observe` reads the
    /// window after the press and after each repeat.
    async fn hold_key<T>(
        &self,
        cx: &mut AsyncApp,
        character: char,
        mut observe: impl FnMut(&mut AsyncApp) -> T,
    ) -> Result<Vec<T>> {
        // The menu bar's key equivalents (File › Close Tab's ⌘W) see a
        // key only in the active app's key window.
        #[cfg(target_os = "macos")]
        let _front = self.foreground(cx, "held keys").await?;
        let mut seen = Vec::new();
        for repeat in std::iter::once(false).chain(std::iter::repeat_n(true, REPEATS)) {
            self.key_down(cx, character, repeat)?;
            self.pump_frame(cx);
            self.pause(cx, Duration::from_millis(30)).await;
            seen.push(observe(cx));
            self.pause(cx, if repeat { REPEAT_EVERY } else { REPEAT_DELAY })
                .await;
        }
        #[cfg(target_os = "macos")]
        crate::native::send_key_up(self.native(cx)?, character, true)?;
        self.pause(cx, Duration::from_millis(100)).await;
        Ok(seen)
    }

    /// One keyDown of `character` with the shortcut modifier: on macOS
    /// through the application, as the keyboard sends it (`isARepeat` for a
    /// repeat); elsewhere as the window's platform reports it.
    fn key_down(&self, cx: &mut AsyncApp, character: char, repeat: bool) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            crate::native::send_key_down(self.native(cx)?, character, true, repeat)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let keystroke = gpui::Keystroke {
                modifiers: gpui::Modifiers {
                    control: true,
                    ..Default::default()
                },
                key: character.to_string(),
                key_char: None,
            };
            self.with_window(cx, move |_, window, cx| {
                window.dispatch_event(
                    gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                        keystroke,
                        is_held: repeat,
                        prefer_character_input: false,
                    }),
                    cx,
                );
            })
        }
    }

    async fn held_keys(&self, cx: &mut AsyncApp, steps: &mut Steps, root: &Path) -> Result<()> {
        let names = ["held-a.tex", "held-b.tex", "held-c.tex"];
        for name in names {
            std::fs::write(root.join(name), format!("% {name}\n"))?;
        }
        let result = self.held_keys_steps(cx, steps, root, &names).await;
        cx.update(|cx| {
            self.shell.update(cx, |shell, cx| {
                for name in names {
                    shell.pin_document(name, false, cx);
                }
                let views: Vec<String> = names
                    .iter()
                    .filter_map(|name| shell.view_of_document(name, cx))
                    .collect();
                shell.workspace().clone().update(cx, |workspace, cx| workspace.close(&views, cx));
            })
        });
        self.pause(cx, Duration::from_millis(300)).await;
        for name in names {
            std::fs::remove_file(root.join(name)).ok();
        }
        result
    }

    async fn held_keys_steps(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        root: &Path,
        names: &[&str],
    ) -> Result<()> {
        let mut editors = Vec::new();
        for name in names {
            editors.push(self.open(cx, &root.join(name), Some(1))?);
            self.pause(cx, Duration::from_millis(200)).await;
            // Kept, so the next file opens a tab of its own rather than
            // taking this preview's place.
            cx.update(|cx| self.shell.update(cx, |shell, cx| shell.pin_document(name, true, cx)));
        }
        let typed_before = self.editor_text(cx, &editors[2]);

        // ⌘K held: the palette opens on the press and stays open.
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.close_overlays(window, cx))
        })?;
        self.pause(cx, Duration::from_millis(200)).await;
        let shell = self.shell.clone();
        let palette = self
            .hold_key(cx, 'k', |cx| cx.update(|cx| shell.read(cx).palette_view().is_some()))
            .await?;
        let opened = palette.windows(2).filter(|pair| !pair[0] && pair[1]).count()
            + usize::from(palette.first() == Some(&true));
        steps.check(
            "⌘K held for a second opens the palette once, and it stays open",
            opened == 1 && palette.iter().all(|open| *open),
            json!({ "open_after_each_key": palette }),
        );
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.close_overlays(window, cx))
        })?;
        self.pause(cx, Duration::from_millis(200)).await;

        // ⌘W held: one tab closes, the window and the other tabs stay.
        self.open(cx, &root.join(names[2]), Some(1))?;
        self.pause(cx, Duration::from_millis(300)).await;
        let open_tabs = |cx: &mut AsyncApp| {
            cx.update(|cx| {
                let shell = shell.read(cx);
                names
                    .iter()
                    .filter(|name| shell.view_of_document(name, cx).is_some())
                    .count()
            })
        };
        let before = open_tabs(cx);
        let during = self.hold_key(cx, 'w', open_tabs).await?;
        let after = open_tabs(cx);
        let window_open = self.with_window(cx, |_, _, _| ()).is_ok();
        steps.check(
            "⌘W held for a second closes one tab",
            before == names.len() && after == before - 1 && window_open,
            json!({ "before": before, "after_each_key": during, "after": after, "window_open": window_open }),
        );

        // ⌘= held: the interface grows a step at a time while it is held.
        let scale = |cx: &mut AsyncApp| cx.update(|cx| crate::interface_zoom::scale(cx));
        let start = scale(cx);
        let sizes = self.hold_key(cx, '=', scale).await?;
        let mut steps_taken = sizes.clone();
        steps_taken.dedup();
        let grew = steps_taken.iter().filter(|size| **size > start).count();
        steps.check(
            "⌘= held for a second steps the interface size more than once",
            grew > 1,
            json!({ "start": start, "after_each_key": sizes }),
        );
        cx.update(|cx| {
            crate::settings::update_appearance(cx, |appearance| appearance.interface_scale = start)
        });
        self.pause(cx, Duration::from_millis(300)).await;

        // A held app key is the app's: nothing reached the document.
        let typed_after = self.editor_text(cx, &editors[2]);
        steps.check(
            "no held key typed into the document",
            typed_after == typed_before,
            json!({ "before": typed_before, "after": typed_after }),
        );
        Ok(())
    }
}

/// This file's registration as the e2e module `held_keys` (`registry.rs`).
pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Terminal,
    rank: 97,
    needs: &[],
    setup: false,
    covers: &["gpui/crates/lattice-gpui/src/shell/commands/held_keys.rs"],
    run: |run, cx, steps, fixture| Box::pin(run.held_keys(cx, steps, &fixture.root)),
};
