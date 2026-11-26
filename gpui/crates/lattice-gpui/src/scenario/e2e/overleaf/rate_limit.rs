//! The e2e check of Overleaf's rate limit on live mode's own sync
//! (`OverleafBridge::failed`): a new file here whose upload Overleaf answers
//! 429 is not reported as a failed sync, stays owed, and goes up once the
//! cool-down is over, exactly once. The 429 asks for 40 s, not the 30 s the
//! doubling starts at, so only a cool-down that honours Retry-After passes.
//! The upload goes when the cool-down ends, not at the 45 s poll's next
//! turn, and meanwhile the toolbar's sync status says it is waiting, and
//! for how long, rather than looking idle.

use std::time::{Duration, Instant};

use anyhow::Result;
use gpui::AsyncApp;
use serde_json::json;

use super::super::Steps;
use crate::{
    scenario::{
        Run,
        overleaf_mock::{MockOverleaf, PROJECT_ID},
    },
    shell::overleaf_flow::control::OverleafBadge,
};

impl Run {
    pub(super) async fn overleaf_rate_limit(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
    ) -> Result<()> {
        const PATH: &str = "notes/rate-limited.txt";
        const TEXT: &str = "Written while Overleaf was rate limiting us.\n";
        const RETRY_AFTER: Duration = Duration::from_secs(40);
        let root = self.root(cx).await?;
        let upload = format!("POST /project/{PROJECT_ID}/upload");
        let settled = self
            .wait_for(cx, Duration::from_secs(30), |shell, cx| {
                let bridge = shell.read(cx).overleaf.read(cx);
                !bridge.syncing() && !bridge.sync_owed()
            })
            .await;
        let failures = |cx: &mut AsyncApp| {
            cx.update(|cx| {
                crate::notices::install(cx)
                    .read(cx)
                    .titles()
                    .into_iter()
                    .filter(|title| title.ends_with(" failed") || title.starts_with("Couldn"))
                    .collect::<std::collections::BTreeSet<_>>()
            })
        };
        let failures_before = failures(cx);
        let uploads_before = mock.requests(&upload);
        let upload_times_before = mock.upload_times().len();
        mock.rate_limit_uploads(1, RETRY_AFTER.as_secs());
        std::fs::create_dir_all(root.join("notes"))?;
        std::fs::write(root.join(PATH), TEXT)?;
        let started = Instant::now();
        // Answered 429: owed again, quietly, rather than failed.
        let owed = self
            .wait_for(cx, Duration::from_secs(40), |shell, cx| {
                shell.read(cx).overleaf.read(cx).sync_owed()
            })
            .await;
        // Held for its Retry-After, less the moment since it came back; the
        // doubling would have left 30 s at most.
        let cooling = cx.update(|cx| {
            self.shell
                .read(cx)
                .overleaf
                .read(cx)
                .rate_limit_left()
                .unwrap_or_default()
        });
        let honoured = cooling > RETRY_AFTER - Duration::from_secs(5) && cooling <= RETRY_AFTER;
        // No failure shown that wasn't up before (one expiring is fine).
        let quiet_owed = failures(cx).is_subset(&failures_before);
        // The toolbar says it is waiting, and counts the seconds down: read
        // twice, 3 s apart, as a tooltip opened at each moment would, with
        // no redraw in between.
        let seconds_left = |cx: &mut AsyncApp| {
            let label = cx.update(|cx| self.shell.read(cx).overleaf_sync_label(cx));
            let seconds = label
                .strip_prefix("Overleaf asked to wait, retrying in ")
                .and_then(|rest| rest.strip_suffix(" s"))
                .and_then(|seconds| seconds.parse::<u64>().ok());
            (label, seconds)
        };
        let badge = |cx: &mut AsyncApp| cx.update(|cx| self.shell.read(cx).overleaf_badge(cx));
        let (waiting_label, waiting) = seconds_left(cx);
        let cooling_badge = badge(cx);
        cx.background_executor().timer(Duration::from_secs(3)).await;
        let (_, later) = seconds_left(cx);
        let counts_down = matches!(
            (waiting, later),
            (Some(first), Some(then)) if first <= RETRY_AFTER.as_secs() && then + 2 <= first
        );
        let landed = self
            .wait_for(cx, Duration::from_secs(100), |_, _| mock.file(PATH).is_some())
            .await;
        let took = started.elapsed();
        let sent_badge = badge(cx);
        // Sent as the cool-down ends, by the mock's own clock: the 45 s poll
        // would leave it anywhere up to 85 s after the 429.
        let upload_times = mock.upload_times();
        let retry_gap = match &upload_times[upload_times_before..] {
            [limited, retried] => Some(retried.saturating_duration_since(*limited)),
            _ => None,
        };
        let prompt = retry_gap.is_some_and(|gap| gap <= RETRY_AFTER + Duration::from_secs(2));
        let text = mock.file(PATH).as_deref() == Some(TEXT);
        // One answered 429, then the one that went up after the cool-down.
        let uploads = mock.requests(&upload) - uploads_before;
        // Its Retry-After waited out, give or take the sync's own start:
        // the doubling's 30 s would land by about 33 s.
        let waited = took >= RETRY_AFTER - Duration::from_secs(2);
        let quiet = quiet_owed && failures(cx).is_subset(&failures_before);
        steps.check(
            "while a 429's cool-down runs, the sync status says Overleaf asked to wait and counts the seconds down",
            owed && counts_down,
            json!({
                "label": waiting_label, "seconds": waiting, "three_seconds_later": later,
            }),
        );
        steps.check(
            "while a 429's cool-down runs, the Overleaf control's dot is amber, and green again once the upload goes",
            owed && cooling_badge == Some(OverleafBadge::Cooling)
                && sent_badge == Some(OverleafBadge::Online),
            json!({
                "cooling": format!("{cooling_badge:?}"), "sent": format!("{sent_badge:?}"),
            }),
        );
        steps.check(
            "a sync Overleaf answers 429 stays owed without an error, and goes up once as the Retry-After it asked for ends",
            settled && owed && honoured && landed && prompt && text && uploads == 2 && waited && quiet,
            json!({
                "settled": settled, "owed": owed, "landed": landed, "text": text,
                "cooling_ms": cooling.as_millis(),
                "retry_after_429_ms": retry_gap.map(|gap| gap.as_millis()),
                "uploads": uploads, "took_ms": took.as_millis(), "quiet": quiet,
                "overleaf": mock.file(PATH),
            }),
        );
        Ok(())
    }
}
