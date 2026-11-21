//! Settings › Agent › Display, the GPUI app's own page: the Agent's font
//! size, apart from the editors', on a slider like theirs. The Agent's page
//! draws at it as a page zoom (`agent/surface.rs`'s `font_zoom`), in the
//! docked panel and in its own window alike, so the conversation, the
//! composer and code blocks grow together and keep their proportions.

use gpui::{AnyElement, Context, ElementId};
use lattice_i18n::t;
use lattice_ui::components::Slider;

use super::{
    controls::{self, PageWidth, Row, Wrap},
    dialog::SettingsDialog,
    editor_pane::row,
};

pub fn render(
    dialog: &mut SettingsDialog,
    width: PageWidth,
    cx: &mut Context<SettingsDialog>,
) -> AnyElement {
    let size = Slider::new(
        ElementId::Name("agent-font-size-slider".into()),
        &dialog.fields.agent_font_size,
    )
    .label(t!("Agent font size"))
    .readout(|value| format!("{}px", value.round()).into());
    let rows = vec![row(
        dialog,
        Row::new("agent-font-size", t!("Agent font size"))
            .description(
                t!("The Agent’s conversation and composer, in the panel and in its own window. The editors keep their own size."),
                cx,
            )
            .control(size)
            .wrap(Wrap::Line),
        width,
        cx,
    )];
    controls::page(
        t!("Display"),
        width,
        vec![controls::first_group(None, rows, cx)],
        cx,
    )
}
