//! The window's side of the Overleaf collaboration panel
//! (`overleaf::collab`): the titlebar's comments button beside the Overleaf
//! control, with what is waiting (open threads, the open file's suggestions
//! and unread chat); the panel in the workspace's "comments" view; a comment's
//! passage shown in its file; and "Annotate selection", the Tauri app's
//! selection toolbar's Annotate, which starts a thread on the selection of a
//! file live with Overleaf.

use gpui::{AnyElement, Context, IntoElement, Window, actions};
use lattice_i18n::t;
use lattice_ui::{Icon, components::IconButton, components::Surface};

use super::Shell;
use crate::overleaf::collab::{CollabEvent, CollabTab};

actions!(
    lattice,
    [
        AnnotateSelection,
        MarginReviewAuto,
        MarginReviewMarkers,
        MarginReviewOff
    ]
);

impl Shell {
    /// The comments button, for a linked project.
    pub(super) fn overleaf_collab_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let bridge = self.overleaf.read(cx);
        bridge.link()?;
        let threads = bridge
            .threads()
            .iter()
            .filter(|thread| !thread.resolved)
            .count();
        let changes = self
            .active_editor()
            .and_then(|editor| bridge.live_review(editor.read(cx).relative()))
            .map_or(0, |review| review.changes.len());
        let waiting = threads + changes + bridge.chat_unread();
        let tooltip = if waiting > 0 {
            t!(
                "Overleaf comments and chat, {count} waiting",
                count = waiting
            )
            .to_string()
        } else {
            t!("Overleaf comments and chat").to_string()
        };
        // The Tauri titlebar's chat button (canvas-toolbar.tsx): the
        // messages mark, and while something waits the toggle's wash with
        // the number after the mark.
        let mut button = IconButton::new("titlebar-overleaf-collab", Icon::MESSAGES_SQUARE)
            .surface(Surface::Titlebar)
            .on(waiting > 0)
            .tooltip(tooltip)
            .on_click(
                cx.listener(|this, _, window, cx| this.open_overleaf_collab(None, window, cx)),
            );
        if waiting > 0 {
            button = button.count(waiting.to_string());
        }
        Some(button.into_any_element())
    }

    /// Bring the panel on screen, on `tab` when given.
    pub(crate) fn open_overleaf_collab(
        &mut self,
        tab: Option<CollabTab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = tab {
            self.overleaf_collab
                .update(cx, |panel, cx| panel.set_tab(tab, cx));
        }
        // What is waiting may be in the chat, which is read when first asked.
        self.overleaf.update(cx, |bridge, cx| bridge.load_chat(cx));
        self.reveal_tool("comments", window, cx);
    }

    /// What the panel asked of the window.
    pub(super) fn on_collab_event(
        &mut self,
        event: &CollabEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let CollabEvent::Reveal { path, position } = event;
        let Some(root) = self.root(cx) else {
            return;
        };
        let open = self
            .open_editors()
            .into_iter()
            .find(|editor| editor.read(cx).relative() == path.as_str());
        let text = match (self.overleaf.read(cx).live_text(path), open) {
            (Some(live), _) => live.to_string(),
            (None, Some(editor)) => editor.read(cx).overleaf_text(cx),
            (None, None) => lattice_core::project::read_file(&root, path).unwrap_or_default(),
        };
        let units: Vec<u16> = text.encode_utf16().collect();
        let before =
            String::from_utf16_lossy(&units[..(*position).max(0).min(units.len() as i64) as usize]);
        let line = before.matches('\n').count() as u32 + 1;
        self.open_path(root.join(path), Some(line), window, cx);
    }

    /// Start a thread on the selection, in a file live with Overleaf; in a
    /// project not linked to it, a local annotation (`annotations_flow`).
    pub(crate) fn annotate_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.overleaf.read(cx).link().is_none() {
            return self.annotate_locally(window, cx);
        }
        if let Some(editor) = self.active_editor() {
            self.comment_on_overleaf(&editor, window, cx);
        }
    }

    /// Start a thread on `editor`'s selection, in a file live with Overleaf
    /// (Annotate selection, and the selection toolbar's Comment on Overleaf).
    pub(crate) fn comment_on_overleaf(
        &mut self,
        editor: &gpui::Entity<crate::editor_panel::EditorPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overleaf.read(cx).link().is_none() {
            return;
        }
        let path = editor.read(cx).relative().to_string();
        if self.overleaf.read(cx).live_file() != Some(path.as_str()) {
            crate::notices::show(
                crate::notices::Notice::new(
                    lattice_ui::components::ToastLevel::Info,
                    t!("Comments go to Overleaf once this file is open live with it."),
                ),
                cx,
            );
            return;
        }
        let Some((from, quote)) = editor.read(cx).overleaf_selection(cx) else {
            return;
        };
        self.overleaf_collab.update(cx, |panel, cx| {
            panel.compose_comment(path, from, quote, window, cx)
        });
        self.reveal_tool("comments", window, cx);
    }

    /// Post the local note `id` as an Overleaf comment on its passage, in a
    /// file live with Overleaf (its card's Share to Overleaf). Once Overleaf
    /// has it anchored on the passage, the note is resolved here, so the
    /// passage shows Overleaf's thread alone; its words and replies stay in
    /// the project's notes. While another comment is on its way, nothing is
    /// sent, so a second press cannot post the note twice.
    pub(crate) fn share_annotation(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root(cx) else {
            return;
        };
        if self.overleaf.read(cx).link().is_none() || self.overleaf.read(cx).review_busy().is_some()
        {
            return;
        }
        let Some(note) = crate::annotations::store::try_store(cx)
            .and_then(|store| store.read(cx).get(&root, id).cloned())
        else {
            return;
        };
        let notice = |message: gpui::SharedString, cx: &mut Context<Self>| {
            crate::notices::show(
                crate::notices::Notice::new(lattice_ui::components::ToastLevel::Info, message),
                cx,
            )
        };
        if self.overleaf.read(cx).live_file() != Some(note.path.as_str()) {
            return notice(
                t!("Comments go to Overleaf once this file is open live with it.").into(),
                cx,
            );
        }
        let passage = self
            .open_editors()
            .into_iter()
            .find(|editor| editor.read(cx).relative() == note.path)
            .and_then(|editor| editor.read(cx).annotation_overleaf_range(id, cx));
        let Some((from, quote)) = passage else {
            return notice(
                t!("The selected text changed. Select it again before commenting.").into(),
                cx,
            );
        };
        let body = crate::annotations::model::note_text(&note);
        if body.trim().is_empty() {
            return;
        }
        let shared = id.to_string();
        self.overleaf.update(cx, |bridge, cx| {
            bridge.add_comment(
                &note.path,
                from,
                quote,
                body,
                move |anchored, cx| {
                    if anchored {
                        let store = crate::annotations::store::store(cx);
                        store.update(cx, |store, cx| store.set_resolved(&root, &shared, true, cx));
                    }
                },
                cx,
            )
        });
        self.open_overleaf_collab(
            Some(crate::overleaf::collab::CollabTab::Comments),
            window,
            cx,
        );
    }
}
