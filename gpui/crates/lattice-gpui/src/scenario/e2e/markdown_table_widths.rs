//! A Markdown table's column widths (`table-column-widths.ts` in the engine,
//! `editor_panel/table_widths.rs` keeping them), driven with native
//! mouse events on the visual editor of a scratch file with two tables:
//!
//! - the pointer resting on a column's border shows the resize cursor;
//! - a drag of the first table's first border widens that column by the
//!   distance dragged, the file's bytes as they were, and the widths are
//!   kept for that table (saved at index 0);
//! - a double-click on the second table's first border fits that column to
//!   its content (its long header), kept as table 1's;
//! - a drag left without its mouse-up, a press on the second table's border
//!   still drags that border, and no resize cursor is left behind;
//! - the tab closed and the file opened again draws both tables at the
//!   widths kept;
//! - a column added to the first table on disk draws it at its content's
//!   widths, the second keeping its own, and nothing saved is dropped: the
//!   column taken away again brings the first table's widths back;
//! - the pointer on a body row's bottom border shows the row-resize cursor,
//!   a drag down makes that row that much higher (kept by its row index
//!   beside the table's widths, the file's bytes as they were), and a
//!   double-click gives it its content's height back.

#![cfg(target_os = "macos")]

use std::{path::Path, time::Duration};

use anyhow::Result;
use gpui::{AsyncApp, Entity, Pixels, Point};
use lattice_webview::Island;
use serde_json::{Value, json};

use super::{Run, Steps};
use crate::{editor_panel::{EditorPanel, table_widths}, native};

const FILE: &str = "e2e-tables.md";

const TABLES: &str = "# Tables

| Model | Score |
| --- | --- |
| Base | 1 |

Between the tables.

| A fairly long header for fitting | B | C |
| --- | --- | --- |
| x | y | z |
";

/// The same file with a third column in its first table.
const WIDER: &str = "# Tables

| Model | Score | Extra |
| --- | --- | --- |
| Base | 1 | 2 |

Between the tables.

| A fairly long header for fitting | B | C |
| --- | --- | --- |
| x | y | z |
";

/// How far the border is dragged, in the page's pixels.
const DRAG: f64 = 80.;
/// How far the second table's border is dragged after a lost release.
const REPRESS_DRAG: f64 = 30.;
/// How far a row's border is dragged down, in the page's pixels.
const ROW_DRAG: f64 = 30.;

/// The page's view of a table: its first header cell's right edge (brought
/// into view first), its columns' set widths, and the width of its frame (a
/// fitted column is never wider).
fn table_facts(index: usize) -> String {
    format!(
        "(() => {{ const table = document.querySelectorAll('.lx-md-surface table')[{index}]; if (!table) return null; \
         const cell = table.querySelector('th'); cell.scrollIntoView({{ block: 'center' }}); const r = cell.getBoundingClientRect(); \
         return JSON.stringify({{ edge: [r.right - 1, r.top + r.height / 2], width: cell.offsetWidth, frame: table.parentElement?.clientWidth ?? 0, \
           cols: [...table.querySelectorAll('col')].map((col) => col.style.width), sized: table.hasAttribute('data-sized'), \
           cursor: document.querySelector('.lx-md-editor')?.hasAttribute('data-column-resize') ?? false }}); }})()"
    )
}

/// The page's view of a table's first body row: a point on its bottom
/// border (brought into view first), its height and its set height.
fn row_facts(index: usize) -> String {
    format!(
        "(() => {{ const table = document.querySelectorAll('.lx-md-surface table')[{index}]; const row = table?.rows[1]; if (!row) return null; \
         const cell = row.cells[0]; cell.scrollIntoView({{ block: 'center' }}); const r = cell.getBoundingClientRect(); \
         return JSON.stringify({{ edge: [r.left + r.width / 2, r.bottom - 1], height: row.offsetHeight, set: row.style.height, \
           cursor: document.querySelector('.lx-md-editor')?.hasAttribute('data-row-resize') ?? false }}); }})()"
    )
}

/// The row heights kept for the table saved at `index`, none for a row at
/// its content's height.
fn heights_at(saved: &Value, index: u64) -> Vec<Option<f64>> {
    saved
        .as_array()
        .and_then(|tables| tables.iter().find(|table| table["index"] == index))
        .and_then(|table| table["heights"].as_array())
        .map(|heights| heights.iter().map(Value::as_f64).collect())
        .unwrap_or_default()
}

/// The widths kept for the table saved at `index` (the file's tables have
/// distinct signatures, so each is saved at its own index).
fn kept_at(saved: &Value, index: u64) -> Vec<f64> {
    saved
        .as_array()
        .and_then(|tables| tables.iter().find(|table| table["index"] == index))
        .and_then(|table| table["widths"].as_array())
        .map(|widths| widths.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default()
}

fn px_widths(facts: &Value) -> Vec<f64> {
    facts["cols"]
        .as_array()
        .map(|cols| {
            cols.iter()
                .filter_map(|col| col.as_str()?.strip_suffix("px")?.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

impl Run {
    pub(super) async fn markdown_table_widths(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        root: &Path,
    ) -> Result<()> {
        // A native press on the page makes its window key: the module's
        // pointer work runs in one foreground section.
        let _front = self.foreground(cx, "table column widths").await?;
        let path = root.join(FILE);
        std::fs::write(&path, TABLES)?;
        let panel = self.open(cx, &path, None)?;
        let Some(island) = self.table_island(cx, &panel).await else {
            steps.check(
                "a Markdown table's column border dragged resizes the column",
                false,
                json!({ "drawn": false }),
            );
            return Ok(());
        };
        let native = self.native(cx)?;
        let to_window = |bounds: gpui::Bounds<Pixels>, xy: &Value| -> Option<Point<Pixels>> {
            let x = xy.get(0)?.as_f64()?;
            let y = xy.get(1)?.as_f64()?;
            Some(bounds.origin + gpui::point(gpui::px(x as f32), gpui::px(y as f32)))
        };
        let bounds = cx.update(|cx| island.read(cx).bounds(cx));

        // Hover, then drag the first table's first border.
        let before = self.evaluate(cx, &island, &table_facts(0)).await;
        let edge = bounds.and_then(|bounds| to_window(bounds, &before["edge"]));
        let mut hover = Value::Null;
        if let Some(edge) = edge {
            native::send_mouse(native, native::Mouse::Moved, edge - gpui::point(gpui::px(20.), gpui::px(0.)))?;
            self.pause(cx, Duration::from_millis(80)).await;
            native::send_mouse(native, native::Mouse::Moved, edge)?;
            self.pause(cx, Duration::from_millis(150)).await;
            hover = self.evaluate(cx, &island, &table_facts(0)).await;
            // A mouse-moved event sent to the window does not reach the
            // page's view (WebKit hears moves through its tracking areas),
            // so the page is also given the move at the same point.
            if hover["cursor"] != true {
                let [x, y] = [&before["edge"][0], &before["edge"][1]].map(|v| v.as_f64().unwrap_or_default());
                hover = self
                    .evaluate(
                        cx,
                        &island,
                        &format!(
                            "(() => {{ const target = document.elementFromPoint({x}, {y}); \
                             target?.dispatchEvent(new MouseEvent('mousemove', {{ bubbles: true, clientX: {x}, clientY: {y} }})); \
                             return {}; }})()",
                            table_facts(0)
                        ),
                    )
                    .await;
            }
            native::send_mouse(native, native::Mouse::Down, edge)?;
            for step in 1..=8 {
                let dx = DRAG as f32 * step as f32 / 8.;
                native::send_mouse(native, native::Mouse::Dragged, edge + gpui::point(gpui::px(dx), gpui::px(0.)))?;
                self.pause(cx, Duration::from_millis(30)).await;
            }
            native::send_mouse(native, native::Mouse::Up, edge + gpui::point(gpui::px(DRAG as f32), gpui::px(0.)))?;
        }
        steps.check(
            "the pointer resting on a Markdown table's column border shows the resize cursor",
            hover["cursor"] == true,
            json!({ "edge": format!("{edge:?}"), "hover": hover }),
        );
        let kept = |cx: &mut AsyncApp| cx.update(|_| table_widths::page_widths(root, FILE));
        self.wait_for(cx, Duration::from_secs(3), |_, _| {
            !kept_at(&table_widths::page_widths(root, FILE), 0).is_empty()
        })
        .await;
        let after = self.evaluate(cx, &island, &table_facts(0)).await;
        let widths = px_widths(&after);
        let expected = before["width"].as_f64().unwrap_or_default() + DRAG;
        let saved = kept(cx);
        let unchanged = std::fs::read_to_string(&path).is_ok_and(|text| text == TABLES);
        steps.check(
            "a Markdown table's column border dragged widens the column by the distance dragged; the widths are kept for that table and the file's bytes stay as they were",
            after["sized"] == true
                && widths.first().is_some_and(|width| (width - expected).abs() <= 2.)
                && kept_at(&saved, 0).len() == 2
                && kept_at(&saved, 0).first().zip(widths.first()).is_some_and(|(kept, shown)| (kept - shown).abs() < 0.5)
                && unchanged,
            json!({ "before": before, "after": after, "expected": expected, "saved": saved, "unchanged": unchanged }),
        );

        // Double-click the second table's first border: the column fits its long header.
        let second = self.evaluate(cx, &island, &table_facts(1)).await;
        let bounds = cx.update(|cx| island.read(cx).bounds(cx));
        if let Some(edge) = bounds.and_then(|bounds| to_window(bounds, &second["edge"])) {
            native::send_mouse(native, native::Mouse::Moved, edge)?;
            self.pause(cx, Duration::from_millis(100)).await;
            native::send_double_click(native, edge)?;
        }
        self.wait_for(cx, Duration::from_secs(3), |_, _| {
            !kept_at(&table_widths::page_widths(root, FILE), 1).is_empty()
        })
        .await;
        let fitted = self.evaluate(cx, &island, &table_facts(1)).await;
        let fitted_widths = px_widths(&fitted);
        let saved = kept(cx);
        steps.check(
            "a double-click on a Markdown table's column border fits the column to its content, kept for that table",
            fitted["sized"] == true
                && kept_at(&saved, 1).len() == 3
                && fitted_widths.first().is_some_and(|width| (width - fitted["width"].as_f64().unwrap_or_default()).abs() <= 1.)
                && kept_at(&saved, 1).first().is_some_and(|width| {
                    // The long header's width, or the frame's in a pane too
                    // narrow for it (as on CI's runner, its window smaller).
                    let frame = fitted["frame"].as_f64().unwrap_or_default();
                    *width > second["width"].as_f64().unwrap_or_default()
                        && (*width >= MIN_FIT || (*width - frame).abs() <= 1.)
                }),
            json!({ "before": second, "after": fitted, "saved": saved }),
        );

        // A drag whose release never reaches the page (the window can keep a
        // mouse-up from it): the next press, on the second table's border,
        // still drags that border, and the resize cursor is gone after it.
        let stale = self.evaluate(cx, &island, &table_facts(0)).await;
        if let Some(edge) = bounds.and_then(|bounds| to_window(bounds, &stale["edge"])) {
            native::send_mouse(native, native::Mouse::Down, edge)?;
            native::send_mouse(native, native::Mouse::Dragged, edge + gpui::point(gpui::px(10.), gpui::px(0.)))?;
            self.pause(cx, Duration::from_millis(100)).await;
        }
        let pressed = self.evaluate(cx, &island, &table_facts(1)).await;
        if let Some(edge) = bounds.and_then(|bounds| to_window(bounds, &pressed["edge"])) {
            native::send_mouse(native, native::Mouse::Down, edge)?;
            for step in 1..=6 {
                let dx = REPRESS_DRAG as f32 * step as f32 / 6.;
                native::send_mouse(native, native::Mouse::Dragged, edge + gpui::point(gpui::px(dx), gpui::px(0.)))?;
                self.pause(cx, Duration::from_millis(30)).await;
            }
            native::send_mouse(native, native::Mouse::Up, edge + gpui::point(gpui::px(REPRESS_DRAG as f32), gpui::px(0.)))?;
        }
        let from = pressed["width"].as_f64().unwrap_or_default();
        let wanted = |saved: &Value| kept_at(saved, 1).first().is_some_and(|width| (width - from - REPRESS_DRAG).abs() <= 2.);
        self.wait_for(cx, Duration::from_secs(3), |_, _| wanted(&table_widths::page_widths(root, FILE)))
            .await;
        let repressed = self.evaluate(cx, &island, &table_facts(1)).await;
        let saved = kept(cx);
        steps.check(
            "a press after a drag whose mouse-up never reached the page drags the border pressed, and leaves no resize cursor behind",
            wanted(&saved) && repressed["cursor"] == false,
            json!({ "stale": stale, "before": pressed, "after": repressed, "saved": saved }),
        );

        // Drag the first table's body row's bottom border down: the row is at
        // least that high, kept by its index; a double-click gives it back
        // its content's height.
        let row = self.evaluate(cx, &island, &row_facts(0)).await;
        let bounds = cx.update(|cx| island.read(cx).bounds(cx));
        let mut hover = Value::Null;
        if let Some(edge) = bounds.and_then(|bounds| to_window(bounds, &row["edge"])) {
            native::send_mouse(native, native::Mouse::Moved, edge)?;
            self.pause(cx, Duration::from_millis(100)).await;
            hover = self.evaluate(cx, &island, &row_facts(0)).await;
            // As for the column's border: the page is also given the move.
            if hover["cursor"] != true {
                let [x, y] = [&row["edge"][0], &row["edge"][1]].map(|v| v.as_f64().unwrap_or_default());
                hover = self
                    .evaluate(
                        cx,
                        &island,
                        &format!(
                            "(() => {{ const target = document.elementFromPoint({x}, {y}); \
                             target?.dispatchEvent(new MouseEvent('mousemove', {{ bubbles: true, clientX: {x}, clientY: {y} }})); \
                             return {}; }})()",
                            row_facts(0)
                        ),
                    )
                    .await;
            }
            native::send_mouse(native, native::Mouse::Down, edge)?;
            for step in 1..=6 {
                let dy = ROW_DRAG as f32 * step as f32 / 6.;
                native::send_mouse(native, native::Mouse::Dragged, edge + gpui::point(gpui::px(0.), gpui::px(dy)))?;
                self.pause(cx, Duration::from_millis(30)).await;
            }
            native::send_mouse(native, native::Mouse::Up, edge + gpui::point(gpui::px(0.), gpui::px(ROW_DRAG as f32)))?;
        }
        self.wait_for(cx, Duration::from_secs(3), |_, _| {
            !heights_at(&table_widths::page_widths(root, FILE), 0).is_empty()
        })
        .await;
        let raised = self.evaluate(cx, &island, &row_facts(0)).await;
        let saved = kept(cx);
        let expected = row["height"].as_f64().unwrap_or_default() + ROW_DRAG;
        let kept_rows = heights_at(&saved, 0);
        let unchanged = std::fs::read_to_string(&path).is_ok_and(|text| text == TABLES);
        steps.check(
            "a Markdown table row's bottom border dragged makes the row that much higher, kept by its index with the table's widths, and the file's bytes stay as they were",
            raised["height"].as_f64().is_some_and(|height| (height - expected).abs() <= 2.)
                && kept_rows.len() == 2
                && kept_rows[0].is_none()
                && kept_rows[1].is_some_and(|height| (height - expected).abs() <= 2.)
                && kept_at(&saved, 0).len() == 2
                && unchanged
                && hover["cursor"] == true,
            json!({ "before": row, "hover": hover, "after": raised, "expected": expected, "saved": saved, "unchanged": unchanged }),
        );
        if let Some(edge) = bounds.and_then(|bounds| to_window(bounds, &raised["edge"])) {
            native::send_mouse(native, native::Mouse::Moved, edge)?;
            self.pause(cx, Duration::from_millis(100)).await;
            native::send_double_click(native, edge)?;
        }
        self.wait_for(cx, Duration::from_secs(3), |_, _| {
            heights_at(&table_widths::page_widths(root, FILE), 0).is_empty()
        })
        .await;
        let reset = self.evaluate(cx, &island, &row_facts(0)).await;
        let saved = kept(cx);
        steps.check(
            "a double-click on a Markdown table row's bottom border gives the row its content's height back, the table keeping its widths",
            reset["set"] == "" && heights_at(&saved, 0).is_empty() && kept_at(&saved, 0).len() == 2,
            json!({ "after": reset, "saved": saved }),
        );

        // Closed and opened again: both tables come back at their widths.
        self.close_tab(cx, panel)?;
        self.pause(cx, Duration::from_millis(300)).await;
        let panel = self.open(cx, &path, None)?;
        let reopened = match self.table_island(cx, &panel).await {
            Some(island) => {
                let mut facts = (Value::Null, Value::Null);
                let started = std::time::Instant::now();
                while started.elapsed() < Duration::from_secs(5) {
                    facts = (
                        self.evaluate(cx, &island, &table_facts(0)).await,
                        self.evaluate(cx, &island, &table_facts(1)).await,
                    );
                    if facts.0["sized"] == true && facts.1["sized"] == true {
                        break;
                    }
                    self.pause(cx, Duration::from_millis(150)).await;
                }
                Some((island, facts))
            }
            None => None,
        };
        let Some((island, (first, second))) = reopened else {
            steps.check(
                "a Markdown file opened again draws its tables at the column widths kept",
                false,
                json!({ "drawn": false }),
            );
            return Ok(());
        };
        let saved = kept(cx);
        let same = |facts: &Value, table: u64| {
            let shown = px_widths(facts);
            let kept = kept_at(&saved, table);
            !kept.is_empty()
                && shown.len() == kept.len()
                && shown.iter().zip(&kept).all(|(shown, kept)| (shown - kept).abs() < 0.5)
        };
        steps.check(
            "a Markdown file closed and opened again draws its tables at the column widths kept",
            same(&first, 0) && same(&second, 1),
            json!({ "first": first, "second": second, "saved": saved }),
        );

        // A column added on disk: the first table sizes to its content, and
        // its widths come back once the column goes again.
        let before_change = kept(cx);
        std::fs::write(&path, WIDER)?;
        let mut first = Value::Null;
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            first = self.evaluate(cx, &island, &table_facts(0)).await;
            if first["cols"].as_array().map(Vec::len) == Some(3) {
                break;
            }
            self.pause(cx, Duration::from_millis(150)).await;
        }
        let second = self.evaluate(cx, &island, &table_facts(1)).await;
        let during = kept(cx);
        std::fs::write(&path, TABLES)?;
        let mut back = Value::Null;
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            back = self.evaluate(cx, &island, &table_facts(0)).await;
            if back["sized"] == true {
                break;
            }
            self.pause(cx, Duration::from_millis(150)).await;
        }
        let after_change = kept(cx);
        steps.check(
            "a Markdown table whose column count changed sizes to its content, the other table keeping its widths, and gets its own back when the column goes again; nothing saved is dropped",
            first["sized"] == false
                && first["cols"].as_array().map(Vec::len) == Some(3)
                && second["sized"] == true
                && px_widths(&back) == kept_at(&before_change, 0)
                && during == before_change
                && after_change == before_change,
            json!({ "first": first, "second": second, "back": back, "before": before_change, "during": during, "after": after_change }),
        );
        self.close_tab(cx, panel)?;
        Ok(())
    }

    /// The file's visual editor once drawn, with its tables.
    async fn table_island(
        &self,
        cx: &mut AsyncApp,
        panel: &Entity<EditorPanel>,
    ) -> Option<Entity<Island>> {
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
        })?;
        if !drawn {
            return None;
        }
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            let count = self
                .evaluate(cx, &island, "document.querySelectorAll('.lx-md-surface table').length")
                .await;
            if count == 2 {
                return Some(island);
            }
            self.pause(cx, Duration::from_millis(150)).await;
        }
        None
    }
}

/// Narrower than the long header, so a fit that measured it is wider.
const MIN_FIT: f64 = 150.;

/// This file's registration as the e2e module `markdown_table_widths` (`registry.rs`).
pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Repair,
    rank: 81,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-gpui/src/editor_panel/table_widths.rs",
        "src/editor/markdown/engine/table-column-widths.ts",
    ],
    run: |run, cx, steps, fixture| Box::pin(run.markdown_table_widths(cx, steps, &fixture.root)),
};
