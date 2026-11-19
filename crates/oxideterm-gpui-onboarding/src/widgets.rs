// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{
    AnyElement, App, CursorStyle, Div, FontWeight, MouseButton, ParentElement, Rgba, SharedString,
    Styled, Window, div, prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with,
};
use oxideterm_gpui_ui::motion::UserSegmentedControlMotionState;
use oxideterm_gpui_ui::theme_glass_card_background_alpha;
use oxideterm_theme::ThemeTokens;

pub(crate) const ONBOARDING_ACCENT_SUBTLE_ALPHA: u32 = 0x12;
pub(crate) const ONBOARDING_ACCENT_BORDER_ALPHA: u32 = 0x70;
pub(crate) const ONBOARDING_HAIRLINE_ALPHA: u32 = 0x45;
pub(crate) const ONBOARDING_HEADER_BORDER_ALPHA: u32 = 0x55;
pub(crate) const ONBOARDING_CARD_BORDER_ALPHA: u32 = 0x80;
pub(crate) const ONBOARDING_DISABLED_OPACITY: f32 = 0.45;

pub(crate) fn card_bg(tokens: &ThemeTokens, has_background: bool) -> Rgba {
    let base = if tokens.ui.is_light() {
        tokens.ui.bg_panel
    } else {
        tokens.ui.bg_card
    };
    if has_background {
        rgba((base << 8) | theme_glass_card_background_alpha(tokens))
    } else {
        rgb(base)
    }
}

pub(crate) fn sunken_bg(tokens: &ThemeTokens, has_background: bool) -> Rgba {
    if has_background {
        rgba((tokens.ui.bg_sunken << 8) | theme_glass_card_background_alpha(tokens))
    } else {
        rgb(tokens.ui.bg_sunken)
    }
}

pub(crate) fn keycap_bg(tokens: &ThemeTokens, has_background: bool) -> Rgba {
    let base = if tokens.ui.is_light() {
        tokens.ui.bg_sunken
    } else {
        tokens.ui.bg_card
    };
    if has_background {
        rgba((base << 8) | theme_glass_card_background_alpha(tokens))
    } else {
        rgb(base)
    }
}

pub(crate) fn render_icon(path: &'static str, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(path)
        .size(px(size))
        .flex_none()
        .text_color(color)
        .into_any_element()
}

pub(crate) fn hairline(tokens: &ThemeTokens) -> AnyElement {
    div()
        .h(px(1.0))
        .w_full()
        .bg(rgba((tokens.ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
        .into_any_element()
}

pub(crate) fn section_header(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    trailing: Option<AnyElement>,
) -> AnyElement {
    div()
        .w_full()
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(tokens.ui.text_muted))
                .child(label.into()),
        )
        .when_some(trailing, |row, item| row.child(item))
        .into_any_element()
}

pub(crate) fn grouped_surface(
    tokens: &ThemeTokens,
    has_background: bool,
    rows: Vec<AnyElement>,
) -> Div {
    let mut card = div()
        .w_full()
        .min_w(px(0.0))
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(rgba((tokens.ui.border << 8) | ONBOARDING_CARD_BORDER_ALPHA))
        .bg(card_bg(tokens, has_background))
        .overflow_hidden()
        .flex()
        .flex_col();
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            card = card.child(hairline(tokens));
        }
        card = card.child(row);
    }
    card
}

pub(crate) fn action_button(
    tokens: &ThemeTokens,
    id: impl Into<SharedString>,
    label: String,
    icon: Option<(&'static str, bool)>,
    variant: ButtonVariant,
    disabled: bool,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = tokens.ui;
    let id = id.into();
    let selector = id.to_string();
    let icon_color = rgb(if variant == ButtonVariant::Default {
        theme.accent_text
    } else {
        theme.text
    });
    button_with(
        tokens,
        label,
        ButtonOptions {
            variant,
            size: ButtonSize::Default,
            radius: ButtonRadius::Md,
            disabled,
        },
    )
    .id(id)
    .debug_selector(move || selector.clone())
    .when_some(icon, |button, (icon_path, leading)| {
        button
            .when(leading, |button| button.flex_row_reverse())
            .child(render_icon(icon_path, 14.0, icon_color))
    })
    .when(variant == ButtonVariant::Default, |button| {
        button
            .bg(rgb(theme.accent))
            .text_color(rgb(theme.accent_text))
    })
    .when(!disabled, |button| {
        button.hover(move |button| {
            button.bg(rgb(if variant == ButtonVariant::Default {
                theme.accent_hover
            } else {
                theme.bg_hover
            }))
        })
    })
    .gap(px(6.0))
    .opacity(if disabled {
        ONBOARDING_DISABLED_OPACITY
    } else {
        1.0
    })
    .cursor(if disabled {
        CursorStyle::OperationNotAllowed
    } else {
        CursorStyle::PointingHand
    })
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        if !disabled {
            on_press(window, cx);
        }
        cx.stop_propagation();
    })
    .into_any_element()
}

pub(crate) fn compact_pill_bar(
    tokens: &ThemeTokens,
    id: &'static str,
    has_background: bool,
    active_index: usize,
    motion: &UserSegmentedControlMotionState,
    items: Vec<AnyElement>,
) -> AnyElement {
    let item_count = items.len().max(1);
    let active_index = active_index.min(item_count - 1);
    let previous_index = motion
        .previous_index_for(id, active_index)
        .unwrap_or(active_index)
        .min(item_count - 1);
    let transitioning = motion.is_active_for(id, active_index);
    let item_width = 1.0 / item_count as f32;
    let active_left = active_index as f32 * item_width;
    let previous_left = previous_index as f32 * item_width;

    // Mirror OxideTerm's segmented indicator track so onboarding pills follow the
    // exact Off / Reduced / Normal / Fast motion policy as settings switches.
    let indicator = div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(relative(item_width))
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(rgba((tokens.ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
        .bg(rgb(tokens.ui.bg_panel));
    let indicator = oxideterm_gpui_ui::segmented_control::selection_indicator(
        tokens,
        id,
        indicator,
        previous_left,
        active_left,
        transitioning,
    );

    div()
        .w_full()
        .min_w(px(0.0))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgba(
            (tokens.ui.border << 8) | ONBOARDING_HEADER_BORDER_ALPHA,
        ))
        .bg(sunken_bg(tokens, has_background))
        .p(px(2.0))
        .overflow_hidden()
        .child(
            div()
                .relative()
                .w_full()
                .grid()
                .grid_cols(item_count as u16)
                .child(indicator)
                .children(items),
        )
        .into_any_element()
}

pub(crate) fn compact_pill_item(
    tokens: &ThemeTokens,
    id: impl Into<SharedString>,
    label: String,
    selected: bool,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = tokens.ui;
    let id = id.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .relative()
        .w_full()
        .min_w(px(0.0))
        .px(px(6.0))
        .py(px(4.5))
        .rounded(px(tokens.radii.sm))
        .when(!selected, |item| {
            item.hover(move |style| style.bg(rgb(theme.bg_hover)))
        })
        .flex()
        .items_center()
        .justify_center()
        .cursor(CursorStyle::PointingHand)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_press(window, cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .text_align(gpui::TextAlign::Center)
                .text_size(px(tokens.metrics.ui_text_xs))
                .line_height(px(tokens.metrics.ui_text_xs * 1.3))
                .font_weight(if selected {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .text_color(rgb(if selected {
                    theme.text_heading
                } else {
                    theme.text_muted
                }))
                .child(label),
        )
        .into_any_element()
}

pub(crate) fn keycap(
    tokens: &ThemeTokens,
    has_background: bool,
    mono_font: SharedString,
    label: String,
) -> AnyElement {
    div()
        .rounded(px(tokens.radii.xs))
        .border_1()
        .border_color(rgba((tokens.ui.border << 8) | ONBOARDING_CARD_BORDER_ALPHA))
        .bg(keycap_bg(tokens, has_background))
        .px(px(5.0))
        .py(px(1.5))
        .font_family(mono_font)
        .text_size(px(tokens.metrics.ui_text_2xs))
        .line_height(px(14.0))
        .text_color(rgb(tokens.ui.text))
        .child(label)
        .into_any_element()
}
