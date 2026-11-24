//! New manuscript from template… (lattice#1259): the Project panel's way to
//! start a paper inside the project that is open, for a repository that
//! keeps its analysis, data and one or more manuscripts together. Offered on
//! a folder's menu and the tree's own, in the + menu and the palette.
//!
//! The dialog takes New project's templates (the built-in venues, then the
//! writer's saved ones), the name of the folder the files go in (inside the
//! folder it was asked from, prefilled with a free "Manuscript") and Set as
//! main document. It says where the files land, "Will create
//! ProjectA/Manuscript/", as New project does. lattice-core writes the files
//! (`project::add_manuscript`) and refuses any that are already there,
//! listing them under the fields; nothing else changes: no project,
//! settings or Agent threads of its own, the open project stays open.

use std::path::PathBuf;

use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_kit::component::input::{InputEvent, InputState};
use lattice_core::project::templates::LocalTemplate;
use lattice_core::project::{AddedManuscript, ManuscriptError, ManuscriptTemplate, Venue};
use lattice_i18n::t;
use lattice_ui::{
    ActiveLattice as _, Elevation, TextRole, TextRoleStyle as _,
    components::{Button, CheckState, Checkbox, Field, Select, TrapFocus as _, rise_in},
    tokens::{CONTAINER_DIALOG_CREATE, FONT_WEIGHT_MEDIUM},
};

use super::{
    local_templates::{self, LOCAL_PREFIX},
    new_project::{DEFAULT_VENUE, VENUES},
};

/// The folder name the dialog offers first.
const FOLDER: &str = "Manuscript";

pub enum NewManuscriptEvent {
    Dismiss,
    /// Write the template \`template\` (a built-in venue's id, or
    /// \`local:<id>\`) into \`folder\` (project-relative); \`main\` sets its
    /// compile root as the main document.
    Create {
        folder: String,
        template: String,
        main: bool,
    },
}

pub struct NewManuscriptDialog {
    /// The project's folder, for the preview and the free name.
    root: PathBuf,
    /// The folder the manuscript's folder goes in ("" is the project root).
    parent: String,
    name: Entity<InputState>,
    /// The template chosen: an index into the built-in venues, then the
    /// saved templates after them.
    template: usize,
    saved: Vec<LocalTemplate>,
    main: bool,
    /// The files already there, from the last Create.
    clash: Vec<String>,
    error: Option<String>,
    busy: bool,
    focus: FocusHandle,
    _subscription: Subscription,
}

impl EventEmitter<NewManuscriptEvent> for NewManuscriptDialog {}

impl NewManuscriptDialog {
    pub fn new(root: PathBuf, parent: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let free = free_name(|name| root.join(&parent).join(name).exists());
        let name = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(FOLDER);
            input.set_value(free, window, cx);
            input
        });
        let subscription = cx.subscribe(&name, |this, _, event, cx| match event {
            InputEvent::PressEnter { .. } => this.create(cx),
            InputEvent::Change => {
                this.clash.clear();
                this.error = None;
                cx.notify()
            }
            _ => {}
        });
        let load = local_templates::load(cx);
        cx.spawn(async move |this, cx| {
            let listed = load.await.unwrap_or_default();
            this.update(cx, |this, cx| {
                this.saved = listed;
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self {
            root,
            parent,
            name,
            template: DEFAULT_VENUE,
            saved: Vec::new(),
            main: false,
            clash: Vec::new(),
            error: None,
            busy: false,
            focus: cx.focus_handle(),
            _subscription: subscription,
        }
    }

    pub fn focus_input(&self, window: &mut Window, cx: &mut App) {
        self.name.update(cx, |input, cx| input.focus(window, cx));
    }

    /// The folder the files go in, project-relative.
    pub fn folder(&self, cx: &App) -> String {
        let name = match self.name.read(cx).value().trim() {
            "" => FOLDER.to_string(),
            name => name.to_string(),
        };
        match self.parent.as_str() {
            "" => name,
            parent => format!("{parent}/{name}"),
        }
    }

    /// "ProjectA/Manuscript/": the folder made, under the project's own name.
    pub fn preview(&self, cx: &App) -> String {
        let project = self
            .root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{project}/{}/", self.folder(cx))
    }

    /// The files the last Create found already there.
    pub fn clash(&self) -> &[String] {
        &self.clash
    }

    pub fn set_name(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.name.update(cx, |input, cx| {
            input.set_value(name.to_string(), window, cx)
        });
        self.clash.clear();
        cx.notify();
    }

    /// Choose the built-in venue \`id\` (for a scenario).
    pub fn choose_venue(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(ix) = VENUES.iter().position(|(venue, _)| *venue == id) {
            self.template = ix;
            cx.notify();
        }
    }

    pub fn set_main(&mut self, main: bool, cx: &mut Context<Self>) {
        self.main = main;
        cx.notify();
    }

    /// The add failed: the files in the way, or why.
    pub fn failed(&mut self, error: ManuscriptError, cx: &mut Context<Self>) {
        self.busy = false;
        match error {
            ManuscriptError::Clash(files) => self.clash = files,
            ManuscriptError::Failed(error) => self.error = Some(error),
        }
        cx.notify();
    }

    /// The template chosen, as the window's Create names it.
    fn template_id(&self) -> String {
        match self.template.checked_sub(VENUES.len()) {
            None => VENUES[self.template].0.to_string(),
            Some(ix) => format!("{LOCAL_PREFIX}{}", self.saved[ix].id),
        }
    }

    pub fn create(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.clash.clear();
        self.error = None;
        cx.emit(NewManuscriptEvent::Create {
            folder: self.folder(cx),
            template: self.template_id(),
            main: self.main,
        });
        cx.notify();
    }
}

/// "Manuscript", or the first "Manuscript-2", "Manuscript-3"… not \`taken\`.
fn free_name(taken: impl Fn(&str) -> bool) -> String {
    std::iter::once(FOLDER.to_string())
        .chain((2..).map(|n| format!("{FOLDER}-{n}")))
        .find(|name| !taken(name))
        .unwrap_or_else(|| FOLDER.to_string())
}

/// Write \`template\` (a venue's id or \`local:<id>\`) into \`folder\` with
/// lattice-core.
pub fn add_manuscript(
    root: PathBuf,
    folder: String,
    template: &str,
) -> Result<AddedManuscript, ManuscriptError> {
    let store = local_templates::store();
    let template = match template.strip_prefix(LOCAL_PREFIX) {
        Some(id) => ManuscriptTemplate::Saved { store: &store, id },
        None => ManuscriptTemplate::Venue(Venue::parse(template)?),
    };
    lattice_core::project::add_manuscript(&root, &folder, template)
}

impl Focusable for NewManuscriptDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.name.focus_handle(cx)
    }
}

impl Render for NewManuscriptDialog {
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
        let template = Select::new(
            "new-manuscript-template",
            VENUES
                .iter()
                .map(|(_, label)| label.to_string())
                .chain(self.saved.iter().map(local_templates::label)),
            Some(self.template),
        )
        .on_change(cx.listener(|this, ix: &usize, _, cx| {
            this.template = *ix;
            this.clash.clear();
            cx.notify();
        }));
        let caption = |text: String, color, cx: &App| {
            div()
                .text_role(TextRole::Caption, cx)
                .text_color(color)
                .min_w_0()
                .child(text)
        };
        let clash = (!self.clash.is_empty()).then(|| {
            div()
                .id("new-manuscript-clash")
                .pt(px(8.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .max_h(px(140.))
                .overflow_y_scroll()
                .child(caption(
                    t!("These files are already there, so nothing was created:").to_string(),
                    palette.danger,
                    cx,
                ))
                .children(
                    self.clash
                        .iter()
                        .map(|file| caption(format!("• {file}"), palette.danger, cx)),
                )
        });
        let card = div()
            .id("new-manuscript")
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
                    .child(t!("New manuscript from template")),
            )
            .child(
                div()
                    .pt(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(label(t!("Template"), template.into_any_element(), cx))
                    .child(label(
                        t!("Folder"),
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(Field::new(&self.name))
                            .child(caption(
                                t!("Will create {path}", path = self.preview(cx)).to_string(),
                                palette.text_secondary,
                                cx,
                            ))
                            .into_any_element(),
                        cx,
                    ))
                    .child(
                        Checkbox::new(
                            "new-manuscript-main",
                            if self.main {
                                CheckState::On
                            } else {
                                CheckState::Off
                            },
                        )
                        .label(t!("Set as main document"))
                        .description(t!("Build compiles this manuscript"))
                        .on_toggle(cx.listener(|this, on: &bool, _, cx| this.set_main(*on, cx))),
                    ),
            )
            .children(clash)
            .children(self.error.clone().map(|error| {
                div()
                    .pt(px(8.))
                    .text_role(TextRole::Caption, cx)
                    .text_color(palette.danger)
                    .child(crate::host_errors::text(&error))
            }))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .pt(px(20.))
                    .child(
                        Button::new("new-manuscript-cancel", t!("Cancel"))
                            .ghost()
                            .on_click(
                                cx.listener(|_, _, _, cx| cx.emit(NewManuscriptEvent::Dismiss)),
                            ),
                    )
                    .child(
                        Button::new("new-manuscript-create", t!("Create manuscript"))
                            .primary()
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                    ),
            );
        div()
            .id("new-manuscript-scrim")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .pt(viewport.height * 0.18)
            .bg(palette.scrim)
            .occlude()
            .trap_focus("new-manuscript-trap", &self.focus)
            // Bubbling, so an open template list takes its own Escape first.
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    cx.emit(NewManuscriptEvent::Dismiss);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.emit(NewManuscriptEvent::Dismiss)),
            )
            .child(rise_in("new-manuscript-in", card))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_offered_is_the_first_free_manuscript() {
        assert_eq!(free_name(|_| false), "Manuscript");
        assert_eq!(
            free_name(|name| ["Manuscript", "Manuscript-2"].contains(&name)),
            "Manuscript-3"
        );
    }

    #[test]
    fn a_template_from_the_dialog_reaches_lattice_core() {
        let root = tempfile::tempdir().unwrap();
        let added = add_manuscript(
            root.path().to_path_buf(),
            "papers/Manuscript".into(),
            "blank",
        )
        .unwrap();
        assert_eq!(added.main, "papers/Manuscript/main.tex");
        assert!(
            root.path()
                .join("papers/Manuscript/references.bib")
                .is_file()
        );
        assert_eq!(
            add_manuscript(
                root.path().to_path_buf(),
                "papers/Manuscript".into(),
                "blank"
            ),
            Err(ManuscriptError::Clash(vec![
                "papers/Manuscript/main.tex".into(),
                "papers/Manuscript/references.bib".into(),
            ]))
        );
        assert!(add_manuscript(root.path().to_path_buf(), "x".into(), "nonsense").is_err());
    }
}
