//! Settings › Agent › Display, the GPUI app's own page: the Agent's font
//! size, apart from the editors', on a slider like theirs. The Agent's page
//! draws at it as a page zoom (`agent/surface.rs`'s `font_zoom`), in the
//! docked panel and in its own window alike, so the conversation, the
//! composer and code blocks grow together and keep their proportions. With
//! a project open, This project's Agent folder follows (lattice#1293).

use std::path::Path;

use gpui::{AnyElement, Context, ElementId, Entity};
use lattice_i18n::t;
use lattice_ui::components::Slider;

use super::{
    controls::{self, PageWidth, Row, Wrap},
    dialog::SettingsDialog,
    editor_pane::row,
};
use crate::project::Project;

pub fn render(
    dialog: &mut SettingsDialog,
    width: PageWidth,
    cx: &mut Context<SettingsDialog>,
) -> AnyElement {
    let size = Slider::new(
        ElementId::Name("agent-font-size-slider".into()),
        &dialog.fields.agent_font_size,
    )
    .label(t!("Agent font size"))
    .readout(|value| format!("{}px", value.round()).into());
    let rows = vec![row(
        dialog,
        Row::new("agent-font-size", t!("Agent font size"))
            .description(
                t!("The Agent’s conversation and composer, in the panel and in its own window. The editors keep their own size."),
                cx,
            )
            .control(size)
            .wrap(Wrap::Line),
        width,
        cx,
    )];
    let mut groups = vec![controls::first_group(None, rows, cx)];
    if let Some(project) = dialog.project.clone() {
        groups.push(agent_folder(dialog, &project, width, cx));
    }
    controls::page(t!("Display"), width, groups, cx)
}

/// This project's Agent folder (lattice#1293): the project's own, or one of
/// the folders above it, below the home folder
/// (`lattice_core::project::may_widen_to`). The history, the settings and
/// the builds stay the project's whichever is chosen.
fn agent_folder(
    dialog: &mut SettingsDialog,
    project: &Entity<Project>,
    width: PageWidth,
    cx: &mut Context<SettingsDialog>,
) -> AnyElement {
    let (root, current) = {
        let project = project.read(cx);
        (project.root().to_path_buf(), project.agent_root())
    };
    let folder_name = |folder: &Path| {
        folder.file_name().map_or_else(
            || folder.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    };
    let mut options = vec![(
        0usize,
        t!("Project folder ({name}/)", name = folder_name(&root)).into(),
    )];
    for (levels, folder) in root
        .ancestors()
        .enumerate()
        .skip(1)
        .take(lattice_core::project::AGENT_FOLDER_MAX_LEVELS)
    {
        if !lattice_core::project::may_widen_to(folder) {
            continue;
        }
        options.push((levels, format!("{}/", folder_name(folder)).into()));
    }
    let chosen = root
        .ancestors()
        .position(|folder| folder == current)
        .unwrap_or(0);
    let project = project.clone();
    let select = controls::select("agent-folder", options, chosen, move |levels, _, cx| {
        let above = project
            .read(cx)
            .root()
            .ancestors()
            .nth(levels)
            .filter(|_| levels > 0)
            .map(Path::to_path_buf);
        project.update(cx, |project, cx| project.set_agent_folder(above, cx));
    });
    let description = if chosen == 0 {
        t!("The Agent reads and edits files in this project’s folder.").to_string()
    } else {
        t!(
            "The Agent reads and edits files in all of {folder}/. This project’s history, settings and builds stay in {project}/.research.",
            folder = folder_name(&current),
            project = folder_name(&root)
        )
    };
    let rows = vec![row(
        dialog,
        Row::new("agent-folder", t!("Agent’s folder"))
            .description(description, cx)
            .control(select),
        width,
        cx,
    )];
    controls::group(Some(t!("This project").into()), rows, cx)
}
