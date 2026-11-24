//! GPUI's overlays above a webview island on Windows
//! (`gpui-webview-windows-overlays.diff`): the WebView2's child window is
//! cut away wherever an overlay is drawn, so, judged from the screen as
//! it shows the window (`native::capture_client`):
//!
//! - the command palette opened over an HTML Preview hides the page where
//!   it is drawn;
//! - a modal dialog's backdrop dims the page's whole area, with the dialog
//!   above it;
//! - with both gone, the page shows again where they were.
//!
//! Without the cutouts the page stayed above them: the palette and the
//! dialog were drawn under it, where nobody could see or click them.

#![cfg(target_os = "windows")]

use std::{path::Path, time::Duration};

use anyhow::{Context as _, Result};
use gpui::{AsyncApp, Bounds, Pixels};
use serde_json::json;

use super::{Fixture, Steps};
use crate::{
    confirm::{self, Confirm},
    native,
    scenario::Run,
};

/// A page of one colour no part of Lattice's interface is drawn in.
const PAGE: &str = "<!doctype html><html><head><style>html, body { margin: 0; height: 100%; \
     background: rgb(30, 200, 60); }</style></head><body></body></html>";

pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Repair,
    rank: 61,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-webview/",
        "gpui/patches/gpui-webview/",
        "gpui/patches/gpui-fast/crates/gpui/src/fast/overlay_bounds.rs",
    ],
    run: |run, cx, steps, fixture| Box::pin(run.island_overlays(cx, steps, &fixture.root)),
};

/// The share of the island's pixels that are the page's colour, in the
/// screen's copy of the window.
fn page_share(image: &image::RgbaImage, island: Bounds<Pixels>, scale: f32) -> f32 {
    let device = |value: Pixels| (f32::from(value) * scale).round().max(0.) as u32;
    let (left, top) = (device(island.left()), device(island.top()));
    let right = device(island.right()).min(image.width());
    let bottom = device(island.bottom()).min(image.height());
    let (mut page, mut all) = (0usize, 0usize);
    for y in (top..bottom).step_by(4) {
        for x in (left..right).step_by(4) {
            let [r, g, b, _] = image.get_pixel(x, y).0;
            page += usize::from(r < 60 && g > 170 && b < 90);
            all += 1;
        }
    }
    if all == 0 { 0. } else { page as f32 / all as f32 }
}

impl Run {
    async fn island_overlays(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        root: &Path,
    ) -> Result<()> {
        let page = root.join("island-overlays.html");
        std::fs::write(&page, PAGE)?;
        let panel = self.open(cx, &page, None)?;
        let preview = cx
            .update(|cx| panel.read(cx).html_preview().cloned())
            .context("the HTML file opens in Preview")?;
        let island = cx.update(|cx| preview.read(cx).island().clone());
        let loaded = self
            .wait_for(cx, Duration::from_secs(20), |_, cx| {
                preview.read(cx).is_loaded() && island.read(cx).native_visible(cx)
            })
            .await;
        self.pause(cx, Duration::from_millis(600)).await;
        let bounds = cx.update(|cx| island.read(cx).bounds(cx));
        let scale = self.with_window(cx, |_, window, _| window.scale_factor())?;
        let share = |cx: &mut AsyncApp, this: &Self| -> Result<f32> {
            let image = native::capture_client(this.native(cx)?)?;
            Ok(bounds.map_or(0., |bounds| page_share(&image, bounds, scale)))
        };

        let shown = share(cx, self)?;
        self.open_palette_now(cx)?;
        self.pause(cx, Duration::from_millis(600)).await;
        let under_palette = share(cx, self)?;
        self.snapshot(cx, "island-overlay-palette").await;
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.close_overlays(window, cx))
        })?;
        steps.check(
            "on Windows the command palette opened over a webview island is drawn above the page",
            loaded && shown > 0.9 && under_palette < shown - 0.05,
            json!({ "loaded": loaded, "page_share": shown, "under_palette": under_palette, "bounds": format!("{bounds:?}") }),
        );

        let host = cx.update(confirm::install);
        cx.update(|cx| {
            confirm::ask(
                Confirm::new("Island overlay check").body("A modal dialog over the page."),
                cx,
            )
        });
        self.pause(cx, Duration::from_millis(800)).await;
        let under_dialog = share(cx, self)?;
        self.snapshot(cx, "island-overlay-dialog").await;
        let answered = self.with_window(cx, {
            let host = host.clone();
            move |_, window, cx| host.update(cx, |host, cx| host.accept(window, cx))
        })?;
        self.pause(cx, Duration::from_millis(800)).await;
        let after = share(cx, self)?;
        steps.check(
            "on Windows a modal dialog's backdrop dims a webview island's whole page, and the page shows again once it closes",
            under_dialog < 0.02 && answered && after > 0.9,
            json!({ "under_dialog": under_dialog, "answered": answered, "after": after }),
        );
        self.close_panel(cx, &panel).await?;
        Ok(())
    }
}
