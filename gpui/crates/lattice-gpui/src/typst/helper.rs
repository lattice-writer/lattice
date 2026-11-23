//! The running Typst helper: started on the first compile (or ahead of it,
//! when a .typ opens) and kept, so its compiler session and fonts carry from
//! one keystroke's compile to the next. One request at a time; a helper that
//! stopped is started again once before the compile is reported failed.
//!
//! Every read has a deadline: the helper's output is read on a thread of its
//! own into a channel, and a helper that says no hello, or a compile that
//! never answers, is killed when its deadline passes, so one stuck compile
//! cannot hold up every later build. The child is also reachable outside the
//! request lock (`CHILD`), so `stop` (Remove, Download again) kills it
//! without waiting for the compile in progress.

use std::{
    collections::HashMap,
    io::{BufRead as _, BufReader, Write as _},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, LazyLock, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    time::Duration,
};

use lattice_core::{
    latex::BuildResult,
    typst_wire::{Hello, PROTOCOL, Reply, Request},
};

/// How long the helper may take to say hello: it reads its fonts first.
#[cfg(not(test))]
const HELLO_DEADLINE: Duration = Duration::from_secs(30);
#[cfg(test)]
const HELLO_DEADLINE: Duration = Duration::from_secs(2);

/// How long one compile may take before the helper is taken as stuck (a
/// document that loops forever) and killed.
#[cfg(not(test))]
const REPLY_DEADLINE: Duration = Duration::from_secs(60);
#[cfg(test)]
const REPLY_DEADLINE: Duration = Duration::from_secs(2);

/// What the helper's environment keeps of Lattice's: where it may write and
/// fetch packages from, and nothing else (no API keys).
const PASSED_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "TMPDIR",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "SYSTEMROOT",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

struct Running {
    binary: PathBuf,
    child: Arc<Mutex<Child>>,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl Drop for Running {
    fn drop(&mut self) {
        kill(&self.child);
    }
}

fn kill(child: &Mutex<Child>) {
    let mut child = child.lock().unwrap_or_else(PoisonError::into_inner);
    child.kill().ok();
    child.wait().ok();
}

/// The requests' lock: held for a whole exchange.
static RUNNING: LazyLock<Mutex<Option<Running>>> = LazyLock::new(Default::default);
/// The running helper's process, outside the requests' lock.
static CHILD: LazyLock<Mutex<Option<Arc<Mutex<Child>>>>> = LazyLock::new(Default::default);
/// How many times `stop` ran: a compile whose helper was stopped under it
/// does not start another.
static STOPS: AtomicU64 = AtomicU64::new(0);

/// A failed exchange, and whether a fresh start may help: a helper that
/// stopped gets one; one that hung already had its deadline.
struct Failure {
    message: String,
    retry: bool,
}

fn stopped(message: impl Into<String>) -> Failure {
    Failure {
        message: message.into(),
        retry: true,
    }
}

/// The next line the helper writes, within `deadline`; on the deadline the
/// helper is killed.
fn read_line(
    lines: &Receiver<String>,
    child: &Mutex<Child>,
    deadline: Duration,
    waiting_for: &str,
) -> Result<String, Failure> {
    match lines.recv_timeout(deadline) {
        Ok(line) => Ok(line),
        Err(RecvTimeoutError::Disconnected) => Err(stopped("Typst support stopped")),
        Err(RecvTimeoutError::Timeout) => {
            kill(child);
            Err(Failure {
                message: format!(
                    "Typst support stopped responding: no {waiting_for} after {} s",
                    deadline.as_secs()
                ),
                retry: false,
            })
        }
    }
}

fn start(binary: &Path) -> Result<Running, Failure> {
    let mut command = Command::new(binary);
    command.env_clear();
    for name in PASSED_ENV {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            stopped(format!(
                "Could not start Typst support ({}): {error}",
                binary.display()
            ))
        })?;
    let stdin = child
        .stdin
        .take()
        .ok_or(stopped("Typst support has no input"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or(stopped("Typst support has no output"))?;
    let (sender, lines) = mpsc::channel();
    std::thread::Builder::new()
        .name("typst-helper-output".into())
        .spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        })
        .map_err(|error| stopped(error.to_string()))?;
    let child = Arc::new(Mutex::new(child));
    *CHILD.lock().unwrap_or_else(PoisonError::into_inner) = Some(child.clone());
    let running = Running {
        binary: binary.to_path_buf(),
        child,
        stdin,
        lines,
    };
    let line =
        read_line(&running.lines, &running.child, HELLO_DEADLINE, "hello").map_err(|failure| {
            Failure {
                message: failure.message.replace("stopped", "did not start"),
                ..failure
            }
        })?;
    let hello: Hello = serde_json::from_str(&line)
        .map_err(|_| stopped("Typst support did not start: it said nothing Lattice understands"))?;
    if hello.protocol != PROTOCOL {
        return Err(Failure {
            message: format!(
                "Typst support {} does not match Lattice {}; download it again in Settings",
                hello.version,
                lattice_core::VERSION
            ),
            retry: false,
        });
    }
    Ok(running)
}

fn exchange(running: &mut Running, request: &Request) -> Result<Reply, Failure> {
    let mut line = serde_json::to_string(request).map_err(|error| stopped(error.to_string()))?;
    line.push('\n');
    running
        .stdin
        .write_all(line.as_bytes())
        .and_then(|()| running.stdin.flush())
        .map_err(|error| stopped(error.to_string()))?;
    let answer = read_line(&running.lines, &running.child, REPLY_DEADLINE, "answer")?;
    serde_json::from_str(&answer).map_err(|error| stopped(error.to_string()))
}

/// The running helper for `binary`, started if it is not.
fn running<'a>(slot: &'a mut Option<Running>, binary: &Path) -> Result<&'a mut Running, Failure> {
    if slot
        .as_ref()
        .is_some_and(|running| running.binary != binary)
    {
        *slot = None;
    }
    if slot.is_none() {
        *slot = Some(start(binary)?);
    }
    Ok(slot.as_mut().expect("started above"))
}

/// Start the helper ahead of the first compile, so its fonts are read by
/// the time the writer builds.
pub fn prewarm(binary: &Path) {
    let mut slot = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Err(failure) = running(&mut slot, binary) {
        log::warn!("typst: {}", failure.message);
    }
}

/// Compile `document` (relative to `root`) with `overlay`'s unsaved
/// buffers through the helper at `binary`.
pub fn build(
    binary: &Path,
    root: &Path,
    document: &str,
    overlay: &HashMap<String, String>,
    one_off: bool,
) -> BuildResult {
    let request = Request {
        root: root.to_path_buf(),
        document: document.to_string(),
        overlay: overlay.clone(),
    };
    let stops = STOPS.load(Ordering::SeqCst);
    let mut slot = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
    let mut reply = running(&mut slot, binary).and_then(|running| exchange(running, &request));
    if reply.as_ref().is_err_and(|failure| failure.retry) && STOPS.load(Ordering::SeqCst) == stops {
        // A helper that stopped (it crashed, or was removed and put back)
        // gets one fresh start before the build says so.
        *slot = None;
        reply = running(&mut slot, binary).and_then(|running| exchange(running, &request));
    }
    if reply.is_err() {
        *slot = None;
    }
    reply
        .unwrap_or_else(|failure| Reply::failed(failure.message))
        .into_build(root, document, one_off)
}

/// Stop the helper, before its files are removed: killed at once, even
/// mid-compile (that compile then reports Typst support stopped).
pub fn stop() {
    STOPS.fetch_add(1, Ordering::SeqCst);
    let child = CHILD.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(child) = child {
        kill(&child);
    }
    *RUNNING.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

#[cfg(all(test, unix))]
mod tests;
