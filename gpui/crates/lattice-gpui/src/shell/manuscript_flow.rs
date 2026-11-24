//! The window's side of New manuscript from template…
//! (`crate::welcome::new_manuscript`): asked for by the Project panel with
//! the folder it goes in, the files written by lattice-core off the main
//! thread, then the tree rescanned, the folders down to the main file opened
//! and scrolled to (as New file and New folder open theirs), the main file
//! opened, and, when asked, set as the main document. A clash stays in the
//! dialog, naming the files.

use gpui::{AnyElement, AppContext as _, Context, Entity, IntoElement as _, Subscription, Window};

use super::{
    Shell,
    parts::{Part, no_commands},
};
use crate::welcome::new_manuscript::{NewManuscriptDialog, NewManuscriptEvent, add_manuscript};

pub(super) const PART: Part = Part {
    install: |_, _| Box::new(ManuscriptFlow::default()),
    views: &[],
    actions: |element, _| element,
    commands: no_commands,
};

#[derive(Default)]
pub(crate) struct ManuscriptFlow {
    dialog: Option<(Entity<NewManuscriptDialog>, Subscription)>,
}

impl Shell {
    /// New manuscript from template… in \`directory\` ("" is the project root).
    pub(super) fn open_new_manuscript(
        &mut self,
        directory: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.new_manuscript_open() || self.overlay_writing(cx) {
            return;
        }
        let Some(root) = self.root(cx) else {
            return;
        };
        let dialog = cx.new(|cx| NewManuscriptDialog::new(root, directory, window, cx));
        let subscription = cx.subscribe_in(&dialog, window, Self::on_new_manuscript_event);
        dialog.update(cx, |dialog, cx| dialog.focus_input(window, cx));
        self.parts.get_mut::<ManuscriptFlow>().dialog = Some((dialog, subscription));
        cx.notify();
    }

    /// The dialog, for the window to draw over everything, deferred with
    /// its other dialogs (`layers.rs`) so it draws above native webviews.
    pub(super) fn new_manuscript_element(&self) -> Option<AnyElement> {
        let (dialog, _) = self.parts.get::<ManuscriptFlow>().dialog.as_ref()?;
        Some(dialog.clone().into_any_element())
    }

    /// The dialog, while it is up (for a scenario).
    pub(crate) fn new_manuscript_dialog(&self) -> Option<Entity<NewManuscriptDialog>> {
        self.parts
            .get::<ManuscriptFlow>()
            .dialog
            .as_ref()
            .map(|(dialog, _)| dialog.clone())
    }

    pub(super) fn new_manuscript_open(&self) -> bool {
        self.parts.get::<ManuscriptFlow>().dialog.is_some()
    }

    pub(crate) fn close_new_manuscript(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.parts.get_mut::<ManuscriptFlow>().dialog = None;
        self.focus_front(window, cx);
        cx.notify();
    }

    fn on_new_manuscript_event(
        &mut self,
        dialog: &Entity<NewManuscriptDialog>,
        event: &NewManuscriptEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            NewManuscriptEvent::Dismiss => self.close_new_manuscript(window, cx),
            NewManuscriptEvent::Create {
                folder,
                template,
                main,
            } => self.add_manuscript(
                dialog.clone(),
                folder.clone(),
                template.clone(),
                *main,
                window,
                cx,
            ),
        }
    }

    fn add_manuscript(
        &mut self,
        dialog: Entity<NewManuscriptDialog>,
        folder: String,
        template: String,
        main: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let root = project.read(cx).root().to_path_buf();
        let added = cx.background_spawn({
            let root = root.clone();
            async move { add_manuscript(root, folder, &template) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let added = added.await;
            this.update_in(cx, |this, window, cx| {
                // Dismissed while it wrote, or another project in front
                // now: the files are there, but the window has moved on.
                let current = this.new_manuscript_dialog().as_ref() == Some(&dialog)
                    && this.project.as_ref() == Some(&project);
                if !current {
                    project.update(cx, |project, cx| project.rescan(cx));
                    return;
                }
                match added {
                    Ok(added) => {
                        this.close_new_manuscript(window, cx);
                        project.update(cx, |project, cx| project.rescan(cx));
                        this.file_tree().clone().update(cx, |tree, cx| {
                            tree.reveal_paths(vec![added.main.clone()], cx)
                        });
                        this.open_path(root.join(&added.main), None, window, cx);
                        if main {
                            project
                                .update(cx, |project, cx| {
                                    project.choose_main_document(added.main.clone(), cx)
                                })
                                .detach();
                        }
                    }
                    Err(error) => dialog.update(cx, |dialog, cx| dialog.failed(error, cx)),
                }
            })
            .ok();
        })
        .detach();
    }
}
