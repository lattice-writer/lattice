//! The typing guard (LEO-6, \`script/typing-guard\`): typing into a long
//! \`.tex\` document costs few frames and little CPU, in its source editor and
//! in its Visual view. Native keys go one every 110 ms at a line of prose of
//! a 240 KB document written into the project (the size of the Ethereum
//! Yellow Paper that LEO-6 measured), and each view's frames per key and this
//! process's CPU are held to limits about 1.5 times what main costs on the
//! Mac mini. A Visual page asking TexLab on every letter again drew 1.8
//! frames a key (lattice#1347), which its limit catches.
//!
//! The frames are GPUI's own count (\`Window::layout_stats\`, test support,
//! hence the offscreen-shots build), the CPU \`getrusage\` of this process,
//! so TexLab and WebKit's processes are not in it. Each phase's epoch-ms
//! window is in the result for an outside sampler that wants them
//! (\`LATTICE_TYPING_SECS\` lengthens the phases for such a run). Typing
//! needs the window key and the page's keyboard focus, so each phase is a
//! foreground section.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use gpui::{AsyncApp, Entity};
use lattice_settings::{AutoBuildMode, LatexView};
use serde_json::{Value, json};

use super::{Run, measure::footprint, sleep};
use crate::{
    editor_panel::{EditorPanel, choose_latex_view},
    native,
};

const PHRASE: &str = "the quick brown fox jumps over the lazy dog ";
const KEY_INTERVAL: Duration = Duration::from_millis(110);
/// Time for opening the project and the document (the PDF, TexLab reading
/// the project) to finish before anything is counted.
const SETTLE: Duration = Duration::from_secs(20);
/// The document typed into, written into the project's folder.
const FILE: &str = "typing-guard.tex";
/// Frames a key may draw, and this process's share of a core while typing,
/// per view. Main on the Mac mini: source 2.2 frames a key at 4.7%, Visual
/// 1.2 at 4.5%.
const SOURCE_FRAMES_PER_KEY: f64 = 3.0;
const VISUAL_FRAMES_PER_KEY: f64 = 1.6;
const CPU_PERCENT: f64 = 10.;
/// Below this the window is not drawing the keys at all (covered, or on a
/// display that sleeps), which would pass the limits while measuring nothing.
const DRAWING_FRAMES_PER_KEY: f64 = 0.5;

fn epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// User plus system CPU time of this process, in seconds.
fn cpu_seconds() -> f64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage fills in the struct it is given.
    let usage = unsafe {
        libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr());
        usage.assume_init()
    };
    let seconds = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    seconds(usage.ru_utime) + seconds(usage.ru_stime)
}

/// A paper of about 240 KB: 300 sections of prose with inline and display
/// maths, emphasis and a list each, the same bytes every run.
fn long_document() -> String {
    const WORDS: [&str; 16] = [
        "state",
        "transition",
        "account",
        "value",
        "message",
        "block",
        "proof",
        "gas",
        "the",
        "of",
        "and",
        "is",
        "every",
        "which",
        "given",
        "header",
    ];
    let mut seed = 7u32;
    let mut word = || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        WORDS[(seed >> 16) as usize % WORDS.len()]
    };
    let mut text = String::from(
        "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\title{Typing guard}\n\\maketitle\n\n",
    );
    for section in 1..=300 {
        text.push_str(&format!(
            "\\section{{Section {section}}}\n\\label{{sec:{section}}}\n"
        ));
        for paragraph in 0..3 {
            for line in 0..2 {
                let words: Vec<&str> = (0..12).map(|_| word()).collect();
                text.push_str(&words.join(" "));
                match (paragraph, line) {
                    (0, 0) => text.push_str(&format!(
                        " with $\\sigma_{{{section}}} = \\Upsilon(\\sigma, T)$"
                    )),
                    (1, 1) => text.push_str(" as \\emph{defined} in \\textbf{this} part"),
                    _ => {}
                }
                text.push_str(".\n");
            }
            text.push('\n');
        }
        text.push_str(&format!(
            "\\begin{{equation}}\n  \\boldsymbol{{\\sigma}}_{{t+{section}}} \\equiv \\Pi(\\boldsymbol{{\\sigma}}_t, B)\n\\end{{equation}}\n\\begin{{itemize}}\n  \\item {} {}\n  \\item {} {}\n\\end{{itemize}}\n\n",
            word(),
            word(),
            word(),
            word()
        ));
    }
    text.push_str("\\end{document}\n");
    text
}

/// The first line past 40% of the document that reads as prose.
fn prose_line(text: &str) -> u32 {
    let lines: Vec<&str> = text.lines().collect();
    let from = lines.len() * 2 / 5;
    lines
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, line)| {
            line.len() > 60 && line.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        })
        .map_or(from as u32 + 1, |(index, _)| index as u32 + 1)
}

impl Run {
    /// Types for \`secs\` and reports what it cost.
    async fn typing_phase(
        &self,
        cx: &mut AsyncApp,
        editor: &Entity<EditorPanel>,
        secs: Duration,
    ) -> Result<Value> {
        let native = self.native(cx)?;
        let before = self.editor_text(cx, editor).len();
        let footprint_before = footprint()["footprint"].clone();
        self.with_window(cx, |_, window, _| window.reset_layout_stats())?;
        let cpu_before = cpu_seconds();
        let start = epoch_ms();
        let began = Instant::now();
        let phrase: Vec<char> = PHRASE.chars().collect();
        let mut keys = 0usize;
        while began.elapsed() < secs {
            native::send_key(native, phrase[keys % phrase.len()], false)?;
            keys += 1;
            sleep(cx, KEY_INTERVAL).await;
        }
        let cpu = 100. * (cpu_seconds() - cpu_before) / began.elapsed().as_secs_f64();
        let end = epoch_ms();
        let frames = self.with_window(cx, |_, window, _| window.layout_stats().frames)?;
        sleep(cx, Duration::from_secs(2)).await;
        let inserted = self.editor_text(cx, editor).len().saturating_sub(before);
        Ok(json!({
            "start": start,
            "end": end,
            "keys": keys,
            "inserted": inserted,
            "frames_per_key": frames as f64 / keys.max(1) as f64,
            "cpu_percent": cpu,
            "footprint_before": footprint_before,
            "footprint_after": footprint()["footprint"].clone(),
        }))
    }

    /// Gives the source editor the keyboard.
    async fn focus_source(&self, cx: &mut AsyncApp, editor: &Entity<EditorPanel>) -> Result<()> {
        cx.update(|cx| choose_latex_view(LatexView::Source, cx));
        sleep(cx, Duration::from_secs(1)).await;
        let bounds = cx
            .update(|cx| editor.read(cx).editor().read(cx).last_bounds().copied())
            .context("the editor has not been laid out")?;
        native::send_click(self.native(cx)?, bounds.center())?;
        sleep(cx, Duration::from_millis(300)).await;
        Ok(())
    }

    /// Shows the Visual view and gives its page the keyboard.
    async fn focus_visual(&self, cx: &mut AsyncApp, editor: &Entity<EditorPanel>) -> Result<()> {
        cx.update(|cx| choose_latex_view(LatexView::Visual, cx));
        let drawn = self
            .wait_for(cx, Duration::from_secs(60), |_, cx| {
                editor
                    .read(cx)
                    .latex_visual_document()
                    .is_some_and(|document| document.read(cx).is_drawn())
            })
            .await;
        anyhow::ensure!(drawn, "the Visual view was not drawn within 60 s");
        sleep(cx, Duration::from_secs(5)).await;
        let bounds = cx
            .update(|cx| {
                editor
                    .read(cx)
                    .latex_visual_document()
                    .and_then(|document| document.read(cx).island().cloned())
                    .and_then(|island| island.read(cx).bounds(cx))
            })
            .context("the Visual view has no bounds")?;
        native::send_click(self.native(cx)?, bounds.center())?;
        sleep(cx, Duration::from_millis(500)).await;
        Ok(())
    }

    pub(super) async fn typing(&self, cx: &mut AsyncApp) -> Result<Value> {
        let root = self.root(cx).await?;
        let secs = Duration::from_secs(
            std::env::var("LATTICE_TYPING_SECS")
                .ok()
                .map(|v| {
                    v.parse()
                        .context("LATTICE_TYPING_SECS is not a number of seconds")
                })
                .transpose()?
                .unwrap_or(30),
        );
        // A save would build the document, whose typesetting is not typing.
        cx.update(|cx| {
            crate::settings::update(cx, |settings| {
                settings.auto_build_mode = AutoBuildMode::Manual
            })
        });
        let document = long_document();
        let line = prose_line(&document);
        std::fs::write(root.join(FILE), &document)?;
        cx.update(|cx| choose_latex_view(LatexView::Source, cx));
        let editor = self.open(cx, &root.join(FILE), Some(1))?;
        sleep(cx, SETTLE).await;
        let mut checks = Vec::new();
        for (view, limit) in [
            ("source", SOURCE_FRAMES_PER_KEY),
            ("Visual", VISUAL_FRAMES_PER_KEY),
        ] {
            let phase = {
                let _front = self.foreground(cx, "typing").await?;
                if view == "source" {
                    self.focus_source(cx, &editor).await?;
                } else {
                    self.focus_visual(cx, &editor).await?;
                }
                self.with_window(cx, {
                    let editor = editor.clone();
                    move |_, window, cx| {
                        editor.update(cx, |panel, cx| panel.go_to_line(line, window, cx))
                    }
                })?;
                sleep(cx, Duration::from_secs(2)).await;
                self.typing_phase(cx, &editor, secs).await?
            };
            let frames = phase["frames_per_key"].as_f64().unwrap_or(0.);
            let typed = phase["inserted"] == phase["keys"];
            checks.push(json!({
                "name": format!("typing in the {view} view of a long document draws at most {limit} frames a key"),
                "passed": typed && (DRAWING_FRAMES_PER_KEY..=limit).contains(&frames),
                "detail": phase,
            }));
            checks.push(json!({
                "name": format!("typing in the {view} view of a long document costs the app at most {CPU_PERCENT}% of a core"),
                "passed": typed && phase["cpu_percent"].as_f64().is_some_and(|cpu| cpu <= CPU_PERCENT),
                "detail": { "cpu_percent": phase["cpu_percent"], "typed": typed },
            }));
        }
        cx.update(|cx| choose_latex_view(LatexView::Source, cx));
        let passed = checks.iter().all(|check| check["passed"] == true);
        Ok(json!({ "scenario": "typing", "passed": passed, "line": line, "checks": checks }))
    }
}
