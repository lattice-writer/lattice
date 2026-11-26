//! The network coming back, as the browser's `online` event tells the Tauri
//! app's live channel (`use-overleaf-realtime-connection.ts`), which then
//! joins again at once rather than at the end of its backoff, up to 30
//! seconds away. On macOS the system's reachability of the internet is
//! watched (`SCNetworkReachability`, on a thread running its own run loop),
//! and each time it turns reachable every window's bridge hears it; the
//! window coming forward stays the other prompt, as it is there.
//!
//! The system waking from sleep is told the same way (`wakes`,
//! `NSWorkspaceDidWakeNotification` on macOS): the socket the channel slept
//! on may be dead without knowing it, so a live channel joins again rather
//! than wait for its heartbeat watchdog, and one waiting out its backoff
//! joins now. The Tauri app has no such prompt; there the watchdog alone
//! finds the dead socket.
//!
//! So is the network changing under a connection that stays reachable
//! throughout (`path_changes`): Wi-Fi to Ethernet, a VPN coming up or going
//! down. The socket may be bound to an interface that is gone, and such a
//! socket fails nothing and hears nothing until the watchdog gives up on
//! it, so a live channel probes it and joins again only if nothing answers;
//! a change that left the socket working costs nothing. A switch fires
//! several reachability callbacks over a moment, so listeners hear one
//! change once they have been quiet for [`PATH_SETTLE`].

use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use futures::channel::mpsc;

/// Each listener's sender; one watcher thread serves them all.
static LISTENERS: Mutex<Vec<mpsc::UnboundedSender<()>>> = Mutex::new(Vec::new());
/// The one reachability watcher, for returns and path changes both.
static WATCHING: OnceLock<()> = OnceLock::new();

/// A stream of the network coming back, from now on.
pub fn returns() -> mpsc::UnboundedReceiver<()> {
    WATCHING.get_or_init(watch);
    let (sender, receiver) = mpsc::unbounded();
    if let Ok(mut listeners) = LISTENERS.lock() {
        listeners.push(sender);
    }
    receiver
}

/// The network is reachable again: every listener hears it (also what a
/// scenario calls, as the system would).
pub fn announce_return() {
    if let Ok(mut listeners) = LISTENERS.lock() {
        listeners.retain(|listener| listener.unbounded_send(()).is_ok());
    }
}

#[cfg(target_os = "macos")]
fn watch() {
    use std::{
        net::{Ipv4Addr, SocketAddr},
        sync::atomic::{AtomicBool, Ordering},
    };

    use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
    use system_configuration::network_reachability::{ReachabilityFlags, SCNetworkReachability};

    let spawned = std::thread::Builder::new()
        .name("network-reachability".into())
        .spawn(|| {
            // The internet at large, not one host: what `online` means.
            let mut reachability =
                SCNetworkReachability::from(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)));
            let reachable = |flags: ReachabilityFlags| {
                flags.contains(ReachabilityFlags::REACHABLE)
                    && !flags.contains(ReachabilityFlags::CONNECTION_REQUIRED)
            };
            let was = AtomicBool::new(reachability.reachability().is_ok_and(reachable));
            // Reachable before and after is the path changing under us: the
            // flags, or only the interface behind them, moved.
            let callback = reachability.set_callback(move |flags| {
                let now = reachable(flags);
                match (was.swap(now, Ordering::SeqCst), now) {
                    (false, true) => announce_return(),
                    (true, true) => announce_path_change(),
                    // Gone: a change still settling would only join in vain.
                    (_, false) => forget_path_change(),
                }
            });
            if callback.is_err() {
                return;
            }
            // SAFETY: the current thread's run loop, in its common modes,
            // which this thread then runs for the app's lifetime.
            let scheduled = unsafe {
                reachability.schedule_with_runloop(&CFRunLoop::get_current(), kCFRunLoopCommonModes)
            };
            if scheduled.is_ok() {
                CFRunLoop::run_current();
            }
        });
    // Without the thread the window coming forward still rejoins.
    drop(spawned);
}

#[cfg(not(target_os = "macos"))]
fn watch() {}

/// How long the reachability callbacks must stay quiet before a path
/// change is told. Short, because the probe that follows takes its own
/// seconds, and a change told twice costs only a second probe.
pub const PATH_SETTLE: Duration = Duration::from_secs(1);

static PATH_LISTENERS: Mutex<Vec<mpsc::UnboundedSender<()>>> = Mutex::new(Vec::new());
/// When the path change pending is told, while one is.
static PATH_DUE: Mutex<Option<Instant>> = Mutex::new(None);

/// A stream of the network's path changing while it stays reachable, each
/// burst told once, from now on.
pub fn path_changes() -> mpsc::UnboundedReceiver<()> {
    WATCHING.get_or_init(watch);
    let (sender, receiver) = mpsc::unbounded();
    if let Ok(mut listeners) = PATH_LISTENERS.lock() {
        listeners.push(sender);
    }
    receiver
}

/// The path changed (also what a scenario calls, as the system would):
/// listeners hear it once no other change has come for [`PATH_SETTLE`].
/// One thread waits out each burst, moving its deadline with every change.
/// Changes as the system wakes are told too: the interfaces come up after
/// the wake has joined again, and the probe finds the new socket working.
pub fn announce_path_change() {
    let Ok(mut due) = PATH_DUE.lock() else {
        return;
    };
    let waiting = due.replace(Instant::now() + PATH_SETTLE).is_some();
    drop(due);
    if waiting {
        return;
    }
    let settling = std::thread::Builder::new()
        .name("network-path-settle".into())
        .spawn(|| {
            loop {
                let Ok(mut due) = PATH_DUE.lock() else {
                    return;
                };
                let Some(deadline) = *due else {
                    return;
                };
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    *due = None;
                    break;
                }
                drop(due);
                std::thread::sleep(left);
            }
            if let Ok(mut listeners) = PATH_LISTENERS.lock() {
                listeners.retain(|listener| listener.unbounded_send(()).is_ok());
            }
        });
    // Without the thread nothing would ever clear the deadline.
    if settling.is_err() {
        forget_path_change();
    }
}

/// Drop the path change still settling, if any.
fn forget_path_change() {
    if let Ok(mut due) = PATH_DUE.lock() {
        *due = None;
    }
}

static WAKE_LISTENERS: Mutex<Vec<mpsc::UnboundedSender<()>>> = Mutex::new(Vec::new());

/// A stream of the system waking from sleep, from now on. Called on the
/// main thread, where AppKit posts the notification.
pub fn wakes() -> mpsc::UnboundedReceiver<()> {
    static WATCHING: OnceLock<()> = OnceLock::new();
    WATCHING.get_or_init(watch_wake);
    let (sender, receiver) = mpsc::unbounded();
    if let Ok(mut listeners) = WAKE_LISTENERS.lock() {
        listeners.push(sender);
    }
    receiver
}

/// The system woke: every listener hears it (also what a scenario calls,
/// as the system would).
pub fn announce_wake() {
    if let Ok(mut listeners) = WAKE_LISTENERS.lock() {
        listeners.retain(|listener| listener.unbounded_send(()).is_ok());
    }
}

#[cfg(target_os = "macos")]
fn watch_wake() {
    use block2::RcBlock;
    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    use objc2_foundation::NSString;

    let woke = RcBlock::new(|_: *mut AnyObject| announce_wake());
    let name = NSString::from_str("NSWorkspaceDidWakeNotification");
    // SAFETY: NSWorkspace's own notification centre, which posts its wake
    // on the main thread; with no queue the block runs right there, and
    // only sends on channels. The observer is kept for the app's lifetime.
    unsafe {
        let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
        let center: Retained<AnyObject> = msg_send![&*workspace, notificationCenter];
        let observer: Option<Retained<AnyObject>> = msg_send![
            &*center,
            addObserverForName: &*name,
            object: std::ptr::null::<AnyObject>(),
            queue: std::ptr::null::<AnyObject>(),
            usingBlock: &*woke
        ];
        std::mem::forget(observer);
    }
}

#[cfg(not(target_os = "macos"))]
fn watch_wake() {}
