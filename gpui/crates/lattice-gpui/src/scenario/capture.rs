//! Saving the window as a PNG, for the scenarios' look reviews
//! (`LATTICE_E2E_SHOTS`, `shot`, the parity scenes).
//!
//! On macOS, built with the `offscreen-shots` feature, the frame is drawn
//! here and read back from Metal, so the screen may be locked and the window
//! covered; otherwise `screencapture -l` copies the window server's image of
//! the window, which needs the window on an unlocked screen.
//!
//! On Windows the window's client area is copied from the screen
//! (`native::capture_client`), so a WebView2 page and whatever covers it
//! are in the image as they show. GPUI's own `render_to_image` draws only
//! GPUI's scene and never a page, so it could not show a menu hidden under
//! one.
//!
//! On Linux the window is read back from the X server
//! (`native::capture_client`): gpui_linux has no `render_to_image` at the
//! gpui-fast pin, so the window must be mapped, as it is on the runner's Xvfb
//! display. What lavapipe presents reads back black there, so the e2e job
//! draws through GL (gpui.yml).

use anyhow::Result;
use gpui::AsyncApp;

use super::Run;
#[cfg(any(
    all(target_os = "macos", feature = "offscreen-shots"),
    target_os = "linux"
))]
use super::sleep;

impl Run {
    #[cfg(all(target_os = "macos", feature = "offscreen-shots"))]
    pub(super) async fn capture(&self, cx: &mut AsyncApp, out: &str) -> Result<()> {
        use std::time::Duration;

        // A covered window gets no frames, so frames are drawn here: each one
        // asks PDFium for the pages it shows, and the images land a little
        // later, so draw until two pages are on screen (or give up after 8 s),
        // then once more for anything the last draw scheduled.
        let started = std::time::Instant::now();
        loop {
            self.with_window(cx, |_, window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            })?;
            sleep(cx, Duration::from_millis(250)).await;
            let drawn = cx.update(|cx| self.shell.read(cx).pdf().read(cx).pages_drawn(cx));
            if drawn >= 2 || started.elapsed() > Duration::from_secs(8) {
                break;
            }
        }
        self.with_window(cx, |_, window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        })?;
        let image = self.with_window(cx, |_, window, _| window.render_to_image())??;
        image.save(out)?;
        Ok(())
    }

    #[cfg(all(target_os = "macos", not(feature = "offscreen-shots")))]
    pub(super) async fn capture(&self, cx: &mut AsyncApp, out: &str) -> Result<()> {
        use anyhow::anyhow;
        use std::process::Command;

        let number = crate::native::window_number(self.native(cx)?);
        let status = Command::new("screencapture")
            .args(["-x", "-o", &format!("-l{number}"), out])
            .status()?;
        if !status.success() {
            return Err(anyhow!(
                "screencapture failed ({status}); a locked screen composites no windows, \
                 build with --features offscreen-shots"
            ));
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub(super) async fn capture(&self, cx: &mut AsyncApp, out: &str) -> Result<()> {
        crate::native::capture_client(self.native(cx)?)?.save(out)?;
        Ok(())
    }

    /// GPUI draws a mapped window on its own clock, so the frame a change
    /// asked for is requested and given a few frames to reach the server.
    #[cfg(target_os = "linux")]
    pub(super) async fn capture(&self, cx: &mut AsyncApp, out: &str) -> Result<()> {
        let native = self.native(cx)?;
        crate::native::order_front(native);
        self.with_window(cx, |_, window, _| window.refresh())?;
        sleep(cx, std::time::Duration::from_millis(300)).await;
        crate::native::capture_client(native)?.save(out)?;
        Ok(())
    }
}
