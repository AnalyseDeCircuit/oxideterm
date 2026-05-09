// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_theme::ThemeTokens;

/// Theme-derived paint values for the GPUI editor surface.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorAppearance {
    pub background_hex: u32,
    pub gutter_background_hex: u32,
    pub current_line_hex: u32,
    pub text_hex: u32,
    pub muted_text_hex: u32,
    pub border_hex: u32,
    pub accent_hex: u32,
    pub selection_hex: u32,
    pub syntax_attribute_hex: u32,
    pub syntax_comment_hex: u32,
    pub syntax_constant_hex: u32,
    pub syntax_function_hex: u32,
    pub syntax_keyword_hex: u32,
    pub syntax_number_hex: u32,
    pub syntax_string_hex: u32,
    pub syntax_type_hex: u32,
    pub syntax_variable_hex: u32,
    pub font_family: String,
}

impl EditorAppearance {
    pub fn from_theme(tokens: &ThemeTokens) -> Self {
        Self {
            background_hex: tokens.ui.bg,
            gutter_background_hex: tokens.ui.bg_panel,
            current_line_hex: tokens.ui.bg_active,
            text_hex: tokens.ui.text,
            muted_text_hex: tokens.ui.text_muted,
            border_hex: tokens.ui.border,
            accent_hex: tokens.ui.accent,
            selection_hex: tokens.ui.bg_hover,
            syntax_attribute_hex: tokens.terminal.bright_magenta,
            syntax_comment_hex: tokens.terminal.bright_black,
            syntax_constant_hex: tokens.terminal.cyan,
            syntax_function_hex: tokens.terminal.blue,
            syntax_keyword_hex: tokens.terminal.magenta,
            syntax_number_hex: tokens.terminal.yellow,
            syntax_string_hex: tokens.terminal.green,
            syntax_type_hex: tokens.terminal.bright_yellow,
            syntax_variable_hex: tokens.ui.text,
            font_family: tokens.metrics.markdown_code_font_family.to_string(),
        }
    }
}

/// Editor layout metrics sourced from theme tokens instead of ad-hoc constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorMetrics {
    pub font_size: f32,
    pub line_height: f32,
    pub char_width: f32,
    pub gutter_width: f32,
    pub gutter_padding_x: f32,
    pub content_padding_x: f32,
    pub overscan_rows: usize,
}

impl EditorMetrics {
    pub fn from_theme(tokens: &ThemeTokens) -> Self {
        let font_size =
            tokens.metrics.markdown_body_font_size * tokens.metrics.markdown_code_font_scale;
        Self {
            font_size,
            line_height: font_size * 1.5,
            char_width: font_size * 0.62,
            gutter_width: tokens.metrics.ui_control_height * 1.8,
            gutter_padding_x: tokens.spacing.two,
            content_padding_x: tokens.spacing.two,
            overscan_rows: 4,
        }
    }
}
