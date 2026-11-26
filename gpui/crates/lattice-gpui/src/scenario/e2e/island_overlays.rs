//! GPUI's overlays above a webview island on Windows
//! (`gpui-webview-windows-overlays.diff`): the WebView2's child window is
//! cut away wherever an overlay is drawn, so, judged from the screen as
//! it shows the window (`native::capture_client`):
//!
//! - the command palette opened over an HTML Preview hides the page where
//!   it is drawn;
//! - a modal dialog's backdrop dims the page's whole area, with the dialog
//!   above it;
//! - a panel's menu, which has no scrim, hides the page under the menu
//!   alone, and the page shows all around it;
//! - with both gone, the page shows again where they were;
//! - at an interface size of 125% the page still covers its element and
//!   the menu's hole is still where the menu is drawn.
//!
//! Without the cutouts the page stayed above them: the palette and the
//! dialog were drawn under it, where nobody could see or click them.

#![cfg(target_os = "windows")]

use std::{cell::RefCell, path::Path, rc::Rc, time::Duration};

use anyhow::{Context as _, Result};
use gpui::{AsyncApp, Bounds, Entity, Pixels, px};
use serde_json::json;

use super::{Fixture, Steps};
use crate::{
    confirm::{self, Confirm},
    editor_panel::EditorPanel,
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

/// Of the island's pixels, the share that are the page's colour inside the
/// holes and outside them, in the screen's copy of the window (device
/// pixels at the window's scale factor, the interface zoom included).
fn page_shares(
    image: &image::RgbaImage,
    island: Bounds<Pixels>,
    holes: &[Bounds<Pixels>],
    scale: f32,
) -> (f32, f32) {
    let device = |value: Pixels| (f32::from(value) * scale).round().max(0.) as u32;
    let (left, top) = (device(island.left()), device(island.top()));
    let right = device(island.right()).min(image.width());
    let bottom = device(island.bottom()).min(image.height());
    let [mut inside, mut inside_all, mut outside, mut outside_all] = [0usize; 4];
    for y in (top..bottom).step_by(4) {
        for x in (left..right).step_by(4) {
            let [r, g, b, _] = image.get_pixel(x, y).0;
            let page = usize::from(r < 60 && g > 170 && b < 90);
            let at = gpui::point(px(x as f32 / scale), px(y as f32 / scale));
            // A pixel on a hole's edge may go either way.
            let near = |hole: &Bounds<Pixels>| hole.dilate(px(2.)).contains(&at);
            let within = |hole: &Bounds<Pixels>| hole.dilate(px(-2.)).contains(&at);
            if holes.iter().any(within) {
                inside += page;
                inside_all += 1;
            } else if !holes.iter().any(near) {
                outside += page;
                outside_all += 1;
            }
        }
    }
    let share = |page: usize, all: usize| {
        if all == 0 {
            0.
        } else {
            page as f32 / all as f32
        }
    };
    (share(inside, inside_all), share(outside, outside_all))
}

/// The share of the island's pixels that are the page's colour.
fn page_share(image: &image::RgbaImage, island: Bounds<Pixels>, scale: f32) -> f32 {
    page_shares(image, island, &[], scale).1
}

type Drawn = Rc<RefCell<Vec<Bounds<Pixels>>>>;

impl Run {
    fn island_scale(&self, cx: &mut AsyncApp) -> Result<f32> {
        self.with_window(cx, |_, window, _| window.scale_factor())
    }

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
        let scale = self.island_scale(cx)?;
        let share = |cx: &mut AsyncApp, this: &Self| -> Result<f32> {
            let image = native::capture_client(this.native(cx)?)?;
            Ok(bounds.map_or(0., |bounds| page_share(&image, bounds, scale)))
        };

        // What the window draws above its root view, as the cutouts see it.
        let overlays: Drawn = Rc::new(RefCell::new(Vec::new()));
        let observer: Rc<gpui::OverlayBoundsObserver> = Rc::new({
            let overlays = overlays.clone();
            move |bounds: &[Bounds<Pixels>]| *overlays.borrow_mut() = bounds.to_vec()
        });
        self.with_window(cx, |_, window, _| window.observe_overlay_bounds(&observer))?;
        let drawn = |overlays: &Drawn| format!("{:?}", overlays.borrow());
        // Start with nothing drawn over the page but its own controls: what
        // an earlier module left open closed, and its exit played out.
        let found = drawn(&overlays);
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.close_overlays(window, cx))
        })?;
        let cleared = match bounds {
            Some(island) => self.wait_for_island_clear(cx, &overlays, island).await,
            None => false,
        };
        let run = std::process::id();
        self.snapshot(cx, &format!("island-overlay-start-{run}"))
            .await;

        let shown = share(cx, self)?;
        let overlays_before = drawn(&overlays);
        self.open_palette_now(cx)?;
        self.pause(cx, Duration::from_millis(600)).await;
        let under_palette = share(cx, self)?;
        self.snapshot(cx, &format!("island-overlay-palette-{run}"))
            .await;
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.close_overlays(window, cx))
        })?;
        steps.check(
            "on Windows the command palette opened over a webview island is drawn above the page",
            loaded && cleared && shown > 0.9 && under_palette < shown - 0.05,
            json!({ "loaded": loaded, "page_share": shown, "under_palette": under_palette, "bounds": format!("{bounds:?}"), "overlays_found": found, "cleared": cleared, "overlays_before": overlays_before }),
        );
        self.pause(cx, Duration::from_millis(600)).await;

        // A menu, with no scrim: a hole where it is drawn, the page around it.
        let menu = self
            .island_menu_cutout(cx, &panel, &overlays, &format!("island-overlay-menu-{run}"))
            .await?;
        steps.check(
            "on Windows a panel's menu over a webview island cuts the page away under the menu alone",
            menu.passed(),
            menu.detail(),
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
        let overlays_dialog = drawn(&overlays);
        self.snapshot(cx, &format!("island-overlay-dialog-{run}"))
            .await;
        let answered = self.with_window(cx, {
            let host = host.clone();
            move |_, window, cx| host.update(cx, |host, cx| host.accept(window, cx))
        })?;
        self.pause(cx, Duration::from_millis(800)).await;
        let after = share(cx, self)?;
        steps.check(
            "on Windows a modal dialog's backdrop dims a webview island's whole page, and the page shows again once it closes",
            under_dialog < 0.02 && answered && after > 0.9,
            json!({ "under_dialog": under_dialog, "answered": answered, "after": after, "overlays_dialog": overlays_dialog, "overlays_after": drawn(&overlays) }),
        );

        // At another interface size the page still covers its element, and
        // the menu's hole is still where the menu is drawn.
        const ZOOM: f32 = 1.25;
        cx.update(|cx| crate::settings::update_appearance(cx, |a| a.interface_scale = ZOOM));
        let zoomed = self.wait_for_zoom(cx, ZOOM).await;
        self.pause(cx, Duration::from_millis(800)).await;
        let zoomed_bounds = cx.update(|cx| island.read(cx).bounds(cx));
        let zoomed_scale = self.island_scale(cx)?;
        let zoomed_share = {
            let image = native::capture_client(self.native(cx)?)?;
            zoomed_bounds.map_or(0., |bounds| page_share(&image, bounds, zoomed_scale))
        };
        let zoomed_menu = self
            .island_menu_cutout(
                cx,
                &panel,
                &overlays,
                &format!("island-overlay-menu-zoomed-{run}"),
            )
            .await?;
        cx.update(|cx| crate::settings::update_appearance(cx, |a| a.interface_scale = 1.0));
        self.wait_for_zoom(cx, 1.0).await;
        steps.check(
            "on Windows at an interface size of 125% a webview island's page covers its element, and a menu's hole is where the menu is drawn",
            zoomed && zoomed_share > 0.9 && zoomed_menu.passed(),
            json!({ "zoomed": zoomed, "scale": zoomed_scale, "bounds": format!("{zoomed_bounds:?}"), "page_share": zoomed_share, "menu": zoomed_menu.detail() }),
        );
        self.close_panel(cx, &panel).await?;
        Ok(())
    }

    /// Wait until no overlay covers a tenth of the island or more: what
    /// was open has closed and its exit has played out.
    async fn wait_for_island_clear(
        &self,
        cx: &mut AsyncApp,
        overlays: &Drawn,
        island: Bounds<Pixels>,
    ) -> bool {
        let overlays = overlays.clone();
        self.wait_for(cx, Duration::from_secs(5), move |_, _| {
            let area = |b: Bounds<Pixels>| f32::from(b.size.width) * f32::from(b.size.height);
            overlays
                .borrow()
                .iter()
                .all(|overlay| area(overlay.intersect(&island)) < area(island) * 0.1)
        })
        .await
    }

    /// Open the ⋯ menu of the island's panel, which drops over the page,
    /// and read the screen: the page's colour gone where the overlays are
    /// drawn, and still there everywhere else on the island.
    async fn island_menu_cutout(
        &self,
        cx: &mut AsyncApp,
        panel: &Entity<EditorPanel>,
        overlays: &Drawn,
        shot: &str,
    ) -> Result<MenuCutout> {
        lattice_ui::parity::enable();
        let island = cx.update(|cx| {
            panel
                .read(cx)
                .html_preview()
                .and_then(|preview| preview.read(cx).island().read(cx).bounds(cx))
        });
        let strip = lattice_ui::parity::regions()
            .get("strip/island-overlays.html")
            .and_then(|region| region.bounds);
        let (Some(island), Some(strip)) = (island, strip) else {
            return Ok(MenuCutout {
                island,
                strip,
                ..Default::default()
            });
        };
        let native = self.native(cx)?;
        // The ⋯ ends the strip: 24px wide, 6px in from its right edge.
        let dots = gpui::point(strip.right() - px(18.), strip.center().y);
        native::send_mouse(native, native::Mouse::Moved, dots)?;
        native::send_click(native, dots)?;
        self.pause(cx, Duration::from_millis(700)).await;
        let holes: Vec<_> = overlays
            .borrow()
            .iter()
            .map(|overlay| overlay.intersect(&island))
            .filter(|overlay| !overlay.is_empty())
            .collect();
        let scale = self.island_scale(cx)?;
        let image = native::capture_client(native)?;
        let (inside, outside) = page_shares(&image, island, &holes, scale);
        self.snapshot(cx, shot).await;
        // `close_overlays` leaves a panel's menu up, and a menu left open
        // would cover the page in the steps after this one: Escape closes
        // it, and a second click on the ⋯ if Escape went elsewhere.
        self.dispatch_keys(cx, "escape")?;
        let mut closed = self.wait_for_island_clear(cx, overlays, island).await;
        if !closed {
            native::send_click(native, dots)?;
            closed = self.wait_for_island_clear(cx, overlays, island).await;
        }
        Ok(MenuCutout {
            island: Some(island),
            strip: Some(strip),
            holes,
            inside,
            outside,
            closed,
        })
    }
}

/// What a menu over the island left of the page.
#[derive(Default)]
struct MenuCutout {
    island: Option<Bounds<Pixels>>,
    strip: Option<Bounds<Pixels>>,
    holes: Vec<Bounds<Pixels>>,
    inside: f32,
    outside: f32,
    closed: bool,
}

impl MenuCutout {
    fn passed(&self) -> bool {
        let island = self.island.unwrap_or_default();
        let area = |b: &Bounds<Pixels>| f32::from(b.size.width) * f32::from(b.size.height);
        // A hole bigger than the island's controls that leaves some of the
        // island around it: at 125% a narrow island can be 86% menu.
        let menu = self
            .holes
            .iter()
            .any(|hole| area(hole) > 4000. && area(&island) - area(hole) > 4000.);
        menu && self.inside < 0.05 && self.outside > 0.95 && self.closed
    }

    fn detail(&self) -> serde_json::Value {
        json!({
            "island": format!("{:?}", self.island),
            "strip": format!("{:?}", self.strip),
            "holes": format!("{:?}", self.holes),
            "page_inside_holes": self.inside,
            "closed": self.closed,
            "page_outside_holes": self.outside,
        })
    }
}
