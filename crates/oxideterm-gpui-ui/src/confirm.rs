use gpui::{
    AnyElement, App, BoxShadow, Hsla, MouseButton, MouseDownEvent, ParentElement, Styled, Window,
    div, point, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_theme::ThemeTokens;
use std::rc::Rc;

use crate::button::{SplitFooterButtonOptions, split_footer_button};
use crate::modal::dismissible_dialog_backdrop;

const CONFIRM_DIALOG_WIDTH: f32 = 384.0; // Tauri useConfirm max-w-sm
const CONFIRM_BORDER_ALPHA: u32 = 0x99; // Tauri border-theme-border/60
const CONFIRM_DIVIDER_ALPHA: u32 = 0x66; // Tauri border-theme-border/40
const CONFIRM_SHADOW_ALPHA: u32 = 0x66; // Tauri shadow-black/40
const CONFIRM_ICON_BG_ALPHA: u32 = 0x1a; // Tauri bg-*-500/10
const CONFIRM_ICON_RING_ALPHA: u32 = 0x33; // Tauri ring-*-500/20
const CONFIRM_ACTION_HOVER_ALPHA: u32 = 0x1a; // Tauri hover:bg-*-500/10
const CONFIRM_ICON_SIZE: f32 = 24.0; // Tauri w-6 h-6
const CONFIRM_ICON_WRAPPER_SIZE: f32 = 48.0; // Tauri w-12 h-12
const CONFIRM_BODY_PAD_X: f32 = 24.0; // Tauri px-6
const CONFIRM_BODY_PAD_TOP: f32 = 24.0; // Tauri pt-6
const CONFIRM_BODY_PAD_BOTTOM: f32 = 16.0; // Tauri pb-4
const CONFIRM_BODY_GAP: f32 = 12.0; // Tauri gap-3
const CONFIRM_ACTION_HEIGHT: f32 = 40.0; // Tauri py-2.5 text-sm
const TW_BLACK: u32 = 0x000000;
const TW_RED_300: u32 = 0xfca5a5;
const TW_RED_400: u32 = 0xf87171;
const TW_RED_500: u32 = 0xef4444;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfirmDialogVariant {
    Default,
    Danger,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfirmDialogAction {
    Cancel,
    Confirm,
}

pub struct ConfirmDialogView {
    pub variant: ConfirmDialogVariant,
    pub title: AnyElement,
    pub description: Option<AnyElement>,
    pub cancel_label: AnyElement,
    pub confirm_label: AnyElement,
}

pub fn confirm_dialog(
    tokens: &ThemeTokens,
    view: ConfirmDialogView,
    on_cancel: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    on_confirm: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    confirm_dialog_with_focus(tokens, view, None, on_cancel, on_confirm)
}

pub fn confirm_dialog_with_focus(
    tokens: &ThemeTokens,
    view: ConfirmDialogView,
    focused_action: Option<ConfirmDialogAction>,
    on_cancel: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    on_confirm: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = tokens.ui;
    let is_danger = view.variant == ConfirmDialogVariant::Danger;
    let icon_path = if is_danger {
        "lucide/alert-triangle.svg"
    } else {
        "lucide/help-circle.svg"
    };
    let accent = if is_danger { TW_RED_500 } else { theme.accent };
    let icon_color = if is_danger { TW_RED_400 } else { theme.accent };
    let confirm_color = if is_danger { TW_RED_400 } else { theme.accent };
    let confirm_hover_color = if is_danger { TW_RED_300 } else { theme.accent };
    let on_cancel = Rc::new(on_cancel);
    let on_backdrop_cancel = on_cancel.clone();

    dismissible_dialog_backdrop()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            // Tauri useConfirm wraps Radix Dialog and maps onOpenChange(false)
            // to cancel, so an outside pointer-down must follow the same path.
            on_backdrop_cancel(event, window, cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .w(px(CONFIRM_DIALOG_WIDTH))
                .rounded(px(tokens.radii.lg))
                .overflow_hidden()
                .border_1()
                .border_color(rgba((theme.border << 8) | CONFIRM_BORDER_ALPHA))
                .bg(rgb(theme.bg_elevated))
                .shadow(vec![BoxShadow {
                    color: Hsla::from(rgba((TW_BLACK << 8) | CONFIRM_SHADOW_ALPHA)),
                    offset: point(px(0.0), px(16.0)),
                    blur_radius: px(32.0),
                    spread_radius: px(0.0),
                }])
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(CONFIRM_BODY_GAP))
                        .px(px(CONFIRM_BODY_PAD_X))
                        .pt(px(CONFIRM_BODY_PAD_TOP))
                        .pb(px(CONFIRM_BODY_PAD_BOTTOM))
                        .child(
                            div()
                                .size(px(CONFIRM_ICON_WRAPPER_SIZE))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .border_1()
                                .border_color(rgba((accent << 8) | CONFIRM_ICON_RING_ALPHA))
                                .bg(rgba((accent << 8) | CONFIRM_ICON_BG_ALPHA))
                                .child(
                                    svg()
                                        .path(icon_path)
                                        .size(px(CONFIRM_ICON_SIZE))
                                        .text_color(rgb(icon_color)),
                                ),
                        )
                        .child(
                            div()
                                .text_align(gpui::TextAlign::Center)
                                .text_size(px(tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(theme.text))
                                .child(view.title),
                        )
                        .when_some(view.description, |body, description| {
                            body.child(
                                div()
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(description),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .border_t_1()
                        .border_color(rgba((theme.border << 8) | CONFIRM_DIVIDER_ALPHA))
                        .child(
                            split_footer_button(
                                tokens,
                                view.cancel_label,
                                SplitFooterButtonOptions {
                                    text_color: rgb(theme.text_muted),
                                    hover_text_color: rgb(theme.text),
                                    hover_background: rgb(theme.bg_hover),
                                    font_weight: gpui::FontWeight::MEDIUM,
                                    focus_visible: focused_action
                                        == Some(ConfirmDialogAction::Cancel),
                                    right_separator: true,
                                    separator_color: Some(rgba(
                                        (theme.border << 8) | CONFIRM_DIVIDER_ALPHA,
                                    )),
                                    disabled: false,
                                    loading: false,
                                    height: Some(CONFIRM_ACTION_HEIGHT),
                                    padding_y: None,
                                    font_size: Some(tokens.metrics.ui_text_sm),
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                move |event, window, cx| {
                                    on_cancel(event, window, cx);
                                },
                            ),
                        )
                        .child(
                            split_footer_button(
                                tokens,
                                view.confirm_label,
                                SplitFooterButtonOptions {
                                    text_color: rgb(confirm_color),
                                    hover_text_color: rgb(confirm_hover_color),
                                    hover_background: rgba(
                                        (accent << 8) | CONFIRM_ACTION_HOVER_ALPHA,
                                    ),
                                    font_weight: gpui::FontWeight::SEMIBOLD,
                                    focus_visible: focused_action
                                        == Some(ConfirmDialogAction::Confirm),
                                    right_separator: false,
                                    separator_color: None,
                                    disabled: false,
                                    loading: false,
                                    height: Some(CONFIRM_ACTION_HEIGHT),
                                    padding_y: None,
                                    font_size: Some(tokens.metrics.ui_text_sm),
                                },
                            )
                            .on_mouse_down(MouseButton::Left, on_confirm),
                        ),
                ),
        )
        .into_any_element()
}
