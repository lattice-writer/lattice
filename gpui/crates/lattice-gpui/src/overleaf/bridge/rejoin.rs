//! What has a channel waiting out its backoff join now: the window coming
//! on screen or forward (`retryNow` on the page's focus), and the network
//! coming back (`network.rs`).

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
}
