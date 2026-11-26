//! The e2e check of Overleaf's rate limit on live mode's own sync
//! (`OverleafBridge::failed`): a new file here whose upload Overleaf answers
//! 429 is not reported as a failed sync, stays owed, and goes up once the
//! cool-down is over, exactly once. The 429 asks for 40 s, not the 30 s the
//! doubling starts at, so only a cool-down that honours Retry-After passes:
//! the cool-down is read off the bridge, since when the upload goes after it
//! also depends on where the 45 s poll stood.

use std::time::{Duration, Instant};

use anyhow::Result;
use gpui::AsyncApp;
use serde_json::json;

use super::super::Steps;
use crate::scenario::{
    Run,
    overleaf_mock::{MockOverleaf, PROJECT_ID},
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
        // The next poll after the cool-down sends it: up to 45 s later.
        let landed = self
            .wait_for(cx, Duration::from_secs(100), |_, _| mock.file(PATH).is_some())
            .await;
        let took = started.elapsed();
        let text = mock.file(PATH).as_deref() == Some(TEXT);
        // One answered 429, then the one that went up after the cool-down.
        let uploads = mock.requests(&upload) - uploads_before;
        // Its Retry-After waited out, give or take the sync's own start:
        // the doubling's 30 s would land by about 33 s.
        let waited = took >= RETRY_AFTER - Duration::from_secs(2);
        let quiet = quiet_owed && failures(cx).is_subset(&failures_before);
        steps.check(
            "a sync Overleaf answers 429 stays owed without an error, and goes up once after the Retry-After it asked for",
            settled && owed && honoured && landed && text && uploads == 2 && waited && quiet,
            json!({
                "settled": settled, "owed": owed, "landed": landed, "text": text,
                "cooling_ms": cooling.as_millis(),
                "uploads": uploads, "took_ms": took.as_millis(), "quiet": quiet,
                "overleaf": mock.file(PATH),
            }),
        );
        Ok(())
    }
}
