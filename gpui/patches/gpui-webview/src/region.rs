//! The region a WebView2's child window keeps (`win32.rs`), as plain
//! arithmetic so every platform's tests check it: the child window's size,
//! and the rectangles cut out of it, in its device pixels.

use gpui::{Bounds, Pixels};

/// A region: the child window's width and height, and the rectangles
/// (left, top, right, bottom) cut out of it.
pub(crate) type Region = ([i32; 2], Vec<[i32; 4]>);

/// The region for a page placed at `document` of which its element shows
/// `visible`, under `overlays`, all in window space, at `scale` device
/// pixels to a GPUI pixel; none when nothing is cut away, so the window
/// keeps no region at all.
///
/// An overlay's hole rounds outward, so nothing of the page shows at its
/// edge; the bands outside the element's clip round inward, the same way,
/// so the page shows nowhere past the clip.
pub(crate) fn region(
    document: Bounds<Pixels>,
    visible: Bounds<Pixels>,
    overlays: &[Bounds<Pixels>],
    scale: f32,
) -> Option<Region> {
    let local = |value: Pixels, origin: Pixels| (f32::from(value) - f32::from(origin)) * scale;
    let outward = |bounds: Bounds<Pixels>| {
        [
            local(bounds.left(), document.left()).floor() as i32,
            local(bounds.top(), document.top()).floor() as i32,
            local(bounds.right(), document.left()).ceil() as i32,
            local(bounds.bottom(), document.top()).ceil() as i32,
        ]
    };
    let size = [
        (f32::from(document.size.width) * scale).round() as i32,
        (f32::from(document.size.height) * scale).round() as i32,
    ];
    let [width, height] = size;
    let mut cut = Vec::new();
    if visible != document {
        let shown_left = local(visible.left(), document.left()).ceil() as i32;
        let shown_top = local(visible.top(), document.top()).ceil() as i32;
        let shown_right = local(visible.right(), document.left()).floor() as i32;
        let shown_bottom = local(visible.bottom(), document.top()).floor() as i32;
        cut.extend([
            [0, 0, width, shown_top],
            [0, shown_bottom, width, height],
            [0, shown_top, shown_left, shown_bottom],
            [shown_right, shown_top, width, shown_bottom],
        ]);
    }
    cut.extend(
        overlays
            .iter()
            .map(|overlay| overlay.intersect(&visible))
            .filter(|overlay| !overlay.is_empty())
            .map(outward),
    );
    cut.retain(|[left, top, right, bottom]| left < right && top < bottom);
    (!cut.is_empty()).then_some((size, cut))
}

#[cfg(test)]
mod tests {
    use super::region;
    use gpui::{Bounds, point, px, size};

    fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<gpui::Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    #[test]
    fn a_page_with_nothing_over_it_keeps_no_region() {
        let page = bounds(100., 50., 400., 300.);
        assert_eq!(region(page, page, &[], 2.), None);
        // An overlay elsewhere in the window cuts nothing.
        assert_eq!(region(page, page, &[bounds(0., 0., 90., 40.)], 2.), None);
    }

    #[test]
    fn an_overlay_cuts_its_hole_rounded_outward_in_the_pages_device_pixels() {
        let page = bounds(100., 50., 400., 300.);
        let menu = bounds(150.3, 60.6, 100., 50.);
        assert_eq!(
            region(page, page, &[menu], 1.5),
            Some(([600, 450], vec![[75, 15, 226, 91]]))
        );
        // Only the part over the page.
        let edge = bounds(450., 300., 100., 100.);
        assert_eq!(
            region(page, page, &[edge], 1.),
            Some(([400, 300], vec![[350, 250, 400, 300]]))
        );
    }

    #[test]
    fn the_bands_outside_the_elements_clip_round_inward() {
        let page = bounds(0., 0., 100., 100.);
        let shown = bounds(10.4, 20.4, 50., 50.);
        assert_eq!(
            region(page, shown, &[], 1.),
            Some((
                [100, 100],
                vec![
                    [0, 0, 100, 21],
                    [0, 70, 100, 100],
                    [0, 21, 11, 70],
                    [60, 21, 100, 70],
                ]
            ))
        );
    }

    #[test]
    fn a_resized_page_under_the_same_holes_needs_a_new_region() {
        let menu = bounds(20., 20., 40., 40.);
        let before = region(
            bounds(0., 0., 200., 200.),
            bounds(0., 0., 200., 200.),
            &[menu],
            2.,
        );
        let after = region(
            bounds(0., 0., 300., 260.),
            bounds(0., 0., 300., 260.),
            &[menu],
            2.,
        );
        assert_eq!(
            before.as_ref().map(|(_, cut)| cut),
            after.as_ref().map(|(_, cut)| cut)
        );
        assert_ne!(before, after);
        assert_eq!(after.map(|(size, _)| size), Some([600, 520]));
    }
}
