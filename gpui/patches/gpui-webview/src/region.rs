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
/// Everything is measured in the window's device pixels and then made
/// relative to the child window's origin as `device_rect` places it, at
/// `round(document.origin × scale)`, so a fractional zoom shifts nothing.
/// An overlay's hole rounds outward, so nothing of the page shows at its
/// edge; the shown area rounds inward, so the page shows nowhere past the
/// clip.
pub(crate) fn region(
    document: Bounds<Pixels>,
    visible: Bounds<Pixels>,
    overlays: &[Bounds<Pixels>],
    scale: f32,
) -> Option<Region> {
    let device = |value: Pixels| f32::from(value) * scale;
    let [origin_x, origin_y] = [
        device(document.left()).round(),
        device(document.top()).round(),
    ];
    let x = |value: Pixels| device(value) - origin_x;
    let y = |value: Pixels| device(value) - origin_y;
    let outward = |bounds: Bounds<Pixels>| {
        [
            x(bounds.left()).floor() as i32,
            y(bounds.top()).floor() as i32,
            x(bounds.right()).ceil() as i32,
            y(bounds.bottom()).ceil() as i32,
        ]
    };
    let size = [
        (device(document.right()).round() - origin_x) as i32,
        (device(document.bottom()).round() - origin_y) as i32,
    ];
    let [width, height] = size;
    let mut cut = Vec::new();
    if visible != document {
        let shown_left = x(visible.left()).ceil() as i32;
        let shown_top = y(visible.top()).ceil() as i32;
        let shown_right = x(visible.right()).floor() as i32;
        let shown_bottom = y(visible.bottom()).floor() as i32;
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
    fn at_a_fractional_zoom_holes_are_measured_from_the_rounded_origin() {
        // At 1.25 the page's origin falls at 125.625 device pixels and the
        // child window is placed at 126, so holes are measured from 126.
        // Measured from 125.625, the menu's left edge would land at 62.
        let page = bounds(100.5, 0., 200.3, 100.);
        let menu = bounds(150.3, 10., 50., 20.);
        // 150.3 × 1.25 = 187.875, less 126 is 61.875: floor 61. Its right,
        // 200.3 × 1.25 = 250.375, less 126 is 124.375: ceil 125.
        assert_eq!(
            region(page, page, &[menu], 1.25),
            Some(([250, 125], vec![[61, 12, 125, 38]]))
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
