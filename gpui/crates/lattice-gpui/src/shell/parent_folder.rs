//! Opening a folder that is no project but holds one (lattice#1293): a
//! study folder `ProjectA/` with the manuscript `ProjectA/Manuscript/`, a
//! project of its own, inside it.
//!
//! Opened as it is, the folder would become a second project with a
//! `.research` of its own, and the manuscript's revisions made through it
//! would go to a second history nobody notices. So before that, the writer is
//! asked: open the manuscript with the whole folder available to the Agent
//! (the manuscript stays the project; only the Agent's working folder widens,
//! `lattice_core::project::agent_folder`), or make the folder its own
//! project, as before. A folder whose one manuscript already gives its Agent
//! that folder opens the manuscript without asking again.
//!
//! A folder that holds several projects opens as before, with a notice that
//! it became a project of its own: choosing among manuscripts is not offered
//! yet. Neither is the home folder, which the Agent is never given.

use std::path::{Path, PathBuf};

use gpui::{AppContext as _, Context, Window};
use lattice_core::project;
use lattice_i18n::t;
use lattice_ui::components::ToastLevel;

use super::Shell;
use crate::{
    confirm::{self, Confirm},
    notices::{self, Notice},
};

/// What a picked folder holds, read before it is opened.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Holds {
    /// A project, or nothing a project is kept in: it opens as it is.
    Itself,
    /// One project inside, which may give its Agent the folder; `given` when
    /// it already does.
    One { project: PathBuf, given: bool },
    /// Several projects inside.
    Several,
}

/// What `folder` holds.
pub(crate) fn holds(folder: &Path) -> Holds {
    if folder.join(".research").is_dir() {
        return Holds::Itself;
    }
    match project::projects_inside(folder, 2).as_slice() {
        [] => Holds::Itself,
        [_, _, ..] => Holds::Several,
        [one] if !project::may_widen_to(folder) => {
            log::info!(
                "{} is not offered to the Agent of {}",
                folder.display(),
                one.display()
            );
            Holds::Itself
        }
        [one] => {
            let given = project::read_manifest(one)
                .is_ok_and(|manifest| project::agent_folder(one, &manifest) == folder);
            Holds::One {
                project: one.clone(),
                given,
            }
        }
    }
}

fn name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

impl Shell {
    /// Open a folder the writer picked (⌘O, Open another folder, the welcome
    /// page), asking first when it holds a project of its own.
    pub(crate) fn open_picked_folder(
        &mut self,
        folder: PathBuf,
        here: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let looked = folder.clone();
        let found = cx.background_spawn(async move { holds(&looked) });
        cx.spawn_in(window, async move |this, cx| {
            let found = found.await;
            this.update_in(cx, |this, window, cx| {
                this.open_holding(folder, found, here, window, cx)
            })
            .ok();
        })
        .detach();
    }

    fn open_holding(
        &mut self,
        folder: PathBuf,
        found: Holds,
        here: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match found {
            Holds::Itself => self.reveal_project(folder, here, window, cx),
            Holds::One {
                project,
                given: true,
            } => self.reveal_project(project, here, window, cx),
            Holds::Several => {
                self.reveal_project(folder, here, window, cx);
                notices::show(
                    Notice::new(ToastLevel::Info, t!("This folder holds several Lattice projects"))
                        .detail(t!(
                            "Lattice opened it as a project of its own, with a history of its own. To keep working in a manuscript’s history, open the manuscript’s folder instead."
                        )),
                    cx,
                );
            }
            Holds::One { project, .. } => self.ask_parent_folder(folder, project, here, cx),
        }
    }

    fn ask_parent_folder(
        &mut self,
        folder: PathBuf,
        inside: PathBuf,
        here: bool,
        cx: &mut Context<Self>,
    ) {
        let (folder_name, project_name) = (name(&folder), name(&inside));
        let shell = cx.entity().downgrade();
        let alone = shell.clone();
        let own = folder.clone();
        let question = Confirm::new(t!(
            "Open {project} with the whole folder available to the Agent?",
            project = project_name.clone()
        ))
        .body(t!(
            "{folder} has no Lattice history of its own, and {project} inside it is a Lattice project. Opening {project} keeps its history, settings and builds in {project}/.research, and lets the Agent work in all of {folder}. You can change the Agent’s folder in Settings › Agent › Display.",
            folder = folder_name.clone(),
            project = project_name.clone()
        ))
        .confirm_label(t!("Open {project}", project = project_name))
        .alternative(t!("Make this folder its own project"), move |window, cx| {
            let folder = own.clone();
            alone
                .update(cx, |shell, cx| shell.reveal_project(folder, here, window, cx))
                .ok();
        })
        .on_confirm(move |window, cx| {
            let (folder, inside) = (folder.clone(), inside.clone());
            shell
                .update(cx, |shell, cx| shell.open_with_agent_folder(inside, folder, here, window, cx))
                .ok();
        });
        confirm::ask(question, cx);
    }

    /// Open the project `inside` with its Agent given `folder`, the folder
    /// above it the writer opened.
    fn open_with_agent_folder(
        &mut self,
        inside: PathBuf,
        folder: PathBuf,
        here: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = inside.clone();
        let written =
            cx.background_spawn(async move { project::set_agent_folder(&root, Some(&folder)) });
        cx.spawn_in(window, async move |this, cx| {
            let written = written.await;
            this.update_in(cx, |this, window, cx| match written {
                Ok(_) => this.reveal_project(inside, here, window, cx),
                Err(error) => notices::error(t!("Could not change the Agent’s folder"), error, cx),
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_read_for_the_projects_it_holds() {
        let scratch = tempfile::tempdir().unwrap();
        let study = scratch.path().join("ProjectA");
        std::fs::create_dir_all(study.join("analysis")).unwrap();
        assert_eq!(holds(&study), Holds::Itself);
        let manuscript = study.join("Manuscript");
        std::fs::create_dir_all(&manuscript).unwrap();
        std::fs::write(manuscript.join("main.tex"), "\\documentclass{article}\n").unwrap();
        project::open(&manuscript).unwrap();
        assert_eq!(
            holds(&study),
            Holds::One {
                project: manuscript.clone(),
                given: false
            }
        );
        std::fs::create_dir_all(study.join("Thesis/.research")).unwrap();
        assert_eq!(holds(&study), Holds::Several);
        std::fs::create_dir_all(study.join(".research")).unwrap();
        assert_eq!(holds(&study), Holds::Itself);
    }
}
