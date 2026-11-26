//! The e2e check of Overleaf's rate limit on live mode's own sync
//! (`OverleafBridge::failed`): a new file here whose upload Overleaf answers
//! 429 is not reported as a failed sync, stays owed, and goes up once the
//! cool-down is over, exactly once.

use std::time::{Duration, Instant};

use anyhow::Result;
use gpui::AsyncApp;
use serde_json::json;

use super::super::Steps;
use crate::scenario::{Run, overleaf_mock::MockOverleaf};

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
        self.wait_for(cx, Duration::from_secs(30), |shell, cx| {
            !shell.read(cx).overleaf.read(cx).syncing()
        })
        .await;
        let uploads_before = mock.requests("POST /project/");
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
        let landed = self
            .wait_for(cx, Duration::from_secs(90), |_, _| mock.file(PATH).is_some())
            .await;
        let took = started.elapsed();
        let once = mock.file(PATH).as_deref() == Some(TEXT);
        steps.check(
            "a sync Overleaf answers 429 stays owed without an error, and goes up once after the cool-down",
            owed && landed && once,
            json!({
                "owed": owed, "landed": landed, "once": once, "took_ms": took.as_millis(),
                "uploads": mock.requests("POST /project/") - uploads_before,
                "overleaf": mock.file(PATH),
            }),
        );
        Ok(())
    }
}
