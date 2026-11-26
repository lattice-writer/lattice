//! The confirmation dialog takes the pointer and Return where it is drawn,
//! asked from Settings as Settings › Plugins, Overleaf and Appearance's
//! Remove theme ask theirs (`confirm.rs`, `Shell::window_overlays`): a
//! click on Cancel or on the answer reaches that button, not the Settings
//! page under the dialog, and Return answers as Cancel does.

use std::{cell::Cell, rc::Rc, time::Duration};

use anyhow::Result;
use gpui::AsyncApp;
use lattice_settings::SettingsTab;
use serde_json::json;

use super::{Fixture, Steps};
use crate::{
    confirm::{self, Confirm},
    native,
    scenario::Run,
};

/// What became of one question.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Outcome {
    #[default]
    Open,
    Confirmed,
    Cancelled,
}

impl Run {
    /// Ask a question over Settings and give back what becomes of it.
    async fn ask_over_settings(&self, cx: &mut AsyncApp) -> Rc<Cell<Outcome>> {
        lattice_ui::parity::forget("confirm/");
        let outcome = Rc::new(Cell::new(Outcome::Open));
        let (confirmed, cancelled) = (outcome.clone(), outcome.clone());
        cx.update(|cx| {
            confirm::ask(
                Confirm::new("Remove Fixture?")
                    .body("A question asked from Settings.")
                    .confirm_label("Remove")
                    .destructive()
                    .on_confirm(move |_, _| confirmed.set(Outcome::Confirmed))
                    .on_cancel(move |_| cancelled.set(Outcome::Cancelled)),
                cx,
            )
        });
        self.pause(cx, Duration::from_millis(600)).await;
        outcome
    }

    /// The centre of the dialog's button drawn as `confirm/<name>`.
    fn confirm_button(&self, name: &str) -> Option<gpui::Point<gpui::Pixels>> {
        lattice_ui::parity::regions()
            .get(format!("confirm/{name}").as_str())
            .and_then(|region| region.bounds)
            .map(|bounds| bounds.center())
    }

    /// Whether the question is still up and Settings still open.
    fn confirm_state(&self, cx: &mut AsyncApp) -> (bool, bool) {
        cx.update(|cx| {
            (
                confirm::is_pending(cx),
                self.shell.read(cx).settings_open(),
            )
        })
    }

    async fn confirm_clicks(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        _: &Fixture,
    ) -> Result<()> {
        lattice_ui::parity::enable();
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_settings(Some(SettingsTab::Appearance), window, cx)
            })
        })?;
        self.pause(cx, Duration::from_millis(600)).await;

        for (name, expected) in [
            ("cancel", Outcome::Cancelled),
            ("answer", Outcome::Confirmed),
        ] {
            let outcome = self.ask_over_settings(cx).await;
            let at = self.confirm_button(name);
            if let Some(at) = at {
                self.click_at(cx, at)?;
            }
            self.pause(cx, Duration::from_millis(600)).await;
            let (asking, settings) = self.confirm_state(cx);
            steps.check(
                &format!(
                    "a click on the {name} button of a question asked over Settings answers it, and Settings stays as it was"
                ),
                at.is_some() && outcome.get() == expected && !asking && settings,
                json!({
                    "at": format!("{at:?}"),
                    "outcome": format!("{:?}", outcome.get()),
                    "still_asking": asking,
                    "settings_open": settings,
                }),
            );
            if asking {
                cx.update(|cx| confirm::install(cx).update(cx, |host, cx| host.dismiss(cx)));
            }
        }

        let outcome = self.ask_over_settings(cx).await;
        self.dispatch_keys(cx, "enter")?;
        self.pause(cx, Duration::from_millis(600)).await;
        let (asking, settings) = self.confirm_state(cx);
        steps.check(
            "Return on a question asked over Settings answers as Cancel does, as the dialog starts on Cancel",
            outcome.get() == Outcome::Cancelled && !asking && settings,
            json!({ "outcome": format!("{:?}", outcome.get()), "still_asking": asking, "settings_open": settings }),
        );
        if asking {
            cx.update(|cx| confirm::install(cx).update(cx, |host, cx| host.dismiss(cx)));
        }

        self.with_window(cx, |shell, _, cx| {
            shell.update(cx, |shell, cx| shell.close_settings(cx))
        })?;
        self.pause(cx, Duration::from_millis(300)).await;

        self.confirm_long_delete_list(cx, steps).await?;
        Ok(())
    }

    /// A destructive remote-delete confirmation with many long file paths keeps
    /// both Cancel and the confirm button inside the viewport while the file
    /// list scrolls.
    async fn confirm_long_delete_list(&self, cx: &mut AsyncApp, steps: &mut Steps) -> Result<()> {
        lattice_ui::parity::forget("confirm/");
        let paths: Vec<String> = (1..=40)
            .map(|i| {
                format!(
                    "figures/data/formal_plot_data_20260730/F05_riskguard_vs_scenario_greedy/file_{i:02}.csv"
                )
            })
            .collect();
        let body = format!(
            "These files were deleted from the local project, but are still on Overleaf. Even if you remove them now, Overleaf's history will keep them.\n\n{}",
            paths.join("\n"),
        );
        let outcome = Rc::new(Cell::new(Outcome::Open));
        let (confirmed, cancelled) = (outcome.clone(), outcome.clone());
        cx.update(|cx| {
            confirm::ask(
                Confirm::new("Remove 40 files from the Overleaf project?")
                    .body(body)
                    .confirm_label("Delete on Overleaf too")
                    .destructive()
                    .on_confirm(move |_, _| confirmed.set(Outcome::Confirmed))
                    .on_cancel(move |_| cancelled.set(Outcome::Cancelled)),
                cx,
            )
        });
        self.pause(cx, Duration::from_millis(600)).await;

        let viewport_height =
            self.with_window(cx, |_, window, _| f32::from(window.viewport_size().height))?;
        let regions = lattice_ui::parity::regions();
        // Read `confirm/body` before scrolling: the probe sits inside the scroll container.
        let bounds_of = |name: &str| regions.get(name).and_then(|region| region.bounds);
        let in_viewport = |bounds: Option<gpui::Bounds<gpui::Pixels>>| {
            bounds.is_some_and(|rect| {
                f32::from(rect.top()) >= 0.0 && f32::from(rect.bottom()) <= viewport_height
            })
        };
        let cancel_bounds = bounds_of("confirm/cancel");
        let answer_bounds = bounds_of("confirm/answer");
        let body_bounds = bounds_of("confirm/body");
        let cancel_visible = in_viewport(cancel_bounds);
        let answer_visible = in_viewport(answer_bounds);

        let (max_scroll, scroll_before) = cx.update(|cx| {
            let host = confirm::install(cx);
            let host = host.read(cx);
            (host.body_max_scroll(), host.body_scroll_offset())
        });
        if let Some(rect) = body_bounds {
            let native = self.native(cx)?;
            let at = rect.center();
            native::send_mouse(native, native::Mouse::Moved, at)?;
            for _ in 0..5 {
                native::send_wheel(native, at, -120)?;
                self.pause(cx, Duration::from_millis(80)).await;
            }
            self.pause(cx, Duration::from_millis(300)).await;
        }
        let scroll_after = cx.update(|cx| confirm::install(cx).read(cx).body_scroll_offset());
        self.drop_shot(cx, "confirm-long-delete").await;

        if let Some(rect) = cancel_bounds {
            self.click_at(cx, rect.center())?;
            self.pause(cx, Duration::from_millis(600)).await;
        }
        let still_asking = cx.update(|cx| confirm::is_pending(cx));
        steps.check(
            "a delete confirmation with 40 files keeps Cancel and confirm inside the viewport and scrolls its file list",
            cancel_visible
                && answer_visible
                && max_scroll > 0.0
                && scroll_after < scroll_before
                && outcome.get() == Outcome::Cancelled
                && !still_asking,
            json!({
                "viewport_height": viewport_height,
                "cancel_bounds": format!("{cancel_bounds:?}"),
                "answer_bounds": format!("{answer_bounds:?}"),
                "body_bounds": format!("{body_bounds:?}"),
                "max_scroll": max_scroll,
                "scroll_before": scroll_before,
                "scroll_after": scroll_after,
                "outcome": format!("{:?}", outcome.get()),
                "still_asking": still_asking,
            }),
        );
        if still_asking {
            cx.update(|cx| confirm::install(cx).update(cx, |host, cx| host.dismiss(cx)));
        }
        Ok(())
    }
}

/// This file's registration as the e2e module `confirm_clicks`
/// (`registry.rs`).
pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Build,
    rank: 96,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-gpui/src/confirm.rs",
        "gpui/crates/lattice-gpui/src/overleaf/remote_deletes.rs",
        "gpui/crates/lattice-gpui/src/shell/layers.rs",
        "gpui/crates/lattice-gpui/src/shell/render.rs",
        "gpui/crates/lattice-ui/src/components/overlays.rs",
        "gpui/crates/lattice-ui/src/components/sheet.rs",
    ],
    run: |run, cx, steps, fixture| Box::pin(run.confirm_clicks(cx, steps, fixture)),
};
