//! The running Typst helper: started on the first compile (or ahead of it,
//! when a .typ opens) and kept, so its compiler session and fonts carry from
//! one keystroke's compile to the next. One request at a time; a helper that
//! stopped is started again once before the compile is reported failed.

use std::{
    collections::HashMap,
    io::{BufRead as _, BufReader, Write as _},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{LazyLock, Mutex, PoisonError},
};

use lattice_core::{
    latex::BuildResult,
    typst_wire::{Hello, PROTOCOL, Reply, Request},
};

struct Running {
    binary: PathBuf,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

static RUNNING: LazyLock<Mutex<Option<Running>>> = LazyLock::new(Default::default);

fn start(binary: &Path) -> Result<Running, String> {
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            format!(
                "Could not start Typst support ({}): {error}",
                binary.display()
            )
        })?;
    let stdin = child.stdin.take().ok_or("Typst support has no input")?;
    let mut stdout = BufReader::new(child.stdout.take().ok_or("Typst support has no output")?);
    let mut line = String::new();
    stdout
        .read_line(&mut line)
        .map_err(|error| format!("Typst support did not start: {error}"))?;
    let hello: Hello = serde_json::from_str(&line).map_err(|_| {
        "Typst support did not start: it said nothing Lattice understands".to_string()
    })?;
    if hello.protocol != PROTOCOL {
        return Err(format!(
            "Typst support {} does not match Lattice {}; download it again in Settings",
            hello.version,
            lattice_core::VERSION
        ));
    }
    Ok(Running {
        binary: binary.to_path_buf(),
        child,
        stdin,
        stdout,
    })
}

fn exchange(running: &mut Running, request: &Request) -> Result<Reply, String> {
    let mut line = serde_json::to_string(request).map_err(|error| error.to_string())?;
    line.push('\n');
    running
        .stdin
        .write_all(line.as_bytes())
        .and_then(|()| running.stdin.flush())
        .map_err(|error| error.to_string())?;
    let mut answer = String::new();
    match running.stdout.read_line(&mut answer) {
        Ok(0) => Err("Typst support stopped".to_string()),
        Ok(_) => serde_json::from_str(&answer).map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// The running helper for `binary`, started if it is not.
fn running<'a>(slot: &'a mut Option<Running>, binary: &Path) -> Result<&'a mut Running, String> {
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
    if let Err(error) = running(&mut slot, binary) {
        log::warn!("typst: {error}");
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
    let mut slot = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
    let mut reply = running(&mut slot, binary).and_then(|running| exchange(running, &request));
    if reply.is_err() {
        // A helper that stopped (it crashed, or was removed and put back)
        // gets one fresh start before the build says so.
        *slot = None;
        reply = running(&mut slot, binary).and_then(|running| exchange(running, &request));
    }
    if reply.is_err() {
        *slot = None;
    }
    reply
        .unwrap_or_else(Reply::failed)
        .into_build(root, document, one_off)
}

/// Stop the helper, before its files are removed.
pub fn stop() {
    *RUNNING.lock().unwrap_or_else(PoisonError::into_inner) = None;
}
