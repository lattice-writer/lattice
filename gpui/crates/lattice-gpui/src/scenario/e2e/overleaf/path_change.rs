//! The e2e check of the network changing under a live channel that stays
//! reachable throughout (`overleaf/network.rs`'s `path_changes`, Wi-Fi to
//! Ethernet or a VPN). A change that leaves the socket working costs
//! nothing: its probe is answered, heartbeats muted so that the answer is
//! all it hears, and the channel stays live without joining again. One
//! that leaves it on a path that is gone, its socket open and silent
//! (`MockOverleaf::go_silent`), has the burst of reachability callbacks
//! join again once, after the burst settles and the probe goes unanswered,
//! long before the heartbeat watchdog would; what was typed onto the dead
//! socket lands once.

use std::time::{Duration, Instant};

use anyhow::Result;
use gpui::AsyncApp;
use lattice_core::overleaf_rt::PATH_PROBE;
use serde_json::json;

use super::super::Steps;
use crate::{
    overleaf::{bridge::ChannelStatus, network},
    scenario::{Run, overleaf_mock::MockOverleaf},
};

impl Run {
    pub(super) async fn overleaf_path_change(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
    ) -> Result<()> {
        let main = self.root(cx).await?.join("main.tex");
        let settled = |shell: &gpui::Entity<crate::shell::Shell>, cx: &mut gpui::App| {
            let bridge = shell.read(cx).overleaf.read(cx);
            let live = bridge.channel() == ChannelStatus::Live
                && bridge.live_file() == Some("main.tex")
                && !bridge.live_sending()
                && !bridge.unconfirmed();
            let text = shell
                .update(cx, |shell, cx| shell.open_editor(&main, cx))
                .map(|editor| editor.read(cx).overleaf_text(cx));
            live && text.is_some() && text == mock.file("main.tex")
        };
        let not_live = |shell: &gpui::Entity<crate::shell::Shell>, cx: &mut gpui::App| {
            shell.read(cx).overleaf.read(cx).channel() != ChannelStatus::Live
        };
        let upgrades = || {
            let requests = mock.socket_requests();
            requests
                .iter()
                .filter(|line| line.contains("/socket.io/1/websocket/"))
                .count()
        };
        // A switch is several callbacks over a moment; returns when the last
        // one was told.
        let burst = async |cx: &mut AsyncApp| {
            let mut last = Instant::now();
            for _ in 0..3 {
                last = Instant::now();
                network::announce_path_change();
                self.pause(cx, Duration::from_millis(300)).await;
            }
            last
        };
        let probed = network::PATH_SETTLE + PATH_PROBE;

        let ready = self.wait_for(cx, Duration::from_secs(40), &settled).await;
        let joins_before = upgrades();
        // Only the probe's answer is heard meanwhile, so it alone can keep
        // the channel.
        mock.mute_heartbeats(true);
        let asked_before = mock.rosters_asked();
        burst(cx).await;
        let dropped = self
            .wait_for(cx, probed + Duration::from_secs(2), &not_live)
            .await;
        let probes = mock.rosters_asked() - asked_before;
        mock.mute_heartbeats(false);
        let joins = upgrades() - joins_before;
        let still = self.wait_for(cx, Duration::from_secs(1), &settled).await;
        steps.check(
            "the network changing under a live channel whose socket still answers costs nothing: no join again and no Reconnecting",
            ready && !dropped && still && joins == 0 && probes == 1,
            json!({ "ready": ready, "dropped": dropped, "still": still, "joins": joins, "probes": probes }),
        );

        let joins_before = upgrades();
        // The old path is gone: its socket stays open, silent.
        mock.go_silent();
        self.type_text(cx, "Shannon ").await?;
        let last = burst(cx).await;
        let left = self
            .wait_for(cx, probed + Duration::from_secs(3), &not_live)
            .await;
        // Measured from the last callback: the settle restarts with each.
        let left_after = last.elapsed();
        let back = self.wait_for(cx, Duration::from_secs(10), &settled).await;
        let took = last.elapsed();
        // One join again for the whole burst.
        let joins = upgrades() - joins_before;
        let overleaf = mock.file("main.tex").unwrap_or_default();
        steps.check(
            "the network changing under a live channel on a silent socket has it join again once the change settles and its probe goes unanswered, and what was typed onto the old path lands once",
            ready
                && left
                && left_after >= probed
                && back
                && took <= probed + Duration::from_secs(4)
                && joins == 1
                && overleaf.matches("Shannon ").count() == 1
                && mock.rejected().is_empty(),
            json!({
                "left": left, "left_ms": left_after.as_millis(),
                "back": back, "took_ms": took.as_millis(), "joins": joins, "overleaf": overleaf,
            }),
        );
        Ok(())
    }
}
