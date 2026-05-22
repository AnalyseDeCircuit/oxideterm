use gpui::{
    AnyElement, BoxShadow, CursorStyle, Div, Hsla, ParentElement, Rgba, Styled, div, point,
    prelude::*, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

use crate::surface::color_for_background;

const BUTTON_FOCUS_RING_ALPHA: u32 = 0xb3; // Tauri focus-visible:ring-theme-accent/70
const BUTTON_FOCUS_RING_WIDTH: f32 = 2.0; // Tauri focus-visible:ring-2
const BUTTON_ACTIVE_BACKGROUND_ALPHA: u32 = 0x66; // Tauri [data-bg-active] color-mix(... 40%, transparent)
const BUTTON_ACTIVE_HOVER_ALPHA: u32 = 0x80; // Tauri [data-bg-active] hover color-mix(... 50%, transparent)
const BUTTON_ACTIVE_BORDER_ALPHA: u32 = 0xbf; // Tauri [data-bg-active] border color-mix(... 75%, transparent)
const TOOLBAR_BUTTON_ICON_GAP: f32 = 6.0; // Tauri toolbar gap-1.5
const ICON_BUTTON_DISABLED_OPACITY: f32 = 0.35; // Tauri disabled icon button opacity
const ICON_BUTTON_IDLE_OPACITY: f32 = 0.5; // Tauri muted toolbar icon opacity

pub fn tauri_focus_visible_ring(tokens: &ThemeTokens) -> Vec<BoxShadow> {
    // Browser :focus-visible is keyboard-owned and shared across shadcn buttons,
    // select triggers, and dialog footer actions. GPUI callers pass the owner
    // state explicitly, but the visual ring must stay centralized.
    vec![BoxShadow {
        color: Hsla::from(rgba((tokens.ui.accent << 8) | BUTTON_FOCUS_RING_ALPHA)),
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(BUTTON_FOCUS_RING_WIDTH),
    }]
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolbarButtonIconPosition {
    Leading,
    Trailing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolbarButtonOptions {
    pub button: ButtonOptions,
    pub has_background: bool,
    pub show_label: bool,
    pub loading: bool,
    pub icon_position: ToolbarButtonIconPosition,
    pub icon_gap: Option<f32>,
    pub background: Option<Rgba>,
    pub border: Option<Rgba>,
    pub text_color: Option<Rgba>,
    pub hover_background: Option<Rgba>,
    pub hover_border: Option<Rgba>,
    pub hover_text_color: Option<Rgba>,
    pub hover_opacity: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub padding_x: Option<f32>,
    pub font_size: Option<f32>,
    pub focus_visible: bool,
}

impl Default for ToolbarButtonOptions {
    fn default() -> Self {
        Self {
            button: ButtonOptions {
                size: ButtonSize::Sm,
                ..ButtonOptions::default()
            },
            has_background: false,
            show_label: true,
            loading: false,
            icon_position: ToolbarButtonIconPosition::Leading,
            icon_gap: None,
            background: None,
            border: None,
            text_color: None,
            hover_background: None,
            hover_border: None,
            hover_text_color: None,
            hover_opacity: None,
            height: None,
            min_width: None,
            padding_x: None,
            font_size: None,
            focus_visible: false,
        }
    }
}

impl ToolbarButtonOptions {
    pub fn compact_text(
        variant: ButtonVariant,
        radius: ButtonRadius,
        height: f32,
        padding_x: f32,
        font_size: f32,
    ) -> Self {
        // Tauri preview toolbars use small text buttons with explicit h/px/text-xs
        // classes. Keep that browser button shape in the shared primitive so
        // FileManager/SFTP previews do not reimplement local div-style buttons.
        Self {
            button: ButtonOptions {
                variant,
                size: ButtonSize::Sm,
                radius,
                disabled: false,
            },
            show_label: true,
            height: Some(height),
            padding_x: Some(padding_x),
            font_size: Some(font_size),
            ..Self::default()
        }
    }

    pub fn compact_text_min_width(
        variant: ButtonVariant,
        radius: ButtonRadius,
        height: f32,
        min_width: f32,
        padding_x: f32,
        font_size: f32,
    ) -> Self {
        let mut options = Self::compact_text(variant, radius, height, padding_x, font_size);
        options.min_width = Some(min_width);
        options
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconButtonOptions {
    pub size: f32,
    pub radius: ButtonRadius,
    pub disabled: bool,
    pub loading: bool,
    pub has_background: bool,
    pub background: Option<Rgba>,
    pub border: Option<Rgba>,
    pub hover_background: Option<Rgba>,
    pub hover_opacity: Option<f32>,
    pub focus_visible: bool,
    pub idle_opacity: f32,
    pub disabled_opacity: f32,
}

impl IconButtonOptions {
    pub fn compact(size: f32) -> Self {
        Self {
            size,
            radius: ButtonRadius::Sm,
            disabled: false,
            loading: false,
            has_background: false,
            background: None,
            border: None,
            hover_background: None,
            hover_opacity: None,
            focus_visible: false,
            idle_opacity: ICON_BUTTON_IDLE_OPACITY,
            disabled_opacity: ICON_BUTTON_DISABLED_OPACITY,
        }
    }

    pub fn opaque_toolbar(size: f32, radius: ButtonRadius) -> Self {
        // Tauri toolbar icon buttons are often normal-opacity buttons whose
        // disabled state fades, unlike muted icon-only affordances. Keep that
        // option bundle in the shared primitive so feature toolbars do not copy
        // the same opacity and radius defaults.
        Self {
            radius,
            idle_opacity: 1.0,
            disabled_opacity: ICON_BUTTON_DISABLED_OPACITY,
            ..Self::compact(size)
        }
    }
}

pub fn button_with(tokens: &ThemeTokens, label: String, options: ButtonOptions) -> Div {
    button_base(tokens, options, false).child(label)
}

pub fn toolbar_button(
    tokens: &ThemeTokens,
    label: String,
    icon: Option<AnyElement>,
    options: ToolbarButtonOptions,
) -> Div {
    let disabled = options.button.disabled || options.loading;
    let hover_bg = options.hover_background.unwrap_or_else(|| {
        color_for_background(
            tokens.ui.bg_hover,
            options.has_background,
            BUTTON_ACTIVE_HOVER_ALPHA,
        )
    });
    let button_options = ButtonOptions {
        disabled,
        ..options.button
    };
    let button = button_base(tokens, button_options, options.has_background)
        .gap(px(options.icon_gap.unwrap_or(TOOLBAR_BUTTON_ICON_GAP)))
        .when_some(options.height, |button, height| button.h(px(height)))
        .when_some(options.min_width, |button, min_width| {
            button.min_w(px(min_width))
        })
        .when_some(options.padding_x, |button, padding_x| {
            button.px(px(padding_x))
        })
        .when_some(options.font_size, |button, font_size| {
            button.text_size(px(font_size))
        })
        .when_some(options.background, |button, background| {
            button.bg(background)
        })
        .when_some(options.border, |button, border| button.border_color(border))
        .when_some(options.text_color, |button, text_color| {
            button.text_color(text_color)
        })
        .hover(move |button| {
            if disabled {
                button
            } else {
                let button = button.bg(hover_bg);
                let button = if let Some(hover_border) = options.hover_border {
                    button.border_color(hover_border)
                } else {
                    button
                };
                let button = if let Some(hover_text_color) = options.hover_text_color {
                    button.text_color(hover_text_color)
                } else {
                    button
                };
                if let Some(hover_opacity) = options.hover_opacity {
                    button.opacity(hover_opacity)
                } else {
                    button
                }
            }
        });
    let button = match (icon, options.icon_position) {
        (Some(icon), ToolbarButtonIconPosition::Leading) => button
            .child(icon)
            .when(options.show_label, |button| button.child(label)),
        (Some(icon), ToolbarButtonIconPosition::Trailing) => button
            .when(options.show_label, |button| button.child(label))
            .child(icon),
        (None, _) => button.when(options.show_label, |button| button.child(label)),
    };
    button_focus_visible(tokens, button, options.focus_visible)
}

pub fn icon_button(tokens: &ThemeTokens, icon: AnyElement, options: IconButtonOptions) -> Div {
    let disabled = options.disabled || options.loading;
    let bg = if let Some(background) = options.background {
        background
    } else if options.has_background {
        color_for_background(
            tokens.ui.bg_panel,
            options.has_background,
            BUTTON_ACTIVE_BACKGROUND_ALPHA,
        )
    } else {
        rgba(0x00000000)
    };
    let opacity = if disabled {
        options.disabled_opacity
    } else {
        options.idle_opacity
    };
    let hover_bg = options.hover_background.unwrap_or_else(|| {
        color_for_background(
            tokens.ui.bg_hover,
            options.has_background,
            BUTTON_ACTIVE_HOVER_ALPHA,
        )
    });
    let button = div()
        .size(px(options.size))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(button_radius_px(tokens, options.radius)))
        .bg(bg)
        .when_some(options.border, |button, border| {
            // Some migrated toolbar actions are icon-only but still use the
            // shadcn outline button chrome in Tauri. Keep that border in the
            // shared primitive so feature helpers do not reimplement it.
            button.border_1().border_color(border)
        })
        .opacity(opacity)
        // Icon buttons appear all over toolbars; disabled/loading must be
        // visible at the primitive level even when the caller owns the action.
        .cursor(if disabled {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        })
        .hover(move |button| {
            if disabled {
                button
            } else {
                let button = button.bg(hover_bg);
                if let Some(hover_opacity) = options.hover_opacity {
                    button.opacity(hover_opacity)
                } else {
                    button
                }
            }
        })
        .child(icon);
    button_focus_visible(tokens, button, options.focus_visible)
}

fn button_base(tokens: &ThemeTokens, options: ButtonOptions, has_background: bool) -> Div {
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
    let radius = button_radius_px(tokens, options.radius);
    let (bg, border, text) = match options.variant {
        ButtonVariant::Default => (rgb(theme.text), rgba(0x00000000), rgb(theme.bg)),
        ButtonVariant::Secondary => (
            color_for_background(
                theme.bg_panel,
                has_background,
                BUTTON_ACTIVE_BACKGROUND_ALPHA,
            ),
            color_for_background(theme.border, has_background, BUTTON_ACTIVE_BORDER_ALPHA),
            rgb(theme.text),
        ),
        ButtonVariant::Outline => (
            rgba(0x00000000),
            color_for_background(theme.border, has_background, BUTTON_ACTIVE_BORDER_ALPHA),
            rgb(theme.text),
        ),
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
        // Tauri/shadcn disabled buttons use opacity plus disabled pointer
        // semantics. Keep the shared primitive from advertising clickability
        // when feature code intentionally omits the mouse handler.
        .cursor(if options.disabled {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        })
}

fn button_radius_px(tokens: &ThemeTokens, radius: ButtonRadius) -> f32 {
    match radius {
        ButtonRadius::None => 0.0,
        ButtonRadius::Sm => tokens.radii.sm,
        ButtonRadius::Md => tokens.radii.md,
    }
}

pub fn button_focus_visible(tokens: &ThemeTokens, button: Div, focused: bool) -> Div {
    if !focused {
        return button;
    }
    // GPUI buttons are drawn from workspace-owned keyboard focus rather than
    // DOM :focus-visible, so the shared primitive applies the same ring when a
    // caller marks the action as keyboard-focused.
    button.shadow(tauri_focus_visible_ring(tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_button_defaults_to_compact_shadcn_order() {
        let options = ToolbarButtonOptions::default();

        assert_eq!(options.button.size, ButtonSize::Sm);
        assert_eq!(options.icon_position, ToolbarButtonIconPosition::Leading);
        assert_eq!(options.icon_gap, None);
        assert_eq!(options.background, None);
        assert_eq!(options.border, None);
        assert_eq!(options.text_color, None);
        assert_eq!(options.hover_background, None);
        assert_eq!(options.hover_border, None);
        assert_eq!(options.hover_text_color, None);
        assert_eq!(options.hover_opacity, None);
        assert_eq!(options.height, None);
        assert_eq!(options.min_width, None);
        assert_eq!(options.padding_x, None);
        assert_eq!(options.font_size, None);
        assert!(options.show_label);
        assert!(!options.loading);
    }

    #[test]
    fn compact_text_toolbar_button_preserves_tauri_preview_metrics() {
        let options = ToolbarButtonOptions::compact_text_min_width(
            ButtonVariant::Secondary,
            ButtonRadius::Sm,
            28.0,
            32.0,
            8.0,
            12.0,
        );

        assert_eq!(options.button.variant, ButtonVariant::Secondary);
        assert_eq!(options.button.size, ButtonSize::Sm);
        assert_eq!(options.button.radius, ButtonRadius::Sm);
        assert_eq!(options.height, Some(28.0));
        assert_eq!(options.min_width, Some(32.0));
        assert_eq!(options.padding_x, Some(8.0));
        assert_eq!(options.font_size, Some(12.0));
        assert!(options.show_label);
        assert!(!options.button.disabled);
    }

    #[test]
    fn compact_icon_button_carries_shared_disabled_opacity() {
        let options = IconButtonOptions::compact(20.0);

        assert_eq!(options.size, 20.0);
        assert_eq!(options.radius, ButtonRadius::Sm);
        assert_eq!(options.idle_opacity, ICON_BUTTON_IDLE_OPACITY);
        assert_eq!(options.disabled_opacity, ICON_BUTTON_DISABLED_OPACITY);
        assert_eq!(options.background, None);
        assert_eq!(options.border, None);
        assert_eq!(options.hover_background, None);
        assert_eq!(options.hover_opacity, None);
        assert!(!options.disabled);
        assert!(!options.loading);
    }
}
