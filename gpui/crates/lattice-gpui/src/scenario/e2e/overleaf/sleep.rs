//! The e2e checks of a live socket that goes quiet without closing, as the
//! one the computer slept on, or whose network went away, does
//! (`MockOverleaf::go_silent`):
//!
//! - the system waking (`overleaf/network.rs`) has a live channel join
//!   again at once, not a heartbeat timeout later, and what was typed onto
//!   the dead socket lands once;
//! - with the system awake, the heartbeat watchdog (lattice-core's
//!   `watch_heartbeat`) gives a silent socket up within the timeout the
//!   handshake named, before the edit sent on it times out; what was typed
//!   before and during the silence lands once when the channel is back, and
//!   the file is never left paused as unconfirmed.

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use anyhow::Result;
use gpui::{App, AsyncApp, Entity};
use serde_json::json;

use super::super::Steps;
use crate::{
    overleaf::{bridge::ChannelStatus, network},
    scenario::{Run, overleaf_mock::MockOverleaf},
    shell::Shell,
};

/// The heartbeat timeout the stand-in names for the watchdog's check:
/// well inside lattice-core's 10 s wait for an answer.
const HEARTBEAT: u64 = 4;

impl Run {
    pub(super) async fn overleaf_sleep(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
    ) -> Result<()> {
        let main = self.root(cx).await?.join("main.tex");
        let unconfirmed = Cell::new(false);
        // Live in main.tex with everything answered, and Overleaf's copy the
        // editor's; noting meanwhile whether any send was left unconfirmed.
        let settled = |shell: &Entity<Shell>, cx: &mut App| {
            let bridge = shell.read(cx).overleaf.read(cx);
            unconfirmed.set(unconfirmed.get() || bridge.unconfirmed());
            let live = bridge.channel() == ChannelStatus::Live
                && bridge.live_file() == Some("main.tex")
                && !bridge.live_sending();
            let text = shell
                .update(cx, |shell, cx| shell.open_editor(&main, cx))
                .map(|editor| editor.read(cx).overleaf_text(cx));
            live && text.is_some() && text == mock.file("main.tex")
        };
        let ready = self.wait_for(cx, Duration::from_secs(40), &settled).await;

        // Waking: the socket died in the sleep, and the next join names a
        // short heartbeat timeout for the check after this one.
        mock.heartbeat_timeout(HEARTBEAT);
        mock.go_silent();
        self.type_text(cx, "Curie ").await?;
        self.pause(cx, Duration::from_millis(800)).await;
        let woke = Instant::now();
        network::announce_wake();
        let left = self
            .wait_for(cx, Duration::from_secs(2), |shell, cx| {
                shell.read(cx).overleaf.read(cx).channel() != ChannelStatus::Live
            })
            .await;
        let back = self.wait_for(cx, Duration::from_secs(6), &settled).await;
        let took = woke.elapsed();
        let overleaf = mock.file("main.tex").unwrap_or_default();
        steps.check(
            "waking from sleep has a live channel join again at once, and what was typed onto the dead socket lands once",
            ready && left && back && !unconfirmed.get() && overleaf.matches("Curie ").count() == 1,
            json!({
                "ready": ready, "left": left, "back": back, "took_ms": took.as_millis(),
                "unconfirmed": unconfirmed.get(), "overleaf": overleaf,
            }),
        );

        // Awake: heartbeats keep a quiet channel up past the timeout...
        self.pause(cx, Duration::from_secs(HEARTBEAT + 2)).await;
        let steady = cx.update(|cx| self.shell.read(cx).overleaf.read(cx).channel())
            == ChannelStatus::Live;
        // ...and silence ends it within the timeout, the network gone too.
        mock.disable_realtime();
        mock.go_silent();
        let silenced = Instant::now();
        self.type_text(cx, "Noether ").await?;
        let dropped = self
            .wait_for(cx, Duration::from_secs(HEARTBEAT + 3), |shell, cx| {
                let bridge = shell.read(cx).overleaf.read(cx);
                unconfirmed.set(unconfirmed.get() || bridge.unconfirmed());
                bridge.channel() != ChannelStatus::Live
            })
            .await;
        let dropped_after = silenced.elapsed();
        self.type_text(cx, "Germain ").await?;
        mock.heartbeat_timeout(60);
        mock.enable_realtime();
        let converged = self.wait_for(cx, Duration::from_secs(40), &settled).await;
        let overleaf = mock.file("main.tex").unwrap_or_default();
        let once = ["Noether ", "Germain "]
            .iter()
            .all(|word| overleaf.matches(word).count() == 1);
        steps.check(
            "a live socket gone silent is given up within its heartbeat timeout, and what was typed before and during the silence lands once",
            steady && dropped && converged && once && !unconfirmed.get() && mock.rejected().is_empty(),
            json!({
                "steady": steady, "dropped": dropped, "dropped_ms": dropped_after.as_millis(),
                "converged": converged, "once": once, "unconfirmed": unconfirmed.get(),
                "overleaf": overleaf, "rejected": mock.rejected(),
            }),
        );
        Ok(())
    }
}
