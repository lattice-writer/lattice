//! Whether a display list can be drawn exactly: every glyph's font and
//! every image loaded, and nothing the live renderer cannot draw. The
//! layer falls back to the PDF's own pixels for what it cannot (`layer.rs`).

use super::{
    fonts::{FontStore, ImageStore, LoadedFont, LoadedImage, image_info},
    protocol::{DisplayList, Item, Line},
};

/// Why a display list cannot be drawn exactly (`None` when it can), over
/// `lines` (all of its lines when `None`) and the items outside them.
pub fn inexact_reason(
    dl: &DisplayList,
    fonts: &FontStore,
    images: &ImageStore,
    lines: Option<&mut dyn Iterator<Item = &Line>>,
) -> Option<String> {
    let flags = dl.flag_names();
    if !flags.is_empty() {
        return Some(format!(
            "contains material the live renderer cannot draw ({})",
            flags.join(", ")
        ));
    }
    let check = |items: &[Item]| -> Option<String> {
        for item in items {
            match item {
                Item::Glyph {
                    font, char, index, ..
                } => {
                    let Some(desc) = dl.fonts.get(&font.to_string()) else {
                        return Some("fonts are loading".into());
                    };
                    let Some(loaded) = fonts.get(desc) else {
                        return Some("fonts are loading".into());
                    };
                    if let LoadedFont::Missing(reason) = loaded.as_ref() {
                        return Some(reason.clone());
                    }
                    if !loaded.drawable(*index, *char) {
                        return Some(format!(
                            "a glyph is missing from {}",
                            desc.psname
                                .as_deref()
                                .or(desc.name.as_deref())
                                .unwrap_or("a font")
                        ));
                    }
                }
                Item::Image { index, .. } => {
                    let Some(info) = image_info(dl, *index) else {
                        return Some("an image has no source file".into());
                    };
                    match images.get(&info.file) {
                        None => {
                            return Some("images are loading".into());
                        }
                        Some(LoadedImage::Missing(reason)) => return Some(reason.clone()),
                        Some(LoadedImage::Bitmap(_)) => {}
                    }
                }
                Item::Literal | Item::Unknown { .. } => {
                    return Some(
                        "contains drawing commands (e.g. TikZ) the live renderer cannot draw"
                            .into(),
                    );
                }
                _ => {}
            }
        }
        None
    };
    if let Some(reason) = check(&dl.other) {
        return Some(reason);
    }
    match lines {
        Some(lines) => {
            for line in lines {
                if let Some(reason) = check(&line.items) {
                    return Some(reason);
                }
            }
            None
        }
        None => dl.lines.iter().find_map(|line| check(&line.items)),
    }
}

/// Whether `reason` only waits for a load (fonts or images): the next draw
/// may be exact.
pub fn only_loading(reason: &str) -> bool {
    reason.ends_with("are loading")
}
