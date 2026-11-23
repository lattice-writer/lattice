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
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            phase: Phase::Starting,
            layout_pending: false,
            detail: None,
            exit: None,
            rebuilding: false,
        }
    }
}
