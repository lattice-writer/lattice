//! Noticing a connection that died without closing: the heartbeat watchdog,
//! and the clock it reads.

use super::{lock, Shared};
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

/// Why a connection ended on an answer that never came.
pub(super) const SILENT_SINCE: &str =
    "Overleaf has sent nothing since, so the connection is presumed lost.";

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
