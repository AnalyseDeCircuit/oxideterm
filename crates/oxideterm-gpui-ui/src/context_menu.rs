use gpui::{
    App, CursorStyle, Div, FontWeight, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Styled, Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

use crate::modal::popover_backdrop;

// Tauri uses bg-theme-bg-elevated/95 for menu surfaces.
const CONTEXT_MENU_SURFACE_ALPHA: u32 = 0xf2;
const CONTEXT_MENU_DISABLED_OPACITY: f32 = 0.5;
const CONTEXT_MENU_RADIO_DOT_SIZE: f32 = 8.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextMenuItemKind {
    Plain,
    Checkbox(bool),
    Radio(bool),
    Submenu,
}

pub fn context_menu_backdrop() -> Div {
    // Radix ContextMenu uses the same transparent outside-hit-test layer as a
    // popover, but naming the role keeps file/tree/table menus separate from
    // generic floating panels when we audit dismissal and focus restoration.
    popover_backdrop()
}

pub fn context_menu_item_is_actionable(disabled: bool, loading: bool) -> bool {
    // Radix disabled menu items keep pointer events from invoking item actions.
    // Loading rows use the same action guard even if a caller keeps them styled
    // as active to show progress.
    !(disabled || loading)
}

pub fn context_menu_action(
    item: Div,
    disabled: bool,
    loading: bool,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    if context_menu_item_is_actionable(disabled, loading) {
        item.on_mouse_down(MouseButton::Left, listener)
    } else {
        item
    }
}

pub fn context_menu_content(tokens: &ThemeTokens) -> Div {
    div()
        .min_w(px(tokens.metrics.ui_menu_min_width))
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba(
            (tokens.ui.bg_elevated << 8) | CONTEXT_MENU_SURFACE_ALPHA,
        ))
        .p(px(tokens.metrics.ui_menu_padding))
        .text_color(rgb(tokens.ui.text))
        .shadow_xl()
}

pub fn context_menu_sub_content(tokens: &ThemeTokens) -> Div {
    context_menu_content(tokens)
}

pub fn context_menu_item(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    kind: ContextMenuItemKind,
    inset: bool,
    disabled: bool,
) -> Div {
    context_menu_item_row(tokens, kind, inset, disabled)
        .child(context_menu_builtin_indicator(tokens, kind))
        .child(label.into())
        .when(kind == ContextMenuItemKind::Submenu, |item| {
            item.child(context_menu_chevron(tokens, "›"))
        })
}

pub fn context_menu_item_with_trailing(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    kind: ContextMenuItemKind,
    inset: bool,
    disabled: bool,
    trailing: impl IntoElement,
) -> Div {
    context_menu_item_row(tokens, kind, inset, disabled)
        .child(context_menu_builtin_indicator(tokens, kind))
        .child(label.into())
        .child(context_menu_trailing_slot().child(trailing))
}

pub fn context_menu_item_with_shortcut(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    shortcut: impl IntoElement,
) -> Div {
    context_menu_item_with_trailing(
        tokens,
        label,
        ContextMenuItemKind::Plain,
        false,
        false,
        shortcut,
    )
}

pub fn context_menu_sub_trigger(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    inset: bool,
    disabled: bool,
) -> Div {
    context_menu_item_row(tokens, ContextMenuItemKind::Submenu, inset, disabled)
        .child(context_menu_builtin_indicator(
            tokens,
            ContextMenuItemKind::Submenu,
        ))
        .child(label.into())
        .child(context_menu_chevron(tokens, "›"))
}

pub fn context_menu_checkbox_item(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    checked: bool,
    disabled: bool,
    check_icon: impl IntoElement,
) -> Div {
    context_menu_item_row(
        tokens,
        ContextMenuItemKind::Checkbox(checked),
        false,
        disabled,
    )
    .child(context_menu_indicator_slot(tokens).when(checked, |slot| slot.child(check_icon)))
    .child(label.into())
}

pub fn context_menu_radio_item(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    checked: bool,
    disabled: bool,
) -> Div {
    context_menu_item_row(tokens, ContextMenuItemKind::Radio(checked), false, disabled)
        .child(context_menu_indicator_slot(tokens).when(checked, |slot| {
            slot.child(
                div()
                    .size(px(CONTEXT_MENU_RADIO_DOT_SIZE))
                    .rounded_full()
                    .bg(rgb(tokens.ui.text)),
            )
        }))
        .child(label.into())
}

pub fn context_menu_label(tokens: &ThemeTokens, label: impl Into<String>, inset: bool) -> Div {
    div()
        .px(px(tokens.metrics.ui_menu_item_padding_x))
        .pl(px(if inset {
            tokens.metrics.ui_menu_inset_padding_left
        } else {
            tokens.metrics.ui_menu_item_padding_x
        }))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(tokens.ui.text_muted))
        .child(label.into())
}

pub fn context_menu_separator(tokens: &ThemeTokens) -> Div {
    div()
        .mx(px(-tokens.metrics.ui_menu_padding))
        .my(px(tokens.metrics.ui_menu_padding))
        .h(px(1.0))
        .bg(rgb(tokens.ui.border))
}

pub fn context_menu_shortcut(tokens: &ThemeTokens, shortcut: impl Into<String>) -> Div {
    div()
        .ml_auto()
        .flex_none()
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text_muted))
        .child(shortcut.into())
}

pub fn context_menu_item_row(
    tokens: &ThemeTokens,
    kind: ContextMenuItemKind,
    inset: bool,
    disabled: bool,
) -> Div {
    let needs_indicator = matches!(
        kind,
        ContextMenuItemKind::Checkbox(_) | ContextMenuItemKind::Radio(_)
    );
    let left_padding = if inset || needs_indicator {
        tokens.metrics.ui_menu_inset_padding_left
    } else {
        tokens.metrics.ui_menu_item_padding_x
    };
    let hover_bg = rgb(tokens.ui.bg_hover);

    let item = div()
        .relative()
        .flex()
        .items_center()
        .rounded(px(tokens.radii.sm))
        .px(px(tokens.metrics.ui_menu_item_padding_x))
        .pl(px(left_padding))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text))
        .opacity(if disabled {
            CONTEXT_MENU_DISABLED_OPACITY
        } else {
            1.0
        })
        // Tauri Radix context-menu disabled rows are visibly and semantically
        // non-interactive. Keep the native shared row from showing a hand
        // cursor while retaining caller-specific event ownership.
        .cursor(if disabled {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        });

    if disabled {
        item
    } else {
        item.hover(move |style| style.bg(hover_bg))
    }
}

pub fn context_menu_indicator_slot(tokens: &ThemeTokens) -> Div {
    div()
        .absolute()
        .left(px(tokens.metrics.ui_menu_item_padding_x))
        .size(px(tokens.metrics.ui_menu_indicator_size))
        .flex()
        .items_center()
        .justify_center()
}

pub fn context_menu_trailing_slot() -> Div {
    div().ml_auto().flex_none()
}

fn context_menu_builtin_indicator(tokens: &ThemeTokens, kind: ContextMenuItemKind) -> Div {
    context_menu_indicator_slot(tokens).when(
        matches!(
            kind,
            ContextMenuItemKind::Checkbox(true) | ContextMenuItemKind::Radio(true)
        ),
        |slot| {
            slot.child(if matches!(kind, ContextMenuItemKind::Radio(true)) {
                "●"
            } else {
                "✓"
            })
        },
    )
}

fn context_menu_chevron(tokens: &ThemeTokens, chevron: impl Into<String>) -> Div {
    div()
        .ml_auto()
        .flex_none()
        .text_size(px(tokens.metrics.ui_menu_icon_size))
        .text_color(rgb(tokens.ui.text_muted))
        .child(chevron.into())
}

#[cfg(test)]
mod tests {
    use super::context_menu_item_is_actionable;

    #[test]
    fn context_menu_action_guard_blocks_disabled_or_loading_items() {
        assert!(context_menu_item_is_actionable(false, false));
        assert!(!context_menu_item_is_actionable(true, false));
        assert!(!context_menu_item_is_actionable(false, true));
        assert!(!context_menu_item_is_actionable(true, true));
    }
}
