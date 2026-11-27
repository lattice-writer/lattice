//! The project switcher's menu, as the Tauri app's (`src/project/project-menu.tsx`):
//! the recent projects other than this one (eight at most, an Overleaf
//! project on the Cloud glyph), each opened in a window of its own by its
//! row or in this window by the row's trailing window glyph, and with more
//! than eight, "All recent projects…", the recent project finder; then Open another folder (with its own in-this-window
//! glyph), New project, Clone repository…, Open from Overleaf (and, with a
//! linked project, Pause or Resume Overleaf sync), the Guided
//! tutorial, and Settings, the titlebar's way into Settings, as the app's
//! menu ends.
//!
//! The glyph is the row's secondary action (`MenuAction::secondary`): the
//! keys reach it with → from its row and run it with Enter, as the app's
//! glyph is an item of its own.
//!
//! First, under This project, where the shown project keeps what it is
//! (lattice#1293), as paths from the folder around the Agent's: the folder
//! that holds its history (`.research`), the main document its builds make,
//! and the folder the Agent works in, which is the project's own unless the
//! project gave the Agent the folder around it. Each opens what it names: the
//! History panel, the main document, Settings › Agent › Display.
//!
//! Export ZIP and Export for arXiv… follow Open
//! from Overleaf, and while an export runs the menu ends with "Exporting ZIP…"
//! after a loader, and the rows that would switch or replace the project
//! under it (the recent projects and their glyphs, Open from Overleaf, the
//! Guided tutorial) are dimmed, as the app's `busy` dims them.

use std::path::{Path, PathBuf};

use gpui::{Action as _, App, IntoElement, Styled as _, WeakEntity, Window, px};
use lattice_i18n::t;
use lattice_settings::SettingsTab;
use lattice_ui::{
    Icon,
    components::{Menu, MenuAction, MenuPopover, ToolbarButton},
};

use crate::{
    recents::{RecentProject, RecentProjects},
    shell::{
        Shell,
        commands::{
            CloneRepository, CloseProject, NewProject, OpenFolder, OpenFolderHere,
            OpenRecentProject,
        },
    },
};

/// How many other projects the menu lists.
const MAX_SWITCHER_PROJECTS: usize = 8;

pub(super) fn project_menu(
    shell: WeakEntity<Shell>,
    menu: Menu,
    _: &mut Window,
    cx: &mut App,
) -> Menu {
    let current = shell.upgrade().and_then(|shell| shell.read(cx).root(cx));
    let moving = shell
        .upgrade()
        .is_some_and(|shell| shell.read(cx).project_drag_view().2);
    let exporting = shell
        .upgrade()
        .is_some_and(|shell| shell.read(cx).exporting_zip());
    // A linked project's sync, while it may sync or is paused (GPUI's own:
    // the Tauri menu has no row for it); `Some(true)` while paused.
    let overleaf_paused = shell.upgrade().and_then(|shell| {
        let bridge = shell.read(cx).overleaf.read(cx);
        (bridge.paused() || bridge.link().is_some()).then(|| bridge.paused())
    });
    RecentProjects::refresh(cx);
    let others = RecentProjects::others(current.as_deref(), cx);
    let more = others.len() > MAX_SWITCHER_PROJECTS;
    let mut menu = menu.min_w(px(208.));
    if let Some(places) = shell.upgrade().and_then(|shell| places(shell.read(cx), cx)) {
        let (history, main, agent) = (shell.clone(), shell.clone(), shell.clone());
        let main_document = places.main_document.clone();
        menu = menu
            .label(t!("This project"))
            .item(
                MenuAction::new(
                    t!("History in {path}", path = places.history),
                    move |window, cx| {
                        history
                            .update(cx, |shell, cx| shell.open_history(window, cx))
                            .ok();
                    },
                )
                .icon(Icon::HISTORY),
            )
            .item(
                MenuAction::new(
                    match &places.main {
                        Some(path) => t!("Builds {path}", path = path.clone()),
                        None => t!("No main document").to_string(),
                    },
                    move |window, cx| {
                        let Some(path) = main_document.clone() else {
                            return;
                        };
                        main.update(cx, |shell, cx| shell.open_path(path, None, window, cx))
                            .ok();
                    },
                )
                .icon(Icon::FILE_TEXT)
                .disabled(places.main.is_none()),
            )
            .item(
                MenuAction::new(
                    t!("Agent works in {path}", path = places.agent),
                    move |window, cx| {
                        agent
                            .update(cx, |shell, cx| {
                                shell.open_settings(Some(SettingsTab::AgentDisplay), window, cx)
                            })
                            .ok();
                    },
                )
                .icon(Icon::ROBOT),
            )
            .separator();
    }
    // With no other project to go back to, the menu starts at its actions.
    if !others.is_empty() {
        menu = menu.label(t!("Recent projects"));
        for RecentProject {
            name,
            path,
            overleaf,
            ..
        } in others.into_iter().take(MAX_SWITCHER_PROJECTS)
        {
            let here = OpenRecentProject {
                path: path.clone(),
                here: true,
            };
            menu = menu.item(
                MenuAction::dispatch(
                    name.clone(),
                    Box::new(OpenRecentProject { path, here: false }),
                )
                .icon(if overleaf { Icon::CLOUD } else { Icon::FOLDER })
                .disabled(exporting)
                .secondary(
                    Icon::APP_WINDOW,
                    lattice_i18n::space_mixed_script(&t!(
                        "Open {name} in this window",
                        name = name
                    )),
                    move |window, cx| window.dispatch_action(here.boxed_clone(), cx),
                ),
            );
        }
        // On its shell directly, as Settings below is.
        if more {
            let shell = shell.clone();
            menu = menu.item(
                MenuAction::new(t!("All recent projects…"), move |window, cx| {
                    shell
                        .update(cx, |shell, cx| shell.open_recent_finder(window, cx))
                        .ok();
                })
                .icon(Icon::SEARCH)
                .disabled(exporting),
            );
        }
        menu = menu.separator();
    }
    let mut menu = menu
        .item(
            MenuAction::dispatch(t!("Open another folder"), Box::new(OpenFolder))
                .icon(Icon::FOLDER_OPEN)
                .secondary(
                    Icon::APP_WINDOW,
                    t!("Open a folder in this window"),
                    |window, cx| window.dispatch_action(Box::new(OpenFolderHere), cx),
                ),
        )
        .item(MenuAction::dispatch(t!("New project"), Box::new(NewProject)).icon(Icon::PLUS))
        .item(
            MenuAction::dispatch(t!("Clone repository…"), Box::new(CloneRepository))
                .icon(Icon::GIT_BRANCH),
        )
        .item(
            MenuAction::dispatch(
                t!("Open from Overleaf"),
                Box::new(crate::shell::overleaf_flow::OpenOverleafProject),
            )
            .icon(Icon::CLOUD)
            .disabled(exporting),
        );
    // On its shell directly, as Settings below is.
    if let Some(paused) = overleaf_paused {
        let shell = shell.clone();
        let (label, icon) = if paused {
            (t!("Resume Overleaf sync"), Icon::CLOUD)
        } else {
            (t!("Pause Overleaf sync"), Icon::CLOUD_OFF)
        };
        menu = menu.item(
            MenuAction::new(label, move |_, cx| {
                shell
                    .update(cx, |shell, cx| {
                        if paused {
                            shell.resume_overleaf_sync(cx)
                        } else {
                            shell.pause_overleaf_sync(cx)
                        }
                    })
                    .ok();
            })
            .icon(icon),
        );
    }
    let menu = menu
        .item(
            MenuAction::dispatch(t!("Export ZIP"), Box::new(crate::shell::ExportProjectZip))
                .icon(Icon::FILE_ARCHIVE),
        )
        .item(
            MenuAction::dispatch(t!("Export for arXiv…"), Box::new(crate::shell::ExportArxiv))
                .icon(Icon::PACKAGE_CHECK),
        )
        .separator()
        .item(
            MenuAction::new(t!("Guided tutorial"), {
                let shell = shell.clone();
                move |window, cx| {
                    shell
                        .update(cx, |shell, cx| shell.open_tutorial(window, cx))
                        .ok();
                }
            })
            .icon(Icon::SPARKLES)
            .disabled(exporting),
        )
        // Settings opens on its shell directly, as the app's `onSettings` does,
        // rather than as an action dispatched from whatever focus the menu
        // interrupted: a press on the titlebar leaves no focused view on the
        // shell's dispatch path to carry it there.
        .item(
            MenuAction::new(t!("Settings"), move |window, cx| {
                shell
                    .update(cx, |shell, cx| shell.open_settings(None, window, cx))
                    .ok();
            })
            .icon(Icon::SETTINGS),
        );
    // The export or a tab's move under way, as the Tauri menu's
    // `busyLabel` line.
    if moving {
        menu.busy(t!("Moving project tab…"))
    } else if exporting {
        menu.busy(t!("Exporting ZIP…"))
    } else {
        menu
    }
}

/// Where the shown project keeps what it is, as the menu names them.
pub(crate) struct Places {
    /// The folder that holds the project's `.research`.
    pub history: String,
    /// The main document, when the project has one.
    pub main: Option<String>,
    /// The folder the Agent works in.
    pub agent: String,
    main_document: Option<PathBuf>,
}

/// The shown project's places, as paths from the folder around the Agent's
/// (`ProjectA/Manuscript/.research`, `ProjectA/Manuscript/main.tex`,
/// `ProjectA/`), so the three read alike whichever folder was opened.
pub(crate) fn places(shell: &Shell, cx: &App) -> Option<Places> {
    let project = shell.project()?.read(cx);
    let root = project.root();
    let agent = project.agent_root();
    let base = agent.parent().unwrap_or(&agent);
    let shown = |path: &Path| {
        path.strip_prefix(base)
            .unwrap_or(path)
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    };
    let main_document = project.main_document().map(|main| root.join(main));
    Some(Places {
        history: shown(&root.join(".research")),
        main: main_document.as_deref().map(shown),
        agent: format!("{}/", shown(&agent)),
        main_document,
    })
}

/// The switcher beside a window's project tabs: the project menu, and a
/// way to close the project shown.
pub(super) fn switcher_for(
    shell: &Shell,
    weak: WeakEntity<Shell>,
    cx: &gpui::App,
) -> impl IntoElement {
    MenuPopover::new(
        "project-switcher-menu",
        ToolbarButton::new("project-switcher", shell.project_name(cx))
            .switcher()
            .max_w(px(super::fit::TAB_SWITCHER_MAX))
            .parity("titlebar/project"),
        move |menu, window, cx| {
            project_menu(weak.clone(), menu, window, cx)
                .separator()
                .item(MenuAction::dispatch(
                    t!("Close this project"),
                    Box::new(CloseProject),
                ))
        },
    )
}
