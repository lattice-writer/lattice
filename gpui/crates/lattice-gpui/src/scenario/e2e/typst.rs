//! A Typst document beside the fixture's LaTeX paper: Build compiles it on
//! its own (a one-off, so the paper stays the default) into its PDF, which
//! the PDF pane shows; an edit, unsaved, compiles live after a pause in
//! typing, its error lands in the error list at its file and line, and
//! following it puts the cursor there; fixing the edit redraws the PDF in
//! place. The edit-to-diagnostic and edit-to-pixels times go in the log.

use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use gpui::AsyncApp;
use serde_json::json;

use super::Steps;
use crate::scenario::Run;

const NOTES: &str = "#set page(width: 12cm, height: 9cm)\n#set heading(numbering: \"1.\")\n= Typst notes <notes>\nSee @notes, and $integral_0^1 x dif x = 1/2$.\n\n#lorem(120)\n";

impl Run {
    async fn typst(&self, cx: &mut AsyncApp, steps: &mut Steps, root: &std::path::Path) -> Result<()> {
        let source = root.join("notes.typ");
        let pdf = root.join("notes.pdf");
        std::fs::write(&source, NOTES)?;
        let editor = self.open(cx, &source, Some(1))?;
        self.pause(cx, Duration::from_millis(200)).await;

        let (built, took) = self.build(cx).await?;
        let (document, one_off) = cx.update(|cx| {
            let shell = self.shell.read(cx);
            let main = shell.project().map(|project| project.read(cx).main_document().map(str::to_string));
            let last = shell.build_panel().read(cx).last().map(|last| last.root_document.clone());
            (last, main)
        });
        let pages = cx.update(|cx| self.shell.read(cx).pdf().read(cx).page_count(cx));
        steps.check(
            "Build compiles a .typ beside a LaTeX paper into its PDF, and the paper stays the default",
            built
                && pdf.is_file()
                && document.as_deref() == Some("notes.typ")
                && one_off.flatten().as_deref() != Some("notes.typ"),
            json!({ "built": built, "seconds": took.as_secs_f32(), "document": document, "pdf": pdf.is_file(), "pages": pages }),
        );
        self.snapshot(cx, "typst-built").await;

        // An error typed and not saved: the live build reports it.
        let shown = |cx: &mut AsyncApp| {
            cx.update(|cx| {
                let view = self.shell.read(cx).pdf().read(cx).live_view().read(cx);
                view.shown_digest().map(str::to_string)
            })
        };
        let before = shown(cx);
        let buffer = cx.update(|cx| editor.read(cx).lattice_editor().read(cx).buffer().clone());
        let line_four = NOTES.match_indices('\n').nth(2).map(|(at, _)| at + 1).context("four lines")?;
        let edited = Instant::now();
        cx.update(|cx| buffer.update(cx, |buffer, cx| buffer.edit([(line_four..line_four, "#nonexistent-call()\n")], None, cx)));
        let reported = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                let panel = shell.read(cx).build_panel().read(cx);
                panel.diagnostics().iter().any(|diagnostic| {
                    diagnostic.level == "error"
                        && diagnostic.file.as_deref() == Some("notes.typ")
                        && diagnostic.line == Some(4)
                        && diagnostic.message.contains("nonexistent-call")
                })
            })
            .await;
        let to_diagnostic = edited.elapsed();
        let unsaved = std::fs::read_to_string(&source).is_ok_and(|text| text == NOTES);
        let error = cx.update(|cx| {
            let panel = self.shell.read(cx).build_panel().read(cx);
            panel
                .diagnostics()
                .iter()
                .find(|diagnostic| diagnostic.level == "error" && diagnostic.message.contains("nonexistent-call"))
                .map(|diagnostic| (diagnostic.message.clone(), panel.location(diagnostic)))
        });
        steps.check(
            "an unsaved edit to a .typ compiles live, and its error is reported at its file and line",
            reported && unsaved,
            json!({ "ms": to_diagnostic.as_millis(), "unsaved": unsaved, "error": error.as_ref().map(|(message, _)| message.clone()) }),
        );
        log::info!("typst e2e: edit to diagnostic {} ms", to_diagnostic.as_millis());

        if let Some((_, Some(location))) = error {
            self.with_window(cx, |shell, window, cx| {
                shell.update(cx, |shell, cx| shell.open_path(location.path.clone(), location.line, window, cx));
            })?;
            self.pause(cx, Duration::from_millis(200)).await;
        }
        let row = self.cursor_row(cx, &editor);
        steps.check("following the Typst error puts the cursor on its line", row == 3, json!({ "row": row + 1 }));

        // Fixed, the live build writes the PDF again and the pane redraws it
        // in place. The fix is new text rather than an undo: an undo gives
        // back the first build's PDF byte for byte, whose digest the pane
        // already shows.
        let fixed = Instant::now();
        let call = "#nonexistent-call()\n";
        cx.update(|cx| buffer.update(cx, |buffer, cx| buffer.edit([(line_four..line_four + call.len(), "Typed live.\n")], None, cx)));
        let redrawn = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                let panel = shell.read(cx).pdf().read(cx);
                let view = panel.live_view().read(cx);
                view.shown_digest().map(str::to_string) != before
                    && panel.pages_drawn(cx) > 0
                    && shell.read(cx).build_panel().read(cx).succeeded() == Some(true)
            })
            .await;
        let to_pixels = fixed.elapsed();
        steps.check(
            "fixing the error recompiles live and the PDF pane shows the new pages",
            redrawn,
            json!({ "ms": to_pixels.as_millis() }),
        );
        log::info!("typst e2e: edit to pixels {} ms", to_pixels.as_millis());
        self.snapshot(cx, "typst-live").await;

        self.close_panel(cx, &editor).await?;
        std::fs::remove_file(&source).ok();
        std::fs::remove_file(&pdf).ok();
        Ok(())
    }
}

/// This file's registration as the e2e module `typst` (`registry.rs`).
pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Repair,
    rank: 90,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-typst/",
        "gpui/crates/lattice-editor/languages/typst/",
    ],
    run: |run, cx, steps, fixture| Box::pin(run.typst(cx, steps, &fixture.root)),
};
