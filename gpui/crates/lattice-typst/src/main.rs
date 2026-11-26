//! The Typst helper the native app downloads on first use and keeps
//! running: it says hello, then answers each request it reads on stdin (a
//! compile, or a jump between source and PDF) on stdout, one JSON object per
//! line
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
        let sent = match serde_json::from_str::<Request>(&line) {
            Ok(Request::Compile(request)) => send(
                &mut stdout,
                &Reply::from_build(&lattice_typst::build(
                    &request.root,
                    &request.document,
                    &request.overlay,
                )),
            ),
            Ok(Request::JumpFromClick(request)) => {
                send(&mut stdout, &lattice_typst::jump_from_click(&request))
            }
            Ok(Request::JumpFromCursor(request)) => {
                send(&mut stdout, &lattice_typst::jump_from_cursor(&request))
            }
            Err(error) => send(
                &mut stdout,
                &Reply::failed(format!(
                    "The Typst helper could not read the request: {error}"
                )),
            ),
        };
        if sent.is_err() {
            break;
        }
    }
}

fn send(out: &mut impl std::io::Write, message: &impl serde::Serialize) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, message)?;
    out.write_all(b"\n")?;
    out.flush()
}
