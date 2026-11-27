//! A study folder around a manuscript that is a project of its own
//! (lattice#1293; `shell/parent_folder.rs`, `project_menu.rs`, Settings ›
//! Agent › Display, `lattice_core::project::agent_folder`), in a second
//! window so the main one keeps its project:
//!
//! - Opening `ProjectA/`, which holds the project `ProjectA/Manuscript/`,
//!   asks before making a second project, and its first answer opens
//!   Manuscript with the Agent working in `ProjectA/`, with no
//!   `ProjectA/.research` made.
//! - The project menu names the history's folder, the main document and the
//!   Agent's folder.
//! - A revision made in that window, then Manuscript reopened directly: the
//!   revision is in the same history as the earlier writing, and opening
//!   `ProjectA/` again opens Manuscript without asking.
//! - Setting the Agent's folder back to the project's own puts the Agent's
//!   page back in Manuscript.
//! - A folder holding two projects opens as its own project with a notice,
//!   and `Make this folder its own project` does what it says.
//!
//! With `LATTICE_E2E_SHOTS=DIR` (and the `offscreen-shots` feature) it saves
//! the question in English and in Simplified Chinese.

use std::{path::Path, time::Duration};

use anyhow::{Context as _, Result};
use gpui::AsyncApp;
use lattice_core::project;
use lattice_settings::InterfaceLanguage;
use serde_json::json;

use super::Steps;
use crate::{scenario::Run, settings, shell::Shell};

const REVISION: &str = "Revised beside the analysis. ";

impl Run {
    async fn agent_folder(&self, cx: &mut AsyncApp, steps: &mut Steps) -> Result<()> {
        let scratch = tempfile::tempdir()?;
        // The project's root is the folder's canonical path (/private/var
        // for /var on macOS).
        let base = scratch.path().canonicalize()?;
        let study = base.join("ProjectA");
        let manuscript = study.join("Manuscript");
        std::fs::create_dir_all(study.join("analysis"))?;
        std::fs::write(study.join("analysis/results.R"), "summary(fit)\n")?;
        std::fs::create_dir_all(&manuscript)?;
        let original = "\\documentclass{article}\n\\begin{document}\nResults.\n\\end{document}\n";
        std::fs::write(manuscript.join("main.tex"), original)?;
        project::open(&manuscript).map_err(anyhow::Error::msg)?;
        // The writing done before, with Manuscript opened on its own.
        let earlier = original.replace("Results.", "Results hold.");
        project::apply_editor_transaction(
            &manuscript,
            "main.tex".into(),
            earlier.clone(),
            Some(original.into()),
            None,
        )
        .map_err(anyhow::Error::msg)?;
        let history_before = project::history(&manuscript).map_err(anyhow::Error::msg)?.len();

        // A window of its own takes the front, so the whole module runs in a
        // foreground section (`input_lock.rs`).
        let _foreground = self
            .foreground(cx, "a study folder around a project, in a window of its own")
            .await?;
        let (window, shell) = cx.update(|cx| Shell::open_window(None, cx))?;
        let second = self.for_window(shell, window);
        second.pause(cx, Duration::from_millis(300)).await;
        let host = cx.update(crate::confirm::install);
        second.with_window(cx, {
            let study = study.clone();
            move |shell, window, cx| {
                shell.update(cx, |shell, cx| shell.open_picked_folder(study, true, window, cx))
            }
        })?;
        let asked = second
            .wait_for(cx, Duration::from_secs(10), |_, cx| host.read(cx).is_open())
            .await;
        let (question, body) = cx.update(|cx| {
            let host = host.read(cx);
            (host.question(), host.question_body())
        });
        second.agent_folder_shot(cx, "en-question").await;
        steps.check(
            "Opening a folder with one project inside asks to open that project with the whole folder for the Agent, before making a second project",
            asked
                && question.as_deref() == Some("Open Manuscript with the whole folder available to the Agent?")
                && body.as_deref().is_some_and(|body| body.contains("Manuscript/.research"))
                && !study.join(".research").exists(),
            json!({ "question": question, "body": body }),
        );

        second.with_window(cx, |_, window, cx| host.update(cx, |host, cx| host.accept(window, cx)))?;
        let opened = second
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                let shell = shell.read(cx);
                shell.root(cx).as_deref() == Some(manuscript.as_path())
                    && shell.agent_folder(cx).as_deref() == Some(study.as_path())
            })
            .await;
        let (page_project, page_folder) = cx.update(|cx| {
            let shell = second.shell.read(cx);
            let surface = shell.agent_surface(cx).read(cx);
            (
                surface.root().map(Path::to_path_buf),
                surface.agent_root().map(Path::to_path_buf),
            )
        });
        let manifest = project::read_manifest(&manuscript).map_err(anyhow::Error::msg)?;
        steps.check(
            "Its first answer opens Manuscript, with the Agent's page working in ProjectA/ and the history, settings and builds left in Manuscript",
            opened
                && manifest.agent_folder.as_deref() == Some("..")
                && page_project.as_deref() == Some(manuscript.as_path())
                && page_folder.as_deref() == Some(study.as_path())
                && !study.join(".research").exists(),
            json!({
                "agent_folder": manifest.agent_folder,
                "page_project": page_project,
                "page_folder": page_folder,
            }),
        );

        let places = cx.update(|cx| {
            crate::titlebar::places(second.shell.read(cx), cx)
                .map(|places| (places.history, places.main, places.agent))
        });
        steps.check(
            "The project menu names the history's folder, the main document and the Agent's folder",
            places
                == Some((
                    "ProjectA/Manuscript/.research".to_string(),
                    Some("ProjectA/Manuscript/main.tex".to_string()),
                    "ProjectA/".to_string(),
                )),
            json!({ "places": places }),
        );

        // A revision in this window, then Manuscript opened on its own.
        let main = manuscript.join("main.tex");
        let editor = second.open(cx, &main, Some(3))?;
        second.pause(cx, Duration::from_millis(300)).await;
        second.type_text(cx, REVISION).await?;
        let save = cx.update(|cx| editor.update(cx, |editor, cx| editor.save(cx)));
        save.await.context("save main.tex")?;
        let saved = std::fs::read_to_string(&main)?.contains(REVISION);
        let elsewhere = base.join("Elsewhere");
        std::fs::create_dir_all(&elsewhere)?;
        std::fs::write(elsewhere.join("main.tex"), "\\documentclass{article}\n")?;
        for root in [&elsewhere, &manuscript] {
            second.settle_builds(cx).await;
            second.with_window(cx, {
                let root = root.clone();
                move |shell, window, cx| shell.update(cx, |shell, cx| shell.open_project(root, window, cx))
            })?;
            second
                .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                    shell.read(cx).root(cx).as_deref() == Some(root.as_path())
                })
                .await;
        }
        let history = project::history(&manuscript).map_err(anyhow::Error::msg)?;
        // Saves close together join one activity, so the earlier writing
        // and the revision may share an entry: what counts is that both are
        // in Manuscript's history.
        let written: Vec<String> = history
            .iter()
            .filter_map(|item| project::get_history_entry(&manuscript, &item.id).ok())
            .flat_map(|record| record.changes)
            .filter(|change| change.path == "main.tex")
            .flat_map(|change| [change.before, change.after])
            .flatten()
            .collect();
        let has_earlier = written.iter().any(|text| text.contains("Results hold."));
        let has_revision = written.iter().any(|text| text.contains(REVISION));
        second.with_window(cx, {
            let study = study.clone();
            move |shell, window, cx| {
                shell.update(cx, |shell, cx| shell.open_picked_folder(study, true, window, cx))
            }
        })?;
        // `holds()` answers later: a question would come within this wait.
        let asked_again = second
            .wait_for(cx, Duration::from_secs(3), {
                let host = host.clone();
                move |_, cx| host.read(cx).is_open()
            })
            .await;
        let same = cx.update(|cx| second.shell.read(cx).root(cx)) == Some(manuscript.clone());
        steps.check(
            "The revision made through ProjectA/ is in Manuscript's one history beside the earlier writing, and ProjectA/ opens Manuscript again without asking",
            saved
                && history_before >= 1
                && has_earlier
                && has_revision
                && !study.join(".research").exists()
                && !asked_again
                && same,
            json!({
                "saved": saved,
                "history": history.iter().map(|item| (&item.label, &item.files)).collect::<Vec<_>>(),
                "before": history_before,
                "asked_again": asked_again,
            }),
        );

        // Settings › Agent › Display's choice, back to the project's own.
        let shown = cx
            .update(|cx| second.shell.read(cx).project().cloned())
            .context("Manuscript is open")?;
        cx.update(|cx| shown.update(cx, |project, cx| project.set_agent_folder(None, cx)));
        let back = second
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                shell.read(cx).agent_surface(cx).read(cx).agent_root() == Some(manuscript.as_path())
            })
            .await;
        let cleared = project::read_manifest(&manuscript)
            .map_err(anyhow::Error::msg)?
            .agent_folder;
        steps.check(
            "Setting the Agent's folder back to the project's own puts the Agent's page in Manuscript again",
            back && cleared.is_none(),
            json!({ "agent_folder": cleared }),
        );

        // Two projects inside: opened as before, with a notice; and the
        // second answer for a folder with one.
        std::fs::create_dir_all(study.join("Thesis"))?;
        std::fs::write(study.join("Thesis/main.tex"), "\\documentclass{article}\n")?;
        project::open(&study.join("Thesis")).map_err(anyhow::Error::msg)?;
        second.settle_builds(cx).await;
        second.with_window(cx, {
            let study = study.clone();
            move |shell, window, cx| {
                shell.update(cx, |shell, cx| shell.open_picked_folder(study, true, window, cx))
            }
        })?;
        let several = second
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                shell.read(cx).root(cx).as_deref() == Some(study.as_path())
            })
            .await;
        let hinted = cx.update(|cx| {
            crate::notices::install(cx)
                .read(cx)
                .titles()
                .iter()
                .any(|title| title == "This folder holds several Lattice projects")
        });
        let other = base.join("Study");
        std::fs::create_dir_all(other.join("Paper"))?;
        std::fs::write(other.join("Paper/main.tex"), "\\documentclass{article}\n")?;
        project::open(&other.join("Paper")).map_err(anyhow::Error::msg)?;
        second.settle_builds(cx).await;
        second.with_window(cx, {
            let other = other.clone();
            move |shell, window, cx| {
                shell.update(cx, |shell, cx| shell.open_picked_folder(other, true, window, cx))
            }
        })?;
        second
            .wait_for(cx, Duration::from_secs(10), |_, cx| host.read(cx).is_open())
            .await;
        second.with_window(cx, |_, window, cx| {
            host.update(cx, |host, cx| host.choose_alternative(window, cx))
        })?;
        let own = second
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                shell.read(cx).root(cx).as_deref() == Some(other.as_path())
            })
            .await;
        let paper_folder = project::read_manifest(&other.join("Paper"))
            .map_err(anyhow::Error::msg)?
            .agent_folder;
        steps.check(
            "A folder with several projects opens as its own project with a notice, and Make this folder its own project makes it one",
            several
                && hinted
                && own
                && other.join(".research").is_dir()
                && paper_folder.is_none(),
            json!({ "several": several, "hinted": hinted, "own": own }),
        );

        if std::env::var_os("LATTICE_E2E_SHOTS").is_some() {
            let zh = base.join("研究");
            std::fs::create_dir_all(zh.join("Manuscript"))?;
            std::fs::write(zh.join("Manuscript/main.tex"), "\\documentclass{article}\n")?;
            project::open(&zh.join("Manuscript")).map_err(anyhow::Error::msg)?;
            second.settle_builds(cx).await;
            cx.update(|cx| {
                settings::update_appearance(cx, |a| a.interface_language = InterfaceLanguage::ZhCn)
            });
            second.with_window(cx, {
                let zh = zh.clone();
                move |shell, window, cx| {
                    shell.update(cx, |shell, cx| shell.open_picked_folder(zh, true, window, cx))
                }
            })?;
            second
                .wait_for(cx, Duration::from_secs(10), |_, cx| host.read(cx).is_open())
                .await;
            second.agent_folder_shot(cx, "zh-question").await;
            cx.update(|cx| host.update(cx, |host, cx| host.dismiss(cx)));
            cx.update(|cx| {
                settings::update_appearance(cx, |a| a.interface_language = InterfaceLanguage::En)
            });
        }

        cx.update(|cx| crate::notices::install(cx).update(cx, |stack, cx| stack.clear(cx)));
        window
            .update(cx, |_, window, _| window.remove_window())
            .ok();
        Ok(())
    }

    /// A switch while a build runs asks first (the shells'
    /// `ask_before_leaving`), with the question these steps answer: each
    /// switch waits for the build instead.
    async fn settle_builds(&self, cx: &mut AsyncApp) {
        self.wait_for(cx, Duration::from_secs(60), |shell, cx| !shell.read(cx).building())
            .await;
    }

    async fn agent_folder_shot(&self, cx: &mut AsyncApp, name: &str) {
        if std::env::var_os("LATTICE_E2E_SHOTS").is_none() {
            return;
        }
        self.pause(cx, Duration::from_millis(400)).await;
        self.snapshot(cx, &format!("agent-folder-{name}")).await;
    }
}

/// This file's registration as the e2e module `agent_folder` (`registry.rs`).
pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Terminal,
    rank: 31,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-gpui/src/shell/parent_folder.rs",
        "gpui/crates/lattice-gpui/src/titlebar/project_menu.rs",
        "gpui/crates/lattice-gpui/src/settings/agent_display_pane.rs",
        "src-tauri/lattice-core/src/project/agent_folder.rs",
        "src/agent/agent-folder-view.ts",
    ],
    run: |run, cx, steps, _| Box::pin(run.agent_folder(cx, steps)),
};
