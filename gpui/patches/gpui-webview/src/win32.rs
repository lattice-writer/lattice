//! Keeping GPUI's overlays above a WebView2 on Windows.
//!
//! Wry hosts a WebView2 in a child window of its own, which sits above
//! everything GPUI presents into the window: gpui-fast's DirectComposition
//! tree can order only a visual, and wry creates no composition controller
//! to give it one. The frame under the child window holds GPUI's overlays
//! already, though, so the child window is cut away, with a window region,
//! wherever the window's overlays are drawn
//! ([`gpui::Window::observe_overlay_bounds`]) and outside the clip of the
//! element showing it, so menus, popovers, dialogs and tooltips show above
//! the page, and presses on them reach GPUI, as with composition on macOS.

use std::{cell::RefCell, rc::Rc};

use gpui::{Bounds, OverlayBoundsObserver, Pixels, Window};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, HRGN, RGN_DIFF, SetWindowRgn},
};
use wry::WebViewExtWindows as _;

/// The child window's region, kept to the visible part of the page minus
/// the window's overlays.
pub(crate) struct OverlayCutouts {
    state: Rc<RefCell<Cutouts>>,
    _observer: Rc<OverlayBoundsObserver>,
}

#[derive(Default)]
struct Cutouts {
    hwnd: Option<HWND>,
    /// The page's bounds, where the child window is placed, and the part of
    /// them its element shows, in window space.
    document: Bounds<Pixels>,
    visible: Bounds<Pixels>,
    scale_factor: f32,
    overlays: Vec<Bounds<Pixels>>,
    /// The rectangles last cut away, in the child window's pixels: none
    /// means the region was cleared.
    applied: Option<Vec<[i32; 4]>>,
}

impl OverlayCutouts {
    pub(crate) fn new(webview: &wry::WebView, window: &Window) -> Self {
        let mut hwnd = HWND::default();
        // SAFETY: the controller is alive as long as `webview` is.
        let hwnd = unsafe { webview.controller().ParentWindow(&mut hwnd) }
            .ok()
            .map(|()| hwnd)
            .filter(|hwnd| !hwnd.is_invalid());
        if hwnd.is_none() {
            log::warn!("WebView2's child window is unknown: GPUI's overlays stay under the page");
        }
        let state = Rc::new(RefCell::new(Cutouts {
            hwnd,
            scale_factor: window.scale_factor() / window.interface_zoom(),
            ..Default::default()
        }));
        let observer: Rc<OverlayBoundsObserver> = Rc::new({
            let state = state.clone();
            move |overlays: &[Bounds<Pixels>]| {
                let mut state = state.borrow_mut();
                state.overlays = overlays.to_vec();
                state.apply();
            }
        });
        window.observe_overlay_bounds(&observer);
        Self {
            state,
            _observer: observer,
        }
    }

    /// Note the display's scale, which wry places the child window by.
    pub(crate) fn set_scale_factor(&self, scale_factor: f32) {
        self.state.borrow_mut().scale_factor = scale_factor;
    }

    /// Note where the page is placed and the part of it shown.
    pub(crate) fn set_geometry(&self, document: Bounds<Pixels>, visible: Bounds<Pixels>) {
        let mut state = self.state.borrow_mut();
        state.document = document;
        state.visible = visible;
        state.apply();
    }
}

impl Cutouts {
    fn apply(&mut self) {
        let Some(hwnd) = self.hwnd else {
            return;
        };
        let device = |bounds: Bounds<Pixels>| {
            let origin = bounds.origin - self.document.origin;
            let scale = self.scale_factor;
            [
                (f32::from(origin.x) * scale).floor() as i32,
                (f32::from(origin.y) * scale).floor() as i32,
                (f32::from(origin.x + bounds.size.width) * scale).ceil() as i32,
                (f32::from(origin.y + bounds.size.height) * scale).ceil() as i32,
            ]
        };
        let mut cut = Vec::new();
        if self.visible != self.document {
            let [left, top, right, bottom] = device(self.document);
            let [shown_left, shown_top, shown_right, shown_bottom] = device(self.visible);
            cut.extend([
                [left, top, right, shown_top],
                [left, shown_bottom, right, bottom],
                [left, shown_top, shown_left, shown_bottom],
                [shown_right, shown_top, right, shown_bottom],
            ]);
        }
        cut.extend(
            self.overlays
                .iter()
                .map(|overlay| overlay.intersect(&self.visible))
                .filter(|overlay| !overlay.is_empty())
                .map(device),
        );
        cut.retain(|[left, top, right, bottom]| left < right && top < bottom);
        let cut = (!cut.is_empty()).then_some(cut);
        if cut == self.applied {
            return;
        }
        // SAFETY: `hwnd` is the WebView2's child window, alive as long as
        // the webview; the system owns a region once it is set on a window,
        // and every other region made here is deleted.
        unsafe {
            let region = match &cut {
                None => HRGN::default(),
                Some(cut) => {
                    let [_, _, width, height] = device(self.document);
                    let region = CreateRectRgn(0, 0, width, height);
                    for &[left, top, right, bottom] in cut {
                        let hole = CreateRectRgn(left, top, right, bottom);
                        CombineRgn(Some(region), Some(region), Some(hole), RGN_DIFF);
                        let _ = DeleteObject(hole.into());
                    }
                    region
                }
            };
            if SetWindowRgn(hwnd, Some(region), true) == 0 {
                log::warn!("cutting GPUI's overlays out of the WebView2 failed");
                if !region.is_invalid() {
                    let _ = DeleteObject(region.into());
                }
                return;
            }
        }
        self.applied = cut;
    }
}
