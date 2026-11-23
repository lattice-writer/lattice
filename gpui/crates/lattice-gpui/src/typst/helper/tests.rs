use std::{
    os::unix::fs::PermissionsExt as _,
    time::{Duration, Instant},
};

use super::*;

/// A helper script in `dir` that runs `body` after its shebang.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn compile(binary: &Path, root: &Path) -> BuildResult {
    build(binary, root, "main.typ", &HashMap::new(), false)
}

/// A helper that never says hello, or that takes a compile and never
/// answers, is killed at its deadline instead of holding every later build;
/// and `stop` kills a helper mid-compile without waiting for the compile.
/// One test, as the helper is one per process.
#[test]
fn a_hung_helper_is_killed_and_stop_never_waits() {
    let dir = tempfile::tempdir().unwrap();
    let hello = format!(
        "echo '{}'",
        serde_json::to_string(&Hello {
            protocol: PROTOCOL,
            version: "test".into()
        })
        .unwrap()
    );
    let silent = script(dir.path(), "silent", "exec sleep 600");
    let hanging = script(dir.path(), "hanging", &format!("{hello}\nexec sleep 600"));

    let started = Instant::now();
    let result = compile(&silent, dir.path());
    assert!(!result.success);
    assert!(
        result.log.contains("did not start: no hello"),
        "{}",
        result.log
    );
    // The hello's deadline, and no second start: it already had its chance.
    assert!(
        started.elapsed() < HELLO_DEADLINE * 2,
        "{:?}",
        started.elapsed()
    );

    let started = Instant::now();
    let result = compile(&hanging, dir.path());
    assert!(!result.success);
    assert!(result.log.contains("stopped responding"), "{}", result.log);
    assert!(
        started.elapsed() < REPLY_DEADLINE + HELLO_DEADLINE,
        "{:?}",
        started.elapsed()
    );

    let root = dir.path().to_path_buf();
    let binary = hanging.clone();
    let compiling = std::thread::spawn(move || compile(&binary, &root));
    std::thread::sleep(Duration::from_millis(300));
    let stopping = Instant::now();
    stop();
    assert!(
        stopping.elapsed() < Duration::from_millis(500),
        "stop waited {:?}",
        stopping.elapsed()
    );
    let result = compiling.join().unwrap();
    assert!(!result.success);
    assert!(result.log.contains("stopped"), "{}", result.log);
    assert!(RUNNING.lock().unwrap().is_none());

    // A stop while the helper is still starting (before its hello) kills it
    // too, and the compile does not start another.
    let root = dir.path().to_path_buf();
    let compiling = std::thread::spawn(move || compile(&silent, &root));
    std::thread::sleep(Duration::from_millis(300));
    let stopping = Instant::now();
    stop();
    let result = compiling.join().unwrap();
    assert!(
        stopping.elapsed() < Duration::from_millis(500),
        "stop and the compile took {:?}",
        stopping.elapsed()
    );
    assert!(!result.success);
    assert!(result.log.contains("did not start"), "{}", result.log);
}
