use gpui::{AnyElement, FontWeight, IntoElement, ParentElement, Rgba, Styled, div, px};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug)]
pub struct IconBadgeMetrics {
    pub width: f32,
    pub gap: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub text_size: f32,
    pub radius: f32,
}

pub fn icon_badge(
    metrics: IconBadgeMetrics,
    label: impl Into<String>,
    icon: impl IntoElement,
    background: Rgba,
    foreground: Rgba,
) -> AnyElement {
    div()
        .w(px(metrics.width))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(metrics.gap))
        .px(px(metrics.padding_x))
        .py(px(metrics.padding_y))
        .rounded(px(metrics.radius))
        .bg(background)
        .text_color(foreground)
        .text_size(px(metrics.text_size))
        .font_weight(FontWeight::MEDIUM)
        .child(icon)
        .child(label.into())
        .into_any_element()
}

pub fn icon_badge_metrics_from_tokens(tokens: &ThemeTokens, width: f32) -> IconBadgeMetrics {
    IconBadgeMetrics {
        width,
        gap: 4.0,
        padding_x: 6.0,
        padding_y: 2.0,
        text_size: 10.0,
        radius: tokens.radii.md,
    }
}
