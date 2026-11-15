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

#[cfg(test)]
mod tests {
    use oxideterm_theme::default_tokens;

    use super::{MENU_DANGER_HOVER_BG_ALPHA, menu_chrome_spec, menu_item_hover_bg};

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
}
