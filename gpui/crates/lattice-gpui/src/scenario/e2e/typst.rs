//! A Typst document beside the fixture's LaTeX paper, with Typst support not
//! installed (`crate::typst`): the first Build asks to download it, with its
//! size, and Not now leaves the project as it was, typing compiling nothing;
//! the next Build asks again, and Download installs the helper from a local
//! stand-in release (gpui/script/e2e makes it, signed with a key of the
//! run's own, `LATTICE_E2E_TYPST_RELEASE`) and runs the build that asked.
//! That build compiles the document on its own (a one-off, so the paper
//! stays the default) into its PDF, which the PDF pane shows; an edit,
//! unsaved, compiles live after a pause in
//! typing, its error lands in the error list at its file and line, and
//! following it puts the cursor there; fixing the edit redraws the PDF in
//! place. The edit-to-diagnostic and edit-to-pixels times go in the log.

use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use gpui::AsyncApp;
use serde_json::json;

use super::Steps;
use crate::{native, scenario::Run};

const PART: &str = "== Included part\nThe included sentence sits here, on a line of its own.\n";

const NOTES: &str = "#set page(width: 12cm, height: 9cm)\n#set heading(numbering: \"1.\")\n= Typst notes <notes>\nSee @notes, and $integral_0^1 x dif x = 1/2$.\n\n#lorem(120)\n";

impl Run {
    async fn typst(&self, cx: &mut AsyncApp, steps: &mut Steps, root: &std::path::Path) -> Result<()> {
        let source = root.join("notes.typ");
        let pdf = root.join("notes.pdf");
        std::fs::write(&source, NOTES)?;
        let editor = self.open(cx, &source, Some(1))?;
        self.pause(cx, Duration::from_millis(200)).await;

        let release = std::env::var_os("LATTICE_E2E_TYPST_RELEASE").map(std::path::PathBuf::from);
        let archive = release.as_ref().map(|release| release.join("lattice-typst.tar.gz"));
        let public_key = release
            .as_ref()
            .and_then(|release| std::fs::read_to_string(release.join("key.pub")).ok());
        let (Some(archive), Some(public_key)) = (archive, public_key) else {
            steps.check(
                "the first Typst build asks to download Typst support, with its size, and Not now keeps the project as it was",
                false,
                json!({ "error": "no stand-in release: LATTICE_E2E_TYPST_RELEASE (gpui/script/e2e makes it from target/release/lattice-typst)" }),
            );
            return Ok(());
        };
        cx.update(|cx| {
            crate::typst::use_source(
                crate::typst::install::Source {
                    urls: vec![format!("file://{}", archive.display())],
                    public_key,
                },
                cx,
            )
        });
        let question = |cx: &mut AsyncApp| {
            cx.update(|cx| crate::confirm::install(cx).read(cx).question().map(|question| question.to_string()))
        };

        // Not installed: Build asks, and Not now leaves everything as it was.
        let (declined_build, _) = self.build(cx).await?;
        let asked = self
            .wait_for(cx, Duration::from_secs(10), |_, cx| crate::confirm::install(cx).read(cx).is_open())
            .await;
        let asked_title = question(cx);
        self.with_window(cx, |_, _, cx| crate::confirm::install(cx).update(cx, |host, cx| host.dismiss(cx)))?;
        let reported = cx.update(|cx| {
            self.shell
                .read(cx)
                .build_panel()
                .read(cx)
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.message.contains("Typst support"))
        });
        let buffer = cx.update(|cx| editor.read(cx).lattice_editor().read(cx).buffer().clone());
        cx.update(|cx| buffer.update(cx, |buffer, cx| buffer.edit([(0..0, "// typed\n")], None, cx)));
        self.pause(cx, Duration::from_millis(600)).await;
        let typing_quiet = cx.update(|cx| !self.shell.read(cx).building() && !crate::confirm::install(cx).read(cx).is_open());
        cx.update(|cx| buffer.update(cx, |buffer, cx| buffer.undo(cx)));
        steps.check(
            "the first Typst build asks to download Typst support, with its size, and Not now keeps the project as it was",
            !declined_build
                && asked
                && asked_title.as_deref().is_some_and(|title| title.contains("one-time download (~") && title.contains("MB"))
                && reported
                && !pdf.exists()
                && typing_quiet,
            json!({ "asked": asked_title, "reported": reported, "pdf": pdf.exists(), "typing_quiet": typing_quiet }),
        );

        // Build asks again; Download installs the helper and runs the build.
        let started = Instant::now();
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.build(&crate::shell::Build, window, cx))
        })?;
        let asked_again = self
            .wait_for(cx, Duration::from_secs(10), |_, cx| crate::confirm::install(cx).read(cx).is_open())
            .await;
        self.with_window(cx, |_, window, cx| {
            crate::confirm::install(cx).update(cx, |host, cx| host.accept(window, cx))
        })?;
        let installed = self
            .wait_for(cx, Duration::from_secs(60), |_, cx| crate::typst::binary(cx).is_some())
            .await;
        let installed_at = Instant::now();
        let rebuilt = self
            .wait_for(cx, Duration::from_secs(60), |shell, cx| {
                !shell.read(cx).building() && pdf.is_file()
            })
            .await;
        // The Build button's time for the build that started the helper is
        // the wall clock the writer waited, the helper's start with it, not
        // the compile alone (it read "0.0 s").
        let waited = installed_at.elapsed();
        let pill = cx.update(|cx| self.shell.read(cx).build_panel().read(cx).chip());
        let compiled = cx.update(|cx| {
            self.shell
                .read(cx)
                .build_panel()
                .read(cx)
                .outcome()
                .map(|outcome| outcome.steps.iter().filter_map(|step| step.duration_ms).sum::<u64>())
        });
        let took = match pill {
            crate::build_panel::ChipState::Succeeded(took) => Some(took),
            _ => None,
        };
        steps.check(
            "the first Typst build's time on the Build button is the wall clock from Build, the helper's start included",
            took.is_some_and(|took| {
                took >= Duration::from_millis(50)
                    && took <= started.elapsed()
                    && compiled.is_none_or(|ms| took.as_millis() >= u128::from(ms))
            }),
            json!({
                "pill_s": took.map(|took| took.as_secs_f32()),
                "compile_ms": compiled,
                "since_install_ms": waited.as_millis(),
            }),
        );
        steps.check(
            "Download installs Typst support from the release, checked with its key, and runs the build that asked",
            asked_again && installed && rebuilt,
            json!({ "asked_again": asked_again, "installed": installed, "built": rebuilt, "ms": started.elapsed().as_millis() }),
        );

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
                    && panel.page_count(cx) > 0
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

        self.typst_jumps(cx, steps, root, &source, &editor, &buffer).await?;

        self.close_panel(cx, &editor).await?;
        std::fs::remove_file(&source).ok();
        std::fs::remove_file(&pdf).ok();
        Ok(())
    }

    /// Source ↔ PDF without SyncTeX: the notes include a second file,
    /// unsaved, and Jump to PDF from that file's second line marks it on the
    /// last page; a double-click on the mark comes back to that line.
    async fn typst_jumps(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        root: &std::path::Path,
        source: &std::path::Path,
        editor: &gpui::Entity<crate::editor_panel::EditorPanel>,
        buffer: &gpui::Entity<language::Buffer>,
    ) -> Result<()> {
        let part = root.join("notes-part.typ");
        std::fs::write(&part, PART)?;
        let builds = cx.update(|cx| self.shell.read(cx).build_panel().read(cx).builds());
        cx.update(|cx| {
            buffer.update(cx, |buffer, cx| {
                let end = buffer.len();
                buffer.edit([(end..end, "#include \"notes-part.typ\"\n")], None, cx)
            })
        });
        let included = self
            .wait_for(cx, Duration::from_secs(20), |shell, cx| {
                let shell = shell.read(cx);
                let panel = shell.build_panel().read(cx);
                !shell.building() && panel.builds() > builds && panel.succeeded() == Some(true)
            })
            .await;
        // The autosave of the notes builds them again (an automatic build
        // of the document in front): let it land first, or it would build
        // the part on its own once the part is in front (no main document).
        let settled = self
            .wait_for(cx, Duration::from_secs(20), |shell, cx| {
                !editor.read(cx).is_dirty(cx) && !shell.read(cx).building()
            })
            .await;
        self.pause(cx, Duration::from_millis(500)).await;
        let settled = settled
            && self
                .wait_for(cx, Duration::from_secs(20), |shell, cx| !shell.read(cx).building())
                .await;

        let part_editor = self.open(cx, &part, Some(2))?;
        self.pause(cx, Duration::from_millis(300)).await;
        // What ⌘⇧J, Jump to PDF and the PDF toolbar's locate button run.
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.forward_sync(&crate::shell::ForwardSync, window, cx))
        })?;
        let marked = self
            .wait_for(cx, Duration::from_secs(20), |shell, cx| {
                shell.read(cx).pdf().read(cx).sync_mark(cx).is_some_and(|(_, scroll)| !scroll)
            })
            .await;
        let (mark, pages) = cx.update(|cx| {
            let pdf = self.shell.read(cx).pdf().read(cx);
            (pdf.sync_mark(cx).map(|(page, _)| page), pdf.page_count(cx))
        });
        steps.check(
            "Typst: Jump to PDF from an included file's line marks it in the PDF, compiled from unsaved text",
            included && settled && marked && pages > 0 && mark == Some(pages - 1),
            json!({ "included": included, "settled": settled, "mark": mark.map(|page| page + 1), "pages": pages }),
        );
        self.snapshot(cx, "typst-jump-to-pdf").await;

        // Somewhere else first, so the jump back is seen to land.
        self.open(cx, source, Some(1))?;
        self.pause(cx, Duration::from_millis(400)).await;
        let center = cx.update(|cx| self.shell.read(cx).pdf().read(cx).sync_mark_center(cx));
        let landed = match center {
            Some(center) => {
                native::send_double_click(self.native(cx)?, center)?;
                self.wait_for(cx, Duration::from_secs(20), |shell, cx| {
                    shell
                        .read(cx)
                        .active_editor_path(cx)
                        .is_some_and(|path| path.ends_with("notes-part.typ"))
                })
                .await
            }
            None => false,
        };
        let row = self.cursor_row(cx, &part_editor);
        steps.check(
            "Typst: a double-click in the PDF puts the cursor on its line in the included file",
            landed && row == 1,
            json!({ "at": center.map(|c| [f32::from(c.x), f32::from(c.y)]), "row": row + 1 }),
        );
        self.snapshot(cx, "typst-jump-to-source").await;

        self.close_panel(cx, &part_editor).await?;
        std::fs::remove_file(&part).ok();
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
        "gpui/crates/lattice-gpui/src/typst/",
        "gpui/crates/lattice-gpui/src/shell/typst_sync.rs",
        "gpui/crates/lattice-editor/languages/typst/",
    ],
    run: |run, cx, steps, fixture| Box::pin(run.typst(cx, steps, &fixture.root)),
};
