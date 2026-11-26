//! Author text colours in the visual editor (`inline-color.ts`,
//! `inline-color-view.ts`), on a scratch `e2e-colors.md`:
//!
//! - a `<span style="color: …">` run is drawn in its colour, the span keeping
//!   the colour as written, and refitted to read at 4.5:1 on the editor
//!   ground where it would not (gold on a light page, navy on a dark one),
//!   again when the theme changes;
//! - a span with any other declaration stays raw source, its style inert;
//! - opening writes nothing, and a word typed inside a coloured run with
//!   native keys is saved inside its tags, every other byte as it was.

#![cfg(target_os = "macos")]

use std::{path::Path, time::Duration};

use anyhow::Result;
use gpui::AsyncApp;
use lattice_ui::{Appearance, LatticeTheme};
use serde_json::{Value, json};

use super::super::{Run, Steps};
use crate::native;

const COLORS: &str = "# Colours\n\nPlain <span style=\"color: navy\">navy note</span> and <span style='color:#ffd700'>gold</span> then <span style=\"color: red; display: none\">shown</span> end.\n";

/// The coloured runs as drawn (text, authored colour, painted colour, its
/// contrast on the page's ground), and whether the inert span stayed source.
const READ: &str = "(() => { const rgb = (s) => (s.match(/[\\d.]+/g) || []).slice(0, 3).map(Number); \
    const lum = (c) => { const [r, g, b] = c.map((v) => { v /= 255; return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }); return 0.2126 * r + 0.7152 * g + 0.0722 * b; }; \
    const ratio = (a, b) => { const [x, y] = [lum(rgb(a)), lum(rgb(b))].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); }; \
    const ground = getComputedStyle(document.querySelector('.markdown-preview')).backgroundColor; \
    const surface = document.querySelector('.lx-md-surface'); if (!surface) return JSON.stringify(null); \
    const runs = [...surface.querySelectorAll('span[data-lattice-color]')].map((node) => { const painted = getComputedStyle(node).color; \
      return { text: node.textContent, authored: node.dataset.latticeColor, painted, contrast: Math.round(ratio(painted, ground) * 100) / 100 }; }); \
    const text = surface.textContent; \
    return JSON.stringify({ theme: document.documentElement.dataset.theme, ground, runs, \
      inert: text.includes('<span style=\"color: red; display: none\">shown</span>') }); })()";

/// Put the caret after "navy" inside the navy run.
const CARET_IN_NAVY: &str = "(() => { const span = document.querySelector('.lx-md-surface span[data-lattice-color=\"navy\"]'); \
    const walker = span && document.createTreeWalker(span, NodeFilter.SHOW_TEXT); const node = walker && walker.nextNode(); \
    if (!node) return 'missing'; const range = document.createRange(); range.setStart(node, 4); range.collapse(true); \
    const selection = window.getSelection(); selection.removeAllRanges(); selection.addRange(range); return 'placed'; })()";

fn run_of<'a>(page: &'a Value, text: &str) -> Option<&'a Value> {
    page["runs"].as_array()?.iter().find(|run| run["text"] == text)
}

fn painted(page: &Value, text: &str) -> Option<String> {
    run_of(page, text).and_then(|run| run["painted"].as_str().map(str::to_string))
}

fn readable(page: &Value, text: &str) -> bool {
    run_of(page, text).is_some_and(|run| run["contrast"].as_f64().is_some_and(|ratio| ratio >= 4.5))
}

impl Run {
    pub(in crate::scenario) async fn markdown_colors(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        root: &Path,
    ) -> Result<()> {
        let name = "a <span style=\"color: …\"> run in the visual editor is drawn in its colour, refitted to 4.5:1 on a light and a dark ground, and a span with another declaration stays inert source";
        let path = root.join("e2e-colors.md");
        std::fs::write(&path, COLORS)?;
        cx.update(|cx| LatticeTheme::set_appearance(Appearance::Light, cx));
        let panel = self.open(cx, &path, None)?;
        let drawn = self
            .wait_for(cx, Duration::from_secs(40), |_, cx| {
                panel
                    .read(cx)
                    .markdown_document()
                    .is_some_and(|document| document.read(cx).is_drawn())
            })
            .await;
        let island = cx.update(|cx| {
            panel
                .read(cx)
                .markdown_document()
                .and_then(|document| document.read(cx).island().cloned())
        });
        let Some(island) = island.filter(|_| drawn) else {
            steps.check(name, false, json!({ "drawn": drawn }));
            return Ok(());
        };

        let light = self.evaluate(cx, &island, READ).await;
        cx.update(|cx| LatticeTheme::set_appearance(Appearance::Dark, cx));
        let mut dark = Value::Null;
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(3) {
            self.pause(cx, Duration::from_millis(150)).await;
            dark = self.evaluate(cx, &island, READ).await;
            if dark["theme"] == "dark" && painted(&dark, "navy note") != painted(&light, "navy note") {
                break;
            }
        }
        cx.update(|cx| LatticeTheme::set_appearance(Appearance::Light, cx));
        self.pause(cx, Duration::from_millis(300)).await;
        let back = self.evaluate(cx, &island, READ).await;
        let authored = |page: &Value| {
            run_of(page, "navy note").is_some_and(|run| run["authored"] == "navy")
                && run_of(page, "gold").is_some_and(|run| run["authored"] == "#ffd700")
                && page["runs"].as_array().is_some_and(|runs| runs.len() == 2)
        };
        steps.check(
            name,
            authored(&light)
                && authored(&dark)
                // Navy reads on a light page as written; gold is darkened.
                && painted(&light, "navy note").as_deref() == Some("rgb(0, 0, 128)")
                && painted(&light, "gold").as_deref() != Some("rgb(255, 215, 0)")
                // On a dark page gold reads as written; navy is lightened.
                && painted(&dark, "gold").as_deref() == Some("rgb(255, 215, 0)")
                && painted(&dark, "navy note").as_deref() != Some("rgb(0, 0, 128)")
                && ["navy note", "gold"].iter().all(|text| readable(&light, text) && readable(&dark, text))
                && painted(&back, "navy note") == painted(&light, "navy note")
                && light["inert"] == true,
            json!({ "light": light, "dark": dark, "back": back }),
        );

        // Opening writes nothing; a word typed inside the navy run lands
        // inside its tags.
        self.pause(cx, Duration::from_millis(1000)).await;
        let unchanged = std::fs::read_to_string(&path).is_ok_and(|text| text == COLORS);
        let Some(bounds) = cx.update(|cx| island.read(cx).bounds(cx)) else {
            steps.check("a word typed inside a coloured run in the visual editor is saved inside its colour span, the file's other bytes as they were", false, json!("no bounds"));
            return Ok(());
        };
        let native = self.native(cx)?;
        let at = self
            .evaluate(cx, &island, &super::paragraph_point("Plain"))
            .await
            .as_array()
            .and_then(|xy| Some((xy.first()?.as_f64()?, xy.get(1)?.as_f64()?)))
            .map(|(x, y)| bounds.origin + gpui::point(gpui::px(x as f32), gpui::px(y as f32)))
            .unwrap_or_else(|| bounds.center());
        native::send_click(native, at)?;
        self.wait_for(cx, Duration::from_secs(3), |_, cx| island.read(cx).page_focused())
            .await;
        let placed = self.evaluate(cx, &island, CARET_IN_NAVY).await;
        self.pause(cx, Duration::from_millis(150)).await;
        for character in "ish".chars() {
            native::send_key(native, character, false)?;
            self.pause(cx, Duration::from_millis(40)).await;
        }
        let expected = COLORS.replacen("navy note", "navyish note", 1);
        let saved = self
            .wait_for(cx, Duration::from_secs(10), |_, _| {
                std::fs::read_to_string(&path).is_ok_and(|text| text.contains("navyish"))
            })
            .await;
        self.pause(cx, Duration::from_millis(400)).await;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let after = self.evaluate(cx, &island, READ).await;
        steps.check(
            "a word typed inside a coloured run in the visual editor is saved inside its colour span, the file's other bytes as they were",
            unchanged
                && saved
                && text == expected
                && run_of(&after, "navyish note").is_some_and(|run| run["authored"] == "navy")
                && painted(&after, "navyish note") == painted(&light, "navy note"),
            json!({
                "unchanged": unchanged,
                "placed": placed,
                "saved": saved,
                "exact": text == expected,
                "text": if text == expected { Value::Null } else { json!(text) },
                "after": after,
            }),
        );
        Ok(())
    }
}
