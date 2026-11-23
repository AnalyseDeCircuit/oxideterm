use gpui::{Div, ParentElement, Styled, div, px, rgb, rgba};
use oxideterm_theme::ThemeTokens;

use crate::typography::tabular_nums_features;

pub const FONT_SIZE_HUD_DURATION_MS: u64 = 1200;

pub fn font_size_hud(tokens: &ThemeTokens, size: f32) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .rounded(px(tokens.radii.xs))
                .border_1()
                .border_color(rgb(tokens.ui.border))
                .bg(rgba((tokens.ui.bg_elevated << 8) | 0xe6))
                .px(px(tokens.metrics.ui_font_hud_padding_x))
                .py(px(tokens.metrics.ui_font_hud_padding_y))
                .shadow_lg()
                .child(
                    div()
                        .flex()
                        .items_end()
                        // Inherit the configured terminal font from the HUD owner.
                        .text_size(px(tokens.metrics.ui_text_2xl))
                        .font_features(tabular_nums_features())
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(tokens.ui.text))
                        .child(format!("{size:.0}"))
                        .child(
                            div()
                                .ml(px(tokens.spacing.one / 2.0))
                                .text_size(px(tokens.metrics.ui_text_base))
                                .font_weight(gpui::FontWeight::NORMAL)
                                .text_color(rgb(tokens.ui.text_muted))
                                .child("px"),
                        ),
                ),
        )
}
