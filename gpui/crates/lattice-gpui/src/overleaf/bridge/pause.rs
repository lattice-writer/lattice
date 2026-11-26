//! Pausing and resuming a project's sync (Settings › Overleaf). Pausing is
//! not unlinking: the link, the last sync's copy and the file table stay in
//! the project's sync state (`lattice_core::overleaf::set_paused`), so the
//! pause holds across restarts and resuming merges what moved meanwhile.
//!
//! Resuming syncs at once (`OverleafBridgeEvent::Resumed`, which the window
//! answers with its Sync: Review changes in manual mode), so what was
//! edited on either side while paused goes through the regular three-way
//! sync rather than waiting for the next edit or the writer's own Sync.

use super::*;

impl OverleafBridge {
    /// The project's link is paused.
    pub fn paused(&self) -> bool {
        self.link.as_ref().is_some_and(|link| link.link.paused)
    }

    /// Pause or resume syncing this project. Pausing keeps the link and
    /// everything it needs, so resuming is an ordinary sync that merges
    /// whatever changed on either side meanwhile.
    pub fn set_paused(&mut self, paused: bool, cx: &mut Context<Self>) {
        if self.staged {
            return;
        }
        let Some(root) = self.root.clone() else {
            notices::error(
                t!("Open the linked Overleaf project before changing its sync setting."),
                "",
                cx,
            );
            return;
        };
        let wrote = root.clone();
        let task = cx.background_spawn(async move { sync::set_paused(&root, paused) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                // The window moved to another project meanwhile: its link
                // was read for it, and nothing here concerns it.
                Ok(()) if !this.is_current(&wrote) => {}
                Ok(()) => {
                    this.resuming = (!paused).then(|| wrote.clone());
                    this.reload_link(cx);
                    if paused {
                        let weak = cx.entity().downgrade();
                        notices::show(
                            Notice::new(ToastLevel::Info, t!("Overleaf sync paused"))
                                .detail(t!("Edits on both sides merge when you resume"))
                                .key("overleaf-sync-paused")
                                .action(NoticeAction::new(t!("Resume"), move |_, cx| {
                                    weak.update(cx, |this, cx| this.set_paused(false, cx)).ok();
                                })),
                            cx,
                        );
                    } else {
                        notices::dismiss("overleaf-sync-paused", cx);
                    }
                }
                Err(error) => notices::error(t!("Could not change Overleaf sync"), error, cx),
            })
            .ok();
        })
        .detach();
    }
}
