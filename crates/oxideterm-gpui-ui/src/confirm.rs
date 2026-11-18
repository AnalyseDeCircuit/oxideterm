use gpui::{
    AnyElement, App, BoxShadow, MouseButton, MouseDownEvent, ParentElement, Styled, Window, div,
    point, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalFooterInputKeyAction<T> {
    Cancel,
    FocusInput,
    FocusFooter(T),
    Activate(T),
}

pub fn modal_footer_input_key_action<T>(
    key: &str,
    shift: bool,
    actions: &[T],
    input_available: bool,
    input_focused: bool,
    current: Option<T>,
    fallback: T,
    activation_fallback: Option<T>,
) -> Option<ModalFooterInputKeyAction<T>>
where
    T: Copy + Eq,
{
    // Some Tauri dialogs place a real input before the footer buttons. GPUI has
    // no DOM tab order, so keep the "input, cancel, primary" focus loop here
    // instead of reimplementing it in each dialog key handler.
    match key {
        "escape" => Some(ModalFooterInputKeyAction::Cancel),
        "tab" => {
            let forward = modal_footer_key_moves_forward(key, shift);
            if input_available && input_focused {
                return Some(ModalFooterInputKeyAction::FocusFooter(
                    next_required_modal_footer_focus(actions, None, forward, fallback),
                ));
            }

            if input_available {
                let first = actions.first().copied().unwrap_or(fallback);
                let last = actions.last().copied().unwrap_or(fallback);
                if (current == Some(first) && !forward) || (current == Some(last) && forward) {
                    return Some(ModalFooterInputKeyAction::FocusInput);
                }
            }

            Some(ModalFooterInputKeyAction::FocusFooter(
                next_required_modal_footer_focus(actions, current, forward, fallback),
            ))
        }
        "arrowleft" | "left" | "arrowright" | "right" | "home" | "end" => {
            modal_footer_key_action(key, shift, actions, current, fallback).map(|action| {
                match action {
                    ModalFooterKeyAction::Cancel => ModalFooterInputKeyAction::Cancel,
                    ModalFooterKeyAction::Focus(action) => {
                        ModalFooterInputKeyAction::FocusFooter(action)
                    }
                    ModalFooterKeyAction::Activate(action) => {
                        ModalFooterInputKeyAction::Activate(action)
                    }
                }
            })
        }
        "enter" | "space" | " " => current
            .or(activation_fallback)
            .map(ModalFooterInputKeyAction::Activate),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalFooterBodyInputKeyAction<T, I> {
    Cancel,
    FocusInput(I),
    FocusFooter(T),
    Activate(T),
}

pub fn modal_footer_body_input_key_action<T, I>(
    key: &str,
    shift: bool,
    actions: &[T],
    current_footer: Option<T>,
    inputs: &[I],
    current_input: Option<I>,
    fallback: T,
    activation_fallback: Option<T>,
) -> Option<ModalFooterBodyInputKeyAction<T, I>>
where
    T: Copy + Eq,
    I: Copy + Eq,
{
    // Dialogs with several body inputs need browser focus edges, not a single
    // "input vs footer" bit: Tab from the last body input enters the footer,
    // Shift+Tab from the first footer action returns to the last body input,
    // and inner body-to-body movement is left to the owning input group.
    match key {
        "escape" => Some(ModalFooterBodyInputKeyAction::Cancel),
        "tab" => {
            let forward = modal_footer_key_moves_forward(key, shift);
            if let Some(input) = current_input {
                let Some(index) = inputs.iter().position(|candidate| *candidate == input) else {
                    return None;
                };
                if forward {
                    if let Some(next_input) = inputs.get(index + 1).copied() {
                        return Some(ModalFooterBodyInputKeyAction::FocusInput(next_input));
                    }
                    return Some(ModalFooterBodyInputKeyAction::FocusFooter(
                        next_required_modal_footer_focus(actions, None, forward, fallback),
                    ));
                }

                if let Some(previous) = index.checked_sub(1).and_then(|i| inputs.get(i).copied()) {
                    return Some(ModalFooterBodyInputKeyAction::FocusInput(previous));
                }
                return Some(ModalFooterBodyInputKeyAction::FocusFooter(
                    next_required_modal_footer_focus(actions, None, forward, fallback),
                ));
            }

            if let (Some(first), Some(last)) = (inputs.first().copied(), inputs.last().copied()) {
                let first_action = actions.first().copied().unwrap_or(fallback);
                let last_action = actions.last().copied().unwrap_or(fallback);
                if current_footer == Some(first_action) && !forward {
                    return Some(ModalFooterBodyInputKeyAction::FocusInput(last));
                }
                if current_footer == Some(last_action) && forward {
                    return Some(ModalFooterBodyInputKeyAction::FocusInput(first));
                }
            }

            Some(ModalFooterBodyInputKeyAction::FocusFooter(
                next_required_modal_footer_focus(actions, current_footer, forward, fallback),
            ))
        }
        "arrowleft" | "left" | "arrowright" | "right" | "home" | "end"
            if current_input.is_none() =>
        {
            modal_footer_key_action(key, shift, actions, current_footer, fallback).map(|action| {
                match action {
                    ModalFooterKeyAction::Cancel => ModalFooterBodyInputKeyAction::Cancel,
                    ModalFooterKeyAction::Focus(action) => {
                        ModalFooterBodyInputKeyAction::FocusFooter(action)
                    }
                    ModalFooterKeyAction::Activate(action) => {
                        ModalFooterBodyInputKeyAction::Activate(action)
                    }
                }
            })
        }
        "enter" | "space" | " " if current_input.is_none() => current_footer
            .or(activation_fallback)
            .map(ModalFooterBodyInputKeyAction::Activate),
        _ => None,
    }
}
use std::rc::Rc;

use crate::button::{SplitFooterButtonEdge, SplitFooterButtonOptions, split_footer_button};
use crate::modal::{
    TauriBackdropRole, rounded_shell_child_radius, staged_overlay_root, staged_overlay_scrim,
};

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
    confirm_dialog_with_focus_motion(
        tokens,
        "confirm-dialog",
        crate::motion::ExitPhase::Visible,
        view,
        focused_action,
        on_cancel,
        on_confirm,
    )
}

pub fn confirm_dialog_with_focus_motion(
    tokens: &ThemeTokens,
    id: impl Into<gpui::ElementId>,
    phase: crate::motion::ExitPhase,
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
    let accent = if is_danger { theme.error } else { theme.accent };
    let icon_color = accent;
    let confirm_color = accent;
    let confirm_hover_color = if is_danger {
        theme.error
    } else {
        theme.accent_hover
    };
    let on_cancel = Rc::new(on_cancel);
    let on_backdrop_cancel = on_cancel.clone();

    let id = id.into();
    let visible = phase == crate::motion::ExitPhase::Visible;
    let card =
        crate::surface::material_surface(tokens, div(), crate::surface::MaterialRole::Dialog)
            .w(px(CONFIRM_DIALOG_WIDTH))
            .rounded(px(tokens.radii.lg))
            .overflow_hidden()
            .border_1()
            .border_color(rgba((theme.border << 8) | CONFIRM_BORDER_ALPHA))
            .shadow(vec![BoxShadow {
                color: rgba((TW_BLACK << 8) | CONFIRM_SHADOW_ALPHA).into(),
                offset: point(px(0.0), px(16.0)),
                blur_radius: px(32.0),
                spread_radius: px(0.0),
                inset: false,
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
                    .w_full()
                    .flex()
                    // Tauri clips split footer button backgrounds through
                    // DialogContent's rounded overflow-hidden surface.
                    // GPUI needs the footer row to own that bottom clip too.
                    .rounded_b(px(rounded_shell_child_radius(tokens.radii.lg)))
                    .overflow_hidden()
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
                                focus_visible: focused_action == Some(ConfirmDialogAction::Cancel),
                                right_separator: true,
                                separator_color: Some(rgba(
                                    (theme.border << 8) | CONFIRM_DIVIDER_ALPHA,
                                )),
                                disabled: false,
                                loading: false,
                                height: Some(CONFIRM_ACTION_HEIGHT),
                                padding_y: None,
                                font_size: Some(tokens.metrics.ui_text_sm),
                                edge: SplitFooterButtonEdge::Left,
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
                                hover_background: rgba((accent << 8) | CONFIRM_ACTION_HOVER_ALPHA),
                                font_weight: gpui::FontWeight::SEMIBOLD,
                                focus_visible: focused_action == Some(ConfirmDialogAction::Confirm),
                                right_separator: false,
                                separator_color: None,
                                disabled: false,
                                loading: false,
                                height: Some(CONFIRM_ACTION_HEIGHT),
                                padding_y: None,
                                font_size: Some(tokens.metrics.ui_text_sm),
                                edge: SplitFooterButtonEdge::Right,
                            },
                        )
                        .on_mouse_down(MouseButton::Left, on_confirm),
                    ),
            );
    // Keep scrim dim and foreground card on independent tracks so outside
    // dismissal works on frame 0 while the card rises with un-multiplied shadow.
    staged_overlay_root()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            // Tauri useConfirm wraps Radix Dialog and maps onOpenChange(false)
            // to cancel, so an outside pointer-down must follow the same path.
            on_backdrop_cancel(event, window, cx);
            cx.stop_propagation();
        })
        .child(staged_overlay_scrim(
            tokens,
            (id.clone(), "scrim"),
            TauriBackdropRole::Dialog,
            visible,
        ))
        .child(crate::motion::zoom_fade_y(
            tokens,
            (id, "card"),
            div().child(card),
            8.0,
            0.95,
            point(0.5, 0.5),
            crate::motion::MotionDuration::Control,
            visible,
        ))
        .into_any_element()
}
mod focus;
pub use focus::*;
