//! The idle guard (lattice#1025, `script/idle-guard`): a window with
//! nothing to show draws nothing. With the editor focused only its caret
//! blinks, about two frames a second; in the background it draws none at
//! all; and left idle for a long while its memory does not grow.
//!
//! The frames are GPUI's own count of drawn frames (`Window::layout_stats`,
//! test support, hence the offscreen-shots build), the memory macOS's
//! `footprint` of this process. Two runs, because `GPUI_LAB_ASSUME_KEY` is
//! read once at launch:
//! - without it, the frames: focused (a foreground section, which takes the
//!   GPUI e2e input lock briefly) and then in the background;
//! - with it and `LATTICE_IDLE_GROWTH_SECS` set, the growth: the window acts
//!   key, so its caret keeps blinking, while the app stays in the background
//!   and sends no input for the whole wait.
//!
//! A focused count needs a window that draws. A covered window or a
//! sleeping display draws nothing, which would pass the limit while
//! measuring nothing, so a focused count under half a frame a second fails.

use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use gpui::AsyncApp;
use serde_json::{Value, json};

use super::{Run, measure::footprint, sleep};
use crate::native;

/// Above this a focused window is drawing for something besides the caret,
/// which blinks twice a second.
const FOCUSED_FRAMES_PER_SECOND: f64 = 2.5;
/// Below this a focused window is not drawing at all: covered, or on a
/// display that sleeps. Its count would pass the limit while measuring
/// nothing, and a growth wait would never draw the frames a leak needs.
const DRAWING_FRAMES_PER_SECOND: f64 = 0.5;
/// How long each frame count runs.
const FRAMES_WINDOW: Duration = Duration::from_secs(10);
/// Time for what opening the project starts (its build, the PDF, the
/// language server) to finish before anything is counted.
const SETTLE: Duration = Duration::from_secs(20);
/// What the footprint may grow by over the growth wait. Over 30 minutes
/// idle on the Mac mini it fell 15 MB (178 to 163 MB, LATA-99); a leak of
/// one frame's worth of retained data per caret blink would pass this
/// within minutes.
const GROWTH_LIMIT_MB: f64 = 24.;

/// `footprint`'s total for this process, in MB.
fn footprint_mb() -> Result<f64> {
    let report = footprint();
    let total = report["footprint"]
        .as_str()
        .context("footprint printed no total")?;
    let mut fields = total.split_whitespace();
    let value: f64 = fields
        .next()
        .and_then(|v| v.parse().ok())
        .context("footprint total")?;
    Ok(match fields.next() {
        Some("KB") => value / 1024.,
        Some("GB") => value * 1024.,
        Some("MB") => value,
        unit => return Err(anyhow!("footprint unit {unit:?}")),
    })
}

impl Run {
    /// Frames drawn per second over `window`, and whether the window was
    /// active at its end.
    async fn frames_per_second(&self, cx: &mut AsyncApp, window: Duration) -> Result<(f64, bool)> {
        self.with_window(cx, |_, window, _| window.reset_layout_stats())?;
        sleep(cx, window).await;
        let (frames, active) = self.with_window(cx, |_, window, _| {
            (window.layout_stats().frames, window.is_window_active())
        })?;
        Ok((frames as f64 / window.as_secs_f64(), active))
    }

    pub(super) async fn idle(&self, cx: &mut AsyncApp) -> Result<Value> {
        let root = self.root(cx).await?;
        let editor = self.open(cx, &root.join("main.tex"), Some(1))?;
        sleep(cx, SETTLE).await;
        let mut checks = Vec::new();
        let mut check = |name: &str, passed: bool, detail: Value| {
            checks.push(json!({ "name": name, "passed": passed, "detail": detail }));
        };
        let growth = std::env::var("LATTICE_IDLE_GROWTH_SECS")
            .ok()
            .map(|v| {
                v.parse::<u64>()
                    .context("LATTICE_IDLE_GROWTH_SECS is not a number of seconds")
            })
            .transpose()?;
        if let Some(secs) = growth {
            let before = footprint_mb()?;
            let (fps, active) = self
                .frames_per_second(cx, Duration::from_secs(secs))
                .await?;
            let after = footprint_mb()?;
            check(
                "left idle, its footprint does not grow",
                after - before <= GROWTH_LIMIT_MB,
                json!({ "secs": secs, "before_mb": before, "after_mb": after, "limit_mb": GROWTH_LIMIT_MB }),
            );
            check(
                "left idle with the window acting key, only the caret draws",
                (DRAWING_FRAMES_PER_SECOND..=FOCUSED_FRAMES_PER_SECOND).contains(&fps),
                json!({ "frames_per_second": fps, "active": active, "range": [DRAWING_FRAMES_PER_SECOND, FOCUSED_FRAMES_PER_SECOND] }),
            );
        } else {
            let native = self.native(cx)?;
            let focused = {
                let _front = self.foreground(cx, "idle with the editor focused").await?;
                let bounds = cx
                    .update(|cx| editor.read(cx).editor().read(cx).last_bounds().copied())
                    .context("the editor has not been laid out")?;
                native::send_click(native, bounds.center())?;
                sleep(cx, Duration::from_secs(2)).await;
                self.frames_per_second(cx, FRAMES_WINDOW).await?
            };
            check(
                "idle with the editor focused, only the caret draws",
                (DRAWING_FRAMES_PER_SECOND..=FOCUSED_FRAMES_PER_SECOND).contains(&focused.0)
                    && focused.1,
                json!({ "frames_per_second": focused.0, "active": focused.1, "range": [DRAWING_FRAMES_PER_SECOND, FOCUSED_FRAMES_PER_SECOND] }),
            );
            sleep(cx, Duration::from_secs(2)).await;
            let background = self.frames_per_second(cx, FRAMES_WINDOW).await?;
            check(
                "idle in the background, the window draws nothing",
                background.0 == 0. && !background.1,
                json!({ "frames_per_second": background.0, "active": background.1 }),
            );
        }
        let passed = checks.iter().all(|check| check["passed"] == true);
        Ok(json!({ "scenario": "idle", "passed": passed, "checks": checks }))
    }
}
