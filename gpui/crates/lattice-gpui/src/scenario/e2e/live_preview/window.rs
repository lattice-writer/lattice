//! Typing with the PDF in its own window: the live layer paints there too,
//! and zoomed past the page cap it gets its full-size part (#1177, #1285).

use std::{path::Path, time::Duration};

use anyhow::Result;
use gpui::{App, AsyncApp, Entity};
use serde_json::json;

use super::{
    super::{super::Run, Steps},
    layer_state, live_detail_sizes,
};
use crate::editor_panel::EditorPanel;

impl Run {
    /// Run right after the save's quiet build has taken every earlier edit
    /// over, with `chapter` open in `editor`.
    pub(super) async fn live_preview_in_window(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        chapter: &Path,
        editor: &Entity<EditorPanel>,
    ) -> Result<()> {
        let pdf = cx.update(|cx| self.shell.read(cx).pdf().clone());
        let preview = cx.update(|cx| self.shell.read(cx).live_preview.clone());
        let view = cx.update(|cx| pdf.read(cx).live_view().clone());
        // Typing goes on painting in the separate window, past the page cap
        // too, where the layer is drawn at full size over the part on
        // screen (#1177: typing stopped showing there). The quiet build has
        // just taken every earlier edit over, so the layer starts empty and
        // anything painted on it is this typing's.
        self.with_window(cx, |_, window, cx| {
            pdf.update(cx, |pdf, cx| pdf.detach(window, cx));
        })?;
        let detached = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                shell.read(cx).pdf().read(cx).in_window()
            })
            .await;
        let window_view = cx.update(|cx| pdf.read(cx).live_view().clone());
        self.open(cx, chapter, Some(2))?;
        self.pause(cx, Duration::from_millis(300)).await;
        let quiet_before = cx.update(|cx| self.shell.read(cx).build_panel().read(cx).quiet_runs());
        // With the document's PDF built from the edits so far, rtex lays a
        // typed paragraph out in the background (`BackgroundScheduled`, then
        // a `LayoutUpdate`) rather than as a `ParagraphUpdate`.
        let updates = |cx: &App| {
            let preview = preview.read(cx);
            preview.received("ParagraphUpdate") + preview.received("LayoutUpdate")
        };
        let before = cx.update(|cx| updates(cx));
        self.type_text(cx, "in the window ").await?;
        let window_painted = self
            .wait_for(cx, Duration::from_secs(30), |_, cx| {
                updates(cx) > before && !layer_state(&window_view, cx).0.is_empty()
            })
            .await;
        self.pause(cx, Duration::from_millis(300)).await;
        // rtex lays the whole page out again here, so the layer covers all
        // of it and its ink is a page's, thinner than a paragraph band's.
        let (_, window_draws, window_covered, window_ink) =
            cx.update(|cx| layer_state(&window_view, cx));
        let (scale, fit) =
            cx.update(|cx| (window_view.read(cx).scale(), window_view.read(cx).fit()));
        self.with_window(cx, {
            let view = window_view.clone();
            move |_, _, cx| view.update(cx, |view, cx| view.zoom_to(5., None, None, cx))
        })?;
        let window_full_size = self
            .wait_for(cx, Duration::from_secs(10), |_, cx| {
                live_detail_sizes(&window_view, cx).is_some_and(|(detail, base)| detail > base)
            })
            .await;
        let window_sizes = cx.update(|cx| live_detail_sizes(&window_view, cx));
        // Docking hands the window's place to the panel and remembers it
        // for the document, so it goes back first.
        self.with_window(cx, {
            let view = window_view.clone();
            move |_, _, cx| {
                view.update(cx, |view, cx| match fit {
                    Some(mode) => view.apply_fit(mode, cx),
                    None => view.zoom_to(scale, None, None, cx),
                })
            }
        })?;
        self.with_window(cx, |_, window, cx| {
            pdf.update(cx, |pdf, cx| pdf.dock(true, window, cx));
        })?;
        let docked = self
            .wait_for(cx, Duration::from_secs(10), |shell, cx| {
                !shell.read(cx).pdf().read(cx).in_window()
            })
            .await;
        // The typing's own save and quiet build take it over before the
        // next step kills rtex.
        let settled = self
            .wait_for(cx, Duration::from_secs(60), |shell, cx| {
                let shell = shell.read(cx);
                !editor.read(cx).is_dirty(cx)
                    && shell.build_panel().read(cx).quiet_runs() > quiet_before
                    && !shell.building()
                    && preview.read(cx).overlays() == 0
                    && layer_state(&view, cx).0.is_empty()
            })
            .await;
        steps.check(
            "typing paints in the separate window, at full size there past the page cap",
            detached
                && window_painted
                && window_covered > 0.001
                && window_ink > 0.001
                && window_full_size
                && docked
                && settled,
            json!({
                "detached": detached,
                "updates": cx.update(|cx| updates(cx)) - before,
                "painted": window_painted,
                "draws": window_draws,
                "covered": window_covered,
                "ink_of_covered": window_ink,
                "detail_and_base_width_px": window_sizes,
                "docked": docked,
                "settled": settled,
            }),
        );
        Ok(())
    }
}
