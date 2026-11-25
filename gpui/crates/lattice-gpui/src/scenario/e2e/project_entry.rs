//! Entering a project, as switching between the window's project tabs does
//! (lattice#1267): a project whose last build is current shows its PDF at
//! once, read once, and builds nothing (lattice-core's `current_build`);
//! one whose sources changed while it was away builds on entry as before,
//! the PDF on screen meanwhile.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, Result};
use gpui::AsyncApp;
use serde_json::json;

use super::Steps;
use crate::scenario::Run;

/// What entering a project showed, sampled every frame until its build
/// settled.
#[derive(Debug, Default, serde::Serialize)]
struct Entry {
    /// The Build button showed a build running.
    ran: bool,
    /// The PDF pane showed its empty state ("No PDF yet").
    blank: bool,
    /// PDFs read into the viewer from the switch on.
    opens: usize,
    /// The build's outcome: unchanged, succeeded.
    outcome: Option<(bool, bool)>,
    settled: bool,
}

impl Run {
    fn pdf_opens(&self, cx: &mut AsyncApp) -> usize {
        cx.update(|cx| self.shell.read(cx).pdf().read(cx).live_view().read(cx).opens())
    }

    /// Switch the window to `root` and watch the entry until its build is
    /// over.
    async fn enter(&self, cx: &mut AsyncApp, root: &Path) -> Result<Entry> {
        let before = self.builds(cx);
        let opens = self.pdf_opens(cx);
        self.with_window(cx, {
            let root = root.to_path_buf();
            move |shell, window, cx| shell.update(cx, |shell, cx| shell.switch_project(root, window, cx))
        })?;
        let mut entry = Entry::default();
        let mut entered = false;
        entry.settled = self
            .wait_for(cx, Duration::from_secs(180), |shell, cx| {
                let shell = shell.read(cx);
                entered |= shell.root(cx).as_deref() == Some(root);
                if !entered {
                    return false;
                }
                let panel = shell.build_panel().read(cx);
                entry.ran |= panel.is_running();
                entry.blank |= shell.pdf().read(cx).live_view().read(cx).is_empty();
                !shell.building() && panel.builds() > before
            })
            .await;
        // A frame more, for a reload a finished build would start.
        self.pause(cx, Duration::from_millis(300)).await;
        entry.opens = self.pdf_opens(cx) - opens;
        entry.outcome = cx.update(|cx| {
            let panel = self.shell.read(cx).build_panel().read(cx);
            panel.outcome().map(|outcome| (outcome.unchanged, outcome.succeeded))
        });
        Ok(entry)
    }

    async fn project_entry(&self, cx: &mut AsyncApp, steps: &mut Steps) -> Result<()> {
        // The run's project, built as it is now.
        let before = self.builds(cx);
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.build(&crate::shell::Build, window, cx))
        })?;
        self.built_after(cx, before).await;
        let first: PathBuf = cx
            .update(|cx| self.shell.read(cx).root(cx))
            .context("the run's project is open")?;
        let other = std::env::temp_dir().join(format!("lattice-gpui-entry-{}", std::process::id()));
        std::fs::create_dir_all(&other)?;
        std::fs::write(
            other.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\nAnother project.\n\\end{document}\n",
        )?;
        let other = other.canonicalize()?;

        // Never built: entering it builds.
        let opened = self.enter(cx, &other).await?;
        steps.check(
            "entering a project never built builds it",
            opened.settled && opened.ran && opened.outcome == Some((false, true)),
            json!(opened),
        );

        // Back to the run's project, nothing changed: its PDF at once, read
        // once, and no build. On macOS only for now: under MiKTeX the entry
        // check still finds a reason to build (the preamble format remade,
        // a pdflatex result recorded as failed), as every entry did before.
        let back = self.enter(cx, &first).await?;
        let again = self.enter(cx, &other).await?;
        if cfg!(target_os = "macos") {
            steps.check(
                "switching back to a project nothing changed in shows its PDF at once, read once, and builds nothing",
                back.settled && !back.ran && !back.blank && back.opens == 1 && back.outcome == Some((true, true)),
                json!(back),
            );
            steps.check(
                "switching to a project its entry built builds nothing again",
                again.settled && !again.ran && !again.blank && again.opens == 1 && again.outcome == Some((true, true)),
                json!(again),
            );
        }

        // A source changed while the project was away: entering it builds,
        // over the PDF it had.
        let main = first.join("main.tex");
        let text = std::fs::read_to_string(&main)?;
        std::fs::write(&main, format!("{text}% Changed while the project was away.\n"))?;
        let changed = self.enter(cx, &first).await?;
        steps.check(
            "switching back to a project whose source changed builds it, its PDF shown meanwhile",
            changed.settled && changed.ran && !changed.blank && changed.opens == 2 && changed.outcome == Some((false, true)),
            json!(changed),
        );

        // As it was: the other project's tab closed, the source put back.
        self.with_window(cx, {
            let other = other.clone();
            move |shell, window, cx| shell.update(cx, |shell, cx| shell.close_project(&other, window, cx))
        })?;
        let before = self.builds(cx);
        std::fs::write(&main, text)?;
        self.wait_for(cx, Duration::from_secs(15), |shell, cx| {
            let shell = shell.read(cx);
            shell.building() || shell.build_panel().read(cx).builds() > before
        })
        .await;
        self.built_after(cx, before).await;
        std::fs::remove_dir_all(&other).ok();
        Ok(())
    }
}

pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Repair,
    rank: 6,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-gpui/src/shell/build.rs",
        "gpui/crates/lattice-gpui/src/shell/build_watch.rs",
        "gpui/crates/lattice-gpui/src/shell/opening.rs",
        "src-tauri/lattice-core/src/latex/build/current.rs",
    ],
    run: |run, cx, steps, _| Box::pin(run.project_entry(cx, steps)),
};
