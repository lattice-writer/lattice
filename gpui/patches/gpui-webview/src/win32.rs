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

use crate::region;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{
        CombineRgn, CreateRectRgn, DeleteObject, HRGN, RGN_DIFF, RGN_ERROR, SetWindowRgn,
    },
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
    /// The child window's size and the rectangles last cut away from it, in
    /// its pixels: none means the region was cleared.
    applied: Option<region::Region>,
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
            scale_factor: window.scale_factor(),
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

    /// Note the window's scale factor, the interface zoom included.
    pub(crate) fn set_scale_factor(&self, scale_factor: f32) {
        self.state.borrow_mut().scale_factor = scale_factor;
    }

    /// Where to place the child window for `document`: in GPUI's device
    /// pixels, so it covers its element at any interface zoom, as GPUI draws
    /// the overlays cut out of it. Wry's logical pixels would leave out the
    /// zoom.
    pub(crate) fn device_rect(&self, document: Bounds<Pixels>) -> wry::Rect {
        let scale = f64::from(self.state.borrow().scale_factor);
        let device = |value: Pixels| (f64::from(value) * scale).round();
        let (left, top) = (device(document.left()), device(document.top()));
        // The size from the rounded edges, as `region` measures it, so the
        // child window ends where the element does.
        wry::Rect {
            position: wry::dpi::PhysicalPosition::new(left, top).into(),
            size: wry::dpi::PhysicalSize::new(
                device(document.right()) - left,
                device(document.bottom()) - top,
            )
            .into(),
        }
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
        let cut = region::region(
            self.document,
            self.visible,
            &self.overlays,
            self.scale_factor,
        );
        if cut == self.applied {
            return;
        }
        // SAFETY: `hwnd` is the WebView2's child window, alive as long as
        // the webview; the system owns a region once it is set on a window,
        // and every other region made here is deleted.
        unsafe {
            let region = match &cut {
                None => HRGN::default(),
                Some(([width, height], cut)) => {
                    let region = CreateRectRgn(0, 0, *width, *height);
                    let mut made = !region.is_invalid();
                    for &[left, top, right, bottom] in cut {
                        if !made {
                            break;
                        }
                        let hole = CreateRectRgn(left, top, right, bottom);
                        made = !hole.is_invalid()
                            && CombineRgn(Some(region), Some(region), Some(hole), RGN_DIFF)
                                != RGN_ERROR;
                        if !hole.is_invalid() {
                            let _ = DeleteObject(hole.into());
                        }
                    }
                    if !made {
                        log::warn!("making the WebView2's region for GPUI's overlays failed");
                        if !region.is_invalid() {
                            let _ = DeleteObject(region.into());
                        }
                        return;
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
