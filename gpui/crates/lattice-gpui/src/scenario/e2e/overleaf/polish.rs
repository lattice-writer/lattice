//! The e2e checks of how the linked window paces itself with Overleaf
//! (`overleaf::bridge::live`, `overleaf::channel`) and of Review here
//! (`shell::overleaf_menus`), against the stand-in:
//!
//! - with the channel carrying documents, live mode's probe of Overleaf's
//!   history slows to every 45 s, and a window off screen asks nothing;
//! - a channel waiting out its backoff joins as soon as the window comes
//!   forward, and says it reconnected;
//! - a board being edited is held from Overleaf, and goes up once the
//!   writer leaves it;
//! - Review here opens the comment on or after the caret's line, not the
//!   file's first or last.

use std::{collections::BTreeSet, time::Duration};

use anyhow::Result;
use gpui::{AsyncApp, Focusable as _};
use serde_json::json;

use super::super::Steps;
use crate::{
    editor_panel::MarginMode,
    overleaf::bridge::ChannelStatus,
    scenario::{
        Run,
        overleaf_mock::{MockOverleaf, PROJECT_ID},
    },
};

const RECONNECTED: &str = "Overleaf live editing reconnected.";

impl Run {
    pub(super) async fn overleaf_polish(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        mock: &MockOverleaf,
    ) -> Result<()> {
        let probes = format!("GET /project/{PROJECT_ID}/updates");
        // main.tex live, so typing in it goes out on the channel (and the
        // drop below follows from it), never as a change to push.
        let main = self.root(cx).await?.join("main.tex");
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_path(main.clone(), Some(1), window, cx)
            })
        })?;
        self.wait_for(cx, Duration::from_secs(20), |shell, cx| {
            let bridge = shell.read(cx).overleaf.read(cx);
            bridge.channel() == ChannelStatus::Live
                && bridge.live_file() == Some("main.tex")
                && !bridge.live_sending()
        })
        .await;

        // The channel is up: only figures and new files are left to the
        // poll, so it asks every 45 s, not every 3 s, and the wait runs from
        // Overleaf's last answer to anyone: a push's probe or a sync reads
        // the same version, so the poll does not ask again after one.
        // Counted from where the poll's clock is known, since an 8 s window
        // fell on the poll's own 45 s probe or not by where that cycle stood
        // (LEO-14: 1, 0, 1, 1 on main, beside the push probe the steps
        // before left owed). Focus makes the poll ask now, so its next
        // is due 45 s on; a sync 5 s later reads the version again, which
        // moves that to 50 s at the earliest. A push the sync's own writes
        // leave owed probes once it has waited out the 30 s sync gap, and is
        // let land, as is any the steps before left; from then to 48 s after
        // the focus the poll must ask nothing. A poll that counted only its
        // own probes asks once at 45 s, and one asking every 3 s many times.
        let focused = std::time::Instant::now();
        self.with_window(cx, |shell, _, cx| {
            shell.update(cx, |shell, cx| {
                shell
                    .overleaf
                    .update(cx, |bridge, cx| bridge.set_window_active(true, cx))
            })
        })?;
        self.pause(cx, Duration::from_secs(5)).await;
        self.with_window(cx, |shell, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.overleaf.update(cx, |bridge, cx| {
                    let _ = bridge.sync_reporting(cx);
                })
            })
        })?;
        self.wait_for(cx, Duration::from_secs(40), |shell, cx| {
            let bridge = shell.read(cx).overleaf.read(cx);
            !bridge.push_waiting() && !bridge.syncing()
        })
        .await;
        let settled_after = focused.elapsed();
        let due = focused + Duration::from_secs(48);
        let (live_probes, live_quiet) = self
            .settled_probes(cx, mock, &probes, Duration::MAX, due)
            .await;
        // The window holds the 45 s mark, settled throughout.
        let live_measured = settled_after < Duration::from_secs(40)
            && live_quiet + Duration::from_secs(1) >= Duration::from_secs(48) - settled_after;
        // Off screen, with nothing of ours owed, it asks nothing.
        self.with_window(cx, |shell, _, cx| {
            shell.update(cx, |shell, cx| {
                shell
                    .overleaf
                    .update(cx, |bridge, _| bridge.set_window_visible(false))
            })
        })?;
        mock.disable_realtime();
        mock.sever(true);
        self.type_text(cx, "% pace\n").await?;
        let dropped = self
            .wait_for(cx, Duration::from_secs(15), |shell, cx| {
                shell.read(cx).overleaf.read(cx).channel() != ChannelStatus::Live
            })
            .await;
        // What the typing saved to disk is ours and owed, so its push probes
        // while hidden, and the sync that sends it leaves pushes of its own
        // writes owed, which the watcher reports over the next seconds. Only
        // the time with neither a push waiting nor a sync running is
        // counted (`settled_probes`), for 7 s of it: a settle wait before
        // one window was satisfied before the typing's push was scheduled,
        // or between a sync's end and the watcher's report of its writes, so
        // those probes fell in the window by phase.
        let (hidden_probes, quiet) = self
            .settled_probes(
                cx,
                mock,
                &probes,
                Duration::from_secs(7),
                std::time::Instant::now() + Duration::from_secs(45),
            )
            .await;
        let measured = quiet >= Duration::from_secs(7);
        steps.check(
            "live mode probes Overleaf every 45 s while the channel carries documents, and not at all from a window off screen",
            live_probes == 0 && live_measured && dropped && measured && hidden_probes == 0,
            json!({
                "live": live_probes,
                "live_settled_after_ms": settled_after.as_millis() as u64,
                "live_settled_ms": live_quiet.as_millis() as u64,
                "dropped": dropped,
                "hidden": hidden_probes,
                "settled_ms": quiet.as_millis() as u64,
            }),
        );

        // Backing off between joins: coming forward joins now. The wait is
        // spread a fifth either way, so a third join or later says 4–20 s.
        let waiting = self
            .wait_for(cx, Duration::from_secs(30), |shell, cx| {
                let bridge = shell.read(cx).overleaf.read(cx);
                bridge.channel() == ChannelStatus::Connecting
                    && bridge.channel_detail().is_some_and(|detail| {
                        (4..=20).any(|seconds| detail.contains(&format!("in {seconds} s")))
                    })
            })
            .await;
        mock.enable_realtime();
        self.with_window(cx, |shell, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.overleaf.update(cx, |bridge, cx| {
                    bridge.set_window_visible(true);
                    bridge.set_window_active(true, cx);
                })
            })
        })?;
        let joined = self
            .wait_for(cx, Duration::from_secs(3), |shell, cx| {
                shell.read(cx).overleaf.read(cx).channel() == ChannelStatus::Live
            })
            .await;
        let told = self
            .wait_for(cx, Duration::from_secs(3), |_, cx| {
                crate::notices::install(cx)
                    .read(cx)
                    .titles()
                    .iter()
                    .any(|title| &**title == RECONNECTED)
            })
            .await;
        steps.check(
            "a channel waiting out its backoff joins as the window comes forward, and says it reconnected",
            waiting && joined && told,
            json!({ "waiting": waiting, "joined": joined, "told": told }),
        );

        // A board being edited waits; leaving it sends it up.
        let root = self.root(cx).await?;
        let uploads = format!("POST /project/{PROJECT_ID}/upload");
        let board = "notes.excalidraw";
        let text = "{\"type\":\"excalidraw\",\"version\":2,\"elements\":[]}";
        self.with_window(cx, |shell, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.overleaf.update(cx, |bridge, cx| {
                    bridge.whole_files_shown(BTreeSet::from([board.to_string()]), cx)
                })
            })
        })?;
        let sent = mock.requests(&uploads);
        std::fs::write(root.join(board), text)?;
        self.with_window(cx, |shell, _, cx| {
            shell.update(cx, |shell, cx| {
                shell.overleaf.update(cx, |bridge, cx| {
                    bridge.files_changed(Some(&[board.to_string()]), cx)
                })
            })
        })?;
        self.pause(cx, Duration::from_secs(5)).await;
        let held = mock.requests(&uploads) == sent && mock.file(board).is_none();
        self.with_window(cx, |shell, _, cx| {
            shell.update(cx, |shell, cx| {
                shell
                    .overleaf
                    .update(cx, |bridge, cx| bridge.whole_files_shown(BTreeSet::new(), cx))
            })
        })?;
        let mut went = false;
        for _ in 0..40 {
            if mock.file(board).as_deref() == Some(text) {
                went = true;
                break;
            }
            self.pause(cx, Duration::from_millis(500)).await;
        }
        steps.check(
            "a board being edited is held from Overleaf, and goes up once the writer leaves it",
            held && went,
            json!({ "held": held, "went": went, "uploads": mock.requests(&uploads) - sent }),
        );

        // Review here: comments at the start of the first line, of one in
        // the middle and of the last, with the caret on the line before the
        // middle one, which is the one that opens.
        let main = root.join("main.tex");
        let text = mock.file("main.tex").unwrap_or_default();
        let starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .filter(|at| *at < text.len())
            .collect();
        if starts.len() < 4 {
            steps.check(
                "Review here opens the comment on or after the caret's line",
                false,
                json!({ "lines": starts.len(), "text": text }),
            );
            return Ok(());
        }
        let utf16 = |byte: usize| text[..byte].encode_utf16().count() as i64;
        let middle = starts.len() / 2;
        let ours: Vec<String> = [0, middle, starts.len() - 1]
            .into_iter()
            .map(|line| {
                let quote: String = text[starts[line]..]
                    .chars()
                    .take_while(|c| *c != '\n')
                    .take(6)
                    .collect();
                let thread = mock.peer_commented("main.tex", utf16(starts[line]), &quote, "Here?");
                format!("comment:{thread}")
            })
            .collect();
        let from = utf16(starts[middle - 1]);
        let items: Vec<(i64, String)> = mock
            .comments("main.tex")
            .into_iter()
            .map(|comment| (comment.position, format!("comment:{}", comment.thread_id)))
            .chain(
                mock.changes("main.tex")
                    .into_iter()
                    .map(|change| (change.position, format!("change:{}", change.id))),
            )
            .collect();
        let expected = items
            .iter()
            .filter(|(at, _)| *at >= from)
            .min_by_key(|(at, _)| *at)
            .map(|(_, key)| key.clone());
        let caret = middle as u32;
        self.with_window(cx, move |shell, window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_path(main.clone(), Some(caret), window, cx);
                shell.set_margin_mode(MarginMode::Markers, cx);
            })
        })?;
        let main = root.join("main.tex");
        let wanted = ours.clone();
        let shown = self
            .wait_for(cx, Duration::from_secs(10), move |shell, cx| {
                shell
                    .update(cx, |shell, cx| shell.open_editor(&main, cx))
                    .is_some_and(|editor| {
                        let painted = editor.read(cx).margin_painted();
                        wanted
                            .iter()
                            .all(|key| painted.iter().any(|(painted, _)| painted == key))
                    })
            })
            .await;
        // The palette runs it with the source focused, which makes it the
        // window's active editor.
        let focused = root.join("main.tex");
        self.with_window(cx, move |shell, window, cx| {
            if let Some(editor) = shell.update(cx, |shell, cx| shell.open_editor(&focused, cx)) {
                window.focus(&editor.read(cx).editor().focus_handle(cx), cx);
            }
        })?;
        self.pause(cx, Duration::from_millis(300)).await;
        self.with_window(cx, |_, window, cx| {
            window.dispatch_action(Box::new(crate::shell::ReviewHere), cx)
        })?;
        let main = root.join("main.tex");
        let middle_key = ours[1].clone();
        let opened = self
            .wait_for(cx, Duration::from_secs(3), move |shell, cx| {
                shell
                    .update(cx, |shell, cx| shell.open_editor(&main, cx))
                    .is_some_and(|editor| {
                        editor.read(cx).margin_open() == Some(middle_key.as_str())
                    })
            })
            .await;
        let main = root.join("main.tex");
        let facts = self.with_window(cx, move |shell, _, cx| {
            let active = shell.read(cx).active_editor_path(cx);
            let editor = shell.update(cx, |shell, cx| shell.open_editor(&main, cx));
            let (layout, open, mode, cursor) = editor.as_ref().map_or(
                (None, None, None, None),
                |editor| {
                    let panel = editor.read(cx);
                    (
                        Some(format!("{:?}", panel.margin_layout())),
                        panel.margin_open().map(str::to_string),
                        Some(format!("{:?}", shell.read(cx).margin_mode)),
                        Some(panel.cursor()),
                    )
                },
            );
            json!({ "active": active, "layout": layout, "open": open, "mode": mode, "cursor": cursor })
        })?;
        self.with_window(cx, |shell, _, cx| {
            shell.update(cx, |shell, cx| shell.set_margin_mode(MarginMode::Auto, cx))
        })?;
        steps.check(
            "Review here opens the comment on or after the caret's line",
            shown && opened && expected.as_deref() == Some(ours[1].as_str()),
            json!({
                "shown": shown,
                "opened": opened,
                "ours": ours,
                "expected": expected,
                "items": items,
                "facts": facts,
            }),
        );
        Ok(())
    }

    /// Probes Overleaf was asked while neither a push waited nor a sync ran,
    /// until `enough` of that time has passed or `deadline`, and how much of
    /// it there was. A push probes only after its 2.5 s quiet period and a
    /// sync sets its flag before it asks anything, so a 50 ms sample never
    /// misses either; the poll's own probes all land in the count.
    async fn settled_probes(
        &self,
        cx: &mut AsyncApp,
        mock: &MockOverleaf,
        probes: &str,
        enough: Duration,
        deadline: std::time::Instant,
    ) -> (usize, Duration) {
        let settled = |cx: &mut AsyncApp| {
            self.with_window(cx, |shell, _, cx| {
                let bridge = shell.read(cx).overleaf.read(cx);
                !bridge.push_waiting() && !bridge.syncing()
            })
            .unwrap_or(false)
        };
        let mut counted = 0;
        let mut quiet = Duration::ZERO;
        let mut was_settled = settled(cx);
        let mut seen = mock.requests(probes);
        let mut last = std::time::Instant::now();
        while quiet < enough && last < deadline {
            self.pause(cx, Duration::from_millis(50)).await;
            let now_settled = settled(cx);
            let count = mock.requests(probes);
            if was_settled && now_settled {
                counted += count - seen;
                quiet += last.elapsed();
            }
            seen = count;
            last = std::time::Instant::now();
            was_settled = now_settled;
        }
        (counted, quiet)
    }
}
