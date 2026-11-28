use gpui::{
    Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Pixels, SharedString, TextSystem,
    Window, px,
};
use oxideterm_terminal_emulator::TerminalRow;
use std::sync::Arc;

pub const TERMINAL_FONT: &str = oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY;
pub const TERMINAL_FONT_SIZE: f32 = 14.0;
pub const TERMINAL_FONT_WEIGHT: f32 = 400.0;
pub const TERMINAL_CHARACTER_SPACING: f32 =
    oxideterm_settings::DEFAULT_TERMINAL_CHARACTER_SPACING as f32;
pub const TERMINAL_LINE_HEIGHT_RATIO: f32 = oxideterm_settings::DEFAULT_TERMINAL_LINE_HEIGHT as f32;
// User padding is applied outside the element; these coordinates are grid-local.
pub const TERMINAL_CONTENT_PADDING: f32 = 0.0;
pub const SCROLLBAR_WIDTH: f32 = 8.0;
pub const SCROLLBAR_ACTIVE_WIDTH: f32 = 12.0;
pub const SCROLLBAR_MIN_THUMB: f32 = 24.0;
pub const SEARCH_SCROLLBAR_MARKER_HEIGHT: f32 = 2.0;
pub const TERMINAL_TIMESTAMP_LABEL_CELLS: usize = 14;
pub const TERMINAL_TIMESTAMP_GUTTER_GAP_CELLS: f32 = 1.0;
pub const TERMINAL_FONT_LIGATURES: bool = false;

pub fn terminal_scrollbar_x_for_viewport(viewport_width: Pixels) -> Pixels {
    // Overlay the viewport edge independently of rounded PTY columns.
    px((f32::from(viewport_width) - TERMINAL_CONTENT_PADDING - SCROLLBAR_WIDTH).max(0.0))
}

#[derive(Clone)]
pub struct TerminalMetrics {
    pub font: Font,
    pub font_size: Pixels,
    pub cell_width: Pixels,
    // Keep platform shaping in sync with scaled `cell_width` so small multipliers
    // (for example 1.05x) do not hit `apply_force_width_to_layout`'s 1px snap threshold.
    pub letter_spacing: Option<Pixels>,
    pub line_height: Pixels,
    pub primary_ascent: Pixels,
    pub primary_descent: Pixels,
}

fn primary_font_vertical_metrics(
    text_system: &TextSystem,
    font_id: gpui::FontId,
    font_size: Pixels,
) -> (Pixels, Pixels) {
    let ascent = px(f32::from(text_system.ascent(font_id, font_size)).max(0.0));
    let descent = px(f32::from(text_system.descent(font_id, font_size)).abs());
    (ascent, descent)
}

/// Snaps terminal row height to an integer physical pixel step so every row shares the same
/// vertical subpixel phase for uniform glyph sharpness and `LinePaintCache` reuse across rows.
fn snap_line_height_to_device_pixel(line_height: Pixels, scale_factor: f32) -> Pixels {
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    px(((f32::from(line_height) * scale).round().max(1.0)) / scale)
}

/// Computes terminal line height by scaling the primary font's natural
/// `ascent + |descent|` box rather than bare `font_size`, matching macOS Terminal.
pub fn terminal_line_height_for_font(
    text_system: &TextSystem,
    font: &Font,
    font_size: Pixels,
    line_height_multiplier: f32,
) -> Pixels {
    let font_id = text_system.resolve_font(font);
    let (ascent, descent) = primary_font_vertical_metrics(text_system, font_id, font_size);
    let natural = f32::from(ascent + descent);
    let base = if natural > 0.0 {
        natural
    } else {
        f32::from(font_size)
    };
    px((base * line_height_multiplier).max(1.0))
}

fn sanitize_character_spacing(multiplier: f32) -> f32 {
    if multiplier.is_finite() {
        multiplier.clamp(
            oxideterm_settings::MIN_TERMINAL_CHARACTER_SPACING as f32,
            oxideterm_settings::MAX_TERMINAL_CHARACTER_SPACING as f32,
        )
    } else {
        TERMINAL_CHARACTER_SPACING
    }
}

fn base_cell_width_for_font(text_system: &TextSystem, font: &Font, font_size: Pixels) -> Pixels {
    let font_id = text_system.resolve_font(font);
    text_system
        .advance(font_id, font_size, 'm')
        .map(|advance| advance.width)
        .unwrap_or_else(|_| text_system.layout_width(font_id, font_size, 'm'))
        .max(px(1.0))
}

fn scaled_cell_width_and_letter_spacing(
    base_width: Pixels,
    character_spacing: f32,
) -> (Pixels, Option<Pixels>) {
    let spacing = sanitize_character_spacing(character_spacing);
    let effective_scale =
        spacing * (oxideterm_settings::TERMINAL_CHARACTER_SPACING_BASELINE_SCALE as f32);
    let cell_width = (base_width * effective_scale).max(px(1.0));
    let delta = cell_width - base_width;
    let letter_spacing = (delta.as_f32().abs() > 0.001).then_some(delta);
    (cell_width, letter_spacing)
}

pub fn terminal_letter_spacing_for_font(
    text_system: &TextSystem,
    font: &Font,
    font_size: Pixels,
    character_spacing: f32,
) -> Option<Pixels> {
    let base_width = base_cell_width_for_font(text_system, font, font_size);
    scaled_cell_width_and_letter_spacing(base_width, character_spacing).1
}

impl TerminalMetrics {
    pub fn measure(
        window: &Window,
        font: Font,
        font_size: Pixels,
        line_height_multiplier: f32,
        character_spacing: f32,
    ) -> Self {
        let font_id = window.text_system().resolve_font(&font);
        let (primary_ascent, primary_descent) =
            primary_font_vertical_metrics(window.text_system(), font_id, font_size);
        let line_height = snap_line_height_to_device_pixel(
            terminal_line_height_for_font(
                window.text_system(),
                &font,
                font_size,
                line_height_multiplier,
            ),
            window.scale_factor(),
        );
        let base_width = base_cell_width_for_font(window.text_system(), &font, font_size);
        let (cell_width, letter_spacing) =
            scaled_cell_width_and_letter_spacing(base_width, character_spacing);

        Self {
            font,
            font_size,
            cell_width,
            letter_spacing,
            line_height,
            primary_ascent,
            primary_descent,
        }
    }

    pub fn cell_width_f32(&self) -> f32 {
        f32::from(self.cell_width)
    }

    pub fn line_height_f32(&self) -> f32 {
        f32::from(self.line_height)
    }
}

pub fn terminal_timestamp_gutter_width(metrics: &TerminalMetrics, enabled: bool) -> f32 {
    if enabled {
        (TERMINAL_TIMESTAMP_LABEL_CELLS as f32 + TERMINAL_TIMESTAMP_GUTTER_GAP_CELLS)
            * metrics.cell_width_f32()
    } else {
        0.0
    }
}

/// Emulator-owned identity is stable across scrolling, eviction and local row movement.
pub fn terminal_row_timestamp_identity(row: &TerminalRow) -> u64 {
    row.source_id as u64
}

pub struct ResolvedTerminalFont {
    pub font: Font,
    pub unavailable_families: Vec<String>,
}

/// The portable host registers Lilex at startup. Use it after any configured families.
pub fn resolve_portable_terminal_font(
    text_system: &TextSystem,
    term: &oxideterm_settings::TerminalSettings,
) -> ResolvedTerminalFont {
    let configured = term
        .font_family
        .terminal_family_name(&term.custom_font_family);
    resolve_terminal_font(
        text_system,
        &format!("{configured}, Lilex"),
        Some(&term.cjk_font_family),
        term.font_ligatures,
        term.font_weight as f32,
    )
}

/// Resolves the primary family before either measuring or shaping terminal cells.
pub fn resolve_terminal_font(
    text_system: &TextSystem,
    family: &str,
    cjk_family: Option<&str>,
    font_ligatures: bool,
    font_weight: f32,
) -> ResolvedTerminalFont {
    let mut font =
        terminal_font_with_family_and_cjk(family, cjk_family, font_ligatures, font_weight);
    let configured = oxideterm_gpui_ui::css_font_family_stack(family);
    let available = |family: &str| {
        // A glyph fallback must not disguise an unavailable configured family.
        text_system
            .font_id(&Font {
                family: terminal_font_family_name(family),
                fallbacks: None,
                ..font.clone()
            })
            .is_ok()
    };
    let mut unavailable_families = Vec::new();
    for name in configured.iter().map(|name| name.as_ref()) {
        let name = name.trim();
        if !name.is_empty() && !available(name) {
            push_font_fallback(&mut unavailable_families, name);
        }
    }
    let primary = configured
        .iter()
        .find(|family| available(family))
        .map(|family| terminal_font_family_name(family))
        .unwrap_or_else(|| TERMINAL_FONT.into());
    // Keep glyph fallbacks intact: CJK and symbol-only fonts need not be usable as a primary.
    font.family = primary;
    ResolvedTerminalFont {
        font,
        unavailable_families,
    }
}

fn terminal_font_family_name(family: &str) -> SharedString {
    if family.eq_ignore_ascii_case("monospace") || family.eq_ignore_ascii_case("ui-monospace") {
        TERMINAL_FONT.into()
    } else {
        oxideterm_gpui_ui::gpui_font_family_name(family)
    }
}

pub fn terminal_font_with_family_and_cjk(
    family: &str,
    cjk_family: Option<&str>,
    font_ligatures: bool,
    font_weight: f32,
) -> Font {
    let configured_families = oxideterm_gpui_ui::css_font_family_stack(family);
    let primary_family = configured_families
        .first()
        .cloned()
        .unwrap_or_else(|| SharedString::from(TERMINAL_FONT));
    let mut fallback_families = Vec::new();
    // Preserve the user-defined stack before appending built-in recovery fonts.
    for configured_fallback in configured_families.iter().skip(1) {
        push_font_fallback(&mut fallback_families, configured_fallback);
    }
    // A bundled Latin monospace must precede optional CJK and system fallbacks.
    push_font_fallback(
        &mut fallback_families,
        oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY,
    );
    if let Some(cjk_family) = cjk_family {
        push_font_fallback(&mut fallback_families, cjk_family);
    }
    if cjk_family.is_none_or(|family| {
        family.trim().is_empty() || family.trim() == oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY
    }) {
        // The large bundled CJK fallback is available only for Auto or an explicit Maple choice.
        push_font_fallback(
            &mut fallback_families,
            oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY,
        );
        push_font_fallback(&mut fallback_families, "Maple Mono NF CN");
    }
    for fallback in [
        "JetBrainsMono Nerd Font",
        "JetBrains Mono NF (Subset)",
        "JetBrains Mono",
        "JetBrainsMonoNL Nerd Font Mono",
        oxideterm_settings::MESLO_SUBSET_FAMILY,
        "MesloLGS Nerd Font Mono",
        "Symbols Nerd Font Mono",
        "Symbols Nerd Font",
        "ui-monospace",
        "SF Mono",
        "Menlo",
        "Monaco",
        "Cascadia Mono",
        "DejaVu Sans Mono",
        "Noto Sans Mono",
        "Liberation Mono",
        "Courier New",
        // Terminal output often contains emoji such as ℹ️ / 🆄. Without the
        // platform emoji font they fall back to arbitrary symbol fonts, whose box
        // size and baseline do not line up with the surrounding text.
        "Segoe UI Emoji",
        "Apple Color Emoji",
        "Noto Color Emoji",
    ] {
        push_font_fallback(&mut fallback_families, fallback);
    }

    Font {
        family: primary_family,
        features: terminal_font_features(font_ligatures),
        fallbacks: Some(FontFallbacks::from_fonts(fallback_families)),
        weight: FontWeight(font_weight.clamp(100.0, 900.0)),
        style: FontStyle::Normal,
    }
}

fn push_font_fallback(fallbacks: &mut Vec<String>, family: &str) {
    let family = family.trim();
    if family.is_empty() || fallbacks.iter().any(|existing| existing == family) {
        return;
    }
    fallbacks.push(family.to_string());
}

pub fn terminal_font_features(font_ligatures: bool) -> FontFeatures {
    if font_ligatures {
        // GPUI enables the font's default OpenType features when no override is supplied.
        FontFeatures::default()
    } else {
        // Standard Latin ligatures must follow the same switch as contextual
        // programming ligatures; required script shaping remains enabled.
        FontFeatures(Arc::new(
            ["liga", "clig", "calt"]
                .map(|tag| (tag.to_owned(), 0))
                .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabling_terminal_ligatures_covers_standard_and_contextual_features() {
        let features = terminal_font_features(false);
        for tag in ["calt", "liga", "clig"] {
            assert!(
                features
                    .tag_value_list()
                    .iter()
                    .any(|(name, value)| name == tag && *value == 0),
                "{tag} must be disabled"
            );
        }
        assert!(terminal_font_features(true).tag_value_list().is_empty());
    }

    #[test]
    fn custom_font_stack_uses_first_family_and_preserves_fallback_order() {
        let font = terminal_font_with_family_and_cjk(
            "\"Maple Mono\",\"Maple Mono NF CN\"",
            None,
            false,
            400.0,
        );
        let fallbacks = font.fallbacks.expect("terminal font fallbacks");

        assert_eq!(font.family.as_ref(), "Maple Mono");
        assert_eq!(
            fallbacks.fallback_list().first().map(String::as_str),
            Some("Maple Mono NF CN")
        );
    }

    #[test]
    fn character_spacing_scales_cell_width_and_derives_matching_letter_spacing() {
        let base_width = px(10.0);
        let assert_scaled =
            |input: f32, expected_cell: f32, expected_letter_spacing: Option<f32>| {
                let (cell_width, letter_spacing) =
                    scaled_cell_width_and_letter_spacing(base_width, input);
                assert!(
                    (f32::from(cell_width) - expected_cell).abs() < 1e-4,
                    "input {input}: cell_width={cell_width:?}, expected {expected_cell}"
                );
                match (letter_spacing, expected_letter_spacing) {
                    (Some(actual), Some(expected)) => assert!(
                        (f32::from(actual) - expected).abs() < 1e-4,
                        "input {input}: letter_spacing={actual:?}, expected {expected}"
                    ),
                    (None, None) => {}
                    (actual, expected) => {
                        panic!("input {input}: letter_spacing={actual:?}, expected={expected:?}")
                    }
                }
            };

        assert_scaled(1.0, 9.3, Some(-0.7));
        assert_scaled(1.1, 10.23, Some(0.23));
        assert_scaled(0.85, 7.905, Some(-2.095));
        assert_scaled(5.0, 18.6, Some(8.6));
        assert_scaled(f32::NAN, 9.3, Some(-0.7));
    }

    #[test]
    fn line_height_snaps_to_integer_device_pixels_per_scale_factor() {
        assert_eq!(snap_line_height_to_device_pixel(px(16.632), 2.0), px(16.5));
        assert_eq!(snap_line_height_to_device_pixel(px(16.632), 1.0), px(17.0));
        assert_eq!(
            snap_line_height_to_device_pixel(px(16.632), 1.5),
            px(25.0 / 1.5)
        );
        assert_eq!(snap_line_height_to_device_pixel(px(0.1), 2.0), px(0.5));
        assert_eq!(
            snap_line_height_to_device_pixel(px(16.632), f32::NAN),
            px(17.0)
        );
    }
}
