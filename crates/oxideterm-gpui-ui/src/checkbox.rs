use gpui::{Div, ParentElement, Styled, div, prelude::*, px, rgb, rgba, svg};
use oxideterm_theme::ThemeTokens;

const CHECKBOX_UNCHECKED_BG_ALPHA: u32 = 0x00; // Tauri unchecked root has no background class.
const CHECKBOX_CHECKED_BG_ALPHA: u32 = 0xff; // Tauri data-[state=checked]:bg-theme-accent.
const CHECKBOX_CHECKED_TEXT: u32 = 0xffffff; // Tauri data-[state=checked]:text-white.
const CHECKBOX_ICON_PATH: &str = "lucide/check.svg";

pub fn checkbox(tokens: &ThemeTokens, label: String, checked: bool) -> Div {
    let theme = tokens.ui;
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .cursor_pointer()
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
