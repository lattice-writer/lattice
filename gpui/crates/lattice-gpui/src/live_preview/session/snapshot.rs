//! Where a session is, as the pill and the e2e scenario read it: its phase,
//! how rtex ended, and what is catching up.

/// Where the session is. `Starting`: rtex is loading the preamble.
/// `FirstLayout`: typesetting the whole document for the first time.
/// `Live`: paragraphs follow typing. `Paused`: rtex could not load the
/// preamble for live typing; whole-document passes still run.
/// `Uncompiled`: the first whole-document pass stopped on a TeX error; rtex
/// keeps running and lays the document out once it compiles. `Exited`: rtex
/// is gone (it exited, or can no longer be written to), and only a new
/// session brings live preview back (`LivePreview::restart_if_exited`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    FirstLayout,
    Live,
    Paused,
    Uncompiled,
    Exited,
}

/// How rtex ended, when it ended on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RtexExit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub phase: Phase,
    /// A whole-document pass is catching up with the text: page breaks and
    /// what follows an edit may move.
    pub layout_pending: bool,
    /// rtex's words when it stopped or failed.
    pub detail: Option<String>,
    /// How rtex ended (`Exited`); `None` while it runs or when it could no
    /// longer be written to.
    pub exit: Option<RtexExit>,
    /// A quiet build is bringing the full PDF up to date: running, or its
    /// PDF loading.
    pub rebuilding: bool,
    /// The first-layout bound (`LivePreview::first_layout_timeout`, counted
    /// from the engine's first Ready) passed with no layout yet. rtex has no
    /// bound of its own: a document that loops keeps it there, and the pill
    /// says so and offers to reinstall rtex if it persists, rather than
    /// spinning. A layout that arrives later still takes over.
    pub overdue: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            phase: Phase::Starting,
            layout_pending: false,
            detail: None,
            exit: None,
            rebuilding: false,
            overdue: false,
        }
    }
}

/// The phase an engine that loaded the preamble (`EngineState` Ready) puts
/// the session in. rtex reports a failed first pass and Ready in either
/// order, so a pass that already failed keeps `Uncompiled`.
pub(super) fn ready_phase(has_layout: bool, first_pass_failed: bool) -> Phase {
    if has_layout {
        Phase::Live
    } else if first_pass_failed {
        Phase::Uncompiled
    } else {
        Phase::FirstLayout
    }
}

/// rtex is still on its way to a first layout in `phase`: no layout yet,
/// and no failed pass to say why.
pub(super) fn awaiting_first_layout(
    phase: Phase,
    has_layout: bool,
    first_pass_failed: bool,
) -> bool {
    !has_layout
        && !first_pass_failed
        && matches!(phase, Phase::Starting | Phase::FirstLayout | Phase::Paused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_after_a_failed_first_pass_keeps_waiting_for_the_document_to_compile() {
        // rtex 0.0.6 on a loaded Mac reports the failed pass before Ready.
        assert_eq!(ready_phase(false, true), Phase::Uncompiled);
        assert_eq!(ready_phase(false, false), Phase::FirstLayout);
        assert_eq!(ready_phase(true, true), Phase::Live);
    }

    #[test]
    fn only_a_session_still_on_its_way_to_a_first_layout_can_be_overdue() {
        for phase in [Phase::Starting, Phase::FirstLayout, Phase::Paused] {
            assert!(awaiting_first_layout(phase, false, false), "{phase:?}");
            assert!(!awaiting_first_layout(phase, true, false), "{phase:?}");
            assert!(!awaiting_first_layout(phase, false, true), "{phase:?}");
        }
        for phase in [Phase::Live, Phase::Uncompiled, Phase::Exited] {
            assert!(!awaiting_first_layout(phase, false, false), "{phase:?}");
        }
    }
}
