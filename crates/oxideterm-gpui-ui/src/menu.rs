use gpui::{CursorStyle, Div, IntoElement, ParentElement, Styled, div, prelude::*, px, rgb, rgba};
use oxideterm_theme::ThemeTokens;

use crate::surface::theme_overlay_surface_shadow;

const MENU_SURFACE_ALPHA: u32 = 0xf2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MenuChromeSpec {
    pub surface_radius: f32,
    pub item_radius: f32,
    pub surface_alpha: u32,
}

pub(crate) fn menu_chrome_spec(tokens: &ThemeTokens) -> MenuChromeSpec {
    MenuChromeSpec {
        surface_radius: tokens.radii.md,
        item_radius: tokens.radii.xs,
        surface_alpha: MENU_SURFACE_ALPHA,
    }
}

pub(crate) fn menu_surface_chrome(tokens: &ThemeTokens) -> Div {
    let chrome = menu_chrome_spec(tokens);
    // All floating menu variants share one elevation and corner contract.
    let surface = div()
        .overflow_hidden()
        .rounded(px(chrome.surface_radius))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba((tokens.ui.bg_elevated << 8) | chrome.surface_alpha))
        .text_color(rgb(tokens.ui.text));
    theme_overlay_surface_shadow(surface, tokens)
}

pub(crate) fn menu_item_chrome(tokens: &ThemeTokens, left_padding: f32, right_padding: f32) -> Div {
    let chrome = menu_chrome_spec(tokens);
    // Structural item chrome stays shared while each primitive owns its state.
    div()
        .relative()
        .flex()
        .items_center()
        .rounded(px(chrome.item_radius))
        .pl(px(left_padding))
        .pr(px(right_padding))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text))
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
        .when(
            matches!(
                kind,
                MenuItemKind::Checkbox(true) | MenuItemKind::Radio(true)
            ),
            |item| {
                item.child(
                    div()
                        .absolute()
                        .left(px(tokens.metrics.ui_menu_item_padding_x))
                        .size(px(tokens.metrics.ui_menu_indicator_size))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(if matches!(kind, MenuItemKind::Radio(true)) {
                            "●"
                        } else {
                            "✓"
                        }),
                )
            },
        )
        .child(label.into())
        .when(kind == MenuItemKind::Submenu, |item| {
            item.child(
                div()
                    .ml_auto()
                    .text_size(px(tokens.metrics.ui_text_sm))
                    .child("›"),
            )
        })
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
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(tokens.ui.text_muted))
        .child(label.into())
}

pub fn menu_separator(tokens: &ThemeTokens) -> Div {
    div()
        .mx(px(-tokens.metrics.ui_menu_padding))
        .my(px(tokens.metrics.ui_menu_padding))
        .h(px(1.0))
        .bg(rgb(tokens.ui.border))
}

pub fn menu_shortcut(tokens: &ThemeTokens, shortcut: impl Into<String>) -> Div {
    div()
        .ml_auto()
        .text_size(px(tokens.metrics.ui_text_xs))
        .opacity(0.6)
        .text_color(rgb(tokens.ui.text_muted))
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
    use super::*;

    #[test]
    fn shared_menu_chrome_tracks_theme_radii_and_overlay_alpha() {
        let tokens = oxideterm_theme::default_tokens();
        let chrome = menu_chrome_spec(&tokens);

        assert_eq!(chrome.surface_radius, tokens.radii.md);
        assert_eq!(chrome.item_radius, tokens.radii.xs);
        assert_eq!(chrome.surface_alpha, MENU_SURFACE_ALPHA);
    }
}
