//! Restarting the sidecar so it picks up a changed environment. Its provider
//! CLIs inherit the environment it was started with, so a proxy turned on
//! later (Clash, Surge) or an edited `~/.zshrc` reaches them only through a
//! new sidecar. Restarting stops its running turns, so it is offered, never
//! forced: a system proxy change raises `ENVIRONMENT_CHANGED_EVENT` for the
//! windows to offer it, and Settings compares the shell as it is now.

use super::environment::{self, AgentEnvironment, EnvironmentReport};
#[cfg(unix)]
use super::shell_env;
use super::shell_env::ShellRead;
use super::{proxy, RunningSynara, SynaraRuntime};
use crate::host::EventSink;
use crate::sidecar::terminate_process_group;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Payload `{ stale }`: whether the running sidecar's proxy settings now
/// differ from the system's.
const ENVIRONMENT_CHANGED_EVENT: &str = "synara-environment-changed";
/// The sidecar was restarted, on request or after it exited; windows using it
/// ask for the new one.
const RESTARTED_EVENT: &str = "synara-runtime-restarted";

pub(super) fn read_login_shell() -> ShellRead {
    #[cfg(windows)]
    return Ok(std::env::vars().filter(|(name, _)| environment::is_inherited(name)).collect());
    #[cfg(not(windows))]
    shell_env::read(
        &crate::terminal::login_shell(),
        shell_env::SHELL_TIMEOUT,
        environment::is_inherited,
    )
}

pub(super) fn watch_system_proxy(
    events: Option<Arc<dyn EventSink>>, running: Arc<Mutex<Option<RunningSynara>>>,
) {
    let Some(events) = events else { return };
    proxy::watch_system_proxy(move || {
        let system = proxy::system_proxy();
        let Some(stale) = with_running_environment(&running, |environment| {
            environment.system_proxy_changed(system.as_ref())
        }) else {
            return;
        };
        events.emit(ENVIRONMENT_CHANGED_EVENT, serde_json::json!({ "stale": stale }));
    });
}

/// How often [`watch_exit`] looks, and the least time between two restarts
/// it asks for. A sidecar that exits again within that gap is left for the
/// next request, which starts a new one and announces it itself.
const EXIT_POLL: Duration = Duration::from_secs(2);
const EXIT_RESTART_GAP: Duration = Duration::from_secs(60);

/// Notice a sidecar that exited on its own, and have the windows ask for a
/// new one at once. Otherwise the exited process waits, unreaped, for the
/// writer's next request to find it gone; on Linux the guard ends the
/// sandbox when its file system stops, and the writer would only learn of
/// it then (lattice#1221).
pub(super) fn watch_exit(
    events: Option<Arc<dyn EventSink>>, running: Arc<Mutex<Option<RunningSynara>>>,
) {
    let Some(events) = events else { return };
    thread::spawn(move || {
        let mut announced = None;
        loop {
            thread::sleep(EXIT_POLL);
            if reap_exited(&running, &mut announced) {
                announce(Some(events.as_ref()));
            }
        }
    });
}

/// Take the running sidecar away if it exited, ending what it left running
/// in its process group, and answer whether to announce a restart.
/// `announced` is when this last did. Within [`EXIT_RESTART_GAP`] of that
/// the exited sidecar stays where it is: `ensure_ready` finds it exited,
/// starts a new one and announces that, so no window keeps the old token.
fn reap_exited(running: &Mutex<Option<RunningSynara>>, announced: &mut Option<Instant>) -> bool {
    let Ok(mut running) = running.lock() else { return false };
    let Some(current) = running.as_mut() else { return false };
    if matches!(current.child.try_wait(), Ok(None)) {
        return false;
    }
    if announced.is_some_and(|announced| announced.elapsed() < EXIT_RESTART_GAP) {
        return false;
    }
    if let Some(mut exited) = running.take() {
        terminate_process_group(&mut exited.child);
    }
    *announced = Some(Instant::now());
    true
}

/// `inspect` of the environment the live sidecar started with; `None` while
/// none runs.
fn with_running_environment<T>(
    running: &Mutex<Option<RunningSynara>>, inspect: impl FnOnce(&AgentEnvironment) -> T,
) -> Option<T> {
    let mut running = running.lock().ok()?;
    let current = running.as_mut()?;
    matches!(current.child.try_wait(), Ok(None)).then(|| inspect(&current.environment))
}

/// Tell every window the sidecar was restarted, so each asks for the new one.
pub(super) fn announce(events: Option<&dyn EventSink>) {
    if let Some(events) = events {
        events.emit(RESTARTED_EVENT, serde_json::Value::Null);
    }
}

impl SynaraRuntime {
    /// The variables an agent start would use now, redacted, and whether the
    /// running sidecar started with different ones. Reads the login shell,
    /// which can take seconds.
    pub fn agent_environment(&self) -> EnvironmentReport {
        let now = AgentEnvironment::resolve(read_login_shell());
        let started = with_running_environment(&self.running, |started| now.differs_from(started));
        let shell = crate::terminal::login_shell()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        now.report(shell, started.is_some(), started.unwrap_or(false))
    }

    /// Stop the sidecar; every window using it starts a new one, which reads
    /// the login shell and system proxy again.
    pub fn restart(&self) {
        self.shutdown();
        announce(self.events.as_deref());
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;

    fn exited() -> Option<RunningSynara> {
        let mut child = Command::new("/bin/sh").args(["-c", "exit 3"]).spawn().unwrap();
        while matches!(child.try_wait(), Ok(None)) {
            thread::sleep(Duration::from_millis(5));
        }
        let info = SynaraRuntime::default().unsupported_info("exited");
        Some(RunningSynara { child, info, environment: AgentEnvironment::default() })
    }

    /// A sidecar that exits on its own is reaped and announced once; one
    /// that exits again soon after is left for `ensure_ready` to find, which
    /// announces the restart it makes.
    #[test]
    fn a_second_exit_within_the_gap_is_left_for_the_next_start_to_announce() {
        let running = Mutex::new(exited());
        let mut announced = None;
        assert!(reap_exited(&running, &mut announced));
        assert!(running.lock().unwrap().is_none());
        // Nothing running: nothing to announce.
        assert!(!reap_exited(&running, &mut announced));

        *running.lock().unwrap() = exited();
        assert!(!reap_exited(&running, &mut announced));
        {
            let mut left = running.lock().unwrap();
            let left = left.as_mut().expect("the exited sidecar stays for ensure_ready");
            assert!(left.child.try_wait().unwrap().is_some());
        }

        // Once the gap has passed, it is reaped and announced again.
        announced = Instant::now().checked_sub(EXIT_RESTART_GAP + Duration::from_secs(1));
        assert!(reap_exited(&running, &mut announced));
        assert!(running.lock().unwrap().is_none());
    }
}
