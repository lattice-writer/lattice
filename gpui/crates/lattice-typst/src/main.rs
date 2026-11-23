//! The Typst helper the native app downloads on first use and keeps
//! running: it says hello, then compiles each request it reads on stdin and
//! writes the reply on stdout, one JSON object per line
//! (`lattice_core::typst_wire`). Nothing else goes to stdout. It stops when
//! the app closes its stdin, which the app does by quitting.

use std::io::BufRead as _;

use lattice_core::typst_wire::{Hello, Reply, Request};

fn main() {
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    if send(&mut stdout, &Hello::current()).is_err() {
        return;
    }
    // Scanning the system's fonts takes the first build's longest wait, so
    // it starts now, while the writer is still reading the document.
    std::thread::spawn(lattice_typst::prewarm_fonts);
    for line in stdin.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(request) => Reply::from_build(&lattice_typst::build(
                &request.root,
                &request.document,
                &request.overlay,
            )),
            Err(error) => Reply::failed(format!(
                "The Typst helper could not read the request: {error}"
            )),
        };
        if send(&mut stdout, &reply).is_err() {
            break;
        }
    }
}

fn send(out: &mut impl std::io::Write, message: &impl serde::Serialize) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, message)?;
    out.write_all(b"\n")?;
    out.flush()
}
