// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{AnyElement, App, IntoElement, MouseDownEvent, Rgba, Window, rgb, rgba};
use oxideterm_gpui_ui::button::{
    ButtonRadius, ButtonVariant, ToolbarButtonOptions, toolbar_action_button,
};
use oxideterm_gpui_ui::surface::{
    color_for_background, color_with_background_scaled_alpha, tauri_glass_surface_shadow,
};
use oxideterm_theme::ThemeTokens;

const SFTP_TEXT_XS: f32 = 12.0; // Tauri text-xs
const SFTP_TEXT_10: f32 = 10.0; // Tauri text-[10px]
const SFTP_BG_ACTIVE_BG_ALPHA: u32 = 0x66; // [data-bg-active] --color-theme-bg 40%
const SFTP_BG_ACTIVE_PANEL_ALPHA: u32 = 0x66; // [data-bg-active] --color-theme-bg-panel 40%
const SFTP_BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // [data-bg-active] --color-theme-bg-hover 50%

#[derive(Clone, Copy)]
pub struct SftpUiMetrics {
    pub text_xs: f32,
    pub text_sm: f32,
    pub text_small: f32,
    scale: f32,
}

impl SftpUiMetrics {
    pub fn from_tokens(tokens: &ThemeTokens) -> Self {
        let scale = tokens.metrics.ui_text_xs / SFTP_TEXT_XS;
        Self {
            text_xs: tokens.metrics.ui_text_xs,
            text_sm: tokens.metrics.ui_text_sm,
            text_small: (SFTP_TEXT_10 * scale * 2.0).round() / 2.0,
            scale,
        }
    }

    pub fn size(self, base: f32) -> f32 {
        (base * self.scale * 2.0).round() / 2.0
    }
}

pub fn sftp_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, SFTP_BG_ACTIVE_BG_ALPHA)
}

pub fn sftp_panel_bg(color: u32, has_background: bool, alpha: u32) -> Rgba {
    color_with_background_scaled_alpha(color, has_background, alpha, SFTP_BG_ACTIVE_PANEL_ALPHA)
}

pub fn sftp_card_surface(surface: gpui::Div, color: u32) -> gpui::Div {
    // SFTP queue subcards mirror Tauri bg-theme-bg-card; the shadow belongs to
    // that card token even when the caller keeps a custom active-background alpha.
    tauri_glass_surface_shadow(surface, color)
}

pub fn sftp_hover_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, SFTP_BG_ACTIVE_HOVER_ALPHA)
}

pub fn sftp_border(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, 0x99)
}

const SFTP_BUTTON_TRANSPARENT_ALPHA: u32 = 0x00;
const SFTP_DESTRUCTIVE_BG_ALPHA: u32 = 0xe6;
const SFTP_DESTRUCTIVE_BORDER_ALPHA: u32 = 0xcc;
const SFTP_DESTRUCTIVE_TEXT: u32 = 0xffffff;

pub(crate) enum SftpButtonVariant {
    Default,
    Secondary,
    Ghost,
    Destructive,
}

pub(crate) fn render_sftp_button_variant(
    tokens: &ThemeTokens,
    label: String,
    variant: SftpButtonVariant,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let ui = SftpUiMetrics::from_tokens(tokens);
    let theme = tokens.ui;
    // Mirrors the Tauri Button variants used by SFTP dialogs and keeps
    // irreversible conflict actions visually distinct from the safe default.
    let (bg, border, text, hover_bg, hover_opacity) = match variant {
        SftpButtonVariant::Default => (
            rgb(theme.text),
            rgba((theme.text << 8) | SFTP_BUTTON_TRANSPARENT_ALPHA),
            rgb(theme.bg),
            rgb(theme.text),
            Some(0.9),
        ),
        SftpButtonVariant::Destructive => (
            rgba((theme.error << 8) | SFTP_DESTRUCTIVE_BG_ALPHA),
            rgba((theme.error << 8) | SFTP_DESTRUCTIVE_BORDER_ALPHA),
            rgb(SFTP_DESTRUCTIVE_TEXT),
            rgb(theme.error),
            Some(0.9),
        ),
        SftpButtonVariant::Secondary => (
            rgb(theme.bg_panel),
            rgb(theme.border),
            rgb(theme.text),
            rgb(theme.bg_hover),
            None,
        ),
        SftpButtonVariant::Ghost => (
            rgba((theme.bg << 8) | SFTP_BUTTON_TRANSPARENT_ALPHA),
            rgba((theme.border << 8) | SFTP_BUTTON_TRANSPARENT_ALPHA),
            rgb(theme.text),
            rgb(theme.bg_hover),
            None,
        ),
    };
    toolbar_action_button(
        tokens,
        label,
        None,
        ToolbarButtonOptions {
            background: Some(bg),
            border: Some(border),
            text_color: Some(text),
            hover_background: Some(hover_bg),
            hover_opacity,
            ..ToolbarButtonOptions::compact_text(
                ButtonVariant::Ghost,
                ButtonRadius::Md,
                ui.size(32.0),
                12.0,
                ui.text_xs,
            )
        },
        listener,
    )
    .into_any_element()
}
