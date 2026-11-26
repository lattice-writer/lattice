//! Typst documents, compiled in process.
//!
//! A project whose root document is a `.typ` builds here instead of through
//! latexmk (`lattice_core::latex::build`), and the result has the same shape,
//! a [`BuildResult`], so the build button, ⌘S, the status pill, the error
//! list and the PDF pane treat it as any build. Unlike LaTeX it also compiles
//! the editor's unsaved text: [`build`] takes the open buffers as an overlay,
//! which is what the live preview hands it on each pause in typing.
//!
//! One compiler session is kept (the project and document last built), so a
//! rebuild re-parses only the files that changed and Typst's memoization
//! reuses the layout of everything else. Fonts are scanned once per process.

use std::{
    collections::HashMap,
    path::Path,
    sync::{LazyLock, Mutex, PoisonError},
    time::Instant,
};

use lattice_core::{
    latex::{BuildResult, BuildStep},
    typst_wire::{ClickJump, CursorJump, Jump, PdfJump, SourceJump},
};

mod diagnostics;
mod jump;
mod world;

pub use world::prewarm_fonts;

/// The tool name the build card shows for the one pass.
pub const TOOL: &str = "typst";

/// Whether `path` is a Typst source.
pub fn is_typst(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".typ")
}

static SESSION: LazyLock<Mutex<Option<world::Session>>> = LazyLock::new(Default::default);

/// Compile `document` (relative to `root`) to the PDF beside it, reading
/// `overlay`'s text (project-relative path → buffer) in place of those files
/// on disk. A document with errors leaves the last PDF where it is, as a
/// failed LaTeX build keeps its last good PDF on screen.
pub fn build(root: &Path, document: &str, overlay: &HashMap<String, String>) -> BuildResult {
    let started = Instant::now();
    let mut guard = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    let session = match guard.take() {
        Some(session) if session.is_for(root, document) => session,
        _ => world::Session::new(root, document),
    };
    let session = guard.insert(session);
    let compiled = session.compile(overlay);
    // Memoized results nobody asked for in the last few compiles go, so a
    // long session of typing does not keep every layout it ever made.
    comemo::evict(10);

    let pdf = root.join(document).with_extension("pdf");
    let mut diagnostics = compiled.diagnostics;
    let mut wrote = false;
    if let (Some(bytes), Some(laid_out)) = (compiled.pdf, compiled.document) {
        match write_atomically(root, &pdf, &bytes) {
            Ok(()) => {
                wrote = true;
                // The jumps read the document of the PDF on screen, which a
                // failed compile after this one leaves there.
                session.keep(laid_out);
            }
            Err(error) => diagnostics.push(diagnostics::note(
                "error",
                format!("Could not write {}: {error}", pdf.display()),
            )),
        }
    }
    drop(guard);
    let errors = diagnostics.iter().any(|d| d.level == "error");
    let duration = started.elapsed();
    let duration_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    log::info!(
        "typst: {document} in {duration_ms} ms, {} pages, {} diagnostics",
        compiled.pages,
        diagnostics.len()
    );
    BuildResult {
        success: wrote && !errors,
        has_pdf: pdf.is_file(),
        log: diagnostics::log(&diagnostics),
        duration_ms: duration.as_millis(),
        diagnostics,
        root_document: document.to_string(),
        one_off: false,
        venue: None,
        steps: vec![BuildStep {
            tool: TOOL.into(),
            run: 1,
            draft: false,
            started_ms: 0,
            duration_ms: Some(duration_ms),
        }],
        pages: wrote.then_some(compiled.pages),
        // Typst writes no SyncTeX map, and the card has no row to show for it.
        synctex: None,
        outputs: if wrote {
            vec![
                Path::new(document)
                    .with_extension("pdf")
                    .to_string_lossy()
                    .replace('\\', "/"),
            ]
        } else {
            Vec::new()
        },
        unchanged: false,
        aux_folder: String::new(),
    }
}

/// Run `jump` on the document this session last wrote the PDF of, when
/// that is `root`'s `document`.
fn with_kept<T>(root: &Path, document: &str, jump: impl FnOnce(&jump::Kept) -> Jump<T>) -> Jump<T> {
    let guard = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    match guard
        .as_ref()
        .filter(|session| session.is_for(root, document))
        .and_then(world::Session::kept)
    {
        Some(kept) => jump(kept),
        None => Jump::NotCompiled,
    }
}

/// From a click in the PDF to the source that put it there.
pub fn jump_from_click(request: &ClickJump) -> SourceJump {
    with_kept(&request.root, &request.document, |kept| {
        jump::source_at(
            kept,
            &request.root,
            &request.overlay,
            request.page,
            request.x,
            request.y,
        )
    })
}

/// From a caret in a source to the places in the PDF its text went.
pub fn jump_from_cursor(request: &CursorJump) -> PdfJump {
    with_kept(&request.root, &request.document, |kept| {
        jump::places_of(
            kept,
            &request.root,
            &request.overlay,
            &request.path,
            request.line,
            request.column,
        )
    })
}

/// The PDF pane reads the file while it may be replaced: write it in the
/// project's `.research` folder, which the project watcher leaves alone, and
/// rename it over the PDF, so the pane never sees half a PDF and the watcher
/// sees one change.
fn write_atomically(root: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let folder = root.join(".research");
    std::fs::create_dir_all(&folder)?;
    let temporary = folder.join("typst-output.pdf.partial");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests;
