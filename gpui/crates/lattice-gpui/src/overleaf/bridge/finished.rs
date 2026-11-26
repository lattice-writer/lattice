//! What a finished sync leaves the writer to know or do
//! (`reportSyncOutcome`): the summary toast, its warnings and conflicts, the
//! version it records, and, for a failure, the words that fit or a quiet
//! retry. Every sync is also one operation in the app's log, quiet no-ops
//! and cancellations included, so a gap in sync history is explained.

use serde_json::json;

use super::*;
use crate::app_log::trace::Trace;

/// How long live mode holds its syncs after Overleaf answers one with 429,
/// doubling with each 429 in a row up to [`RATE_LIMIT_COOLDOWN_MAX`].
const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(30);
const RATE_LIMIT_COOLDOWN_MAX: Duration = Duration::from_secs(300);
/// 429s in a row held quietly (30 s + 60 s + 120 s + 240 s, about 7.5 min)
/// before one is reported, so the writer learns their edits aren't
/// reaching Overleaf; that one is reported once, and later ones in the same
/// run stay quiet, with the cool-down still holding.
const RATE_LIMIT_RETRIES: u32 = 4;

/// The cool-down after the `rate_limits`th 429 in a row (from 1).
///
/// A 429 that says when to come back (`retry_after`, its `Retry-After`)
/// is taken at its word, held between [`RATE_LIMIT_COOLDOWN`] and
/// [`RATE_LIMIT_COOLDOWN_MAX`]; one that doesn't gets the doubling. The
/// floor stays because a short Retry-After that reaches here has already
/// failed: lattice-core waits out one of up to 8 s itself and asks again,
/// twice, before giving up (an upload, which it never resends, is the one
/// exception and pays a few extra seconds). Taken as-is, it would also
/// spend the four quiet 429s in seconds and report a limit Overleaf lifts
/// within the minute. The cap keeps an absurd value, or a date far off,
/// from stalling live mode past the five minutes it would wait anyway.
fn rate_limit_cooldown(rate_limits: u32, retry_after: Option<Duration>) -> Duration {
    retry_after
        .unwrap_or_else(|| {
            RATE_LIMIT_COOLDOWN.saturating_mul(1 << rate_limits.saturating_sub(1).min(16))
        })
        .clamp(RATE_LIMIT_COOLDOWN, RATE_LIMIT_COOLDOWN_MAX)
}

impl OverleafBridge {
    /// A failed sync is owed again, quietly, for a scenario's checks.
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    pub fn sync_owed(&self) -> bool {
        self.transport_retry
    }

    /// What is left of a 429's cool-down, for a scenario's checks.
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    pub fn rate_limit_left(&self) -> Option<Duration> {
        self.rate_limited_until
            .map(|until| until.saturating_duration_since(Instant::now()))
    }
}

/// End a sync's operation in the log: its counts, then the summary, or the
/// failure under the words its notice uses (`runOverleafSync`'s trace).
pub(super) fn trace_outcome(
    trace: &mut Trace,
    result: &Result<OverleafSyncResult, String>,
    options: &SyncOptions,
    cx: &mut App,
) {
    match result {
        Ok(result) => {
            trace.enrich([
                ("automatic", json!(options.automatic)),
                ("pulled", json!(result.pulled.len())),
                ("pushed", json!(result.pushed.len())),
                ("merged", json!(result.merged.len())),
                ("conflicts", json!(result.conflicts.len())),
                ("deleted_local", json!(result.deleted_local.len())),
                ("read_only", json!(result.read_only)),
            ]);
            trace.finish("success", Some(outcome::summary(result)), cx);
        }
        Err(error) => {
            let refused = outcome::is_project_refusal(error);
            let transient = outcome::is_transient_transport_failure(error);
            if options.automatic && (transient || refused) {
                trace.enrich([("retry_scheduled", json!(true))]);
            }
            let title = if outcome::is_signed_out(error) {
                t!("Overleaf sync paused").to_string()
            } else if refused {
                t!("Overleaf refused this project").to_string()
            } else if transient {
                t!("Couldn’t reach Overleaf").to_string()
            } else {
                t!("{action} failed", action = t!("Sync")).to_string()
            };
            trace.fail(title, error, cx);
        }
    }
}

impl OverleafBridge {
    /// Drop the running sync, ending its operation in the log as cancelled.
    pub(super) fn cancel_sync(&mut self, cx: &mut Context<Self>) {
        self.sync_task = None;
        if let Some(mut trace) = self.sync_trace.take() {
            trace.finish(
                "cancelled",
                Some(t!("Overleaf sync cancelled").to_string()),
                cx,
            );
        }
    }

    /// A sync finished: say what it left, and settle what it asks of the
    /// writer.
    pub(super) fn synced(
        &mut self,
        root: PathBuf,
        result: OverleafSyncResult,
        options: SyncOptions,
        cx: &mut Context<Self>,
    ) {
        self.transport_retry = false;
        self.rate_limited_until = None;
        self.rate_limits = 0;
        self.refusals = 0;
        self.remote_changes = false;
        // Edited here while the sync ran: left as they were, so they still
        // need to go up.
        if !result.edited_during_sync.is_empty() {
            self.local_pending = true;
            if self.mode == OverleafSyncMode::Live {
                self.schedule_push(cx);
            }
        }
        // Old versions uploaded PDF-inspection renders from an app-owned
        // folder: removed silently, outside the writer's preference.
        if !result.automatic_remote_deletes.is_empty() && !result.read_only {
            remote_deletes::settle(
                self,
                &root,
                result.automatic_remote_deletes.clone(),
                true,
                cx,
            );
        }
        // What happens to a file deleted here is the writer's call.
        if !result.skipped_remote_deletes.is_empty() {
            remote_deletes::settle(
                self,
                &root,
                result.skipped_remote_deletes.clone(),
                false,
                cx,
            );
        }
        let this = cx.entity().downgrade();
        for warning in outcome::warnings(&result) {
            let mut notice = Notice::new(ToastLevel::Warning, warning.message);
            if let Some(file) = warning.resolve {
                let this = this.clone();
                notice = notice.action(NoticeAction::new(t!("Resolve"), move |_, cx| {
                    this.update(cx, |_, cx| {
                        cx.emit(OverleafBridgeEvent::OpenConflict(file.clone()))
                    })
                    .ok();
                }));
            }
            notices::show(notice, cx);
        }
        self.report_conflicts(&root, &result, cx);
        crate::agent::host_writes::note(&root, crate::agent::host_writes::sync_writes(&result), cx);
        let incoming = outcome::incoming(&result);
        if incoming {
            cx.emit(OverleafBridgeEvent::Incoming);
        }
        let moved =
            !result.pulled.is_empty() || !result.pushed.is_empty() || !result.merged.is_empty();
        // A quiet background no-op is only logged.
        if !options.automatic || moved {
            notices::show(
                Notice::new(ToastLevel::Success, outcome::summary(&result)).key(SYNC_KEY),
                cx,
            );
        }
        // Only a real content change becomes a version.
        if incoming || !result.pushed.is_empty() {
            let version_root = root.clone();
            cx.background_spawn(async move {
                let _ = lattice_core::git::auto_commit(
                    &version_root,
                    crate::history::messages::OVERLEAF_SYNC,
                    None,
                );
            })
            .detach();
        }
        if self.channel == ChannelStatus::Error && !self.rejoined {
            self.rejoined = true;
            self.channel_task = None;
        }
        self.reload_link(cx);
    }

    /// Say which files a sync left in conflict, and open the first that has
    /// markers in it in the editor, where each spot carries its own choices
    /// (`reportSyncOutcome`). A later write can have settled one already, so
    /// each is read back first. A binary file names the copy it kept, with
    /// the action that puts the other one back.
    fn report_conflicts(
        &mut self,
        root: &Path,
        result: &OverleafSyncResult,
        cx: &mut Context<Self>,
    ) {
        if result.conflicts.is_empty() {
            return;
        }
        let marked: Vec<_> = result
            .conflicts
            .iter()
            .filter(|conflict| conflict.markers)
            .filter(|conflict| {
                lattice_core::project::read_file(root, &conflict.path)
                    .map(|text| crate::history::conflicts::has_conflict_markers(&text))
                    .unwrap_or(true)
            })
            .collect();
        let message = outcome::conflict_message(&marked);
        if !message.is_empty() {
            notices::show(Notice::new(ToastLevel::Error, message), cx);
        }
        for conflict in result.conflicts.iter().filter(|conflict| !conflict.markers) {
            let (message, label) = outcome::binary_conflict(conflict);
            let (root, set_aside, path) = (
                root.to_path_buf(),
                conflict.set_aside.clone(),
                conflict.path.clone(),
            );
            let action = NoticeAction::new(label, move |_, cx| {
                if let Err(error) =
                    lattice_core::overleaf::restore_set_aside(&root, &set_aside, &path)
                {
                    notices::error(t!("Could not restore the file"), error, cx);
                }
            });
            notices::show(Notice::new(ToastLevel::Warning, message).action(action), cx);
        }
        if let Some(first) = marked.first() {
            cx.emit(OverleafBridgeEvent::OpenConflict(first.path.clone()));
        }
    }

    /// A sync failed: say so in the words that fit, or quietly owe it again
    /// for a brief outage in live mode.
    pub(super) fn failed(&mut self, error: String, options: SyncOptions, cx: &mut Context<Self>) {
        let refused = outcome::is_project_refusal(&error);
        if refused {
            self.refusals += 1;
        }
        // A 429 on live mode's own sync is Overleaf asking us to slow
        // down, not a failure to report: the edit stays owed and goes once
        // the cool-down is over, with nothing in the way meanwhile.
        let rate_limited = options.automatic && outcome::is_rate_limited(&error);
        if rate_limited {
            self.rate_limits += 1;
            let cooldown = rate_limit_cooldown(self.rate_limits, outcome::retry_after(&error));
            self.rate_limited_until = Some(Instant::now() + cooldown);
        }
        let retryable = outcome::is_transient_transport_failure(&error)
            || (rate_limited && self.rate_limits != RATE_LIMIT_RETRIES)
            || (refused && self.refusals < outcome::REFUSAL_RETRIES);
        let this = cx.entity().downgrade();
        let sync_again = || {
            let this = this.clone();
            NoticeAction::new(t!("Sync again"), move |_, cx| {
                this.update(cx, |this, cx| this.sync(SyncOptions::default(), cx))
                    .ok();
            })
        };
        if options.automatic && retryable {
            // The live loop retries on its rate-limited cadence: retrying
            // now risks another 429, and a toast for a brief outage
            // interrupts work.
            self.transport_retry = true;
            log::warn!("Overleaf sync will be retried: {error}");
            return;
        }
        let notice = if outcome::is_signed_out(&error) {
            let this = this.clone();
            Notice::new(ToastLevel::Error, t!("Overleaf sync paused"))
                .detail(t!(
                    "Your Overleaf session ended. Your edits stay on this Mac until you sign in"
                ))
                .action(NoticeAction::new(
                    t!("Sign in to Overleaf"),
                    move |_, cx| {
                        this.update(cx, |_, cx| cx.emit(OverleafBridgeEvent::OpenSettings))
                            .ok();
                    },
                ))
        } else if refused {
            // Signed in, yet this project's requests keep being turned away:
            // most often a share that was removed.
            self.transport_retry = false;
            Notice::new(ToastLevel::Error, t!("Overleaf refused this project"))
                .detail(t!("You are still signed in. Check that the project still opens on Overleaf; your edits stay on this Mac"))
                .action(sync_again())
        } else if outcome::is_transient_transport_failure(&error) {
            Notice::new(ToastLevel::Error, t!("Couldn’t reach Overleaf"))
                .detail(t!("Your edits stay on this Mac until the next sync"))
                .action(sync_again())
        } else {
            Notice::new(
                ToastLevel::Error,
                t!("{action} failed", action = t!("Sync")),
            )
            .detail(error)
        };
        notices::show(notice.key(SYNC_KEY), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_limit_in_a_row_cools_down_twice_as_long_up_to_five_minutes() {
        assert_eq!(rate_limit_cooldown(1, None), Duration::from_secs(30));
        assert_eq!(rate_limit_cooldown(2, None), Duration::from_secs(60));
        assert_eq!(rate_limit_cooldown(4, None), Duration::from_secs(240));
        assert_eq!(rate_limit_cooldown(5, None), RATE_LIMIT_COOLDOWN_MAX);
        assert_eq!(rate_limit_cooldown(u32::MAX, None), RATE_LIMIT_COOLDOWN_MAX);
    }

    #[test]
    fn a_rate_limit_that_says_when_cools_down_that_long_within_the_bounds() {
        let secs = |s| Some(Duration::from_secs(s));
        // Its word, not the doubling's, whichever 429 in a row it is.
        assert_eq!(rate_limit_cooldown(1, secs(45)), Duration::from_secs(45));
        assert_eq!(rate_limit_cooldown(3, secs(45)), Duration::from_secs(45));
        assert_eq!(rate_limit_cooldown(1, secs(200)), Duration::from_secs(200));
        // Too short, or none at all, is the floor; too long, the cap.
        assert_eq!(rate_limit_cooldown(1, secs(5)), RATE_LIMIT_COOLDOWN);
        assert_eq!(rate_limit_cooldown(1, secs(0)), RATE_LIMIT_COOLDOWN);
        assert_eq!(
            rate_limit_cooldown(1, secs(86_400)),
            RATE_LIMIT_COOLDOWN_MAX
        );
    }
}
