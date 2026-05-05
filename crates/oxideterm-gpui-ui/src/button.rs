use gpui::{Div, ParentElement, Styled, div, prelude::*, px, rgb, rgba};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonTone {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonVariant {
    Default,
    Secondary,
    Outline,
    Ghost,
    Destructive,
    Link,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonSize {
    Default,
    Sm,
    Lg,
    Icon,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonRadius {
    None,
    Sm,
    Md,
}

pub fn button(tokens: &ThemeTokens, label: String, tone: ButtonTone) -> Div {
    button_with(
        tokens,
        label,
        ButtonOptions {
            variant: if tone == ButtonTone::Primary {
                ButtonVariant::Default
            } else {
                ButtonVariant::Secondary
            },
            size: ButtonSize::Default,
            radius: ButtonRadius::Md,
            disabled: false,
        },
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ButtonOptions {
    pub variant: ButtonVariant,
    pub size: ButtonSize,
    pub radius: ButtonRadius,
    pub disabled: bool,
}

impl Default for ButtonOptions {
    fn default() -> Self {
        Self {
            variant: ButtonVariant::Secondary,
            size: ButtonSize::Default,
            radius: ButtonRadius::Md,
            disabled: false,
        }
    }
}

pub fn button_with(tokens: &ThemeTokens, label: String, options: ButtonOptions) -> Div {
    let theme = tokens.ui;
    let metrics = tokens.metrics;
    let (height, padding_x, width) = match options.size {
        ButtonSize::Default => (
            metrics.ui_button_default_height,
            metrics.ui_button_default_padding_x,
            None,
        ),
        ButtonSize::Sm => (
            metrics.ui_button_sm_height,
            metrics.ui_button_sm_padding_x,
            None,
        ),
        ButtonSize::Lg => (
            metrics.ui_button_lg_height,
            metrics.ui_button_lg_padding_x,
            None,
        ),
        ButtonSize::Icon => (
            metrics.ui_button_icon_size,
            0.0,
            Some(metrics.ui_button_icon_size),
        ),
    };
    let radius = match options.radius {
        ButtonRadius::None => 0.0,
        ButtonRadius::Sm => tokens.radii.sm,
        ButtonRadius::Md => tokens.radii.md,
    };
    let (bg, border, text) = match options.variant {
        ButtonVariant::Default => (rgb(theme.text), rgba(0x00000000), rgb(theme.bg)),
        ButtonVariant::Secondary => (rgb(theme.bg_panel), rgb(theme.border), rgb(theme.text)),
        ButtonVariant::Outline => (rgba(0x00000000), rgb(theme.border), rgb(theme.text)),
        ButtonVariant::Ghost | ButtonVariant::Link => {
            (rgba(0x00000000), rgba(0x00000000), rgb(theme.text))
        }
        ButtonVariant::Destructive => (
            rgba((theme.error << 8) | 0xe6),
            rgba((theme.error << 8) | 0xcc),
            rgb(0xffffff),
        ),
    };
    let font_size = if options.size == ButtonSize::Sm {
        metrics.ui_text_xs
    } else {
        metrics.ui_text_sm
    };
    div()
        .h(px(height))
        .when_some(width, |this, width| this.w(px(width)))
        .px(px(padding_x))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_size(px(font_size))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(text)
        .opacity(if options.disabled { 0.5 } else { 1.0 })
        .cursor_pointer()
        .child(label)
}
