use gpui::{
    AnyElement, Div, FontWeight, IntoElement, ParentElement, Rgba, Styled, div, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

use crate::surface::color_with_alpha;

const STATUS_PILL_BACKGROUND_ALPHA: u32 = 0x1a;
const STATUS_PILL_BORDER_ALPHA: u32 = 0x40;
const STATUS_PILL_STRONG_BACKGROUND_ALPHA: u32 = 0x33;
const STATUS_PILL_STRONG_BORDER_ALPHA: u32 = 0x80;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusTone {
    Neutral,
    Accent,
    Success,
    Warning,
    Error,
    Info,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusPillSize {
    Compact,
    Normal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatusPillOptions {
    pub tone: StatusTone,
    pub size: StatusPillSize,
    pub strong: bool,
}

impl StatusPillOptions {
    pub const fn new(tone: StatusTone) -> Self {
        Self {
            tone,
            size: StatusPillSize::Normal,
            strong: false,
        }
    }

    pub const fn compact(mut self) -> Self {
        self.size = StatusPillSize::Compact;
        self
    }

    pub const fn strong(mut self) -> Self {
        self.strong = true;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatusPillColors {
    pub background: Rgba,
    pub border: Rgba,
    pub text: Rgba,
}

pub fn status_pill(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    options: StatusPillOptions,
) -> Div {
    status_pill_element(tokens, label.into(), options)
}

pub fn status_pill_element(
    tokens: &ThemeTokens,
    child: impl IntoElement,
    options: StatusPillOptions,
) -> Div {
    let colors = status_pill_colors(tokens, options.tone, options.strong);
    let (padding_x, padding_y, text_size) = match options.size {
        StatusPillSize::Compact => (tokens.spacing.two, 1.0, tokens.metrics.ui_text_xs),
        StatusPillSize::Normal => (
            tokens.spacing.two + tokens.spacing.one,
            2.0,
            tokens.metrics.ui_text_xs,
        ),
    };

    // Status pills are tiny semantic signals; they should not carry large
    // amounts of text or become the visual frame for an entire row.
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .max_w(px(180.0))
        .px(px(padding_x))
        .py(px(padding_y))
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(colors.border)
        .bg(colors.background)
        .text_size(px(text_size))
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.text)
        .whitespace_nowrap()
        .child(child)
}

pub fn status_pill_colors(
    tokens: &ThemeTokens,
    tone: StatusTone,
    strong: bool,
) -> StatusPillColors {
    let color = match tone {
        StatusTone::Neutral => tokens.ui.text_muted,
        StatusTone::Accent => tokens.ui.accent,
        StatusTone::Success => tokens.ui.success,
        StatusTone::Warning => tokens.ui.warning,
        StatusTone::Error => tokens.ui.error,
        StatusTone::Info => tokens.ui.info,
    };
    let background_alpha = if strong {
        STATUS_PILL_STRONG_BACKGROUND_ALPHA
    } else {
        STATUS_PILL_BACKGROUND_ALPHA
    };
    let border_alpha = if strong {
        STATUS_PILL_STRONG_BORDER_ALPHA
    } else {
        STATUS_PILL_BORDER_ALPHA
    };
    StatusPillColors {
        background: color_with_alpha(color, background_alpha),
        border: color_with_alpha(color, border_alpha),
        text: if tone == StatusTone::Neutral {
            rgb(tokens.ui.text_secondary)
        } else {
            rgba((color << 8) | 0xff)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_pill_uses_semantic_token_color() {
        let tokens = oxideterm_theme::default_tokens();
        let colors = status_pill_colors(&tokens, StatusTone::Success, false);

        assert_eq!(
            colors.background,
            color_with_alpha(tokens.ui.success, STATUS_PILL_BACKGROUND_ALPHA)
        );
        assert_eq!(
            colors.border,
            color_with_alpha(tokens.ui.success, STATUS_PILL_BORDER_ALPHA)
        );
        assert_eq!(colors.text, rgba((tokens.ui.success << 8) | 0xff));
    }

    #[test]
    fn strong_status_pill_uses_stronger_alpha_pair() {
        let tokens = oxideterm_theme::default_tokens();
        let colors = status_pill_colors(&tokens, StatusTone::Warning, true);

        assert_eq!(
            colors.background,
            color_with_alpha(tokens.ui.warning, STATUS_PILL_STRONG_BACKGROUND_ALPHA)
        );
        assert_eq!(
            colors.border,
            color_with_alpha(tokens.ui.warning, STATUS_PILL_STRONG_BORDER_ALPHA)
        );
    }
}
