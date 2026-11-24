//! How a build was asked for, which decides what it may do, and the one
//! build a queue of asks stands for.

/// How a build was asked for, which decides what it may do.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::shell) struct BuildRequest {
    /// Typeset from scratch.
    pub(in crate::shell) force: bool,
    /// Delete the auxiliary files first (a clean rebuild).
    pub(in crate::shell) clean: bool,
    /// Lattice started it (a save, an outside edit), nobody asked: a failing
    /// document nothing changed in is not typeset again (`latex::build`).
    pub(in crate::shell) unattended: bool,
    /// The writer asked (Build, the chip): its failure comes back even when
    /// dismissed, and the editor goes to its first error.
    pub(in crate::shell) requested: bool,
    /// An automatic build the live preview's pause in typing let through:
    /// no spinner on the Build button (`live_preview::quiet_builds`).
    pub(in crate::shell) quiet: bool,
    /// It waited behind another build (the log's trigger "queued").
    pub(in crate::shell) queued: bool,
    /// Typst's live preview: compile the open buffers as they are, saving
    /// nothing.
    pub(in crate::shell) live: bool,
    /// The build that entering a project starts: when the project's last
    /// build is current (`latex::current_build`), that build is the answer,
    /// and nothing shows a build running.
    pub(in crate::shell) entry: bool,
}

impl BuildRequest {
    pub(in crate::shell) fn automatic() -> Self {
        Self {
            unattended: true,
            ..Self::default()
        }
    }

    /// Entering a project: build it, unless its last build is current.
    pub(in crate::shell) fn entry() -> Self {
        Self {
            entry: true,
            ..Self::default()
        }
    }

    pub(in crate::shell) fn requested() -> Self {
        Self {
            requested: true,
            ..Self::default()
        }
    }

    /// The writer's clean rebuild: auxiliary files deleted, typeset from
    /// scratch.
    pub(in crate::shell) fn clean_rebuild() -> Self {
        Self {
            force: true,
            clean: true,
            requested: true,
            ..Self::default()
        }
    }

    /// Typst's live build, after a pause in typing: quiet, unattended, and
    /// from the buffers.
    pub(in crate::shell) fn typst_live() -> Self {
        Self {
            live: true,
            quiet: true,
            ..Self::automatic()
        }
    }

    /// The build queued behind a running one stands for both asks.
    pub(in crate::shell) fn merge(self, other: Self) -> Self {
        Self {
            live: self.live && other.live,
            entry: self.entry && other.entry,
            force: self.force || other.force,
            clean: self.clean || other.clean,
            unattended: self.unattended && other.unattended,
            requested: self.requested || other.requested,
            quiet: self.quiet && other.quiet,
            queued: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_queued_build_stands_for_every_ask() {
        let queued = BuildRequest::automatic().merge(BuildRequest::requested());
        assert!(queued.requested && !queued.unattended && !queued.force);
        let automatic = BuildRequest::automatic().merge(BuildRequest::automatic());
        assert!(automatic.unattended && !automatic.requested);
        let clean = BuildRequest::automatic().merge(BuildRequest::clean_rebuild());
        assert!(clean.clean && clean.force && clean.requested && !clean.unattended);
    }

    #[test]
    fn an_entry_queued_with_another_ask_builds() {
        assert!(BuildRequest::entry().merge(BuildRequest::entry()).entry);
        assert!(!BuildRequest::entry().merge(BuildRequest::automatic()).entry);
        assert!(!BuildRequest::requested().merge(BuildRequest::entry()).entry);
    }
}
