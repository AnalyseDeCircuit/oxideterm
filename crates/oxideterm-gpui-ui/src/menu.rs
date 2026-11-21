use gpui::{
    CursorStyle, Div, IntoElement, ParentElement, Rgba, Styled, div, prelude::*, px, rgb, svg,
};
use oxideterm_theme::ThemeTokens;

use crate::surface::{
    MaterialRole, SurfaceKind, SurfaceOptions, color_with_alpha, material_surface, surface_chrome,
    theme_overlay_surface_shadow,
};

const MENU_FROSTED_BG_ALPHA: u32 = 0xf0;
const MENU_HOVER_BG_ALPHA: u32 = 0xd9;
const MENU_DANGER_HOVER_BG_ALPHA: u32 = 0x1f;
const MENU_SEPARATOR_ALPHA: u32 = 0x66;
const MENU_RADIO_DOT_SIZE: f32 = 6.0;
const MENU_ITEM_ICON_ALPHA: u32 = 0xd9;
const MENU_SHORTCUT_ALPHA: u32 = 0xcc;
const MENU_CHECK_ICON_PATH: &str = "lucide/check.svg";
const MENU_CHEVRON_RIGHT_PATH: &str = "lucide/chevron-right.svg";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MenuChromeSpec {
    pub surface_radius: f32,
    pub item_radius: f32,
}

pub(crate) fn menu_chrome_spec(tokens: &ThemeTokens) -> MenuChromeSpec {
    MenuChromeSpec {
        surface_radius: tokens.radii.lg,
        item_radius: tokens.radii.md,
    }
}

pub(crate) fn menu_surface_chrome(tokens: &ThemeTokens) -> Div {
    let chrome = menu_chrome_spec(tokens);
    let elevation = surface_chrome(tokens, SurfaceOptions::new(SurfaceKind::ElevatedPopover));
    // All floating menu variants share one elevation, UI font, and corner contract.
    let surface = material_surface(tokens, div(), MaterialRole::Popover)
        .when(crate::modal::backdrop_blur_allowed(), |surface| {
            // Keep dense terminal/table text beneath menus from muddying item labels.
            surface.bg(color_with_alpha(
                tokens.ui.bg_elevated,
                MENU_FROSTED_BG_ALPHA,
            ))
        })
        .overflow_hidden()
        .rounded(px(chrome.surface_radius))
        .border_1()
        .border_color(elevation.border)
        .font_family(crate::tauri_ui_font_family(tokens.metrics.font_family))
        .text_color(rgb(tokens.ui.text));
    theme_overlay_surface_shadow(surface, tokens)
}

pub(crate) fn menu_item_hover_bg(tokens: &ThemeTokens, danger: bool) -> Rgba {
    if danger {
        color_with_alpha(tokens.ui.error, MENU_DANGER_HOVER_BG_ALPHA)
    } else {
        color_with_alpha(tokens.ui.bg_hover, MENU_HOVER_BG_ALPHA)
    }
}

pub(crate) fn menu_item_chrome(tokens: &ThemeTokens, left_padding: f32, right_padding: f32) -> Div {
    let chrome = menu_chrome_spec(tokens);
    // Structural item chrome stays shared while each primitive owns its state.
    div()
        .relative()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .rounded(px(chrome.item_radius))
        .pl(px(left_padding))
        .pr(px(right_padding))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text))
}

pub(crate) fn menu_indicator_icon(tokens: &ThemeTokens, checked: bool, radio: bool) -> Div {
    let mut slot = div()
        .absolute()
        .left(px(tokens.metrics.ui_menu_item_padding_x))
        .size(px(tokens.metrics.ui_menu_indicator_size))
        .flex()
        .items_center()
        .justify_center();
    if checked {
        slot = if radio {
            slot.child(
                div()
                    .size(px(MENU_RADIO_DOT_SIZE))
                    .rounded_full()
                    .bg(rgb(tokens.ui.accent)),
            )
        } else {
            slot.child(
                svg()
                    .path(MENU_CHECK_ICON_PATH)
                    .size(px(tokens.metrics.ui_menu_indicator_size - 1.0))
                    .text_color(rgb(tokens.ui.accent)),
            )
        };
    }
    slot
}

pub(crate) fn menu_submenu_chevron(tokens: &ThemeTokens) -> Div {
    div()
        .ml_auto()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            svg()
                .path(MENU_CHEVRON_RIGHT_PATH)
                .size(px(tokens.metrics.ui_menu_indicator_size))
                .text_color(rgb(tokens.ui.text_muted)),
        )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuItemKind {
    Plain,
    Checkbox(bool),
    Radio(bool),
    Submenu,
}

pub fn menu_content(tokens: &ThemeTokens) -> Div {
    menu_surface_chrome(tokens)
        .min_w(px(tokens.metrics.ui_menu_min_width))
        .p(px(tokens.metrics.ui_menu_padding))
}

pub fn menu_item(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    kind: MenuItemKind,
    inset: bool,
    disabled: bool,
) -> Div {
    let left_padding =
        if inset || matches!(kind, MenuItemKind::Checkbox(_) | MenuItemKind::Radio(_)) {
            tokens.metrics.ui_menu_inset_padding_left
        } else {
            tokens.metrics.ui_menu_item_padding_x
        };
    let checked = matches!(
        kind,
        MenuItemKind::Checkbox(true) | MenuItemKind::Radio(true)
    );
    let hover_bg = menu_item_hover_bg(tokens, false);
    menu_item_chrome(tokens, left_padding, tokens.metrics.ui_menu_item_padding_x)
        .opacity(if disabled { 0.5 } else { 1.0 })
        // Browser/Radix menu items expose disabled state as both visual
        // opacity and a non-clickable cursor; callers still decide whether to
        // attach handlers, but the primitive should not imply activation.
        .cursor(if disabled {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        })
        .when(!disabled, |item| {
            item.hover(move |style| style.bg(hover_bg))
        })
        .when(checked, |item| {
            item.child(menu_indicator_icon(
                tokens,
                true,
                matches!(kind, MenuItemKind::Radio(true)),
            ))
        })
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .when(kind == MenuItemKind::Submenu, |item| {
            item.child(menu_submenu_chevron(tokens))
        })
}

pub fn menu_item_with_icon(
    tokens: &ThemeTokens,
    icon_path: &'static str,
    label: impl Into<String>,
    danger: bool,
    disabled: bool,
) -> Div {
    let text_color = if danger {
        tokens.ui.error
    } else {
        tokens.ui.text
    };
    let hover_bg = menu_item_hover_bg(tokens, danger);
    menu_item_chrome(
        tokens,
        tokens.metrics.ui_menu_item_padding_x,
        tokens.metrics.ui_menu_item_padding_x,
    )
    .text_color(rgb(text_color))
    .opacity(if disabled { 0.5 } else { 1.0 })
    .cursor(if disabled {
        CursorStyle::OperationNotAllowed
    } else {
        CursorStyle::PointingHand
    })
    .when(!disabled, |item| {
        item.hover(move |style| style.bg(hover_bg))
    })
    .child(
        svg()
            .path(icon_path)
            .size(px(tokens.metrics.ui_menu_indicator_size))
            .flex_none()
            .text_color(if danger {
                rgb(text_color).into()
            } else {
                color_with_alpha(text_color, MENU_ITEM_ICON_ALPHA)
            }),
    )
    .child(div().flex_1().min_w_0().truncate().child(label.into()))
}

pub fn menu_label(tokens: &ThemeTokens, label: impl Into<String>, inset: bool) -> Div {
    div()
        .px(px(tokens.metrics.ui_menu_item_padding_x))
        .pl(px(if inset {
            tokens.metrics.ui_menu_inset_padding_left
        } else {
            tokens.metrics.ui_menu_item_padding_x
        }))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .text_size(px(tokens.metrics.ui_text_2xs))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(tokens.ui.text_muted))
        .child(label.into())
}

pub fn menu_separator(tokens: &ThemeTokens) -> Div {
    div()
        .mx(px(tokens.metrics.ui_menu_item_padding_x * 0.5))
        .my(px(tokens.metrics.ui_menu_padding))
        .h(px(1.0))
        .bg(color_with_alpha(tokens.ui.border, MENU_SEPARATOR_ALPHA))
}

pub fn menu_shortcut(tokens: &ThemeTokens, shortcut: impl Into<String>) -> Div {
    div()
        .ml_auto()
        .pl(px(tokens.spacing.three))
        .flex_none()
        .text_size(px(tokens.metrics.ui_text_2xs))
        .text_color(color_with_alpha(tokens.ui.text_muted, MENU_SHORTCUT_ALPHA))
        .child(shortcut.into())
}

pub fn menu_item_with_shortcut(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    shortcut: impl IntoElement,
) -> Div {
    menu_item(tokens, label, MenuItemKind::Plain, false, false).child(shortcut)
}

/// Estimates the horizontal advance width in pixels of single-line UI text at
/// `font_size_px`, distinguishing wide East Asian glyphs from Latin glyphs.
pub fn estimate_text_width(text: &str, font_size_px: f32) -> f32 {
    let mut units = 0.0_f32;
    for ch in text.chars() {
        units += if is_wide_glyph(ch) {
            1.02
        } else if matches!(
            ch,
            'i' | 'l' | 'I' | '1' | '|' | '!' | '.' | ',' | ';' | ':' | '\'' | '`' | ' '
        ) {
            0.36
        } else if matches!(ch, 'm' | 'w' | 'M' | 'W' | '@' | '—' | '–') {
            0.84
        } else if ch.is_uppercase() {
            0.68
        } else {
            0.57
        };
    }
    units * font_size_px
}

fn is_wide_glyph(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x115F
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE6F
            | 0xFF01..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x20000..=0x2FFFD
    )
}

/// Computes a popover menu width in `[min_width, max_width]` that fits the
/// widest localized item label plus menu chrome without truncating longer
/// European or CJK translations.
pub fn estimate_menu_width<'a>(
    tokens: &ThemeTokens,
    min_width: f32,
    max_width: f32,
    has_leading_slot: bool,
    labels: impl IntoIterator<Item = &'a str>,
) -> f32 {
    estimate_menu_width_with_shortcuts(
        tokens,
        min_width,
        max_width,
        has_leading_slot,
        labels.into_iter().map(|label| (label, None)),
    )
}

/// Variant of [`estimate_menu_width`] for menus whose rows include trailing
/// shortcut hints alongside the primary item label.
pub fn estimate_menu_width_with_shortcuts<'a>(
    tokens: &ThemeTokens,
    min_width: f32,
    max_width: f32,
    has_leading_slot: bool,
    rows: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
) -> f32 {
    let label_font = tokens.metrics.ui_text_xs;
    let shortcut_font = tokens.metrics.ui_text_2xs;
    let menu_chrome = tokens.metrics.ui_menu_padding * 2.0 + 2.0;
    let item_padding = tokens.metrics.ui_menu_item_padding_x * 2.0;
    let leading = if has_leading_slot {
        tokens.metrics.ui_menu_indicator_size + tokens.spacing.two
    } else {
        0.0
    };
    let shortcut_gap = tokens.spacing.three;
    let extra_chrome = menu_chrome + item_padding + leading + 12.0;
    let widest = rows
        .into_iter()
        .map(|(label, shortcut)| {
            let shortcut_w = shortcut
                .filter(|s| !s.is_empty())
                .map(|s| shortcut_gap + estimate_text_width(s, shortcut_font))
                .unwrap_or(0.0);
            (estimate_text_width(label, label_font) + shortcut_w + extra_chrome).ceil()
        })
        .fold(min_width, f32::max);
    widest.clamp(min_width, max_width.max(min_width))
}

#[cfg(test)]
mod tests {
    use oxideterm_theme::default_tokens;

    use super::{
        MENU_DANGER_HOVER_BG_ALPHA, estimate_menu_width, estimate_menu_width_with_shortcuts,
        menu_chrome_spec, menu_item_hover_bg,
    };

    #[test]
    fn menu_chrome_uses_concentric_rounded_radii_and_semantic_danger_tint() {
        let tokens = default_tokens();
        let chrome = menu_chrome_spec(&tokens);

        assert_eq!(chrome.surface_radius, tokens.radii.lg);
        assert_eq!(chrome.item_radius, tokens.radii.md);
        assert!(chrome.surface_radius > chrome.item_radius);

        let normal_hover = menu_item_hover_bg(&tokens, false);
        let danger_hover = menu_item_hover_bg(&tokens, true);
        assert_ne!(normal_hover, danger_hover);
        assert!(
            (danger_hover.a - (MENU_DANGER_HOVER_BG_ALPHA as f32 / 255.0)).abs() < 0.01,
            "danger hover should keep a subtle tinted background instead of solid fill"
        );
    }

    #[test]
    fn estimate_menu_width_grows_for_long_localized_labels_and_clamps_bounds() {
        let tokens = default_tokens();

        let compact = estimate_menu_width(&tokens, 160.0, 320.0, true, ["Copy", "Paste"]);
        assert_eq!(compact, 160.0);

        let german = estimate_menu_width(
            &tokens,
            160.0,
            320.0,
            true,
            ["Archivierte Unterhaltungen", "Einstellungen"],
        );
        assert!(
            german > 200.0 && german < 320.0,
            "expected German menu width to grow beyond 200px without hitting cap, got {german}"
        );

        let capped = estimate_menu_width(
            &tokens,
            160.0,
            320.0,
            true,
            ["Utiliser une connexion enregistrée comme saut suivant dans une nouvelle fenêtre"],
        );
        assert_eq!(capped, 320.0);

        let without_shortcut = estimate_menu_width_with_shortcuts(
            &tokens,
            160.0,
            360.0,
            true,
            [("Archivierte Unterhaltungen", None)],
        );
        let with_shortcut = estimate_menu_width_with_shortcuts(
            &tokens,
            160.0,
            360.0,
            true,
            [("Archivierte Unterhaltungen", Some("Ctrl+Shift+A"))],
        );
        assert!(with_shortcut > without_shortcut);
    }
}
