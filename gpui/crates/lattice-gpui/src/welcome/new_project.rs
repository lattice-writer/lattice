//! New project (⌘⇧N), as the Tauri app asks for one (`CreateProjectDialog`
//! in `src/project/project-dialogs.tsx`, `.create-project-modal` in
//! `src/styles/dialogs.css`): a 440px dialog with its title in the serif,
//! the project's name (Untitled research when left empty) and its venue
//! template (NeurIPS by default), then Cancel and Choose parent folder…,
//! which asks for the folder the project's own folder goes in. Once one is
//! chosen the dialog says what Create project will make there, "Will create
//! ProjectA/Manuscript/", and warns when the parent already carries the
//! project's name, so a doubled Manuscript/Manuscript/ shows before anything
//! is made (lattice#1259). Enter in the name does what the primary button
//! says.
//!
//! Under the venue, My templates lists the templates the writer saved and
//! offers Save a local template… (`local_templates.rs`, `save_template.rs`);
//! choosing one creates the project from it instead.
//!
//! The project is made by lattice-core (`create_with_venue`, or
//! `templates::instantiate` for a saved template, as the Tauri app's
//! `create_project`), and an error lands under the fields.

use std::path::{Path, PathBuf};

use super::save_template::{SaveTemplateDialog, SaveTemplateEvent};

use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, ParentElement as _,
    PathPromptOptions, Render, Styled as _, Subscription, Window, div, px,
};
use gpui_kit::component::input::{InputEvent, InputState};
use lattice_i18n::t;
use lattice_ui::{
    ActiveLattice as _, Elevation, TextRole, TextRoleStyle as _,
    components::{Button, Field, Select, TrapFocus as _, rise_in},
    tokens::{CONTAINER_DIALOG_CREATE, FONT_WEIGHT_MEDIUM},
};

/// The templates a new project starts from, in the order the app lists
/// them, each with its name as the select shows it
/// (`src/project/project-venues.ts`).
pub const VENUES: [(&str, &str); 15] = [
    ("blank", "Blank article"),
    ("chinese-article", "Chinese article (ctexart)"),
    ("neurips", "NeurIPS 2026 (preprint)"),
    ("icml", "ICML 2026 (preprint)"),
    ("iclr", "ICLR 2026"),
    ("acl", "ACL / ARR (preprint)"),
    ("cvpr", "CVPR 2027"),
    ("cvpr-rebuttal", "CVPR 2027 Rebuttal"),
    ("aaai", "AAAI 2027 (preprint)"),
    ("ieee-conference", "IEEE conference paper (IEEEtran)"),
    ("ieee-journal", "IEEE journal article (IEEEtran)"),
    ("acm", "ACM paper (acmart)"),
    ("thuthesis", "Tsinghua University thesis (ThuThesis)"),
    ("ustcthesis", "USTC thesis (ustcthesis)"),
    // The native app's alone: the Tauri app cannot build Typst.
    ("typst", "Blank Typst document"),
];
pub(crate) const DEFAULT_VENUE: usize = 2;

pub enum NewProjectEvent {
    Dismiss,
    /// Create a project named `name` in `parent` from `venue` (a built-in
    /// venue's id, or `local:<id>`).
    Create {
        parent: PathBuf,
        name: String,
        venue: String,
    },
}

/// What the project is made from.
#[derive(Clone, PartialEq)]
enum Venue {
    BuiltIn(usize),
    /// A saved template, by id.
    Local(String),
}

pub struct NewProjectDialog {
    name: Entity<InputState>,
    venue: Venue,
    /// The folder the project's folder goes in, once chosen.
    parent: Option<PathBuf>,
    error: Option<String>,
    /// My templates.
    mine: super::local_templates::MyTemplates,
    /// The project open in the window, which Save a local template offers.
    current: Option<PathBuf>,
    /// Save a local template, while it is up.
    saving: Option<(Entity<SaveTemplateDialog>, Subscription)>,
    focus: FocusHandle,
    _subscription: Subscription,
}

impl EventEmitter<NewProjectEvent> for NewProjectDialog {}

impl NewProjectDialog {
    pub fn new(current: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(t!("Untitled research")));
        // The preview follows the name as it is typed.
        let subscription = cx.subscribe(&name, |this, _, event, cx| match event {
            InputEvent::PressEnter { .. } => this.create(cx),
            InputEvent::Change => cx.notify(),
            _ => {}
        });
        let load = super::local_templates::load(cx);
        cx.spawn(async move |this, cx| {
            let listed = load.await;
            this.update(cx, |this, cx| {
                this.mine.loaded(listed);
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self {
            name,
            venue: Venue::BuiltIn(DEFAULT_VENUE),
            parent: None,
            error: None,
            mine: Default::default(),
            current,
            saving: None,
            focus: cx.focus_handle(),
            _subscription: subscription,
        }
    }

    /// The saved template at `ix` in My templates, chosen.
    pub(crate) fn choose_local(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(item) = self.mine.items.get(ix) {
            self.venue = Venue::Local(item.id.clone());
            cx.notify();
        }
    }

    pub(super) fn toggle_template_files(&mut self, cx: &mut Context<Self>) {
        self.mine.files_open = !self.mine.files_open;
        cx.notify();
    }

    /// Save a local template… : its dialog over this one.
    pub(crate) fn open_save_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving.is_some() {
            return;
        }
        let store = super::local_templates::store();
        let dialog = cx.new(|cx| SaveTemplateDialog::new(self.current.clone(), store, window, cx));
        let subscription = cx.subscribe_in(&dialog, window, |this, _, event, window, cx| {
            if let SaveTemplateEvent::Saved(item) = event {
                this.mine.items.push(item.clone());
                this.mine.error = None;
                this.venue = Venue::Local(item.id.clone());
            }
            this.saving = None;
            this.focus_input(window, cx);
            cx.notify();
        });
        dialog.update(cx, |dialog, cx| dialog.focus_dialog(window, cx));
        self.saving = Some((dialog, subscription));
        cx.notify();
    }

    /// Save a local template, while it is up (for a scenario).
    pub fn save_template_dialog(&self) -> Option<&Entity<SaveTemplateDialog>> {
        self.saving.as_ref().map(|(dialog, _)| dialog)
    }

    /// Whether My templates has read the store.
    pub fn templates_loaded(&self) -> bool {
        self.mine.loaded
    }

    /// The saved templates My templates lists, and the one chosen.
    pub fn local_templates(&self) -> (Vec<String>, Option<String>) {
        let chosen = match &self.venue {
            Venue::Local(id) => Some(id.clone()),
            Venue::BuiltIn(_) => None,
        };
        (
            self.mine.items.iter().map(|item| item.id.clone()).collect(),
            chosen,
        )
    }

    pub fn focus_input(&self, window: &mut Window, cx: &mut App) {
        self.name.update(cx, |input, cx| input.focus(window, cx));
    }

    pub fn set_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    /// The name the project is made with: Untitled research when left empty.
    fn project_name(&self, cx: &App) -> String {
        match self.name.read(cx).value().trim() {
            "" => t!("Untitled research").to_string(),
            name => name.to_string(),
        }
    }

    /// Choose parent folder…: the system's folder picker, its answer the
    /// parent the preview names.
    pub(crate) fn choose_parent(&mut self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t!("Choose parent folder").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            if let Some(parent) = paths.into_iter().next() {
                this.update(cx, |this, cx| this.set_parent(parent, cx)).ok();
            }
        })
        .detach();
    }

    /// Type `name` as the project's name (for a scenario).
    pub fn set_name(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.name.update(cx, |input, cx| {
            input.set_value(name.to_string(), window, cx)
        });
        cx.notify();
    }

    pub fn set_parent(&mut self, parent: PathBuf, cx: &mut Context<Self>) {
        self.parent = Some(parent);
        self.error = None;
        cx.notify();
    }

    /// What Create project makes, as the dialog says it, and whether the
    /// name is doubled; None until a parent is chosen (for a scenario too).
    pub fn preview(&self, cx: &App) -> Option<(String, bool)> {
        let parent = self.parent.as_deref()?;
        Some(creates(parent, &self.project_name(cx)))
    }

    /// The primary button: choose the parent first, then create.
    fn create(&mut self, cx: &mut Context<Self>) {
        let Some(parent) = self.parent.clone() else {
            self.choose_parent(cx);
            return;
        };
        let venue = match &self.venue {
            Venue::BuiltIn(ix) => VENUES[*ix].0.to_string(),
            Venue::Local(id) => format!("{}{id}", super::local_templates::LOCAL_PREFIX),
        };
        cx.emit(NewProjectEvent::Create {
            parent,
            name: self.project_name(cx),
            venue,
        });
    }
}

/// The folder a project `name` makes in `parent`, named from the parent's
/// own folder (`ProjectA/Manuscript/`), and whether the parent already has
/// that name, which would make `Manuscript/Manuscript/`.
pub(crate) fn creates(parent: &Path, name: &str) -> (String, bool) {
    let shown = parent
        .file_name()
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_else(|| parent.display().to_string());
    let doubled = parent
        .file_name()
        .is_some_and(|folder| folder.to_string_lossy().to_lowercase() == name.to_lowercase());
    (
        format!("{}/{name}/", shown.trim_end_matches(['/', '\\'])),
        doubled,
    )
}

/// `path` with the home folder written `~`.
pub(crate) fn home_relative(path: &Path) -> String {
    dirs::home_dir()
        .and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
        .map(|rest| format!("~/{}", rest.display()))
        .unwrap_or_else(|| path.display().to_string())
}

/// Create the project in `parent` from the venue, or the saved template
/// `local:<id>` names, with lattice-core.
pub fn create_project(parent: PathBuf, name: String, venue: &str) -> Result<PathBuf, String> {
    if let Some(id) = venue.strip_prefix(super::local_templates::LOCAL_PREFIX) {
        let store = super::local_templates::store();
        return lattice_core::project::templates::instantiate(&store, id, &parent, &name);
    }
    let venue = lattice_core::project::Venue::parse(venue)?;
    lattice_core::project::create_with_venue(&parent, &name, venue)
}

impl Focusable for NewProjectDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.name.focus_handle(cx)
    }
}

impl Render for NewProjectDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette().clone();
        let viewport = window.viewport_size();
        let width = CONTAINER_DIALOG_CREATE
            .to_pixels(window.rem_size())
            .min(viewport.width - px(40.));
        let label = |text: &'static str, control: gpui::AnyElement, cx: &App| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .text_role(TextRole::Caption, cx)
                .font_weight(FONT_WEIGHT_MEDIUM)
                .text_color(palette.text)
                .child(text)
                .child(control)
        };
        let built_in = match self.venue {
            Venue::BuiltIn(ix) => Some(ix),
            Venue::Local(_) => None,
        };
        let venue = Select::new(
            "new-project-venue",
            VENUES.iter().map(|(_, label)| *label),
            built_in,
        )
        .placeholder(t!("Choose a built-in template"))
        .on_change(cx.listener(|this, ix: &usize, _, cx| {
            this.venue = Venue::BuiltIn(*ix);
            cx.notify();
        }));
        let chosen = match &self.venue {
            Venue::Local(id) => Some(id.as_str()),
            Venue::BuiltIn(_) => None,
        };
        let mine = super::local_templates::render(&self.mine, chosen, cx);
        let location = self.parent.clone().map(|parent| {
            let (made, doubled) = creates(&parent, &self.project_name(cx));
            let name = self.project_name(cx);
            div()
                .id("new-project-location")
                .pt(px(12.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.))
                        .min_w_0()
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_role(TextRole::Caption, cx)
                                .font_weight(FONT_WEIGHT_MEDIUM)
                                .text_color(palette.text)
                                .child(t!("Will create {path}", path = made)),
                        )
                        .child(
                            Button::new("new-project-parent", t!("Change…"))
                                .ghost()
                                .compact()
                                .on_click(cx.listener(|this, _, _, cx| this.choose_parent(cx))),
                        ),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_role(TextRole::Caption, cx)
                        .text_color(palette.text_secondary)
                        .child(t!("In {folder}", folder = home_relative(&parent))),
                )
                .children(doubled.then(|| {
                    div()
                        .text_role(TextRole::Caption, cx)
                        .text_color(palette.warning)
                        .child(t!(
                            "The parent folder is already named “{name}”, so this makes {name}/{name}/.",
                            name = name
                        ))
                }))
        });
        let card = div()
            .id("new-project")
            .w(width)
            .flex()
            .flex_col()
            .pt(px(20.))
            .px(px(20.))
            .pb(px(16.))
            .rounded(px(14.))
            .bg(palette.raised)
            .shadow(Elevation::Dialog.shadows(&palette))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_role(TextRole::Title, cx)
                    .text_color(palette.text)
                    .child(t!("Create a research project")),
            )
            .child(
                div()
                    .pt(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(label(
                        t!("Project name"),
                        Field::new(&self.name).into_any_element(),
                        cx,
                    ))
                    .child(label(t!("Venue template"), venue.into_any_element(), cx)),
            )
            .child(mine)
            .children(location)
            .children(self.error.clone().map(|error| {
                div()
                    .pt(px(6.))
                    .text_role(TextRole::Caption, cx)
                    .text_color(palette.danger)
                    .child(error)
            }))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .pt(px(if self.error.is_some() { 16. } else { 24. }))
                    .child(
                        Button::new("new-project-cancel", t!("Cancel"))
                            .ghost()
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(NewProjectEvent::Dismiss))),
                    )
                    .child(
                        Button::new(
                            "new-project-create",
                            if self.parent.is_some() {
                                t!("Create project")
                            } else {
                                t!("Choose parent folder…")
                            },
                        )
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                    ),
            );
        div()
            .id("new-project-scrim")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .pt(viewport.height * 0.18)
            .bg(palette.scrim)
            .occlude()
            .trap_focus("new-project-trap", &self.focus)
            // Bubbling, so an open venue list takes its own Escape first.
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    cx.emit(NewProjectEvent::Dismiss);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.emit(NewProjectEvent::Dismiss)),
            )
            .child(rise_in("new-project-in", card))
            .children(self.saving.as_ref().map(|(dialog, _)| dialog.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_venue_is_one_lattice_core_knows() {
        for (id, _) in VENUES {
            assert!(lattice_core::project::Venue::parse(id).is_ok(), "{id}");
        }
        assert_eq!(VENUES[DEFAULT_VENUE].0, "neurips");
    }

    // lattice#1259: the preview names the parent's own folder, and a parent
    // already named for the project is the doubled folder it warns about.
    #[test]
    fn the_preview_names_the_folder_made_and_a_doubled_name() {
        assert_eq!(
            creates(Path::new("/Users/a/ProjectA"), "Manuscript"),
            ("ProjectA/Manuscript/".to_string(), false)
        );
        assert_eq!(
            creates(Path::new("/Users/a/ProjectA/manuscript"), "Manuscript"),
            ("manuscript/Manuscript/".to_string(), true)
        );
        assert_eq!(
            creates(Path::new("/"), "Paper"),
            ("/Paper/".to_string(), false)
        );
    }
}
