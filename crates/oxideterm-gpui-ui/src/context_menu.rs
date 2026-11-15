use gpui::{
    App, CursorStyle, Div, FontWeight, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Rgba, Styled, Window, div, prelude::*, px, rgb,
};
use oxideterm_theme::ThemeTokens;

use crate::{
    menu::{
        menu_indicator_icon, menu_item_chrome, menu_item_hover_bg, menu_item_with_icon,
        menu_separator, menu_shortcut, menu_submenu_chevron, menu_surface_chrome,
    },
    modal::popover_backdrop,
};

const CONTEXT_MENU_DISABLED_OPACITY: f32 = 0.5;
const CONTEXT_MENU_RADIO_DOT_SIZE: f32 = 6.0;
const CONTEXT_MENU_LINE_HEIGHT_RATIO: f32 = 1.4;

pub fn context_menu_line_height(tokens: &ThemeTokens) -> f32 {
    // Menu overlays can be mounted under terminal/editor roots that set their
    // own text metrics. Normalize the line box so rows do not inherit those
    // larger surface-specific line heights.
    tokens.metrics.ui_text_xs * CONTEXT_MENU_LINE_HEIGHT_RATIO
}

pub fn context_menu_item_height_estimate(tokens: &ThemeTokens) -> f32 {
    context_menu_line_height(tokens) + tokens.metrics.ui_menu_item_padding_y * 2.0
}

pub fn context_menu_separator_height_estimate(tokens: &ThemeTokens) -> f32 {
    1.0 + tokens.metrics.ui_menu_padding * 2.0
}

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

fn context_menu_action_cursor(disabled: bool, loading: bool) -> CursorStyle {
    // Loading rows are not Radix `disabled` attributes, but they have the same
    // pointer contract: no hand cursor and no activation while work is pending.
    if context_menu_item_is_actionable(disabled, loading) {
        CursorStyle::PointingHand
    } else {
        CursorStyle::OperationNotAllowed
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContextMenuActionableStyle {
    pub hover_background: Option<Rgba>,
    pub hover_text_color: Option<Rgba>,
}

pub fn context_menu_actionable_row(
    item: Div,
    disabled: bool,
    loading: bool,
    style: ContextMenuActionableStyle,
) -> Div {
    // Radix keeps disabled/loading menu rows visible but inert. Centralizing the
    // row state prevents feature menus from drifting on cursor, hover, or opacity.
    if context_menu_item_is_actionable(disabled, loading) {
        let item = item.cursor_pointer();
        if style == ContextMenuActionableStyle::default() {
            return item;
        }
        item.update_hover_style(move |hover| {
            let mut hover = hover.unwrap_or_default();
            if let Some(background) = style.hover_background {
                hover = hover.bg(background);
            }
            if let Some(color) = style.hover_text_color {
                hover = hover.text_color(color);
            }
            Some(hover)
        })
    } else {
        // Loading rows are semantically inert like disabled Radix menu items.
        // Force the cursor here because many callers build the visual row with
        // `disabled = false` and rely on this shared action guard for loading.
        item.opacity(CONTEXT_MENU_DISABLED_OPACITY)
            .cursor(context_menu_action_cursor(disabled, loading))
            .update_hover_style(|_| None)
    }
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

pub fn context_menu_event_boundary(menu: Div) -> Div {
    // Radix ContextMenuContent is an event island: pointer/wheel input inside
    // the menu does not reach the underlying trigger row, file list, or terminal.
    context_menu_pointer_event_boundary(menu).on_scroll(|_, _, cx| {
        cx.stop_propagation();
    })
}

pub fn context_menu_pointer_event_boundary(menu: Div) -> Div {
    // Scrollable menu-like popovers need nested scroll surfaces to receive
    // wheel input, but pointer input should still stay inside the menu island.
    menu.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
        cx.stop_propagation();
    })
    .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
        cx.stop_propagation();
    })
}

pub fn context_menu_content(tokens: &ThemeTokens) -> Div {
    menu_surface_chrome(tokens)
        .min_w(px(tokens.metrics.ui_menu_min_width))
        .p(px(tokens.metrics.ui_menu_padding))
        .line_height(px(context_menu_line_height(tokens)))
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
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .when(kind == ContextMenuItemKind::Submenu, |item| {
            item.child(menu_submenu_chevron(tokens))
        })
}

pub fn context_menu_item_with_icon(
    tokens: &ThemeTokens,
    icon_path: &'static str,
    label: impl Into<String>,
    danger: bool,
    disabled: bool,
) -> Div {
    menu_item_with_icon(tokens, icon_path, label, danger, disabled)
        .line_height(px(context_menu_line_height(tokens)))
}

pub fn context_menu_item_with_icon_and_shortcut(
    tokens: &ThemeTokens,
    icon_path: &'static str,
    label: impl Into<String>,
    shortcut: impl IntoElement,
    danger: bool,
    disabled: bool,
) -> Div {
    context_menu_item_with_icon(tokens, icon_path, label, danger, disabled)
        .child(context_menu_trailing_slot().child(shortcut))
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
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
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
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .child(menu_submenu_chevron(tokens))
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
    .child(div().flex_1().min_w_0().truncate().child(label.into()))
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
                    .bg(rgb(tokens.ui.accent)),
            )
        }))
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
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
        .text_size(px(tokens.metrics.ui_text_2xs))
        .line_height(px(context_menu_line_height(tokens)))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(tokens.ui.text_muted))
        .child(label.into())
}

pub fn context_menu_separator(tokens: &ThemeTokens) -> Div {
    menu_separator(tokens)
}

pub fn context_menu_shortcut(tokens: &ThemeTokens, shortcut: impl Into<String>) -> Div {
    menu_shortcut(tokens, shortcut)
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
    let hover_bg = menu_item_hover_bg(tokens, false);

    let item = menu_item_chrome(tokens, left_padding, tokens.metrics.ui_menu_item_padding_x)
        .line_height(px(context_menu_line_height(tokens)))
        .opacity(if disabled {
            CONTEXT_MENU_DISABLED_OPACITY
        } else {
            1.0
        })
        // Tauri Radix context-menu disabled rows are visibly and semantically
        // non-interactive. Keep the native shared row from showing a hand
        // cursor while retaining caller-specific event ownership.
        .cursor(context_menu_action_cursor(disabled, false));

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
    menu_indicator_icon(
        tokens,
        matches!(
            kind,
            ContextMenuItemKind::Checkbox(true) | ContextMenuItemKind::Radio(true)
        ),
        matches!(kind, ContextMenuItemKind::Radio(true)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dropdown_menu::{DropdownMenuItemKind, dropdown_menu_item};

    #[test]
    fn context_menu_action_guard_blocks_disabled_or_loading_items() {
        assert!(context_menu_item_is_actionable(false, false));
        assert!(!context_menu_item_is_actionable(true, false));
        assert!(!context_menu_item_is_actionable(false, true));
        assert!(!context_menu_item_is_actionable(true, true));
    }

    #[gpui::test]
    fn dropdown_actions_compose_hover_and_keep_loading_rows_inert(cx: &mut gpui::TestAppContext) {
        struct Menu {
            clicked: Option<usize>,
        }

        const HEIGHT: f32 = 30.0;
        const OVERRIDE_BG: u32 = 0x3155a4;
        impl gpui::Render for Menu {
            fn render(
                &mut self,
                _window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let tokens = oxideterm_theme::default_tokens();
                div().w(px(180.0)).flex().flex_col().children(
                    [Some(rgb(OVERRIDE_BG)), None, None]
                        .into_iter()
                        .enumerate()
                        .map(|(index, hover_background)| {
                            let loading = index == 2;
                            let item = dropdown_menu_item(
                                &tokens,
                                "Name",
                                DropdownMenuItemKind::Radio(false),
                                false,
                                false,
                            )
                            .h(px(HEIGHT));
                            let item = context_menu_actionable_row(
                                item,
                                false,
                                loading,
                                ContextMenuActionableStyle {
                                    hover_background,
                                    hover_text_color: None,
                                },
                            );
                            context_menu_action(
                                item,
                                false,
                                loading,
                                cx.listener(move |this, _, _, _| this.clicked = Some(index)),
                            )
                        }),
                )
            }
        }

        let (view, cx) = cx.add_window_view(|_, _| Menu { clicked: None });
        let default_bg = menu_item_hover_bg(&oxideterm_theme::default_tokens(), false);
        let modifiers = gpui::Modifiers::default();
        for (index, expected) in [Some(rgb(OVERRIDE_BG)), Some(default_bg), None]
            .into_iter()
            .enumerate()
        {
            let pointer = gpui::point(px(90.0), px(HEIGHT * (index as f32 + 0.5)));
            cx.simulate_mouse_move(pointer, None, modifiers);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.update(|window, _| {
                let backgrounds = window
                    .painted_quads()
                    .into_iter()
                    .filter_map(|quad| quad.background.as_solid())
                    .collect::<Vec<_>>();
                assert_eq!(
                    backgrounds,
                    expected.map(Into::into).into_iter().collect::<Vec<_>>()
                );
            });
            cx.simulate_click(pointer, modifiers);
            assert_eq!(
                view.read_with(cx, |view, _| view.clicked),
                Some(index.min(1))
            );
        }
    }
}
