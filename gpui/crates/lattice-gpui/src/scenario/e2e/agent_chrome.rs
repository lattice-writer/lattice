//! The Agent panel's own chrome around its page (`agent/surface/`):
//!
//! - the native skeleton stands over the page until it has drawn, and goes;
//! - the header's permission picker appears once the frame reports its
//!   mode, follows the report, and a mode chosen in it reaches the page;
//!   a report without a runtime takes it away;
//! - a Finder drag over the panel lights it and leaving puts it out; any
//!   file is an attachment (lattice#1254), so a drop of a figure, a .docx,
//!   an .xlsx or an unknown extension has the page read that drop's bytes,
//!   and only that drop's, for the composer, never into the project; a
//!   folder is refused.

#![cfg(target_os = "macos")]

use std::{path::Path, time::Duration};

use anyhow::Result;
use gpui::{AsyncApp, Entity};
use lattice_webview::{Island, IslandEvent};
use serde_json::{Value, json};

use super::Steps;
use crate::{
    agent::surface::permission::{AgentPermission, PermissionMode},
    scenario::Run,
};

/// The smallest PNG: one transparent pixel.
const PIXEL: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

const LISTEN: &str = "(() => { if (!window.latticeHost) return false; \
     if (!window.__e2eChrome) { window.__e2eChrome = { modes: [], drags: [] }; \
       window.latticeHost.on('agent:set-permission-mode', (p) => window.__e2eChrome.modes.push(p)); \
       window.latticeHost.on('agent:file-drag', (p) => window.__e2eChrome.drags.push(p)); } \
     return true; })()";
const HEARD: &str = "JSON.stringify(window.__e2eChrome || null)";
const LIT: &str = "!!document.querySelector('.agent-drop-active')";

impl Run {
    pub(super) async fn agent_chrome(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        root: &Path,
    ) -> Result<()> {
        self.open(cx, &root.join("main.tex"), Some(1))?;
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.show_view("agent", true, window, cx))
        })?;
        let surface = cx.update(|cx| self.shell.read(cx).agent.panel.read(cx).surface().clone());
        let Some(island) = cx.update(|cx| surface.read(cx).island().cloned()) else {
            steps.check(
                "the Agent panel hosts the Agent island",
                false,
                json!({ "reason": "no island: the island pages are not built" }),
            );
            return Ok(());
        };

        // The skeleton, while the page has not drawn (an earlier module may
        // have drawn it already: then it must be gone).
        let (drawn_first, awaiting_first) = cx.update(|cx| {
            let surface = surface.read(cx);
            (surface.is_drawn(), surface.awaiting_page().is_some())
        });
        let drawn = self
            .wait_for(cx, Duration::from_secs(20), |_, cx| surface.read(cx).is_drawn())
            .await;
        let awaiting_after = cx.update(|cx| surface.read(cx).awaiting_page().is_some());
        steps.check(
            "the native skeleton stands over the Agent page until it draws, then goes",
            drawn && (drawn_first || awaiting_first) && !awaiting_after,
            json!({ "drawnFirst": drawn_first, "awaitingFirst": awaiting_first,
                    "drawn": drawn, "awaitingAfter": awaiting_after }),
        );
        let listening = self.evaluate(cx, &island, LISTEN).await == Value::Bool(true);

        // The permission picker, as the frame reports its mode.
        self.page_posts(
            cx,
            &island,
            "agent:permission",
            json!({ "mode": "auto", "autoModeAvailable": false }),
        )
        .await;
        let reported = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                surface.read(cx).permission()
                    == Some(AgentPermission {
                        mode: PermissionMode::Auto,
                        auto_available: false,
                    })
            })
            .await;
        steps.check(
            "the header's permission picker shows the mode the frame reports",
            listening && reported,
            json!({ "listening": listening,
                    "permission": format!("{:?}", cx.update(|cx| surface.read(cx).permission())) }),
        );
        cx.update(|cx| {
            surface.update(cx, |surface, cx| {
                surface.set_permission_mode(PermissionMode::ApprovalRequired, cx)
            })
        });
        let heard = self
            .heard(cx, &island, |heard| {
                heard["modes"]
                    .as_array()
                    .is_some_and(|modes| modes.iter().any(|mode| mode["mode"] == "approval-required"))
            })
            .await;
        steps.check(
            "a mode chosen in the picker reaches the Agent's page",
            heard.is_some(),
            json!({ "heard": heard }),
        );
        self.page_posts(cx, &island, "agent:permission", json!({ "mode": null }))
            .await;
        let gone = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                surface.read(cx).permission().is_none()
            })
            .await;
        steps.check(
            "without a runtime the header shows no permission picker",
            gone,
            json!({}),
        );

        // Finder drags over the panel: any file is an attachment
        // (lattice#1254), whatever its format.
        let figure = root.with_file_name("agent-drop-figure.png");
        let docx = root.with_file_name("agent-drop-report.docx");
        let xlsx = root.with_file_name("agent-drop-budget.xlsx");
        let odd = root.with_file_name("agent-drop-trace.qzx");
        let folder = root.with_file_name("agent-drop-folder");
        std::fs::write(&figure, PIXEL)?;
        std::fs::write(&docx, b"PK\x03\x04word/document.xml\x00\xff\xfe")?;
        std::fs::write(&xlsx, b"PK\x03\x04xl/workbook.xml\x00\x80\x81")?;
        std::fs::write(&odd, b"\x00\x01\x02 no known format")?;
        std::fs::create_dir_all(&folder)?;
        let path = |path: &Path| path.to_string_lossy().into_owned();
        let figure_path = path(&figure);
        let binaries = [path(&docx), path(&xlsx), path(&odd)];
        let binaries: Vec<&str> = binaries.iter().map(String::as_str).collect();
        drag(cx, &island, "enter", &[&figure_path]);
        drag(cx, &island, "over", &[]);
        let lit = self.page_shows(cx, &island, LIT, true).await;
        drag(cx, &island, "leave", &[]);
        let out = self.page_shows(cx, &island, LIT, false).await;
        steps.check(
            "a figure dragged over the Agent panel lights it, and leaving puts it out",
            lit && out,
            json!({ "lit": lit, "out": out }),
        );
        drag(cx, &island, "enter", &binaries);
        drag(cx, &island, "over", &[]);
        let lit = self.page_shows(cx, &island, LIT, true).await;
        drag(cx, &island, "leave", &[]);
        steps.check(
            "binary and unknown files dragged over the Agent panel light it too",
            lit,
            json!({}),
        );
        drag(cx, &island, "enter", &[&figure_path]);
        drag(cx, &island, "drop", &[&figure_path]);
        let read = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                surface.read(cx).drop_read().is_some()
            })
            .await;
        let outcome = cx.update(|cx| surface.read(cx).drop_read().cloned());
        let put_out = self.page_shows(cx, &island, LIT, false).await;
        let figure_read = Some(Ok(vec![(
            "agent-drop-figure.png".to_string(),
            "image/png".to_string(),
        )]));
        steps.check(
            "a figure dropped on the Agent panel has the page read its bytes for the composer",
            read && outcome == figure_read && put_out,
            json!({ "outcome": format!("{outcome:?}"), "putOut": put_out }),
        );
        // A .docx, an .xlsx and an extension nobody knows go to the composer
        // as attachments, read as bytes and never into an editor or the
        // project: no tab opens and the project folder gains no file.
        let opened = |cx: &mut AsyncApp| {
            cx.update(|cx| {
                let shell = self.shell.read(cx);
                binaries.iter().any(|path| {
                    let name = Path::new(path).file_name().unwrap_or_default().to_string_lossy();
                    shell.view_of_document(path, cx).is_some()
                        || shell.view_of_document(&name, cx).is_some()
                })
            })
        };
        let entries_before = project_entries(root);
        drag(cx, &island, "enter", &binaries);
        drag(cx, &island, "drop", &binaries);
        let octet = |name: &str| (name.to_string(), "application/octet-stream".to_string());
        let wanted = Ok(vec![
            octet("agent-drop-report.docx"),
            octet("agent-drop-budget.xlsx"),
            octet("agent-drop-trace.qzx"),
        ]);
        let read = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                surface.read(cx).drop_read() == Some(&wanted)
            })
            .await;
        self.pause(cx, Duration::from_millis(300)).await;
        let untouched = !opened(cx) && project_entries(root) == entries_before;
        steps.check(
            "binary and unknown files dropped on the Agent panel go to the composer as attachments, not into the project",
            read && untouched,
            json!({ "outcome": format!("{:?}", cx.update(|cx| surface.read(cx).drop_read().cloned())),
                    "untouched": untouched }),
        );
        drag(cx, &island, "enter", &[&path(&folder)]);
        drag(cx, &island, "drop", &[&path(&folder)]);
        let refused = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                surface
                    .read(cx)
                    .drop_read()
                    .is_some_and(|read| read.as_ref().is_err_and(|error| error.contains("folder")))
            })
            .await;
        steps.check(
            "a folder dropped on the Agent panel is refused with a reason",
            refused,
            json!({ "outcome": format!("{:?}", cx.update(|cx| surface.read(cx).drop_read().cloned())) }),
        );
        // A page's read with no drop behind it reads nothing.
        let again = self
            .evaluate(
                cx,
                &island,
                "(async () => { try { await window.__TAURI_INTERNALS__.invoke(\
                   'read_agent_composer_files', { paths: ['/etc/hosts'] }); return 'read'; } \
                 catch (error) { return 'refused'; } })()",
            )
            .await;
        let refused = self
            .wait_for(cx, Duration::from_secs(5), |_, cx| {
                surface.read(cx).drop_read().is_some_and(Result::is_err)
            })
            .await;
        steps.check(
            "the page cannot have the app read a path that was not just dropped",
            refused,
            json!({ "page": again, "outcome": format!("{:?}", cx.update(|cx| surface.read(cx).drop_read().cloned())) }),
        );
        // Nor by posting a drop of its own before the read.
        self.evaluate(
            cx,
            &island,
            &format!(
                "(() => {{ const paths = [{path}, {path}]; \
                 window.latticeHost?.post('island:file-drop', {{ phase: 'drop', paths }}); \
                 setTimeout(() => window.__TAURI_INTERNALS__.invoke(\
                   'read_agent_composer_files', {{ paths }}).catch(() => {{}}), 200); \
                 return true; }})()",
                path = json!(figure_path)
            ),
        )
        .await;
        let forged = self
            .wait_for(cx, Duration::from_secs(3), |_, cx| {
                surface.read(cx).drop_read().is_some_and(|read| read.as_ref().is_ok_and(|files| files.len() == 2))
            })
            .await;
        steps.check(
            "a drop the page posts itself does not let it read that path",
            !forged,
            json!({ "outcome": format!("{:?}", cx.update(|cx| surface.read(cx).drop_read().cloned())) }),
        );
        for file in [&figure, &docx, &xlsx, &odd] {
            std::fs::remove_file(file).ok();
        }
        std::fs::remove_dir(&folder).ok();
        Ok(())
    }

    /// Have the page post `kind` to its owner, as its own code would.
    async fn page_posts(
        &self,
        cx: &mut AsyncApp,
        island: &Entity<Island>,
        kind: &str,
        payload: Value,
    ) {
        let script = format!(
            "(() => {{ window.latticeHost?.post({}, {payload}); return true; }})()",
            json!(kind)
        );
        self.evaluate(cx, island, &script).await;
    }

    /// What the page heard, once `done` holds of it (within 5 s).
    async fn heard(
        &self,
        cx: &mut AsyncApp,
        island: &Entity<Island>,
        done: impl Fn(&Value) -> bool,
    ) -> Option<Value> {
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            let heard = self.evaluate(cx, island, HEARD).await;
            if done(&heard) {
                return Some(heard);
            }
            self.pause(cx, Duration::from_millis(100)).await;
        }
        None
    }

    /// Whether `script` comes to answer `wanted` within 2 s.
    async fn page_shows(
        &self,
        cx: &mut AsyncApp,
        island: &Entity<Island>,
        script: &str,
        wanted: bool,
    ) -> bool {
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(2) {
            if self.evaluate(cx, island, script).await == Value::Bool(wanted) {
                return true;
            }
            self.pause(cx, Duration::from_millis(100)).await;
        }
        false
    }
}

/// The names in the project folder, to see a drop added none.
fn project_entries(root: &Path) -> Vec<std::ffi::OsString> {
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .map(|entries| entries.flatten().map(|entry| entry.file_name()).collect())
        .unwrap_or_default();
    entries.sort();
    entries
}

/// A phase of a Finder drag, as the island's drag handler hands it on
/// (`lattice-webview`'s `island/file_drops.rs`).
fn drag(cx: &mut AsyncApp, island: &Entity<Island>, phase: &str, paths: &[&str]) {
    let payload = json!({ "phase": phase, "paths": paths, "x": 40., "y": 40. });
    cx.update(|cx| {
        island.update(cx, |_, cx| {
            cx.emit(IslandEvent::FileDrop(payload))
        })
    });
}

pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Terminal,
    rank: 32,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-gpui/src/agent/surface/permission.rs",
        "gpui/crates/lattice-gpui/src/agent/surface/skeleton.rs",
        "gpui/crates/lattice-gpui/src/agent/surface/drops.rs",
        "gpui/crates/lattice-webview/src/island/file_drops.rs",
        "gpui/islands/agent/drops.ts",
    ],
    run: |run, cx, steps, fixture| Box::pin(run.agent_chrome(cx, steps, &fixture.root)),
};
