//! Inline colours in LaTeX source (lattice#1328): the text a
//! `\textcolor{c}{…}` colours, and what a `\color{c}` declaration colours,
//! from the declaration to the end of its group (the `}` or `\end` that
//! closes the group it stands in, or the end of what was read). A colour is
//! one of xcolor's base names or `[HTML]{RRGGBB}`; any other colour (a model,
//! a mix such as `red!50`, a `\definecolor`) is the document's to say and is
//! left unpainted. Where colours nest, the innermost (the latest to start)
//! wins, as in TeX. Pure text, mirrored by the Visual page's
//! `src/editor/latex/latex-inline-colors.ts`. Offsets are bytes; every
//! pattern is ASCII.

use std::ops::Range;

use crate::decorations::{Mark, MarkStyle};

/// xcolor's base colours (xcolor.sty, `\definecolor` in the rgb model).
const XCOLOR_BASE: [(&str, u32); 19] = [
    ("red", 0xFF0000),
    ("green", 0x00FF00),
    ("blue", 0x0000FF),
    ("cyan", 0x00FFFF),
    ("magenta", 0xFF00FF),
    ("yellow", 0xFFFF00),
    ("black", 0x000000),
    ("white", 0xFFFFFF),
    ("gray", 0x808080),
    ("darkgray", 0x404040),
    ("lightgray", 0xBFBFBF),
    ("brown", 0xBF8040),
    ("lime", 0xBFFF00),
    ("olive", 0x808000),
    ("orange", 0xFF8000),
    ("pink", 0xFFBFBF),
    ("purple", 0xBF0040),
    ("teal", 0x008080),
    ("violet", 0x800080),
];

/// Text painted in one colour, `0xRRGGBB`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineColor {
    pub range: Range<usize>,
    pub rgb: u32,
}

/// The colour a `[model]{spec}` names, if this paints it.
fn resolve(model: &str, spec: &str) -> Option<u32> {
    let spec = spec.trim();
    if model.trim().eq_ignore_ascii_case("html") {
        return (spec.len() == 6 && spec.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| u32::from_str_radix(spec, 16).ok())
            .flatten();
    }
    if !model.trim().is_empty() {
        return None;
    }
    XCOLOR_BASE
        .iter()
        .find(|(name, _)| *name == spec)
        .map(|&(_, rgb)| rgb)
}

fn skip_comment(text: &[u8], index: usize) -> usize {
    text[index..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(text.len(), |at| index + at)
}

/// Past the blanks before an argument, one line break among them, as TeX
/// reads them.
fn skip_space(text: &[u8], mut index: usize) -> usize {
    let mut breaks = 0;
    while let Some(&byte) = text.get(index) {
        match byte {
            b' ' | b'\t' => {}
            b'\n' if breaks == 0 => breaks += 1,
            _ => break,
        }
        index += 1;
    }
    index
}

fn name_at(text: &[u8], index: usize) -> &[u8] {
    let rest = &text[index.min(text.len())..];
    let len = rest
        .iter()
        .position(|b| !(b.is_ascii_alphabetic() || *b == b'@'))
        .unwrap_or(rest.len());
    &rest[..len]
}

/// Just past the brace that closes the group opened at `open`.
fn group_end(text: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut index = open;
    while index < text.len() {
        match text[index] {
            b'\\' => index += 1,
            b'%' => index = skip_comment(text, index),
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Where the group a declaration at `from` stands in ends.
fn declaration_end(text: &[u8], from: usize) -> usize {
    let (mut depth, mut environments) = (0usize, 0usize);
    let mut index = from;
    while index < text.len() {
        match text[index] {
            b'%' => index = skip_comment(text, index),
            b'{' => depth += 1,
            b'}' => {
                if depth == 0 {
                    return index;
                }
                depth -= 1;
            }
            b'\\' => {
                let name = name_at(text, index + 1);
                if name == b"begin" {
                    environments += 1;
                } else if name == b"end" {
                    if environments == 0 && depth == 0 {
                        return index;
                    }
                    environments = environments.saturating_sub(1);
                }
                index += name.len().max(1);
            }
            _ => {}
        }
        index += 1;
    }
    text.len()
}

/// `[model]{spec}` from `index`: the colour, and just past it.
fn color_argument(text: &str, index: usize) -> Option<(Option<u32>, usize)> {
    let bytes = text.as_bytes();
    let mut at = skip_space(bytes, index);
    let mut model = "";
    if bytes.get(at) == Some(&b'[') {
        let close = at + bytes[at..].iter().position(|&b| b == b']')?;
        model = &text[at + 1..close];
        at = skip_space(bytes, close + 1);
    }
    if bytes.get(at) != Some(&b'{') {
        return None;
    }
    let end = group_end(bytes, at)?;
    Some((resolve(model, &text[at + 1..end - 1]), end))
}

/// Every painted colour in `text`, flattened so no two overlap.
pub fn inline_colors(text: &str) -> Vec<InlineColor> {
    let bytes = text.as_bytes();
    let mut spans: Vec<InlineColor> = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                index = skip_comment(bytes, index) + 1;
                continue;
            }
            b'\\' => {}
            _ => {
                index += 1;
                continue;
            }
        }
        let name = name_at(bytes, index + 1);
        let declaration = match name {
            b"color" => true,
            b"textcolor" => false,
            _ => {
                index += name.len().max(1) + 1;
                continue;
            }
        };
        let Some((rgb, end)) = color_argument(text, index + 1 + name.len()) else {
            index += name.len() + 1;
            continue;
        };
        if declaration {
            if let Some(rgb) = rgb {
                spans.push(InlineColor {
                    range: end..declaration_end(bytes, end),
                    rgb,
                });
            }
        } else {
            let open = skip_space(bytes, end);
            if bytes.get(open) == Some(&b'{')
                && let (Some(close), Some(rgb)) = (group_end(bytes, open), rgb)
            {
                spans.push(InlineColor {
                    range: open + 1..close - 1,
                    rgb,
                });
            }
        }
        index = end;
    }
    let mut cuts: Vec<usize> = spans
        .iter()
        .flat_map(|span| [span.range.start, span.range.end])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut flat: Vec<InlineColor> = Vec::new();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let Some(inner) = spans
            .iter()
            .filter(|span| span.range.start <= from && to <= span.range.end)
            .max_by_key(|span| span.range.start)
        else {
            continue;
        };
        match flat.last_mut() {
            Some(last) if last.range.end == from && last.rgb == inner.rgb => last.range.end = to,
            _ => flat.push(InlineColor {
                range: from..to,
                rgb: inner.rgb,
            }),
        }
    }
    flat
}

/// The most a colour's OKLab lightness may be in a light editor, and the
/// least in a dark one.
const LIGHT_MAX: f32 = 0.5;
const DARK_MIN: f32 = 0.74;

fn linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn gamma(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    }
}

/// `rgb` as the editor draws it so its text stays readable: in a light
/// editor its OKLab lightness is brought down to at most 0.50, in a dark one
/// up to at least 0.74, keeping its hue and chroma (clipped to sRGB). Every
/// base colour then reads at 4.5:1 or more, WCAG's AA for body text, on white
/// paper and on a #1E1E1E ground; a colour already that dark or light is
/// drawn as written. The Visual page draws it the same way
/// (`readableColor`).
pub fn readable(rgb: u32, dark: bool) -> u32 {
    let channel = |shift: u32| linear(((rgb >> shift) & 0xFF) as f32 / 255.);
    let (r, g, b) = (channel(16), channel(8), channel(0));
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    let lightness = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
    let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
    let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
    let target = if dark {
        lightness.max(DARK_MIN)
    } else {
        lightness.min(LIGHT_MAX)
    };
    if target == lightness {
        return rgb;
    }
    let l = (target + 0.396_337_78 * a + 0.215_803_76 * bb).powi(3);
    let m = (target - 0.105_561_346 * a - 0.063_854_17 * bb).powi(3);
    let s = (target - 0.089_484_18 * a - 1.291_485_5 * bb).powi(3);
    let channels = [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ];
    channels.iter().fold(0, |rgb, &c| {
        (rgb << 8) | (gamma(c.clamp(0., 1.)).clamp(0., 1.) * 255.).round() as u32
    })
}

/// The marks that paint `text`'s inline colours (`text` is the document
/// from `offset` on), each in its colour made readable on the editor's
/// ground. The bytes in `inked` are left out: a light editor's highlight
/// draws its text in the paper's ink, as the Visual page's `.cm-hl-paint`
/// does over a colour, so the source gives the same answer whichever layer
/// Zed would put on top.
pub fn paint(text: &str, offset: usize, dark: bool, inked: &[Range<usize>]) -> Vec<Mark> {
    let mut marks = Vec::new();
    for color in inline_colors(text) {
        let style = MarkStyle {
            color: Some(gpui::rgb(readable(color.rgb, dark)).into()),
            ..MarkStyle::default()
        };
        let mut from = color.range.start + offset;
        let to = color.range.end + offset;
        let mut cuts: Vec<&Range<usize>> = inked
            .iter()
            .filter(|ink| ink.start < to && from < ink.end)
            .collect();
        cuts.sort_by_key(|ink| ink.start);
        for ink in cuts {
            if from < ink.start {
                marks.push(Mark {
                    range: from..ink.start,
                    style,
                });
            }
            from = from.max(ink.end);
        }
        if from < to {
            marks.push(Mark {
                range: from..to,
                style,
            });
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(text: &str) -> Vec<(&str, u32)> {
        inline_colors(text)
            .into_iter()
            .map(|span| (&text[span.range], span.rgb))
            .collect()
    }

    fn contrast(a: u32, b: u32) -> f32 {
        let luminance = |rgb: u32| {
            let channel = |shift: u32| linear(((rgb >> shift) & 0xFF) as f32 / 255.);
            0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
        };
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn textcolor_and_a_declaration_to_the_end_of_its_group() {
        assert_eq!(
            spans(r"a \textcolor{red}{revised} b {\color{blue} new text} c"),
            vec![("revised", 0xFF0000), (" new text", 0x0000FF)]
        );
        assert_eq!(
            spans(r"\textcolor[HTML]{1a7f37}{ok}"),
            vec![("ok", 0x1A7F37)]
        );
        assert_eq!(
            spans("\\textcolor{red}\n  {next line}"),
            vec![("next line", 0xFF0000)]
        );
        let quote = r"\begin{quote}\color{red} one {two} \begin{itemize}\item x\end{itemize} y\end{quote} after";
        assert_eq!(
            spans(quote),
            vec![(
                r" one {two} \begin{itemize}\item x\end{itemize} y",
                0xFF0000
            )]
        );
    }

    #[test]
    fn the_innermost_colour_wins_and_a_top_level_declaration_runs_on() {
        assert_eq!(
            spans(r"\color{red} a \textcolor{blue}{b} c"),
            vec![
                (r" a \textcolor{blue}{", 0xFF0000),
                ("b", 0x0000FF),
                ("} c", 0xFF0000)
            ]
        );
    }

    #[test]
    fn unknown_colours_comments_and_other_commands_are_left_alone() {
        assert!(
            inline_colors(r"\textcolor{red!50}{a} \color[rgb]{1,0,0} b \textcolor{myaccent}{c}")
                .is_empty()
        );
        assert!(
            inline_colors(r"\colorbox{red}{a} \colorlet{x}{red} % \color{red} hidden").is_empty()
        );
        assert_eq!(
            spans("{\\color{red} a % }\n b} c"),
            vec![(" a % }\n b", 0xFF0000)]
        );
        assert_eq!(resolve("", "Red"), None);
        assert_eq!(
            spans(r"{\color{red} a {\end{x}} \begin{y}b\end{y} c} d"),
            vec![(r" a {\end{x}} \begin{y}b\end{y} c", 0xFF0000)]
        );
    }

    #[test]
    fn every_base_colour_reads_on_either_ground() {
        for (name, rgb) in XCOLOR_BASE {
            let light = readable(rgb, false);
            let dark = readable(rgb, true);
            assert!(
                contrast(light, 0xFFFFFF) >= 4.5,
                "{name} on white: {light:06X}"
            );
            assert!(
                contrast(dark, 0x1E1E1E) >= 4.5,
                "{name} on dark: {dark:06X}"
            );
        }
        assert_eq!(readable(0x0000FF, false), 0x0000FF);
        assert_eq!(readable(0xFFFF00, true), 0xFFFF00);
    }

    #[test]
    fn paint_marks_the_coloured_text_from_its_offset() {
        let text = r"x {\color{blue} revised}";
        let marks = paint(&text[1..], 1, true, &[]);
        assert_eq!(marks.len(), 1);
        assert_eq!(&text[marks[0].range.clone()], " revised");
        assert_eq!(
            marks[0].style.color,
            Some(gpui::rgb(readable(0x0000FF, true)).into())
        );
        // A light highlight's ink takes the bytes it covers.
        let revised = text.find("revised").unwrap();
        let inked = paint(&text[1..], 1, false, &[revised..revised + 3]);
        let ranges: Vec<&str> = inked.iter().map(|mark| &text[mark.range.clone()]).collect();
        assert_eq!(ranges, vec![" ", "ised"]);
    }
}
