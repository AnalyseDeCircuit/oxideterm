// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Style helpers that map OxideTerm theme tokens to GPUI text / element styling.

use gpui::{AbsoluteLength, Font, FontStyle, FontWeight, Hsla, Rgba, SharedString};
use oxideterm_theme::ThemeTokens;

use crate::options::MarkdownOptions;

// ── colour helpers ──────────────────────────────────────────────────────

/// Convert a `0xRRGGBB` hex value to a GPUI `Hsla` colour at full opacity.
pub fn hex_to_hsla(hex: u32) -> Hsla {
    let r = ((hex >> 16) & 0xff) as f32 / 255.0;
    let g = ((hex >> 8) & 0xff) as f32 / 255.0;
    let b = (hex & 0xff) as f32 / 255.0;
    Rgba { r, g, b, a: 1.0 }.into()
}

// ── token-derived style values ──────────────────────────────────────────

pub fn text_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.text)
}

pub fn heading_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.text_heading)
}

pub fn muted_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.text_muted)
}

pub fn accent_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.accent)
}

pub fn code_bg_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.bg_elevated)
}

pub fn code_block_bg_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.bg_panel)
}

pub fn code_block_border_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.border);
    c.a = 0.2;
    c
}

pub fn code_block_header_bg_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.bg_panel)
}

pub fn code_block_header_border_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.border);
    c.a = 0.05;
    c
}

pub fn code_action_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.text_muted);
    c.a = 0.5;
    c
}

pub fn divider_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.divider)
}

pub fn bg_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.bg)
}

/// Left-border colour for blockquotes — muted text at reduced opacity.
pub fn blockquote_border_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.text_muted);
    c.a = 0.5;
    c
}

/// Background colour for table header cells.
pub fn table_header_bg(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.bg_elevated)
}

/// Border colour used between table rows.
pub fn table_border_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.border)
}

/// Background colour for inline code spans (semantically separate from code blocks).
pub fn inline_code_bg_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.bg_elevated)
}

// ── fonts ───────────────────────────────────────────────────────────────

pub fn body_font(opts: &MarkdownOptions) -> Font {
    Font {
        family: SharedString::from(opts.body_font_family.clone()),
        features: Default::default(),
        fallbacks: Default::default(),
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

pub fn bold_font(opts: &MarkdownOptions) -> Font {
    Font {
        weight: FontWeight::BOLD,
        ..body_font(opts)
    }
}

pub fn italic_font(opts: &MarkdownOptions) -> Font {
    Font {
        style: FontStyle::Italic,
        ..body_font(opts)
    }
}

pub fn code_font(opts: &MarkdownOptions) -> Font {
    Font {
        family: SharedString::from(opts.code_font_family.clone()),
        features: Default::default(),
        fallbacks: Default::default(),
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

pub fn heading_font(opts: &MarkdownOptions) -> Font {
    Font {
        weight: FontWeight::BOLD,
        ..body_font(opts)
    }
}

// ── sizing ──────────────────────────────────────────────────────────────

/// Font size for a heading level (1 = largest, 6 = smallest).
pub fn heading_font_size(level: u8, opts: &MarkdownOptions) -> AbsoluteLength {
    let scale = opts
        .heading_font_scales
        .get(level.saturating_sub(1) as usize)
        .copied()
        .unwrap_or_else(|| *opts.heading_font_scales.last().unwrap_or(&1.0));
    AbsoluteLength::Pixels(gpui::px(opts.base_font_size * scale))
}

pub fn body_font_size(opts: &MarkdownOptions) -> AbsoluteLength {
    AbsoluteLength::Pixels(gpui::px(opts.base_font_size))
}

pub fn code_font_size(opts: &MarkdownOptions) -> AbsoluteLength {
    AbsoluteLength::Pixels(gpui::px(opts.base_font_size * opts.code_font_scale))
}

pub fn code_label_font_size(opts: &MarkdownOptions) -> AbsoluteLength {
    AbsoluteLength::Pixels(gpui::px(
        opts.base_font_size * opts.code_font_scale * opts.code_label_font_scale,
    ))
}

pub fn footnote_font_size(opts: &MarkdownOptions) -> AbsoluteLength {
    AbsoluteLength::Pixels(gpui::px(opts.base_font_size * opts.footnote_font_scale))
}
