//! What has a channel waiting out its backoff join now: the window coming
//! on screen or forward (`retryNow` on the page's focus), and the network
//! coming back (`network.rs`). The system waking from sleep also has a
//! live channel join again.

use super::*;

impl OverleafBridge {
    /// The window came on screen or went off it.
    pub fn set_window_visible(&mut self, visible: bool) {
        if std::mem::replace(&mut self.window_visible, visible) != visible && visible {
            self.window_came_forward();
        }
    }

    /// The window became the key window, or stopped being it.
    pub fn set_window_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if active {
            self.window_came_forward();
        } else {
            // Going to the browser is when co-authors look: what waits in a
            // whole-file editor goes up.
            self.flush_whole_files_soon(cx);
        }
    }

    /// Back from the browser is when stale content is most obvious: probe
    /// now, and join a channel that is waiting to.
    fn window_came_forward(&mut self) {
        if let Some(wake) = &self.poll_wake {
            wake.unbounded_send(()).ok();
        }
        if let Some(retry) = &self.channel_retry {
            retry.unbounded_send(()).ok();
        }
    }

    /// The network came back (`network.rs`): a channel waiting to join
    /// does so now, as when the window comes forward.
    pub fn network_came_back(&mut self) {
        if let Some(retry) = &self.channel_retry {
            retry.unbounded_send(()).ok();
        }
    }

    /// The system woke from sleep (`network.rs`): a channel that was up
    /// joins again, as its socket may have died while the system slept, and
    /// one waiting to join does so now. Documents with work out are held
    /// over the drop and carried on (`documents/resume.rs`).
    pub fn system_woke(&mut self) {
        if self.channel == ChannelStatus::Live
            && let Some(client) = &self.client
        {
            client.rejoin_after_sleep();
            return;
        }
        self.network_came_back();
    }

    /// The network's path changed while it stayed reachable (`network.rs`):
    /// a live channel probes its socket, which may be bound to an interface
    /// that is gone, and joins again over the new one only when the probe
    /// hears nothing (`probe_after_path_change`); one waiting to join does
    /// so now. Documents with work out are carried over, as after a wake.
    pub fn network_path_changed(&mut self) {
        if self.channel == ChannelStatus::Live
            && let Some(client) = &self.client
        {
            client.probe_after_path_change();
            return;
        }
        self.network_came_back();
    }
}
