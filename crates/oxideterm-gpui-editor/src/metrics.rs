// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{
    Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, SharedString, TextRun, Window, px,
    rgb,
};
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
        Self::from_theme_with_editor_typography(tokens, font_size, 1.5)
    }

    pub fn from_theme_with_editor_typography(
        tokens: &ThemeTokens,
        font_size: f32,
        line_height: f32,
    ) -> Self {
        let font_size = font_size.clamp(8.0, 32.0);
        // Tauri CodeMirror uses CSS `line-height` as a multiplier. Native keeps
        // the same persisted IDE setting shape and converts it to pixels here.
        let line_height = line_height.clamp(0.8, 3.0);
        Self {
            font_size,
            line_height: font_size * line_height,
            char_width: font_size * 0.62,
            gutter_width: tokens.metrics.ui_control_height * 1.8,
            gutter_padding_x: tokens.spacing.two,
            content_padding_x: tokens.spacing.two,
            overscan_rows: 4,
        }
    }

    pub fn measure_code_cell_width(&mut self, window: &mut Window, font_family: &str) -> bool {
        let font = editor_code_font(font_family);
        let font_size = px(self.font_size);
        let font_id = window.text_system().resolve_font(&font);
        let measured = window
            .text_system()
            .advance(font_id, font_size, 'm')
            .map(|advance| advance.width)
            .unwrap_or_else(|_| fallback_code_cell_width(window, &font, font_size));
        let measured = f32::from(measured.max(px(1.0)));
        if (self.char_width - measured).abs() <= 0.01 {
            return false;
        }
        self.char_width = measured;
        true
    }
}

fn editor_code_font(family: &str) -> Font {
    Font {
        family: SharedString::from(family.to_string()),
        features: FontFeatures::disable_ligatures(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "JetBrainsMono Nerd Font Mono".to_string(),
            "JetBrains Mono NF (Subset)".to_string(),
            "JetBrains Mono".to_string(),
            "SF Mono".to_string(),
            "Menlo".to_string(),
            "Monaco".to_string(),
            "Cascadia Mono".to_string(),
            "DejaVu Sans Mono".to_string(),
            "Noto Sans Mono".to_string(),
            "Liberation Mono".to_string(),
            "Courier New".to_string(),
        ])),
        weight: FontWeight::default(),
        style: FontStyle::Normal,
    }
}

fn fallback_code_cell_width(
    window: &mut Window,
    font: &Font,
    font_size: gpui::Pixels,
) -> gpui::Pixels {
    let sample = SharedString::from("m");
    let run = TextRun {
        len: sample.len(),
        font: font.clone(),
        color: rgb(0xe6e8eb).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(sample, font_size, &[run], None)
        .width
}
