//! New manuscript from template… and New project's location preview
//! (lattice#1259; `welcome/new_manuscript.rs`, `shell/manuscript_flow.rs`,
//! `welcome/new_project.rs`):
//!
//! - New project says nothing about where until a parent folder is chosen,
//!   then "Will create ProjectA/Manuscript/", and warns when the parent is
//!   already named for the project (`Manuscript/Manuscript/`).
//! - New manuscript from template…, asked for with a folder selected, offers
//!   a free "Manuscript" folder inside it; created from a venue with Set as
//!   main document, it writes the files there, opens the main file with its
//!   folders opened in the tree and scrolled to, and makes it the main
//!   document, the same project still open and no project made inside.
//! - Asked again for the same folder, it writes nothing and lists the files
//!   already there.
//!
//! With `LATTICE_E2E_SHOTS=DIR` (and the `offscreen-shots` feature) it saves
//! both dialogs, in English and in Simplified Chinese.

use std::{path::Path, time::Duration};

use anyhow::{Context as _, Result};
use gpui::{AsyncApp, Entity};
use lattice_settings::InterfaceLanguage;
use serde_json::json;

use super::Steps;
use crate::{
    scenario::Run,
    settings,
    shell::{commands::NewProject, palette_commands::NewManuscriptInProject},
    welcome::{new_manuscript::NewManuscriptDialog, new_project::NewProjectDialog},
};

const MAIN: &str = "chapters/Manuscript/main.tex";

impl Run {
    async fn manuscripts(&self, cx: &mut AsyncApp, steps: &mut Steps, root: &Path) -> Result<()> {
        let parents = tempfile::tempdir()?;
        let project_a = parents.path().join("ProjectA");
        let doubled = parents.path().join("Manuscript");
        std::fs::create_dir_all(&project_a)?;
        std::fs::create_dir_all(&doubled)?;

        let dialog = self.open_new_project(cx).await?;
        let before = cx.update(|cx| dialog.read(cx).preview(cx));
        self.with_window(cx, {
            let dialog = dialog.clone();
            let project_a = project_a.clone();
            move |_, window, cx| {
                dialog.update(cx, |dialog, cx| {
                    dialog.set_name("Manuscript", window, cx);
                    dialog.set_parent(project_a, cx);
                })
            }
        })?;
        let plain = cx.update(|cx| dialog.read(cx).preview(cx));
        self.manuscript_shot(cx, "en-new-project-preview").await;
        cx.update(|cx| dialog.update(cx, |dialog, cx| dialog.set_parent(doubled.clone(), cx)));
        let twice = cx.update(|cx| dialog.read(cx).preview(cx));
        self.manuscript_shot(cx, "en-new-project-doubled").await;
        steps.check(
            "New project names no folder before a parent is chosen, then the folder it will make, and warns of a doubled Manuscript/Manuscript/",
            before.is_none()
                && plain == Some(("ProjectA/Manuscript/".to_string(), false))
                && twice == Some(("Manuscript/Manuscript/".to_string(), true)),
            json!({ "before": before, "plain": plain, "doubled": twice }),
        );
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.dismiss_new_project(window, cx))
        })?;

        // The manifest as it was, for the steps after these: Set as main
        // document marks its choice there, and a chosen main document changes
        // what a second root document's build does.
        let manifest = root.join(".research/project.json");
        let manifest_before = std::fs::read(&manifest)?;

        // New manuscript from template… with chapters/ selected.
        let tree = cx.update(|cx| self.shell.read(cx).file_tree().clone());
        let manuscript = self.open_new_manuscript_in_chapters(cx, &tree).await?;
        let (folder, preview) = cx.update(|cx| {
            let dialog = manuscript.read(cx);
            (dialog.folder(cx), dialog.preview(cx))
        });
        self.with_window(cx, {
            let manuscript = manuscript.clone();
            move |_, _, cx| {
                manuscript.update(cx, |dialog, cx| {
                    dialog.choose_venue("ieee-conference", cx);
                    dialog.set_main(true, cx);
                })
            }
        })?;
        self.manuscript_shot(cx, "en-new-manuscript").await;
        let project_name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        steps.check(
            "New manuscript from template… offers a free Manuscript folder inside the selected folder, and says where it goes",
            folder == "chapters/Manuscript"
                && preview == format!("{project_name}/chapters/Manuscript/"),
            json!({ "folder": folder, "preview": preview }),
        );

        cx.update(|cx| manuscript.update(cx, |dialog, cx| dialog.create(cx)));
        let opened = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                shell.read(cx).active_document(cx).as_deref() == Some(MAIN)
            })
            .await;
        let main_set = self
            .wait_for(cx, Duration::from_secs(5), |shell, cx| {
                shell
                    .read(cx)
                    .project()
                    .is_some_and(|project| project.read(cx).main_document() == Some(MAIN))
            })
            .await;
        let shown = self
            .wait_for(cx, Duration::from_secs(5), |shell, cx| {
                let tree = shell.read(cx).file_tree().read(cx);
                tree.row_paths(cx).iter().any(|row| row == MAIN)
                    && tree
                        .selection_state(cx)
                        .1
                        .iter()
                        .any(|open| open == "chapters/Manuscript")
            })
            .await;
        let (closed, same_root, rows) = cx.update(|cx| {
            let shell = self.shell.read(cx);
            (
                shell.new_manuscript_dialog().is_none(),
                shell.root(cx).as_deref() == Some(root),
                shell.file_tree().read(cx).row_paths(cx),
            )
        });
        let written = ["main.tex", "IEEEtran.cls", "IEEEtran.bst", "references.bib"]
            .iter()
            .all(|file| root.join("chapters/Manuscript").join(file).is_file());
        self.manuscript_shot(cx, "en-new-manuscript-created").await;
        steps.check(
            "Create manuscript writes the template into chapters/Manuscript, opens its main file with its folder open in the tree, sets it as main, and keeps the same project open with none made inside",
            closed
                && written
                && opened
                && main_set
                && shown
                && same_root
                && !root.join("chapters/Manuscript/.research").exists(),
            json!({
                "closed": closed, "written": written, "opened": opened, "main": main_set,
                "shown": shown, "same_root": same_root, "rows": rows,
            }),
        );

        // The same again: nothing written, the files in the way listed.
        let again = self.open_new_manuscript_in_chapters(cx, &tree).await?;
        self.with_window(cx, {
            let again = again.clone();
            move |_, window, cx| {
                again.update(cx, |dialog, cx| {
                    dialog.set_name("Manuscript", window, cx);
                    dialog.choose_venue("ieee-conference", cx);
                    dialog.create(cx);
                })
            }
        })?;
        let refused = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| !again.read(cx).clash().is_empty())
            .await;
        let clash = cx.update(|cx| again.read(cx).clash().to_vec());
        self.manuscript_shot(cx, "en-new-manuscript-clash").await;
        steps.check(
            "a second manuscript into the same folder writes nothing and lists the files already there",
            refused
                && clash.iter().any(|file| file == MAIN)
                && clash.iter().any(|file| file == "chapters/Manuscript/IEEEtran.cls"),
            json!({ "clash": clash }),
        );
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.close_new_manuscript(window, cx))
        })?;

        if std::env::var_os("LATTICE_E2E_SHOTS").is_some() {
            self.chinese_manuscript_shots(cx, &tree, &doubled).await?;
        }

        // The project as the steps after these expect it: main.tex the main
        // document and the live PDF again (built, so the manuscript's build
        // gives its place back), no tab of the manuscript's left, and the
        // manuscript gone.
        std::fs::write(&manifest, &manifest_before)?;
        cx.update(|cx| {
            if let Some(project) = self.shell.read(cx).project().cloned() {
                project.update(cx, |project, cx| project.reload_manifest(cx));
            }
        });
        self.open(cx, &root.join("main.tex"), Some(1))?;
        self.pause(cx, Duration::from_millis(300)).await;
        self.build(cx).await?;
        self.wait_for(cx, Duration::from_secs(20), |shell, cx| {
            let shell = shell.read(cx);
            shell.live_document() == Some("main.tex")
                && !shell
                    .root_pdf_tabs()
                    .iter()
                    .any(|(document, _)| document == "main.tex")
        })
        .await;
        cx.update(|cx| {
            self.shell.update(cx, |shell, cx| {
                let mut views: Vec<String> = shell
                    .root_pdf_tabs()
                    .into_iter()
                    .filter(|(document, _)| document.starts_with("chapters/Manuscript/"))
                    .map(|(document, _)| crate::shell::views::root_pdf_id(&document))
                    .collect();
                views.extend(shell.view_of_document(MAIN, cx));
                shell
                    .workspace()
                    .clone()
                    .update(cx, |workspace, cx| workspace.close(&views, cx));
            })
        });
        self.pause(cx, Duration::from_millis(300)).await;
        let left = cx.update(|cx| {
            self.shell
                .read(cx)
                .root_pdf_tabs()
                .into_iter()
                .map(|(document, _)| document)
                .collect::<Vec<_>>()
        });
        steps.check(
            "after the manuscript steps the manifest is as it was and main.tex the live PDF again, with no tab of the manuscript's left",
            left.is_empty()
                && std::fs::read(&manifest).ok() == Some(manifest_before)
                && cx.update(|cx| {
                    let shell = self.shell.read(cx);
                    shell.live_document() == Some("main.tex")
                        && shell.view_of_document(MAIN, cx).is_none()
                }),
            json!({ "root_pdf_tabs": left }),
        );
        std::fs::remove_dir_all(root.join("chapters/Manuscript")).ok();
        cx.update(|cx| tree.update(cx, |tree, cx| tree.refresh(cx)));
        self.pause(cx, Duration::from_millis(500)).await;
        Ok(())
    }

    async fn open_new_project(&self, cx: &mut AsyncApp) -> Result<Entity<NewProjectDialog>> {
        self.with_window(cx, |shell, window, cx| {
            window.focus(&shell.read(cx).focus_handle.clone(), cx);
            window.dispatch_action(Box::new(NewProject), cx)
        })?;
        self.wait_for(cx, Duration::from_secs(2), |shell, cx| {
            shell.read(cx).new_project_dialog().is_some()
        })
        .await;
        cx.update(|cx| self.shell.read(cx).new_project_dialog().cloned())
            .context("New project is up")
    }

    /// The palette's New manuscript from template…, chapters/ selected, as
    /// the + menu and the folder's menu ask for it.
    async fn open_new_manuscript_in_chapters(
        &self,
        cx: &mut AsyncApp,
        tree: &Entity<crate::file_tree::FileTreePanel>,
    ) -> Result<Entity<NewManuscriptDialog>> {
        self.with_window(cx, {
            let tree = tree.clone();
            move |shell, window, cx| {
                tree.update(cx, |tree, cx| tree.select_row("chapters", window, cx));
                window.focus(&shell.read(cx).focus_handle.clone(), cx);
                window.dispatch_action(Box::new(NewManuscriptInProject), cx)
            }
        })?;
        self.wait_for(cx, Duration::from_secs(2), |shell, cx| {
            shell.read(cx).new_manuscript_dialog().is_some()
        })
        .await;
        let dialog = cx
            .update(|cx| self.shell.read(cx).new_manuscript_dialog())
            .context("New manuscript from template is up")?;
        self.pause(cx, Duration::from_millis(200)).await;
        Ok(dialog)
    }

    /// Both dialogs in Simplified Chinese: New project's doubled-name
    /// warning, and New manuscript before and after a clash.
    async fn chinese_manuscript_shots(
        &self,
        cx: &mut AsyncApp,
        tree: &Entity<crate::file_tree::FileTreePanel>,
        doubled: &Path,
    ) -> Result<()> {
        cx.update(|cx| {
            settings::update_appearance(cx, |a| a.interface_language = InterfaceLanguage::ZhCn)
        });
        self.pause(cx, Duration::from_millis(200)).await;
        let dialog = self.open_new_project(cx).await?;
        self.with_window(cx, {
            let doubled = doubled.to_path_buf();
            move |_, window, cx| {
                dialog.update(cx, |dialog, cx| {
                    dialog.set_name("Manuscript", window, cx);
                    dialog.set_parent(doubled, cx);
                })
            }
        })?;
        self.manuscript_shot(cx, "zh-new-project-doubled").await;
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.dismiss_new_project(window, cx))
        })?;
        let manuscript = self.open_new_manuscript_in_chapters(cx, tree).await?;
        cx.update(|cx| {
            manuscript.update(cx, |dialog, cx| {
                dialog.choose_venue("ieee-conference", cx);
                dialog.set_main(true, cx);
            })
        });
        self.manuscript_shot(cx, "zh-new-manuscript").await;
        self.with_window(cx, {
            let manuscript = manuscript.clone();
            move |_, window, cx| {
                manuscript.update(cx, |dialog, cx| {
                    dialog.set_name("Manuscript", window, cx);
                    dialog.create(cx);
                })
            }
        })?;
        self.wait_for(cx, Duration::from_secs(5), |_, cx| {
            !manuscript.read(cx).clash().is_empty()
        })
        .await;
        self.manuscript_shot(cx, "zh-new-manuscript-clash").await;
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.close_new_manuscript(window, cx))
        })?;
        cx.update(|cx| {
            settings::update_appearance(cx, |a| a.interface_language = InterfaceLanguage::En)
        });
        self.pause(cx, Duration::from_millis(200)).await;
        Ok(())
    }

    async fn manuscript_shot(&self, cx: &mut AsyncApp, name: &str) {
        if std::env::var_os("LATTICE_E2E_SHOTS").is_none() {
            return;
        }
        self.pause(cx, Duration::from_millis(400)).await;
        self.snapshot(cx, &format!("manuscripts-{name}")).await;
    }
}

/// This file's registration as the e2e module `manuscripts` (`registry.rs`).
pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Repair,
    rank: 36,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-gpui/src/welcome/new_manuscript.rs",
        "gpui/crates/lattice-gpui/src/shell/manuscript_flow.rs",
        "gpui/crates/lattice-gpui/src/welcome/new_project.rs",
        "gpui/crates/lattice-gpui/src/file_tree/menu.rs",
        "src-tauri/lattice-core/src/project/manuscript.rs",
    ],
    run: |run, cx, steps, fixture| Box::pin(run.manuscripts(cx, steps, &fixture.root)),
};
