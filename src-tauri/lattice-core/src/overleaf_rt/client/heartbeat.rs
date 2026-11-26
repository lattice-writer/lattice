//! Noticing a connection that died without closing: the heartbeat watchdog,
//! the clock it reads, and ending one the app has reason to doubt.

use super::super::codec::{encode_event, encode_frame, FRAME_EVENT};
use super::super::PATH_PROBE;
use super::{lock, Ack, RealtimeClient, Shared};
use crate::runtime as rt;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the heartbeat watchdog looks at the clock.
const WATCHDOG_TICK: Duration = Duration::from_millis(500);

/// Time on a clock that keeps counting while the computer sleeps, for the
/// heartbeat watchdog.
///
/// `Instant` will not do on macOS: it reads `CLOCK_UPTIME_RAW`, which stops
/// while the Mac sleeps, so a socket that died with the lid shut looked as
/// fresh on waking as when it was last heard, and the watchdog waited out a
/// whole heartbeat timeout of awake silence before giving it up — long
/// enough for every edit typed meanwhile to time out unanswered.
/// `CLOCK_MONOTONIC` there, and `CLOCK_BOOTTIME` on Linux, count the sleep;
/// Windows' `Instant` already does.
pub(in crate::overleaf_rt) fn awake_or_asleep() -> Duration {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        #[cfg(target_os = "macos")]
        const CLOCK: libc::clockid_t = libc::CLOCK_MONOTONIC;
        #[cfg(target_os = "linux")]
        const CLOCK: libc::clockid_t = libc::CLOCK_BOOTTIME;
        let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: `now` is a valid, writable timespec, and both clocks exist
        // on every version of the systems this runs on.
        if unsafe { libc::clock_gettime(CLOCK, &mut now) } == 0 {
            return Duration::new(now.tv_sec as u64, now.tv_nsec as u32);
        }
    }
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// Ends a connection the server has gone quiet on.
///
/// A socket can stay open after the network under it is gone — the laptop
/// slept, the Wi-Fi changed — and then nothing arrives and nothing fails:
/// collaborators' edits stop, ours time out one by one, and no reconnect is
/// ever asked for, because no disconnect was ever seen. Overleaf sends a
/// heartbeat well inside the timeout it names in the handshake, and the
/// Socket.IO 0.9 client its editor uses closes the connection when that much
/// time passes without hearing anything; so does this. Finishing reports
/// `Disconnected`, which is what sets the app reconnecting.
///
/// Silence is counted with the sleep in it (`awake_or_asleep`), so a socket
/// the computer slept on past the timeout is given up within a tick of
/// waking. The ack timeout stays on awake time on purpose: after a long
/// sleep this gets there first, and an edit still out is then held for the
/// reconnect rather than reported as having no known outcome.
pub(super) fn watch_heartbeat(shared: Arc<Shared>, timeout: Duration) {
    std::thread::spawn(move || loop {
        std::thread::sleep(WATCHDOG_TICK);
        if shared.finished.load(Ordering::SeqCst) {
            return;
        }
        let silent = awake_or_asleep().saturating_sub(*lock(&shared.last_heard));
        if silent > timeout {
            shared.finish(format!(
                "Overleaf has sent nothing for {}s, so the connection is presumed lost.",
                silent.as_secs()
            ));
            return;
        }
    });
}

/// Nothing at all has arrived since `since`, on [`awake_or_asleep`]'s clock.
///
/// An answer that times out on a socket that has stayed silent since the
/// request went out is a dead connection, not a lost answer: Overleaf
/// answers an update within milliseconds, and a live socket hears
/// heartbeats and collaborators meanwhile. Taking it as a drop lets the app
/// hold the document and prove the edit by replay on a fresh connection,
/// rather than pausing the file on this one until the watchdog notices. A
/// socket still heard from keeps the timeout as an unknown outcome.
pub(super) fn heard_nothing_since(shared: &Shared, since: Duration) -> bool {
    *lock(&shared.last_heard) <= since
}

const PATH_CHANGED: &str = "The network changed, so the connection is joined again.";

/// How often a path change's probe looks for its answer.
const PROBE_TICK: Duration = Duration::from_millis(100);

impl RealtimeClient {
    /// The network under the connection changed while staying reachable
    /// (Wi-Fi to Ethernet, a VPN): ask the server something, and end the
    /// connection, reported `Disconnected` as after a sleep, unless the
    /// answer arrives within [`PATH_PROBE`]. A socket bound to an
    /// interface that is gone fails nothing and hears nothing until the
    /// watchdog's timeout, and a polite close would wait on it the same way;
    /// but most changes (a roam, an IPv6 address rotating, a VPN's tunnel
    /// flapping) leave the socket working, and those should cost nothing.
    ///
    /// The probe is `clientTracking.getConnectedUsers`, unawaited: Socket.IO
    /// 0.9 servers never answer a heartbeat the client sends, and Overleaf
    /// sends its own only every 25 s or so, while this one it answers within
    /// a second or two. It is what Overleaf's own editor asks on joining, so
    /// one more costs the project a roster refresh and nothing else.
    ///
    /// Only that answer counts, not anything heard meanwhile: after a VPN
    /// reroutes what we send, Overleaf's heartbeats and collaborators' edits
    /// can still arrive on the old interface while nothing of ours gets
    /// through, and only an answer proves the way there works too.
    pub fn probe_after_path_change(&self) {
        let Ok(payload) = encode_event("clientTracking.getConnectedUsers", ()) else {
            return;
        };
        let id = self.shared.next_ack.fetch_add(1, Ordering::SeqCst);
        let (answer, mut answered) = rt::channel::<Ack>(1);
        lock(&self.shared.pending).insert(id, answer);
        self.shared.queue(encode_frame(FRAME_EVENT, &format!("{id}+"), "", &payload));
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let deadline = Instant::now() + PATH_PROBE;
            while Instant::now() < deadline {
                if shared.finished.load(Ordering::SeqCst) || answered.try_recv().is_ok() {
                    return;
                }
                std::thread::sleep(PROBE_TICK);
            }
            lock(&shared.pending).remove(&id);
            if answered.try_recv().is_err() {
                shared.finish(PATH_CHANGED.to_string());
            }
        });
    }
}
