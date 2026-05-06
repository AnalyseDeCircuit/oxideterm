use gpui::{
    BoxShadow, CursorStyle, Div, ParentElement, Styled, div, point, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_theme::ThemeTokens;

const CHECKBOX_UNCHECKED_BG_ALPHA: u32 = 0x00; // Tauri unchecked root has no background class.
const CHECKBOX_CHECKED_BG_ALPHA: u32 = 0xff; // Tauri data-[state=checked]:bg-theme-accent.
const CHECKBOX_CHECKED_TEXT: u32 = 0xffffff; // Tauri data-[state=checked]:text-white.
const CHECKBOX_DISABLED_OPACITY: f32 = 0.5; // Tauri disabled:opacity-50.
const CHECKBOX_ENABLED_OPACITY: f32 = 1.0;
const CHECKBOX_FOCUS_RING_ALPHA: u32 = 0xb3; // Tauri focus-visible:ring-theme-accent/70.
const CHECKBOX_FOCUS_RING_WIDTH: f32 = 2.0; // Tauri focus-visible:ring-2.
const CHECKBOX_FOCUS_RING_OFFSET: f32 = 1.0; // Tauri focus-visible:ring-offset-1.
const CHECKBOX_ICON_PATH: &str = "lucide/check.svg";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CheckboxOptions {
    pub focused: bool,
    pub disabled: bool,
}

pub fn checkbox(tokens: &ThemeTokens, label: String, checked: bool) -> Div {
    checkbox_with(tokens, label, checked, CheckboxOptions::default())
}

pub fn checkbox_with(
    tokens: &ThemeTokens,
    label: String,
    checked: bool,
    options: CheckboxOptions,
) -> Div {
    let theme = tokens.ui;
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .cursor(if options.disabled {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        })
        .opacity(if options.disabled {
            CHECKBOX_DISABLED_OPACITY
        } else {
            CHECKBOX_ENABLED_OPACITY
        })
        .child(
            div()
                .size(px(tokens.metrics.ui_checkbox_size))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(tokens.radii.xs))
                .border_1()
                .border_color(if checked {
                    rgb(theme.accent)
                } else {
                    rgb(theme.border)
                })
                .bg(if checked {
                    rgba((theme.accent << 8) | CHECKBOX_CHECKED_BG_ALPHA)
                } else {
                    rgba((theme.bg << 8) | CHECKBOX_UNCHECKED_BG_ALPHA)
                })
                .when(options.focused, |box_el| {
                    box_el.shadow(checkbox_focus_ring(tokens))
                })
                .when(checked, |box_el| {
                    box_el.child(
                        svg()
                            .path(CHECKBOX_ICON_PATH)
                            .size(px(tokens.metrics.ui_checkbox_icon_size))
                            .text_color(rgb(CHECKBOX_CHECKED_TEXT)),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .text_color(rgb(theme.text))
                .child(label),
        )
}

fn checkbox_focus_ring(tokens: &ThemeTokens) -> Vec<BoxShadow> {
    let zero = point(px(0.0), px(0.0));
    vec![
        BoxShadow {
            color: gpui::Hsla::from(rgb(tokens.ui.bg)),
            offset: zero,
            blur_radius: px(0.0),
            spread_radius: px(CHECKBOX_FOCUS_RING_OFFSET),
        },
        BoxShadow {
            color: gpui::Hsla::from(rgba((tokens.ui.accent << 8) | CHECKBOX_FOCUS_RING_ALPHA)),
            offset: zero,
            blur_radius: px(0.0),
            spread_radius: px(CHECKBOX_FOCUS_RING_OFFSET + CHECKBOX_FOCUS_RING_WIDTH),
        },
    ]
}
