// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Style helpers that map OxideTerm theme tokens to GPUI text / element styling.

use gpui::{AbsoluteLength, Font, FontStyle, FontWeight, Hsla, Rgba, SharedString};
use oxideterm_theme::ThemeTokens;

use crate::options::MarkdownOptions;

const BACKGROUND_SURFACE_CODE_ALPHA: f32 = 0.4; // Match the application-wide image-background surface opacity.
pub(crate) const BODY_LINE_HEIGHT: f32 = 22.0 / 14.0;

/// Returns the vertical spacing before `current` given the preceding sibling block.
///
/// Spacing is owned by each block's top padding (with zero container flex gap) so
/// virtualized layout measurements, navigation offsets, and visual rhythm stay
/// identical while allowing tighter heading-to-body and lead-in-to-list pairs.
pub(crate) fn block_spacing_before(
    prev: Option<&crate::model::Block>,
    current: &crate::model::Block,
    opts: &MarkdownOptions,
) -> f32 {
    use crate::model::Block;

    let Some(prev) = prev.map(Block::unlocated) else {
        return 0.0;
    };
    let current = current.unlocated();
    let g = opts.block_gap;

    let is_heavy_block = |block: &Block| {
        matches!(
            block,
            Block::CodeBlock { .. }
                | Block::Table { .. }
                | Block::Blockquote { .. }
                | Block::Details { .. }
        )
    };

    if let Block::Heading { level, .. } = current {
        return match prev {
            Block::Heading { .. } => g * 0.45,
            Block::HorizontalRule => g * 0.5,
            _ => match level {
                1 => g * 1.85,
                2 => g * 1.55,
                3 => g * 1.2,
                _ => g * 0.95,
            },
        };
    }

    if let Block::Heading { level, .. } = prev {
        return match level {
            1 => g * 0.7,
            2 => g * 0.55,
            3 => g * 0.4,
            _ => g * 0.3,
        };
    }

    if matches!(current, Block::HorizontalRule) || matches!(prev, Block::HorizontalRule) {
        return g * 0.75;
    }

    if matches!(prev, Block::Paragraph { .. })
        && matches!(
            current,
            Block::UnorderedList { .. } | Block::OrderedList { .. }
        )
    {
        return g * 0.45;
    }

    if is_heavy_block(prev) || is_heavy_block(current) {
        return g * 0.9;
    }

    g * 0.7
}

// ── colour helpers ──────────────────────────────────────────────────────

/// Convert a `0xRRGGBB` hex value to a GPUI `Hsla` colour at full opacity.
pub fn hex_to_hsla(hex: u32) -> Hsla {
    let r = ((hex >> 16) & 0xff) as f32 / 255.0;
    let g = ((hex >> 8) & 0xff) as f32 / 255.0;
    let b = (hex & 0xff) as f32 / 255.0;
    Rgba { r, g, b, a: 1.0 }.into()
}

/// Returns true when a `0xRRGGBB` background colour is perceptually light.
pub fn is_light_surface(hex: u32) -> bool {
    let r = ((hex >> 16) & 0xff) as f32 / 255.0;
    let g = ((hex >> 8) & 0xff) as f32 / 255.0;
    let b = (hex & 0xff) as f32 / 255.0;
    (0.299 * r + 0.587 * g + 0.114 * b) > 0.55
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

pub fn accent_text_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.accent_text)
}

pub fn code_bg_color(tokens: &ThemeTokens, opts: &MarkdownOptions) -> Hsla {
    background_surface_color(tokens.ui.bg_elevated, opts)
}

pub fn code_block_bg_color(tokens: &ThemeTokens, opts: &MarkdownOptions) -> Hsla {
    background_surface_color(tokens.ui.bg_panel, opts)
}

pub fn code_block_border_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.border);
    c.a = 0.45;
    c
}

pub fn code_block_header_bg_color(tokens: &ThemeTokens, opts: &MarkdownOptions) -> Hsla {
    let mut c = background_surface_color(tokens.ui.bg_elevated, opts);
    if !opts.background_surface_active {
        c.a = 0.65;
    }
    c
}

fn background_surface_color(color: u32, opts: &MarkdownOptions) -> Hsla {
    let mut color = hex_to_hsla(color);
    if opts.background_surface_active {
        color.a = BACKGROUND_SURFACE_CODE_ALPHA;
    }
    color
}

pub fn code_block_header_border_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.border);
    c.a = 0.35;
    c
}

pub fn code_action_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.text_muted);
    c.a = 0.8;
    c
}

pub fn code_action_hover_bg(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.bg_hover);
    c.a = 0.75;
    c
}

pub fn divider_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.divider);
    c.a = 0.7;
    c
}

pub fn heading_divider_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.divider);
    c.a = 0.6;
    c
}

pub fn bg_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.bg)
}

/// Left-border colour for blockquotes — muted text at reduced opacity.
pub fn blockquote_border_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.text_muted);
    c.a = 0.55;
    c
}

/// Subtle tinted background for plain blockquotes and semantic GitHub callouts.
pub fn blockquote_bg_color(
    is_callout: bool,
    accent: Hsla,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> Hsla {
    if is_callout {
        let mut c = accent;
        c.a = if is_light_surface(tokens.ui.bg) {
            0.07
        } else {
            0.10
        };
        return c;
    }
    let mut c = background_surface_color(tokens.ui.bg_elevated, opts);
    if !opts.background_surface_active {
        c.a = 0.4;
    }
    c
}

/// Faint outer border colour for semantic callout cards.
pub fn callout_outer_border_color(accent: Hsla) -> Hsla {
    let mut c = accent;
    c.a = 0.22;
    c
}

/// Background colour for table header cells.
pub fn table_header_bg(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.bg_elevated);
    c.a = 0.85;
    c
}

/// Soft alternating row tint for multi-row tables.
pub fn table_zebra_bg(tokens: &ThemeTokens, opts: &MarkdownOptions) -> Hsla {
    let mut c = background_surface_color(tokens.ui.bg_elevated, opts);
    if !opts.background_surface_active {
        c.a = 0.28;
    }
    c
}

/// Subtle hover highlight for table body rows.
pub fn table_row_hover_bg(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.bg_hover);
    c.a = 0.45;
    c
}

/// Border colour used for table outline and horizontal row dividers.
pub fn table_border_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.border);
    c.a = 0.5;
    c
}

/// Ultra-subtle vertical divider between adjacent table columns.
pub fn table_cell_divider_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.border);
    c.a = 0.22;
    c
}

/// Background colour for inline code spans (semantically separate from code blocks).
pub fn inline_code_bg_color(tokens: &ThemeTokens, opts: &MarkdownOptions) -> Hsla {
    let mut c = background_surface_color(tokens.ui.bg_elevated, opts);
    if !opts.background_surface_active {
        c.a = 0.85;
    }
    c
}

/// Foreground colour for inline code spans with crisp contrast against `inline_code_bg_color`.
pub fn inline_code_text_color(tokens: &ThemeTokens) -> Hsla {
    hex_to_hsla(tokens.ui.text_heading)
}

/// Soft link underline colour that keeps dense link lists visually calm.
pub fn link_underline_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = accent_color(tokens);
    c.a = 0.45;
    c
}

/// Border colour for unchecked task list checkboxes.
pub fn checkbox_border_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.text_muted);
    c.a = 0.65;
    c
}

/// Softened text colour for completed task list items.
pub fn checked_item_text_color(tokens: &ThemeTokens) -> Hsla {
    let mut c = hex_to_hsla(tokens.ui.text_muted);
    c.a = 0.85;
    c
}

/// Theme-aware background used by safe HTML `<mark>` content.
pub fn highlight_bg_color(tokens: &ThemeTokens) -> Hsla {
    let mut color = accent_color(tokens);
    color.a = 0.22;
    color
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
        weight: FontWeight::SEMIBOLD,
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
