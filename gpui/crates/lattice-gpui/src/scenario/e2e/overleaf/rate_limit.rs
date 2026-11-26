//! The e2e check of Overleaf's rate limit on live mode's own sync
//! (`OverleafBridge::failed`): a new file here whose upload Overleaf answers
//! 429 is not reported as a failed sync, stays owed, and goes up once the
//! cool-down is over, exactly once.

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
                    .iter()
                    .filter(|title| title.ends_with(" failed") || title.starts_with("Couldn"))
                    .count()
            })
        };
        let failures_before = failures(cx);
        let uploads_before = mock.requests(&upload);
        mock.rate_limit_uploads(1);
        std::fs::create_dir_all(root.join("notes"))?;
        std::fs::write(root.join(PATH), TEXT)?;
        let started = Instant::now();
        // Answered 429: owed again, quietly, rather than failed.
        let owed = self
            .wait_for(cx, Duration::from_secs(40), |shell, cx| {
                shell.read(cx).overleaf.read(cx).sync_owed()
            })
            .await;
        let quiet_owed = failures(cx) == failures_before;
        let landed = self
            .wait_for(cx, Duration::from_secs(90), |_, _| mock.file(PATH).is_some())
            .await;
        let took = started.elapsed();
        let text = mock.file(PATH).as_deref() == Some(TEXT);
        // One answered 429, then the one that went up after the cool-down.
        let uploads = mock.requests(&upload) - uploads_before;
        let waited = took >= Duration::from_secs(25);
        let quiet = quiet_owed && failures(cx) == failures_before;
        steps.check(
            "a sync Overleaf answers 429 stays owed without an error, and goes up once after the cool-down",
            settled && owed && landed && text && uploads == 2 && waited && quiet,
            json!({
                "settled": settled, "owed": owed, "landed": landed, "text": text,
                "uploads": uploads, "took_ms": took.as_millis(), "quiet": quiet,
                "overleaf": mock.file(PATH),
            }),
        );
        Ok(())
    }
}
