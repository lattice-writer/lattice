//! The titlebar's Overleaf control (`canvas-toolbar.tsx`) is the app's:
//! unlinked, a cloud that opens the picker; linked, a split button on the
//! accent wash, its left half syncing (Review changes in manual mode) and
//! its chevron opening a menu with the project's name, Open in Overleaf and
//! Open another Overleaf project. A dot on its corner says where the link
//! stands: green while syncing, editing live or with the channel up, a
//! still amber while a 429's cool-down holds live mode's syncs, an accent
//! dot for work waiting on Overleaf in manual mode, a breathing amber while
//! the channel joins, and red when it failed
//! (syncing carries on; the tooltip says why). Its icon fades up into place
//! when it changes between linked and syncing (`StateSwap`).

use std::time::Duration;

use gpui::{
    AnimationExt as _, AnyElement, App, Context, Corners, IntoElement, ParentElement as _,
    Styled as _, div, px,
};
use lattice_i18n::t;
use lattice_settings::TitlebarTool;
use lattice_ui::{
    ActiveLattice as _, Icon,
    components::{IconButton, MenuAction, MenuPopover, Surface},
    motion::Motion,
    tokens::RADIUS_PILL,
};

use crate::shell::Shell;
use crate::{build::chip_mark::infinity_loader, overleaf::ChannelStatus};

/// The dot on the titlebar's Overleaf control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverleafBadge {
    /// Held for a 429's cool-down.
    Cooling,
    Online,
    /// Manual mode: Overleaf has work this project has not pulled.
    Pending,
    Connecting,
    Error,
}

impl Shell {
    /// The sync status the titlebar's Overleaf control names.
    pub(crate) fn overleaf_sync_label(&self, cx: &App) -> String {
        let bridge = self.overleaf.read(cx);
        let channel = bridge.channel();
        let detail = bridge.channel_detail();
        if bridge.link().is_none() {
            t!("Open a project from Overleaf").into()
        } else if bridge.syncing() {
            t!("Syncing with Overleaf…").into()
        } else if let Some(waiting) = bridge.cooldown_status() {
            // Held for a 429: idle-looking words would hide that the edit
            // is still on its way.
            waiting
        } else if bridge.remote_changes() {
            t!("New changes on Overleaf").into()
        } else if bridge.live_file().is_some() {
            t!("Live with Overleaf").into()
        } else {
            // When the channel is not up, say why: silently falling back to
            // syncing looks exactly like the feature being broken.
            match channel {
                ChannelStatus::Connecting => detail
                    .map(|detail| detail.to_string())
                    .unwrap_or_else(|| t!("Connecting to Overleaf's live channel…").into()),
                ChannelStatus::Error => match detail {
                    Some(detail) => t!(
                        "Live editing unavailable ({detail}), syncing instead",
                        detail = detail.as_ref()
                    ),
                    None => t!("Live editing unavailable, syncing instead").into(),
                },
                ChannelStatus::Live => detail
                    .map(|detail| detail.to_string())
                    .unwrap_or_else(|| t!("Live").into()),
                _ => t!("Sync with Overleaf").into(),
            }
        }
    }

    /// The dot the titlebar's Overleaf control wears, if any.
    pub(crate) fn overleaf_badge(&self, cx: &App) -> Option<OverleafBadge> {
        let bridge = self.overleaf.read(cx);
        bridge.link()?;
        let channel = bridge.channel();
        let syncing = bridge.syncing();
        // Online whenever something reaches Overleaf: a sync, live editing,
        // or the channel up. Held for a 429, the channel may be up while
        // nothing of ours goes: amber, as the label says it waits.
        let online = syncing || bridge.live_file().is_some() || channel == ChannelStatus::Live;
        if !syncing && bridge.cooldown_status().is_some() {
            Some(OverleafBadge::Cooling)
        } else if online {
            Some(OverleafBadge::Online)
        } else if bridge.remote_changes() {
            Some(OverleafBadge::Pending)
        } else if channel == ChannelStatus::Connecting {
            Some(OverleafBadge::Connecting)
        } else if channel == ChannelStatus::Error {
            Some(OverleafBadge::Error)
        } else {
            None
        }
    }

    /// The titlebar's Overleaf control, unless Settings › Appearance hides it.
    pub(in crate::shell) fn overleaf_sync_control(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let hidden = &crate::settings::get(cx).appearance.hidden_titlebar_tools;
        if hidden.contains(&TitlebarTool::Overleaf) {
            return None;
        }
        let label = self.overleaf_sync_label(cx);
        let bridge = self.overleaf.read(cx);
        let palette = cx.palette().clone();
        let linked = bridge.link().is_some();
        let syncing = bridge.syncing();
        let project_name = bridge.link().map(|link| link.project_name.clone());
        let dot = |color: gpui::Hsla| {
            div()
                .absolute()
                .top(px(3.))
                .right(px(3.))
                .size(px(7.))
                .rounded(RADIUS_PILL)
                .bg(color)
                .border(px(1.5))
                .border_color(palette.titlebar)
        };
        let badge: Option<AnyElement> = match self.overleaf_badge(cx) {
            // The cool-down's amber holds still: it waits on a clock, not on
            // a channel still joining.
            Some(OverleafBadge::Cooling) => Some(dot(palette.warning).into_any_element()),
            Some(OverleafBadge::Online) => {
                // Coming online sends one ripple out of the green dot
                // (`overleaf-dot-ripple`): a 1.5px ring in the success hue at
                // 90%, swelling to 2.6× as it fades, on the flourish tier. The
                // dot is the same element while it stays online, so the ripple
                // runs once per coming online; none under reduced motion.
                // The ring is the shadow spread round the dot's 4px padding box,
                // scaled about its centre: 7px across with a 1.5px edge at rest.
                let ripple = (!cx.reduce_motion()).then(|| {
                    div()
                        .absolute()
                        .rounded(RADIUS_PILL)
                        .border_color(palette.success)
                        .with_animation(
                            "overleaf-channel-ripple",
                            Motion::Flourish.animation(),
                            |ring, t| {
                                let scale = 1. + 1.6 * t;
                                let across = 7. * scale;
                                let at = (4. - across) / 2.;
                                ring.size(px(across))
                                    .left(px(at))
                                    .top(px(at))
                                    .border(px(1.5 * scale))
                                    .opacity(0.9 * (1. - t))
                            },
                        )
                });
                Some(dot(palette.success).children(ripple).into_any_element())
            }
            Some(OverleafBadge::Pending) => Some(dot(palette.accent).into_any_element()),
            Some(OverleafBadge::Connecting) => Some(
                dot(palette.warning)
                    .with_animation(
                        "overleaf-channel-breathe",
                        gpui::Animation::new(Duration::from_millis(1_600)).repeat(),
                        |this, t| {
                            let phase = (t * std::f32::consts::TAU).cos();
                            this.opacity(0.675 + 0.325 * phase)
                        },
                    )
                    .into_any_element(),
            ),
            Some(OverleafBadge::Error) => Some(
                dot(palette.danger)
                    .with_animation(
                        "overleaf-channel-pop",
                        Motion::Slow.animation(),
                        |this, t| {
                            let scale = if t < 0.6 {
                                0.4 + (1.35 - 0.4) * (t / 0.6)
                            } else {
                                1.35 - 0.35 * ((t - 0.6) / 0.4)
                            };
                            let inset = 3.5 * (1. - scale);
                            this.size(px(7. * scale))
                                .top(px(3. + inset))
                                .right(px(3. + inset))
                        },
                    )
                    .into_any_element(),
            ),
            None => None,
        };
        let radius = px(6.);
        let mut primary = IconButton::new(
            "overleaf",
            if linked {
                Icon::CLOUD_UPLOAD
            } else {
                Icon::CLOUD
            },
        )
        .surface(Surface::Titlebar)
        .on(linked)
        .disabled(syncing)
        .tooltip(label)
        .on_click(cx.listener(move |shell, _, window, cx| {
            if linked {
                shell.sync_overleaf(window, cx)
            } else {
                shell.open_overleaf_picker(window, cx)
            }
        }));
        if linked {
            primary = primary.animated(lattice_ui::animated_icon::ProductIcon::CloudUploadOutline);
        }
        let key = if syncing {
            "syncing"
        } else if linked {
            "linked"
        } else {
            "unlinked"
        };
        let changes = match self.overleaf_layers.swap.get() {
            None => 0,
            Some((last, changes)) if last == key => changes,
            Some((_, changes)) => changes + 1,
        };
        self.overleaf_layers.swap.set(Some((key, changes)));
        // The unlinked cloud keeps the button's own ink, which follows the
        // pointer; the linked icons sit on the accent wash in its ink.
        let swapped: Option<AnyElement> = if syncing {
            Some(infinity_loader("overleaf-syncing", px(14.), &palette).into_any_element())
        } else if linked {
            // The linked cloud's arrow dips when the button is hovered or
            // focused (`provided-icons.tsx`).
            Some(
                lattice_ui::animated_icon::AnimatedProductIcon::new(
                    lattice_ui::animated_icon::button_icon_id("overleaf"),
                    lattice_ui::animated_icon::ProductIcon::CloudUploadOutline,
                    px(14.),
                    palette.accent,
                )
                .into_any_element(),
            )
        } else {
            None
        };
        if let Some(content) = swapped {
            primary = primary.content(if changes == 0 {
                content
            } else {
                // Rises 3px as it fades up, on the moderate spring.
                div()
                    .relative()
                    .child(content)
                    .with_animation(
                        gpui::ElementId::NamedInteger("overleaf-swap".into(), changes as u64),
                        Motion::Moderate.animation(),
                        |this, t| this.opacity(t.clamp(0., 1.)).top(px(3. * (1. - t))),
                    )
                    .into_any_element()
            });
        }
        if let Some(badge) = badge {
            primary = primary.badge(badge);
        }
        if !linked {
            return Some(primary.into_any_element());
        }
        let primary = primary.corners(Corners {
            top_left: radius,
            bottom_left: radius,
            top_right: px(0.),
            bottom_right: px(0.),
        });
        let shell = cx.entity().downgrade();
        let button = IconButton::new("overleaf-menu", Icon::CHEVRON_DOWN)
            .surface(Surface::Titlebar)
            .on(true)
            .frame(px(18.), px(28.))
            .icon_size(px(10.))
            .corners(Corners {
                top_left: px(0.),
                bottom_left: px(0.),
                top_right: radius,
                bottom_right: radius,
            })
            .tooltip(t!("Overleaf project actions"));
        let menu = MenuPopover::new("overleaf-menu-popover", button, move |menu, _, _| {
            let (open, another) = (shell.clone(), shell.clone());
            let mut menu = menu.min_w(px(256.));
            if let Some(name) = project_name.clone() {
                menu = menu.label(name).separator();
            }
            menu.item(
                MenuAction::new(t!("Open in Overleaf"), move |_, cx| {
                    open.update(cx, |shell, cx| shell.open_on_overleaf(cx)).ok();
                })
                .icon(Icon::EXTERNAL_LINK),
            )
            .item(
                MenuAction::new(t!("Open another Overleaf project"), move |window, cx| {
                    another
                        .update(cx, |shell, cx| shell.open_overleaf_picker(window, cx))
                        .ok();
                })
                .icon(Icon::CLOUD),
            )
        });
        Some(
            div()
                .flex()
                .flex_none()
                .items_center()
                .child(primary)
                .child(menu)
                .into_any_element(),
        )
    }
}
