//! The Agent's font size (Settings › Agent › Display): the page settings
//! draws the row; a new size draws the Agent's page larger at once, by its
//! own page zoom, while Settings' Synara pages keep their size; the page
//! built again after hibernating opens at it, and the Agent's own window
//! shows it at the same size; back at the default, the page is at its
//! natural size again.
//!
//! The size is read as the page draws it: its body's CSS font size times
//! the window pixels each of its CSS pixels covers (the native view's width
//! over the page's `innerWidth`), which a page zoom changes and a CSS font
//! size alone does not.

#![cfg(target_os = "macos")]

use std::{path::Path, time::Duration};

use anyhow::Result;
use gpui::{AnyWindowHandle, AsyncApp, Entity};
use lattice_settings::{AGENT_FONT_SIZE, SettingsTab};
use lattice_webview::{Island, IslandState};
use serde_json::{Value, json};

use super::{Run, Steps};
use crate::settings;

const PAGE_TEXT: &str = "(() => JSON.stringify({ inner: window.innerWidth, \
     font: parseFloat(getComputedStyle(document.body).fontSize) }))()";
/// The size the scenario picks, well apart from the default.
const LARGER: f32 = 18.;

impl Run {
    async fn agent_font_size(&self, cx: &mut AsyncApp, steps: &mut Steps, root: &Path) -> Result<()> {
        let before = cx.update(|cx| settings::get(cx).appearance.agent_font_size);
        cx.update(|cx| settings::update_appearance(cx, |a| a.agent_font_size = AGENT_FONT_SIZE));
        self.open(cx, &root.join("main.tex"), Some(1))?;
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.show_view("agent", true, window, cx))
        })?;
        let panel = cx.update(|cx| self.shell.read(cx).agent.panel.clone());
        let surface = cx.update(|cx| panel.read(cx).surface().clone());
        let Some(island) = cx.update(|cx| surface.read(cx).island().cloned()) else {
            steps.check(
                "Agent font size: a new size draws the Agent's page larger",
                false,
                json!({ "reason": "no island: the island pages are not built" }),
            );
            return Ok(());
        };
        self.wait_for(cx, Duration::from_secs(20), |_, cx| surface.read(cx).is_drawn())
            .await;
        let home = self.window;
        let natural = self.drawn_text(cx, &island).await;

        // Settings › Agent › Display draws the row.
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_settings(Some(SettingsTab::AgentDisplay), window, cx)
            })
        })?;
        self.pause(cx, Duration::from_millis(400)).await;
        let rows = cx.update(|cx| {
            self.shell
                .read(cx)
                .settings
                .as_ref()
                .map(|dialog| (dialog.read(cx).tab(), dialog.read(cx).drawn_rows()))
        });
        let row_drawn = rows.as_ref().is_some_and(|(tab, rows)| {
            *tab == SettingsTab::AgentDisplay && rows.contains(&"agent-font-size")
        });
        self.with_window(cx, |shell, _, cx| shell.update(cx, |shell, cx| shell.close_settings(cx)))?;
        steps.check(
            "Agent font size: Settings › Agent › Display draws its row",
            row_drawn,
            json!({ "rows": format!("{rows:?}") }),
        );

        // A larger size: the page draws larger at once, the editors and
        // Settings' Synara page untouched.
        let editor_size = cx.update(|cx| settings::get(cx).appearance.editor_font_size);
        cx.update(|cx| settings::update_appearance(cx, |a| a.agent_font_size = LARGER));
        self.pause(cx, Duration::from_millis(600)).await;
        let larger = self.drawn_text(cx, &island).await;
        let settings_zoom = cx.update(|cx| {
            let settings = self.shell.read(cx).agent.settings.read(cx);
            settings.island().map(|island| island.read(cx).page_zoom())
        });
        let editor_after = cx.update(|cx| settings::get(cx).appearance.editor_font_size);
        let expected = LARGER / AGENT_FONT_SIZE;
        steps.check(
            "Agent font size: a new size draws the Agent's page larger at once, apart from the editors and Settings' pages",
            scaled(&natural, &larger, expected)
                && editor_after == editor_size
                && settings_zoom.is_none_or(|zoom| zoom == 1.),
            json!({ "natural": natural, "larger": larger, "expected": expected, "settings_zoom": settings_zoom, "editor": [editor_size, editor_after] }),
        );

        // Reopened: hidden and torn down, the page built again opens at it.
        cx.update(|cx| lattice_webview::set_hibernate_after(Some(Duration::from_millis(500)), cx));
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.toggle_view("agent", window, cx))
        })?;
        let hibernated = self
            .wait_for(cx, Duration::from_secs(8), |_, cx| {
                *island.read(cx).state() == IslandState::Hibernated
            })
            .await;
        cx.update(|cx| lattice_webview::set_hibernate_after(None, cx));
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.show_view("agent", true, window, cx))
        })?;
        let rebuilt = self
            .wait_for(cx, Duration::from_secs(20), |_, cx| {
                surface.read(cx).is_drawn() && island.read(cx).is_ready()
            })
            .await;
        self.pause(cx, Duration::from_millis(400)).await;
        let reopened = self.drawn_text(cx, &island).await;
        steps.check(
            "Agent font size: the Agent's page built again after hibernating opens at the chosen size",
            hibernated && rebuilt && scaled(&natural, &reopened, expected),
            json!({ "hibernated": hibernated, "rebuilt": rebuilt, "reopened": reopened }),
        );

        // In its own window: the same page, at the same size.
        let _foreground = self.foreground(cx, "the Agent's font size in its own window").await?;
        let windows = cx.update(|cx| cx.windows().len());
        cx.update(|cx| panel.update(cx, |panel, cx| panel.detach(cx)));
        let opened = self
            .wait_for(cx, Duration::from_secs(10), |_, cx| {
                cx.windows().len() == windows + 1
                    && island.read(cx).window().window_id() != home.window_id()
                    && island.read(cx).native_visible(cx)
            })
            .await;
        self.pause(cx, Duration::from_millis(400)).await;
        let windowed = self.drawn_text(cx, &island).await;
        self.with_window(cx, |_, _, cx| panel.update(cx, |panel, cx| panel.dock(true, cx)))?;
        let docked = self
            .wait_for(cx, Duration::from_secs(10), |_, cx| {
                cx.windows().len() == windows
                    && island.read(cx).window().window_id() == home.window_id()
            })
            .await;
        steps.check(
            "Agent font size: the Agent's own window shows its page at the chosen size",
            opened && docked && scaled(&natural, &windowed, expected),
            json!({ "opened": opened, "docked": docked, "windowed": windowed }),
        );

        // Back at the default: its natural size.
        cx.update(|cx| settings::update_appearance(cx, |a| a.agent_font_size = AGENT_FONT_SIZE));
        self.pause(cx, Duration::from_millis(600)).await;
        let reset = self.drawn_text(cx, &island).await;
        steps.check(
            "Agent font size: back at the default, the Agent's page draws at its natural size",
            scaled(&natural, &reset, 1.),
            json!({ "natural": natural, "reset": reset }),
        );
        cx.update(|cx| settings::update_appearance(cx, |a| a.agent_font_size = before));
        Ok(())
    }

    /// The page's body text as drawn, in window pixels, with what it was
    /// read from.
    async fn drawn_text(&self, cx: &mut AsyncApp, island: &Entity<Island>) -> Value {
        let page = self.evaluate(cx, island, PAGE_TEXT).await;
        let (frame, window): (Option<f32>, AnyWindowHandle) = cx.update(|cx| {
            let island = island.read(cx);
            (
                island.native_frame(cx).map(|frame| f32::from(frame.size.width)),
                island.window(),
            )
        });
        let drawn = match (page["font"].as_f64(), page["inner"].as_f64(), frame) {
            (Some(font), Some(inner), Some(frame)) if inner > 0. => {
                Some(font * f64::from(frame) / inner)
            }
            _ => None,
        };
        json!({ "drawn": drawn, "page": page, "frame": frame, "window": format!("{:?}", window.window_id()) })
    }
}

/// `after` is drawn `ratio` times `before`'s size, within a CSS pixel's
/// rounding of the page's width.
fn scaled(before: &Value, after: &Value, ratio: f32) -> bool {
    match (before["drawn"].as_f64(), after["drawn"].as_f64()) {
        (Some(before), Some(after)) if before > 0. => {
            (after / before - f64::from(ratio)).abs() < 0.03
        }
        _ => false,
    }
}

pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Repair,
    rank: 92,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-gpui/src/agent/surface.rs",
        "gpui/crates/lattice-gpui/src/settings/agent_display_pane.rs",
        "gpui/crates/lattice-settings/src/model.rs",
    ],
    run: |run, cx, steps, fixture| Box::pin(run.agent_font_size(cx, steps, &fixture.root)),
};
