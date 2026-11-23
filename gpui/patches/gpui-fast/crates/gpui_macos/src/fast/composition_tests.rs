use super::{CursorLayer, cursor_holes, subtract_rect, whole_scene_layer};
use gpui::{
    AppContext as _, Bounds, ComposedSceneLayer, CompositionSurfaceId, ContentMask, Context,
    DevicePixels, IntoElement, ParentElement as _, PlatformHeadlessRenderer as _, Quad, Render,
    ScaledPixels, Scene, Styled as _, TestAppContext, Window, div, point, px, size,
};

use crate::metal_renderer::MetalHeadlessRenderer;

#[gpui::test]
fn whole_scene_with_empty_layers_matches_balanced_replay(cx: &mut TestAppContext) {
    struct View;
    impl Render for View {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size(px(64.)).bg(gpui::white()).child(
                div()
                    .size(px(64.))
                    .rounded(px(24.))
                    .overflow_hidden()
                    .child(div().size(px(64.)).bg(gpui::black())),
            )
        }
    }

    let window = cx.update(|cx| {
        cx.open_window(Default::default(), |_, cx| cx.new(|_| View))
            .unwrap()
    });
    let mut renderer = MetalHeadlessRenderer::new().unwrap();
    window
        .update(cx, |_, window, cx| {
            window.draw(cx).clear(cx);
            let scene = window.painted_scene();
            assert!(
                !scene.layers.frames.is_empty(),
                "rounded clips were compiled"
            );
            let full = ComposedSceneLayer {
                surface: CompositionSurfaceId::default(),
                ranges: vec![0..scene.len()],
            };
            let empty = ComposedSceneLayer {
                surface: CompositionSurfaceId::default(),
                ranges: vec![],
            };

            // The existing layer_scene path replays the original operations,
            // including rounded clips, and finishes the new surface scene.
            let mut replayed = Scene::default();
            replayed
                .replay_balanced(full.ranges[0].clone(), scene)
                .unwrap();
            replayed.finish();

            let side = (64. * window.scale_factor()) as i32;
            let extent = size(DevicePixels(side), DevicePixels(side));
            let expected = renderer.render_scene_to_image(&replayed, extent).unwrap();
            for (layers, expected_target) in [
                (vec![full.clone(), empty.clone()], 0),
                (vec![empty.clone(), full.clone()], 1),
                (vec![full.clone()], 0),
                (vec![empty.clone(), full, empty], 1),
            ] {
                let (target_index, borrowed) = whole_scene_layer(scene, &layers).unwrap();
                assert_eq!(target_index, expected_target);
                assert!(std::ptr::eq(borrowed, scene));
                let actual = renderer.render_scene_to_image(borrowed, extent).unwrap();
                assert_eq!(actual.as_raw(), expected.as_raw());
                assert_eq!(actual.get_pixel(1, 1).0, [255, 255, 255, 255]);
                assert_eq!(
                    actual.get_pixel(side as u32 / 2, side as u32 / 2).0,
                    [0, 0, 0, 255]
                );
            }
        })
        .unwrap();
}

#[test]
fn partial_or_split_layers_require_replay_even_when_primitives_match() {
    let mut scene = Scene::default();
    let bounds = Bounds::new(
        point(ScaledPixels(0.), ScaledPixels(0.)),
        size(ScaledPixels(64.), ScaledPixels(64.)),
    );
    scene.push_layer(bounds);
    scene.insert_primitive(Quad {
        bounds,
        content_mask: ContentMask { bounds },
        ..Default::default()
    });
    scene.pop_layer();
    scene.finish();
    let len = scene.len();
    let layer = |ranges| ComposedSceneLayer {
        surface: CompositionSurfaceId::default(),
        ranges,
    };
    // The inner range has the same sole primitive, but must restore its layer.
    for layers in [
        vec![layer(vec![1..len - 1])],
        vec![layer(vec![0..len - 1])],
        vec![layer(vec![1..len])],
        vec![layer(vec![0..1, 1..len])],
        vec![layer(vec![0..len]), layer(vec![0..len])],
        vec![layer(vec![0..len]), layer(vec![1..len - 1])],
        vec![layer(vec![0..len]), layer(vec![0..0])],
        vec![layer(vec![])],
        vec![layer(vec![]), layer(vec![])],
        vec![layer(vec![0..len + 1])],
        vec![],
    ] {
        assert!(whole_scene_layer(&scene, &layers).is_none(), "{layers:?}");
    }

    let mut empty = Scene::default();
    empty.finish();
    assert!(whole_scene_layer(&empty, &[layer(vec![])]).is_none());
    for (layers, expected_target) in [
        (vec![layer(vec![0..0])], 0),
        (vec![layer(vec![]), layer(vec![0..0]), layer(vec![])], 1),
    ] {
        let (target_index, borrowed) = whole_scene_layer(&empty, &layers).unwrap();
        assert_eq!(target_index, expected_target);
        assert!(std::ptr::eq(borrowed, &empty));
    }
    assert!(whole_scene_layer(&empty, &[layer(vec![0..0]), layer(vec![0..0])]).is_none());
}

#[test]
fn cursor_rects_leave_out_a_native_view() {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let rect = |x: f64, y: f64, w: f64, h: f64| NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    let area = |rects: &[NSRect]| {
        rects
            .iter()
            .map(|r| r.size.width * r.size.height)
            .sum::<f64>()
    };

    let window = rect(0., 0., 100., 80.);
    let inside = subtract_rect(window, rect(20., 10., 30., 40.));
    assert_eq!(inside.len(), 4);
    assert_eq!(area(&inside), 100. * 80. - 30. * 40.);

    // A view at the window's edge leaves the rest whole; one outside it
    // leaves the window as it is.
    let edge = subtract_rect(window, rect(60., 0., 40., 80.));
    assert_eq!(edge.len(), 1);
    assert_eq!(area(&edge), 60. * 80.);
    let outside = subtract_rect(window, rect(200., 0., 10., 10.));
    assert_eq!(outside.len(), 1);
    assert_eq!(outside[0].size, window.size);
}

#[test]
fn cursor_holes_keep_gpui_overlays_above_a_native_view() {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let rect = |x: f64, y: f64, w: f64, h: f64| NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    let area = |rects: &[NSRect]| {
        rects
            .iter()
            .map(|r| r.size.width * r.size.height)
            .sum::<f64>()
    };
    let page = rect(0., 0., 200., 100.);
    let menu = rect(50., 20., 40., 30.);

    // A menu over the page keeps GPUI's cursor where it takes the pointer.
    let over = cursor_holes(&[
        CursorLayer::Gpui(vec![rect(0., 0., 400., 400.)]),
        CursorLayer::Native(Some(page)),
        CursorLayer::Gpui(vec![menu]),
    ]);
    assert_eq!(area(&over), 200. * 100. - 40. * 30.);
    assert!(over.iter().all(|hole| subtract_rect(*hole, menu).len() == 1
        && subtract_rect(*hole, menu)[0].size == hole.size));

    // The base surface under the page, a hidden view and a view forwarding
    // its input to GPUI leave no hole.
    let under = cursor_holes(&[
        CursorLayer::Gpui(vec![rect(0., 0., 400., 400.)]),
        CursorLayer::Native(Some(page)),
        CursorLayer::Native(None),
    ]);
    assert_eq!(under.len(), 1);
    assert_eq!(area(&under), 200. * 100.);
}
