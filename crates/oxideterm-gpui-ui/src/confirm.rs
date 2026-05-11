use gpui::{
    AnyElement, App, BoxShadow, Hsla, MouseButton, MouseDownEvent, ParentElement, Styled, Window,
    div, point, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_theme::ThemeTokens;

use crate::modal::dialog_backdrop;

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

    dialog_backdrop()
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
                            div()
                                .flex_1()
                                .h(px(CONFIRM_ACTION_HEIGHT))
                                .flex()
                                .items_center()
                                .justify_center()
                                .border_r_1()
                                .border_color(rgba((theme.border << 8) | CONFIRM_DIVIDER_ALPHA))
                                .text_size(px(tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(theme.text_muted))
                                .hover(move |button| {
                                    button.text_color(rgb(theme.text)).bg(rgb(theme.bg_hover))
                                })
                                .cursor_pointer()
                                .child(view.cancel_label)
                                .on_mouse_down(MouseButton::Left, on_cancel),
                        )
                        .child(
                            div()
                                .flex_1()
                                .h(px(CONFIRM_ACTION_HEIGHT))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(confirm_color))
                                .hover(move |button| {
                                    button
                                        .text_color(rgb(confirm_hover_color))
                                        .bg(rgba((accent << 8) | CONFIRM_ACTION_HOVER_ALPHA))
                                })
                                .cursor_pointer()
                                .child(view.confirm_label)
                                .on_mouse_down(MouseButton::Left, on_confirm),
                        ),
                ),
        )
        .into_any_element()
}
