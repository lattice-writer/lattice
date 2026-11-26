//! Source ↔ PDF jumps for Typst, which writes no SyncTeX map: typst-ide's
//! `jump_from_click` and `jump_from_cursor` read the compiled document's
//! spans instead.
//!
//! The jumps look in the last document whose PDF was written, with the
//! sources it was compiled from ([`Kept`]), never the session's current
//! ones: a compile that failed since has re-parsed them, and the spans of
//! the document on screen no longer point into them. The text on screen may
//! be newer than both (typing outruns the live compile), so a place found
//! in the compiled text is carried onto the current text, and back, across
//! the one span of text the two differ in ([`carry`]).

use std::{
    collections::HashMap,
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use lattice_core::{
    latex::PdfSyncTarget,
    typst_wire::{Jump, PdfJump, SourceJump, SourceSpot},
};
use typst::{
    Library, World,
    diag::{FileError, FileResult},
    foundations::{Bytes, Datetime},
    introspection::PagedPosition,
    layout::{Abs, Frame, FrameItem, Point},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook, TextItem},
    utils::LazyHash,
};
use typst_ide::IdeWorld;
use typst_layout::PagedDocument;

use crate::world::{FONTS, LIBRARY, project_path};

/// The last document whose PDF was written, and every source it was
/// compiled from, as a `World` the jumps read spans through.
pub(crate) struct Kept {
    pub(crate) document: PagedDocument,
    pub(crate) sources: HashMap<FileId, Source>,
    pub(crate) main: FileId,
}

impl World for Kept {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        FONTS.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.sources.get(&id).cloned().ok_or_else(|| not_kept(id))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(not_kept(id))
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.font(index)
    }

    fn today(&self, _: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}

impl IdeWorld for Kept {
    fn upcast(&self) -> &dyn World {
        self
    }
}

fn not_kept(id: FileId) -> FileError {
    FileError::NotFound(PathBuf::from(id.vpath().get_without_slash()))
}

/// The text of `path` (project-relative) as the writer sees it now: the
/// editor's buffer, else the file.
fn current_text(root: &Path, overlay: &HashMap<String, String>, path: &str) -> Option<String> {
    overlay
        .get(path)
        .cloned()
        .or_else(|| std::fs::read_to_string(root.join(path)).ok())
}

/// From a click at (`x`, `y`) points on the 1-based `page` to the source
/// that put it there, in the text on screen now. A click that hits nothing
/// (a margin, between lines) takes the text nearest to it, as SyncTeX takes
/// the nearest line.
pub(crate) fn source_at(
    kept: &Kept,
    root: &Path,
    overlay: &HashMap<String, String>,
    page: u32,
    x: f64,
    y: f64,
) -> SourceJump {
    let Some(page) = NonZeroUsize::new(page as usize) else {
        return Jump::Nowhere;
    };
    let Some(frame) = kept
        .document
        .pages()
        .get(page.get() - 1)
        .map(|page| &page.frame)
    else {
        return Jump::Nowhere;
    };
    let jump = |point| match typst_ide::jump_from_click(
        kept,
        &kept.document,
        &PagedPosition { page, point },
    ) {
        Some(typst_ide::Jump::File(id, offset)) => Some((id, offset)),
        _ => None,
    };
    let click = Point::new(Abs::pt(x), Abs::pt(y));
    // A link's destination is the PDF pane's to follow; its source is the
    // text under it, which the nearest text finds as for a margin.
    let found = jump(click).or_else(|| nearest_text(frame, click).and_then(jump));
    let Some((id, offset)) = found else {
        return Jump::Nowhere;
    };
    let (Some(path), Some(compiled)) = (project_path(id), kept.sources.get(&id)) else {
        return Jump::Nowhere;
    };
    let current = current_text(root, overlay, &path).unwrap_or_else(|| compiled.text().into());
    let (line, column) = line_column(&current, carry(compiled.text(), &current, offset));
    Jump::To(SourceSpot { path, line, column })
}

/// From the caret at the 1-based `line` and `column` of `path` in the text
/// on screen now to every place the text there is laid out, first page
/// first, each as the line of text it sits on.
pub(crate) fn places_of(
    kept: &Kept,
    root: &Path,
    overlay: &HashMap<String, String>,
    path: &str,
    line: u32,
    column: u32,
) -> PdfJump {
    let Ok(vpath) = VirtualPath::new(path) else {
        return Jump::Nowhere;
    };
    let id = RootedPath::new(VirtualRoot::Project, vpath).intern();
    let Some(compiled) = kept.sources.get(&id) else {
        return Jump::Nowhere;
    };
    let current = current_text(root, overlay, path).unwrap_or_else(|| compiled.text().into());
    let cursor = carry(&current, compiled.text(), offset_of(&current, line, column));
    let places: Vec<PdfSyncTarget> = typst_ide::jump_from_cursor(&kept.document, compiled, cursor)
        .into_iter()
        .filter_map(|position| {
            let frame = &kept.document.pages().get(position.page.get() - 1)?.frame;
            let (x, y) = (position.point.x.to_pt(), position.point.y.to_pt());
            let (left, top, right, bottom) =
                line_box(frame, position.point).unwrap_or((x, y - 10.0, x + 20.0, y + 2.5));
            Some(PdfSyncTarget {
                page: u32::try_from(position.page.get()).ok()?,
                x: left,
                y: top.max(0.0),
                width: right - left,
                height: bottom - top.max(0.0),
            })
        })
        .collect();
    if places.is_empty() {
        Jump::Nowhere
    } else {
        Jump::To(places)
    }
}

/// A rectangle as its left, top, right and bottom edges, in points.
type Edges = (f64, f64, f64, f64);

/// A text run's rectangle: from its size above its baseline to a quarter
/// of it below, where descenders end.
fn run_edges(at: Point, text: &TextItem) -> Edges {
    let size = text.size.to_pt();
    let (x, baseline) = (at.x.to_pt(), at.y.to_pt());
    (
        x,
        baseline - size,
        x + text.width().to_pt(),
        baseline + size / 4.0,
    )
}

/// `edges` in a frame placed at `at` with `transform`, in the outer frame.
fn placed(edges: Edges, at: Point, transform: typst::layout::Transform) -> Edges {
    let corner = |x: f64, y: f64| at + Point::new(Abs::pt(x), Abs::pt(y)).transform(transform);
    let (a, b) = (corner(edges.0, edges.1), corner(edges.2, edges.3));
    (
        a.x.to_pt().min(b.x.to_pt()),
        a.y.to_pt().min(b.y.to_pt()),
        a.x.to_pt().max(b.x.to_pt()),
        a.y.to_pt().max(b.y.to_pt()),
    )
}

fn same_baseline(a: Point, b: Point) -> bool {
    (a.y - b.y).to_pt().abs() < 0.01
}

/// The rectangle, on the page, of `frame`'s line of text through `point`
/// (where `jump_from_cursor` puts a glyph: on its baseline): every run in
/// the same frame on that baseline, as SyncTeX marks a line's box.
fn line_box(frame: &Frame, point: Point) -> Option<Edges> {
    for (at, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                let Some(inverse) = group.transform.invert() else {
                    continue;
                };
                if let Some(edges) = line_box(&group.frame, (point - *at).transform(inverse)) {
                    return Some(placed(edges, *at, group.transform));
                }
            }
            FrameItem::Text(text)
                if same_baseline(*at, point)
                    && point.x >= at.x - Abs::pt(0.01)
                    && point.x <= at.x + text.width() + Abs::pt(0.01) =>
            {
                return frame
                    .items()
                    .filter_map(|(at, item)| match item {
                        FrameItem::Text(text) if same_baseline(*at, point) => {
                            Some(run_edges(*at, text))
                        }
                        _ => None,
                    })
                    .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)));
            }
            _ => {}
        }
    }
    None
}

/// A point inside the text run nearest to `click` on the page, or `None`
/// on a page without text.
fn nearest_text(frame: &Frame, click: Point) -> Option<Point> {
    /// Each run's rectangle on the page.
    fn runs(frame: &Frame, place: &dyn Fn(Edges) -> Edges, out: &mut Vec<Edges>) {
        for (at, item) in frame.items() {
            match item {
                FrameItem::Group(group) => {
                    let (at, transform) = (*at, group.transform);
                    runs(
                        &group.frame,
                        &|edges| place(placed(edges, at, transform)),
                        out,
                    );
                }
                FrameItem::Text(text) if !text.glyphs.is_empty() => {
                    out.push(place(run_edges(*at, text)));
                }
                _ => {}
            }
        }
    }
    let mut all = Vec::new();
    runs(frame, &|edges| edges, &mut all);
    let (x, y) = (click.x.to_pt(), click.y.to_pt());
    let (left, top, right, bottom) = all.into_iter().min_by(|a, b| {
        let distance = |(left, top, right, bottom): &Edges| {
            (x.clamp(*left, *right) - x).hypot(y.clamp(*top, *bottom) - y)
        };
        distance(a).total_cmp(&distance(b))
    })?;
    // Inside the run: off its ends, and halfway up it.
    let margin = ((right - left) / 2.0).min(0.25);
    Some(Point::new(
        Abs::pt(x.clamp(left + margin, right - margin)),
        Abs::pt((top + bottom) / 2.0),
    ))
}

/// `offset` (bytes) in `from`, carried to the same place in `to`: the two
/// are taken to differ in one span (the edits since the compile), so a place
/// after it (or where text was inserted) keeps its distance from the end, one
/// before it keeps its offset, and one inside it goes to where the span
/// starts.
///
/// Line endings are compared as `\n`: a file compiled from disk with CRLF
/// and then opened reads with LF in the editor, which is no edit at all.
pub(crate) fn carry(from: &str, to: &str, offset: usize) -> usize {
    let mut offset = offset.min(from.len());
    while !from.is_char_boundary(offset) {
        offset -= 1;
    }
    let (from_lf, _) = without_cr(from);
    let (to_lf, to_original) = without_cr(to);
    let removed = from[..offset].matches("\r\n").count()
        + usize::from(from[offset..].starts_with('\n') && from[..offset].ends_with('\r'));
    to_original[carry_lf(&from_lf, &to_lf, offset - removed)]
}

/// `text` with the `\r` of each `\r\n` taken out, and for each byte offset
/// in that (its length included), the offset in `text`.
fn without_cr(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut original = Vec::with_capacity(text.len() + 1);
    for (at, character) in text.char_indices() {
        if character == '\r' && text[at + 1..].starts_with('\n') {
            continue;
        }
        original.extend((0..character.len_utf8()).map(|byte| at + byte));
        out.push(character);
    }
    original.push(text.len());
    (out, original)
}

/// [`carry`] between texts with the same line endings.
fn carry_lf(from: &str, to: &str, offset: usize) -> usize {
    let mut offset = offset.min(from.len());
    while !from.is_char_boundary(offset) {
        offset -= 1;
    }
    let prefix = from
        .char_indices()
        .zip(to.chars())
        .find(|((_, a), b)| a != b)
        .map_or(from.len().min(to.len()), |((at, _), _)| at);
    // The common end, not reaching back into the common start.
    let room = from.len().min(to.len()) - prefix;
    let mut suffix = 0;
    for (a, b) in from.chars().rev().zip(to.chars().rev()) {
        if a != b || suffix + a.len_utf8() > room {
            break;
        }
        suffix += a.len_utf8();
    }
    // Text inserted at the place pushes it on: the end is checked first.
    if offset >= from.len() - suffix {
        to.len() - (from.len() - offset)
    } else if offset <= prefix {
        offset
    } else {
        prefix
    }
}

/// The byte offset of the 1-based `line` and `column` (in characters) of
/// `text`, held to the line's end and the text's end.
pub(crate) fn offset_of(text: &str, line: u32, column: u32) -> usize {
    let start = match line.saturating_sub(1) as usize {
        0 => 0,
        before => text
            .match_indices('\n')
            .nth(before - 1)
            .map_or(text.len(), |(at, _)| at + 1),
    };
    let rest = &text[start..];
    let rest = &rest[..rest.find('\n').unwrap_or(rest.len())];
    start
        + rest
            .char_indices()
            .nth(column.saturating_sub(1) as usize)
            .map_or(rest.len(), |(at, _)| at)
}

/// The 1-based line and column (in characters) at byte `offset` of `text`.
pub(crate) fn line_column(text: &str, offset: usize) -> (u32, u32) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    (
        u32::try_from(before.matches('\n').count() + 1).unwrap_or(u32::MAX),
        u32::try_from(text[line_start..offset].chars().count() + 1).unwrap_or(u32::MAX),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_carried_across_the_one_span_the_texts_differ_in() {
        let old = "alpha\nbeta gamma\ndelta\n";
        let new = "alpha\nNEW\nbeta gamma\ndelta\n";
        let beta = old.find("beta").unwrap();
        assert_eq!(carry(old, new, beta), new.find("beta").unwrap());
        assert_eq!(carry(new, old, new.find("beta").unwrap()), beta);
        // Before the edit, the place keeps its offset.
        assert_eq!(carry(old, new, 2), 2);
        // Inside text that is gone, the place goes to where it was.
        let gone = "alpha\ndelta\n";
        assert_eq!(carry(old, gone, beta + 3), gone.find("delta").unwrap());
        // Identical texts and the ends.
        assert_eq!(carry(old, old, 7), 7);
        assert_eq!(carry(old, new, old.len()), new.len());
        // A repeated character at the edit is not counted twice.
        assert_eq!(carry("aa", "aaa", 2), 3);
        // Offsets inside a character snap to its start.
        assert_eq!(carry("é x", "é x", 1), 0);
    }

    #[test]
    fn crlf_on_disk_and_lf_in_the_editor_are_the_same_text() {
        let disk = "= One\r\nThe first line.\r\nThe second line.\r\n";
        let editor = "= One\nThe first line.\nThe second line.\n";
        let second = disk.find("second").unwrap();
        let carried = carry(disk, editor, second);
        assert_eq!(carried, editor.find("second").unwrap());
        assert_eq!(line_column(editor, carried), (3, 5));
        assert_eq!(carry(editor, disk, carried), second);
        // With a line typed above, too.
        let typed = format!("New.\n{editor}");
        assert_eq!(line_column(&typed, carry(disk, &typed, second)), (4, 5));
        // The end of a line, between its \r and \n.
        let end = disk.find("\r\nThe second").unwrap();
        assert_eq!(
            carry(disk, editor, end + 1),
            editor.find("\nThe second").unwrap()
        );
        assert_eq!(carry(disk, disk, end + 1), end + 1);
    }

    #[test]
    fn lines_and_columns_count_characters_from_one() {
        let text = "first\nsé cond\n\nlast";
        assert_eq!(line_column(text, 0), (1, 1));
        let cond = text.find("cond").unwrap();
        assert_eq!(line_column(text, cond), (2, 4));
        assert_eq!(offset_of(text, 2, 4), cond);
        assert_eq!(offset_of(text, 3, 9), text.find("\n\n").unwrap() + 1);
        assert_eq!(offset_of(text, 9, 1), text.len());
        assert_eq!(offset_of(text, 1, 99), 5);
        for offset in [0, 3, 6, cond, text.len()] {
            let (line, column) = line_column(text, offset);
            assert_eq!(offset_of(text, line, column), offset);
        }
    }
}
