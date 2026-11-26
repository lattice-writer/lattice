//! The highlights painted in a `.tex` source (`highlights::paint`), read
//! around the lines the editor shows and painted again as they change.

use std::ops::Range;

use gpui::{App, Context};
use lattice_editor::latex::highlights;
use lattice_ui::ActiveLattice as _;

use super::{EditorPanel, Kind};

/// The paint's layers: the tints, and the ink, underlines and faded
/// wrappers.
const PAINT: &str = "highlight-paint";
const PAINT_MARKS: &str = "highlight-paint-marks";
const PAINT_SWATCHES: &str = "highlight-paint-swatches";
/// How far past the lines the editor shows the paint reads (the Tauri
/// paint's `MARGIN`), so a long document costs what its view costs.
const PAINT_MARGIN: usize = 2000;

impl EditorPanel {
    /// The bytes of the lines the editor shows.
    fn shown_bytes(&self, cx: &mut Context<Self>) -> Range<usize> {
        use editor::{DisplayPoint, display_map::DisplayRow};
        self.editor.update(cx, |editor, cx| {
            let snapshot = editor.display_snapshot(cx);
            let top = editor.scroll_position(cx).y.max(0.);
            let rows = editor.visible_line_count().unwrap_or(200.);
            let offset_at = |row: f64| {
                let row = DisplayRow(row.floor() as u32).min(snapshot.max_point().row());
                let point = DisplayPoint::new(row, 0).to_point(&snapshot);
                snapshot.buffer_snapshot().point_to_offset(point).0
            };
            offset_at(top)..offset_at(top + rows + 1.)
        })
    }

    /// Paint the `.tex` file's highlights (`highlights::paint`) around the
    /// lines the editor shows.
    pub(in crate::editor_panel) fn paint_highlights(&mut self, cx: &mut Context<Self>) {
        if self.toolbar_kind() != Some(Kind::Latex) {
            return;
        }
        let dark = cx.palette().appearance == lattice_ui::Appearance::Dark;
        let shown = self.shown_bytes(cx);
        let buffer = self.buffer.read(cx);
        let start = buffer.clip_offset(
            shown.start.saturating_sub(PAINT_MARGIN),
            language::Bias::Left,
        );
        let end = buffer.clip_offset(
            (shown.end + PAINT_MARGIN).min(buffer.len()),
            language::Bias::Right,
        );
        let text = buffer.text_for_range(start..end).collect::<String>();
        let version = buffer.version();
        let breaks = self.box_breaks(cx);
        let paint = highlights::paint(&text, start, dark, breaks);
        self.toolbar.painted_dark = Some(dark);
        self.toolbar.painted = paint.washes.len();
        self.toolbar.painted_window = start..end;
        self.toolbar.painted_boxes = paint.boxes;
        self.toolbar.painted_version = Some(version);
        self.lattice.update(cx, |lattice, cx| {
            lattice.set_highlights(PAINT, paint.washes, cx);
            lattice.set_marks(PAINT_MARKS, paint.marks, cx);
            lattice.set_inline_texts(PAINT_SWATCHES, paint.swatches, cx);
        });
    }

    /// Paint the highlights again once the lines the editor shows (scrolled,
    /// resized or folded) go past what was read.
    pub(in crate::editor_panel) fn highlights_shown(&mut self, cx: &mut Context<Self>) {
        if self.toolbar_kind() != Some(Kind::Latex) {
            return;
        }
        let (shown, painted) = (self.shown_bytes(cx), &self.toolbar.painted_window);
        if shown.start < painted.start || shown.end > painted.end {
            self.paint_highlights(cx);
            return;
        }
        // A box the soft wrap now breaks, or no longer breaks, is painted
        // again (a resize, a font size). After an edit the boxes' offsets are
        // stale until the paint that typing pauses for.
        if self.toolbar.painted_boxes.is_empty()
            || self.toolbar.painted_version.as_ref() != Some(&self.buffer.read(cx).version())
        {
            return;
        }
        let breaks = self.box_breaks(cx);
        if self
            .toolbar
            .painted_boxes
            .iter()
            .any(|(range, broken)| breaks(range.clone()) != *broken)
        {
            self.paint_highlights(cx);
        }
    }

    /// Whether the start of a byte range and its end lie on different rows
    /// of the editor, as [`highlights::paint`] asks of a `\colorbox`.
    fn box_breaks(&self, cx: &mut Context<Self>) -> impl Fn(Range<usize>) -> bool + use<> {
        use editor::{MultiBufferOffset, display_map::ToDisplayPoint as _};
        let snapshot = self
            .editor
            .update(cx, |editor, cx| editor.display_snapshot(cx));
        move |range: Range<usize>| {
            let len = snapshot.buffer_snapshot().len().0;
            let row = |offset: usize| {
                MultiBufferOffset(offset.min(len))
                    .to_display_point(&snapshot)
                    .row()
            };
            row(range.start) != row(range.end)
        }
    }

    /// Paint the highlights again once typing pauses.
    pub(in crate::editor_panel) fn highlights_edited(&mut self, cx: &mut Context<Self>) {
        if self.toolbar_kind() != Some(Kind::Latex) {
            return;
        }
        self.toolbar._paint = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(150))
                .await;
            this.update(cx, |this, cx| this.paint_highlights(cx)).ok();
        }));
    }

    /// How many highlights are painted in the source, for scripted checks.
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    pub fn highlights_painted(&self, _: &App) -> usize {
        self.toolbar.painted
    }
}
