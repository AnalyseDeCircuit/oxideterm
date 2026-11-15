// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{
    AnimationExt, BoxShadow, CursorStyle, Div, ElementId, ParentElement, SharedString, Styled, div,
    point, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

use crate::motion::{MotionDuration, lerp, spatial_spring};

const SWITCH_TRACK_WIDTH: f32 = 34.0;
const SWITCH_TRACK_HEIGHT: f32 = 20.0;
const SWITCH_THUMB_SIZE: f32 = 16.0;
const SWITCH_THUMB_INSET: f32 = 1.0;
const SWITCH_THUMB_TRAVEL: f32 =
    SWITCH_TRACK_WIDTH - 2.0 - SWITCH_THUMB_SIZE - SWITCH_THUMB_INSET * 2.0;
const SWITCH_UNCHECKED_BG_ALPHA: u32 = 0x24;
const SWITCH_UNCHECKED_DARK_BORDER_ALPHA: u32 = 0x99;
const SWITCH_UNCHECKED_LIGHT_BORDER_ALPHA: u32 = 0x2e;
const SWITCH_THUMB_COLOR: u32 = 0xffffff;
const SWITCH_THUMB_RIM_SHADOW_ALPHA: u32 = 0x1f;
const SWITCH_THUMB_DROP_SHADOW_ALPHA: u32 = 0x33;
const SWITCH_FOCUS_RING_OFFSET: f32 = 2.0;
const SWITCH_FOCUS_RING_WIDTH: f32 = 2.0;
const SWITCH_FOCUS_RING_ALPHA: u32 = 0xb3;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SwitchOptions {
    pub focused: bool,
    pub disabled: bool,
}

impl SwitchOptions {
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

pub fn switch(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
) -> Div {
    switch_with(tokens, id, label, checked, SwitchOptions::default())
}

pub fn switch_with(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    options: SwitchOptions,
) -> Div {
    let label = label.into();
    let theme = tokens.ui;
    let id = id.into();

    let track_bg = if checked {
        rgba((theme.accent << 8) | 0xff)
    } else {
        rgba((theme.text << 8) | SWITCH_UNCHECKED_BG_ALPHA)
    };
    let track_border = if checked {
        rgba((theme.accent << 8) | 0xff)
    } else if theme.is_light() {
        // Light glass cards over bright wallpaper wash out `theme.border`; a low-alpha
        // ink hairline keeps the off-track silhouette crisp without darkening the well.
        rgba((theme.text << 8) | SWITCH_UNCHECKED_LIGHT_BORDER_ALPHA)
    } else {
        rgba((theme.border << 8) | SWITCH_UNCHECKED_DARK_BORDER_ALPHA)
    };

    let thumb = crate::surface::opaque_control_surface(
        div()
            .absolute()
            .top(px(SWITCH_THUMB_INSET))
            .size(px(SWITCH_THUMB_SIZE))
            .rounded_full()
            .bg(rgb(SWITCH_THUMB_COLOR))
            .shadow(vec![
                BoxShadow {
                    color: rgba(SWITCH_THUMB_RIM_SHADOW_ALPHA).into(),
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(0.5),
                    spread_radius: px(0.5),
                    inset: false,
                },
                BoxShadow {
                    color: rgba(SWITCH_THUMB_DROP_SHADOW_ALPHA).into(),
                    offset: point(px(0.0), px(1.0)),
                    blur_radius: px(2.0),
                    spread_radius: px(0.0),
                    inset: false,
                },
            ])
            .with_spring(
                id,
                spatial_spring(
                    tokens,
                    MotionDuration::Micro,
                    if checked { 1.0 } else { 0.0 },
                ),
                move |thumb, progress| {
                    let left = lerp(
                        SWITCH_THUMB_INSET,
                        SWITCH_THUMB_INSET + SWITCH_THUMB_TRAVEL,
                        progress.clamp(0.0, 1.0),
                    );
                    thumb.left(px(left))
                },
            ),
    );

    let mut track = div()
        .relative()
        .flex_none()
        .w(px(SWITCH_TRACK_WIDTH))
        .h(px(SWITCH_TRACK_HEIGHT))
        .rounded_full()
        .border_1()
        .border_color(track_border)
        .bg(track_bg)
        .child(thumb);

    if options.focused {
        track = track.shadow(switch_focus_ring(tokens));
    }

    let mut root = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .opacity(if options.disabled { 0.5 } else { 1.0 })
        .cursor(if options.disabled {
            CursorStyle::Arrow
        } else {
            CursorStyle::PointingHand
        })
        .child(track);

    if !label.is_empty() {
        root = root.child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .text_color(rgb(theme.text))
                .child(label),
        );
    }

    root
}

fn switch_focus_ring(tokens: &ThemeTokens) -> Vec<BoxShadow> {
    let zero = point(px(0.0), px(0.0));
    vec![
        BoxShadow {
            color: rgb(tokens.ui.bg).into(),
            offset: zero,
            blur_radius: px(0.0),
            spread_radius: px(SWITCH_FOCUS_RING_OFFSET),
            inset: false,
        },
        BoxShadow {
            color: rgba((tokens.ui.accent << 8) | SWITCH_FOCUS_RING_ALPHA).into(),
            offset: zero,
            blur_radius: px(0.0),
            spread_radius: px(SWITCH_FOCUS_RING_OFFSET + SWITCH_FOCUS_RING_WIDTH),
            inset: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use gpui::{Context, IntoElement, Render, Window};

    use super::*;

    struct SwitchTestView {
        tokens: ThemeTokens,
        checked: bool,
    }

    impl Render for SwitchTestView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            switch(&self.tokens, "test-switch", String::new(), self.checked)
        }
    }

    #[gpui::test]
    fn switch_positions_thumb_symmetrically_for_off_and_on_states(cx: &mut gpui::TestAppContext) {
        let mut tokens = oxideterm_theme::default_tokens();
        tokens.motion.enabled = false;
        tokens.motion.spatial_enabled = false;

        let (view, cx) = cx.add_window_view(|_, _| SwitchTestView {
            tokens,
            checked: false,
        });
        cx.simulate_resize(gpui::size(px(120.0), px(60.0)));

        for (checked, expected_left_offset) in [(false, 2.0_f32), (true, 16.0_f32)] {
            view.update(cx, |view, cx| {
                view.checked = checked;
                cx.notify();
            });
            let quads = cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                window.painted_quads()
            });
            let scale = cx.update(|window, _| window.scale_factor());
            let track = quads
                .iter()
                .find(|quad| {
                    (quad.bounds.size.width.0 / scale - SWITCH_TRACK_WIDTH).abs() < 0.01
                        && (quad.bounds.size.height.0 / scale - SWITCH_TRACK_HEIGHT).abs() < 0.01
                })
                .expect("switch track quad should be painted");
            let thumb = quads
                .iter()
                .find(|quad| {
                    (quad.bounds.size.width.0 / scale - SWITCH_THUMB_SIZE).abs() < 0.01
                        && (quad.bounds.size.height.0 / scale - SWITCH_THUMB_SIZE).abs() < 0.01
                })
                .expect("switch thumb quad should be painted");
            assert_eq!(
                (thumb.bounds.left().0 - track.bounds.left().0) / scale,
                expected_left_offset
            );
            assert_eq!((thumb.bounds.top().0 - track.bounds.top().0) / scale, 2.0);
            if checked {
                assert_eq!(track.background, rgb(tokens.ui.accent).into());
            }
        }
    }

    struct LightWallpaperSwitchCard {
        tokens: ThemeTokens,
        checked: bool,
    }

    impl Render for LightWallpaperSwitchCard {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            // Reproduces a Settings glass card on a light theme (`ui.bg == 0xffffff`)
            // wrapped in `tab_surface`, where the card already consumes the vibrancy
            // tint budget before painting child controls.
            crate::surface::tab_surface(
                &self.tokens,
                div()
                    .w(px(120.0))
                    .h(px(48.0))
                    .bg(rgb(self.tokens.ui.bg_card))
                    .child(switch(
                        &self.tokens,
                        "light-bg-switch",
                        String::new(),
                        self.checked,
                    ))
                    .into_any_element(),
            )
        }
    }

    #[gpui::test]
    fn switch_thumb_stays_opaque_inside_light_tab_surface_card(cx: &mut gpui::TestAppContext) {
        let mut tokens = ThemeTokens::from_builtin(oxideterm_theme::theme_by_id("code-light"));
        assert_eq!(tokens.ui.bg, SWITCH_THUMB_COLOR);
        tokens.metrics.sidebar_vibrancy_alpha = 0.72;
        tokens.motion.enabled = false;
        tokens.motion.spatial_enabled = false;

        let (view, cx) = cx.add_window_view(|_, _| LightWallpaperSwitchCard {
            tokens,
            checked: true,
        });
        cx.simulate_resize(gpui::size(px(160.0), px(80.0)));

        for checked in [true, false] {
            view.update(cx, |view, cx| {
                view.checked = checked;
                cx.notify();
            });
            let quads = cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                window.painted_quads()
            });
            let scale = cx.update(|window, _| window.scale_factor());
            let card = quads
                .iter()
                .find(|quad| (quad.bounds.size.width.0 / scale - 120.0).abs() < 0.01)
                .expect("card quad should be painted");
            let card_bg = card.background.as_solid().expect("solid card fill");
            assert!(
                card_bg.a < 0.80,
                "card fill must remain translucent under tab_surface, got alpha {}",
                card_bg.a
            );

            let thumb = quads
                .iter()
                .find(|quad| {
                    (quad.bounds.size.width.0 / scale - SWITCH_THUMB_SIZE).abs() < 0.01
                        && (quad.bounds.size.height.0 / scale - SWITCH_THUMB_SIZE).abs() < 0.01
                })
                .expect("switch thumb quad should be painted");
            assert_eq!(
                thumb.background,
                rgb(SWITCH_THUMB_COLOR).into(),
                "checked={checked}: thumb must remain 100% opaque white inside light tab_surface"
            );
        }
    }
}
