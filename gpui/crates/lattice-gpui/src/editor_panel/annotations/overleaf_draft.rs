//! An Overleaf comment written beside its passage (#1396). The selection
//! toolbar's Comment on Overleaf opens the annotation composer on the
//! selection, as Annotate does, with Overleaf as its target: no intent
//! chips, and Save reads Comment on Overleaf. Sending hands the comment to
//! the window (`EditorPanelEvent::OverleafComment`), which posts it through
//! the bridge's add-comment path and answers (`answer_overleaf_comment`).
//!
//! The text stays in the composer until Overleaf has it, so a refusal
//! leaves it to send again; while the file is not live with Overleaf the
//! composer says why and sends nothing. Once sent, the composer closes and
//! the keyboard is back in the editor, on the passage.

use editor::SelectionEffects;
use gpui::{Context, Focusable as _, SharedString, Window};
use language::ToOffset as _;
use lattice_i18n::t;

use super::super::{EditorPanel, EditorPanelEvent};
use crate::overleaf::bridge::OverleafSaveState;

/// Why a comment cannot go to Overleaf right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Blocked {
    /// The project's link is paused.
    Paused,
    /// The live channel dropped.
    Offline,
    /// A sync is running.
    Syncing,
    /// The file is not open live with Overleaf (or the project not linked).
    NotLive,
}

impl Blocked {
    /// Why, as the composer says it.
    pub(super) fn message(self) -> SharedString {
        match self {
            Blocked::Paused => {
                t!("Sync with Overleaf is paused. Resume it in Settings › Overleaf to comment.")
            }
            Blocked::Offline => {
                t!("Not connected to Overleaf. The comment can be sent once it reconnects.")
            }
            Blocked::Syncing => t!("A sync with Overleaf is running. Send once it finishes."),
            Blocked::NotLive => t!("Comments go to Overleaf once this file is open live with it."),
        }
        .into()
    }
}

/// Whether a comment on a file in `state` can go to Overleaf now.
pub(super) fn blocked(state: OverleafSaveState) -> Option<Blocked> {
    match state {
        OverleafSaveState::Live | OverleafSaveState::Sending => None,
        OverleafSaveState::Paused => Some(Blocked::Paused),
        OverleafSaveState::Offline => Some(Blocked::Offline),
        OverleafSaveState::Syncing => Some(Blocked::Syncing),
        OverleafSaveState::Manual => Some(Blocked::NotLive),
    }
}

impl EditorPanel {
    /// Open the composer on the selection for an Overleaf comment (the
    /// selection toolbar's Comment on Overleaf). Returns whether it opened.
    pub fn comment_on_overleaf(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open_on_selection(true, window, cx)
    }

    /// Whether an Overleaf comment is being written here.
    pub fn composing_overleaf_comment(&self) -> bool {
        self.annotations
            .draft
            .as_ref()
            .is_some_and(|draft| draft.overleaf)
    }

    /// Why the comment being written cannot be sent now, if it cannot.
    pub(super) fn overleaf_comment_blocked(&self) -> Option<Blocked> {
        self.overleaf_save().map_or(Some(Blocked::NotLive), blocked)
    }

    /// Send the comment being written, on its passage as it stands now.
    pub(super) fn send_overleaf_comment(&mut self, cx: &mut Context<Self>) {
        let blocked = self.overleaf_comment_blocked();
        let Some(draft) = &mut self.annotations.draft else {
            return;
        };
        let body = draft.field.read(cx).value().trim().to_string();
        if draft.sending || body.is_empty() || blocked.is_some() {
            return;
        }
        let snapshot = self.buffer.read(cx).snapshot();
        let range = draft.range.start.to_offset(&snapshot)..draft.range.end.to_offset(&snapshot);
        let text = self.buffer.read(cx).text();
        if text.get(range.clone()) != Some(draft.quote.as_str()) {
            draft.error =
                Some(t!("The selected text changed. Select it again before commenting.").into());
            cx.notify();
            return;
        }
        draft.sending = true;
        draft.error = None;
        cx.emit(EditorPanelEvent::OverleafComment {
            id: draft.id,
            from: text[..range.start].encode_utf16().count() as i64,
            quote: draft.quote.clone(),
            body,
        });
        cx.notify();
    }

    /// The window's answer to comment `id`: taken, the composer closes and
    /// the keyboard is back on the passage; refused, the text stays and the
    /// composer says why.
    pub fn answer_overleaf_comment(
        &mut self,
        id: u64,
        answer: Result<(), SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self
            .annotations
            .draft
            .as_mut()
            .filter(|draft| draft.overleaf && draft.id == id)
        else {
            return;
        };
        if let Err(error) = answer {
            draft.sending = false;
            draft.error = Some(error);
            cx.notify();
            return;
        }
        let snapshot = self.buffer.read(cx).snapshot();
        let range = draft.range.start.to_offset(&snapshot)..draft.range.end.to_offset(&snapshot);
        self.annotations.draft = None;
        self.place_annotations(cx);
        let (start, end) = (
            snapshot.offset_to_point(range.start),
            snapshot.offset_to_point(range.end),
        );
        self.editor.update(cx, |editor, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                selections.select_ranges([start..end])
            });
        });
        window.focus(&self.editor.focus_handle(cx), cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Blocked, blocked};
    use crate::overleaf::bridge::OverleafSaveState;

    #[test]
    fn a_live_file_takes_comments() {
        assert_eq!(blocked(OverleafSaveState::Live), None);
        assert_eq!(blocked(OverleafSaveState::Sending), None);
    }

    #[test]
    fn a_file_not_live_says_why() {
        assert_eq!(blocked(OverleafSaveState::Paused), Some(Blocked::Paused));
        assert_eq!(blocked(OverleafSaveState::Offline), Some(Blocked::Offline));
        assert_eq!(blocked(OverleafSaveState::Syncing), Some(Blocked::Syncing));
        assert_eq!(blocked(OverleafSaveState::Manual), Some(Blocked::NotLive));
    }
}
