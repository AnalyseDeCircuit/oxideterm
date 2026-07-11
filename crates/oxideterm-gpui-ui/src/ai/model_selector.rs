use gpui::{
    AnyElement, Div, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Stateful, StatefulInteractiveElement, Styled, div, prelude::*, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

use super::tokens::{
    AI_HOVER_BG_ALPHA, AI_MUTED_TEXT_70_ALPHA, AI_TEXT_9, AI_TEXT_10, AI_TEXT_12, AiTone, bg_alpha,
    muted_text, tone_color,
};
use crate::button::button_focus_visible;

const MODEL_SELECTOR_DROPDOWN_WIDTH: f32 = 256.0; // Tauri w-64.
const MODEL_SELECTOR_LIST_MAX_HEIGHT: f32 = 320.0; // Tauri max-h-80.
const MODEL_SELECTOR_TRIGGER_DOT_SIZE: f32 = 6.0; // Tauri w-1.5 h-1.5.
const MODEL_SELECTOR_SEARCH_ICON_SIZE: f32 = 12.0; // Tauri w-3 h-3.
const MODEL_SELECTOR_HEADER_ICON_SIZE: f32 = 12.0; // Tauri provider chevron w-3 h-3.
const MODEL_SELECTOR_REFRESH_ICON_SIZE: f32 = 10.0; // Tauri refresh w-2.5 h-2.5.
const MODEL_SELECTOR_STATUS_DOT_SIZE: f32 = 8.0; // Tauri local status w-2 h-2.
const MODEL_SELECTOR_ACTIVE_CHECK_SIZE: f32 = 12.0; // Tauri Check w-3 h-3.
const MODEL_SELECTOR_PROVIDER_TOP_RULE_HEIGHT: f32 = 2.0; // Tauri h-[2px].
const MODEL_SELECTOR_SEARCH_BG_ALPHA: u32 = 0x80; // Tauri bg-theme-bg/50.
const MODEL_SELECTOR_SEARCH_BORDER_ALPHA: u32 = 0x80; // Tauri border-theme-border/50.
const MODEL_SELECTOR_OPEN_BG_ALPHA: u32 = 0x1a; // Tauri bg-theme-accent/10.
const MODEL_SELECTOR_PROVIDER_BORDER_ALPHA: u32 = 0x33; // Tauri border-theme-border/20.
const MODEL_SELECTOR_ACTIVE_CHIP_BG_ALPHA: u32 = 0x66; // Tauri bg-theme-bg-hover/40.
const MODEL_SELECTOR_ACTIVE_CHIP_TEXT_ALPHA: f32 = 0.80; // Tauri text-theme-text-muted/80.
const MODEL_SELECTOR_MODELS_BG_ALPHA: u32 = 0xe6; // Tauri bg-theme-bg-panel/90.
const MODEL_SELECTOR_MODEL_ACTIVE_TEXT_ALPHA: f32 = 0.85; // Tauri text-theme-text/85.
const MODEL_SELECTOR_MODEL_INACTIVE_TEXT_ALPHA: f32 = 0.70; // Tauri text-theme-text-muted/70.
const MODEL_SELECTOR_NO_KEY_TEXT_ALPHA: u32 = 0xcc; // Tauri text-amber-400/80.
const MODEL_SELECTOR_FOOTER_BORDER_ALPHA: u32 = 0x4d; // Tauri border-theme-border/30.
const MODEL_SELECTOR_INACTIVE_MODEL_INDENT: f32 = 20.0; // Tauri inactive model ml-5.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiModelSelectorPlacement {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiModelSelectorProviderState {
    Ready,
    MissingKey,
    Offline,
}

pub fn ai_model_selector_root() -> Div {
    div().relative().min_w_0().flex_1()
}

pub fn ai_model_selector_no_provider_button(
    tokens: &ThemeTokens,
    icon: impl IntoElement,
    label: impl Into<String>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one))
        .rounded(px(tokens.radii.md))
        .px(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(tokens.ui.warning))
        .cursor_pointer()
        .hover(|style| {
            style.bg(rgba(
                (tokens.ui.warning << 8) | MODEL_SELECTOR_OPEN_BG_ALPHA,
            ))
        })
        .child(icon)
        .child(label.into())
}

pub fn ai_model_selector_trigger_compact(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    ready: bool,
    open: bool,
    focus_visible: bool,
    chevron: impl IntoElement,
) -> Div {
    let trigger = div()
        .flex()
        .w_full()
        .max_w_full()
        .min_w_0()
        .items_center()
        .gap(px(tokens.spacing.one))
        .rounded(px(tokens.radii.md))
        .px(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if open {
            rgb(tokens.ui.text)
        } else {
            rgb(tokens.ui.text_muted)
        })
        .bg(if open {
            bg_alpha(tokens, tokens.ui.accent, MODEL_SELECTOR_OPEN_BG_ALPHA)
        } else {
            rgba(0x00000000)
        })
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(bg_alpha(
                    tokens,
                    tokens.ui.accent,
                    MODEL_SELECTOR_OPEN_BG_ALPHA,
                ))
                .text_color(rgb(tokens.ui.text))
        })
        .child(
            div()
                .flex_none()
                .size(px(MODEL_SELECTOR_TRIGGER_DOT_SIZE))
                .rounded_full()
                .bg(rgb(if ready {
                    tokens.ui.success
                } else {
                    tokens.ui.warning
                })),
        )
        .child(div().min_w_0().flex_1().truncate().child(label.into()))
        .child(div().flex_none().child(chevron));
    // The model selector is visually a compact select trigger. Keep its
    // browser focus-visible ring inside the primitive so page renderers only
    // decide keyboard-vs-pointer origin, not ring styling.
    button_focus_visible(tokens, trigger, focus_visible)
}

pub fn ai_model_selector_dropdown(
    tokens: &ThemeTokens,
    placement: AiModelSelectorPlacement,
) -> Div {
    let panel = div()
        .w(px(MODEL_SELECTOR_DROPDOWN_WIDTH))
        .overflow_hidden()
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(tokens.ui.bg_elevated))
        // Tauri dropdown menus are wheel boundaries: scrolling over the model
        // selector must never move the chat/sidebar underneath the overlay.
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .when(placement == AiModelSelectorPlacement::Down, |panel| panel)
        .when(placement == AiModelSelectorPlacement::Up, |panel| panel);
    crate::surface::theme_overlay_surface_shadow(panel, tokens)
}

pub fn ai_model_selector_search_bar(
    tokens: &ThemeTokens,
    search_icon: impl IntoElement,
    input: impl IntoElement,
    clear_button: Option<AnyElement>,
) -> Div {
    div()
        .px(px(tokens.spacing.two))
        .pt(px(tokens.spacing.two))
        .pb(px(tokens.spacing.one))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
                .rounded(px(tokens.radii.md))
                .border_1()
                .border_color(bg_alpha(
                    tokens,
                    tokens.ui.border,
                    MODEL_SELECTOR_SEARCH_BORDER_ALPHA,
                ))
                .bg(bg_alpha(
                    tokens,
                    tokens.ui.bg,
                    MODEL_SELECTOR_SEARCH_BG_ALPHA,
                ))
                .px(px(tokens.spacing.two))
                .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
                .child(
                    div()
                        .flex_none()
                        .size(px(MODEL_SELECTOR_SEARCH_ICON_SIZE))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(search_icon),
                )
                .child(div().min_w_0().flex_1().child(input))
                .when_some(clear_button, |bar, clear_button| {
                    bar.child(
                        div()
                            .flex_none()
                            .text_color(rgb(tokens.ui.text_muted))
                            .hover(|style| style.text_color(rgb(tokens.ui.text)))
                            .child(clear_button),
                    )
                }),
        )
}

pub fn ai_model_selector_list(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .max_h(px(MODEL_SELECTOR_LIST_MAX_HEIGHT))
        .overflow_y_scroll()
        .py(px(4.0))
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

pub fn ai_model_selector_empty_search(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.three))
        .text_size(px(AI_TEXT_10))
        .italic()
        .text_color(rgb(tokens.ui.text_muted))
        .child(label.into())
}

pub fn ai_model_selector_provider_header(
    tokens: &ThemeTokens,
    provider_name: impl Into<String>,
    expanded_icon: impl IntoElement,
    active_model: Option<String>,
    status: impl IntoElement,
    refresh_button: Option<AnyElement>,
    has_key: bool,
    first: bool,
) -> Div {
    div()
        .relative()
        .flex()
        .items_center()
        .justify_between()
        .border_t_1()
        .border_b_1()
        .border_color(bg_alpha(
            tokens,
            tokens.ui.border,
            MODEL_SELECTOR_PROVIDER_BORDER_ALPHA,
        ))
        .bg(rgb(tokens.ui.bg_elevated))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .when(first, |header| {
            header.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top_0()
                    .h(px(MODEL_SELECTOR_PROVIDER_TOP_RULE_HEIGHT))
                    .bg(bg_alpha(
                        tokens,
                        tokens.ui.border,
                        MODEL_SELECTOR_SEARCH_BORDER_ALPHA,
                    )),
            )
        })
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_1()
                .items_center()
                .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
                .text_color(bg_alpha(tokens, tokens.ui.accent, 0xcc))
                .cursor_pointer()
                .child(
                    div()
                        .flex_none()
                        .size(px(MODEL_SELECTOR_HEADER_ICON_SIZE))
                        .child(expanded_icon),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(AI_TEXT_10))
                        .font_weight(FontWeight::BOLD)
                        .text_color(if has_key {
                            rgb(tokens.ui.text_heading)
                        } else {
                            rgb(tokens.ui.text_muted)
                        })
                        .child(provider_name.into()),
                )
                .when_some(active_model, |row, active_model| {
                    row.child(ai_model_selector_active_model_chip(tokens, active_model))
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
                .when_some(refresh_button, |actions, refresh| actions.child(refresh))
                .child(status),
        )
}

pub fn ai_model_selector_refresh_button(tokens: &ThemeTokens, icon: impl IntoElement) -> Div {
    div()
        .p(px(tokens.spacing.one / 2.0))
        .text_color(rgb(tokens.ui.text_muted))
        .cursor_pointer()
        .hover(|style| style.text_color(rgb(tokens.ui.text)))
        .child(div().size(px(MODEL_SELECTOR_REFRESH_ICON_SIZE)).child(icon))
}

pub fn ai_model_selector_local_status(
    tokens: &ThemeTokens,
    online: bool,
    label: impl Into<String>,
) -> Div {
    ai_model_selector_status_label(
        tokens,
        label,
        if online {
            AiModelSelectorProviderState::Ready
        } else {
            AiModelSelectorProviderState::Offline
        },
        true,
    )
}

pub fn ai_model_selector_key_status(
    tokens: &ThemeTokens,
    has_key: bool,
    key_icon: impl IntoElement,
    label: impl Into<String>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_9))
        .text_color(rgb(if has_key {
            tokens.ui.success
        } else {
            tokens.ui.warning
        }))
        .child(
            div()
                .size(px(MODEL_SELECTOR_REFRESH_ICON_SIZE))
                .child(key_icon),
        )
        .child(label.into())
}

pub fn ai_model_selector_models_panel(tokens: &ThemeTokens) -> Div {
    div().bg(bg_alpha(
        tokens,
        tokens.ui.bg_panel,
        MODEL_SELECTOR_MODELS_BG_ALPHA,
    ))
}

pub fn ai_model_selector_provider_message(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    state: AiModelSelectorProviderState,
    clickable: bool,
) -> Div {
    let text = match state {
        AiModelSelectorProviderState::Ready | AiModelSelectorProviderState::Offline => {
            rgb(tokens.ui.text_muted)
        }
        AiModelSelectorProviderState::MissingKey => {
            rgba((tokens.ui.warning << 8) | MODEL_SELECTOR_NO_KEY_TEXT_ALPHA)
        }
    };
    let message = div()
        .w_full()
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .text_size(px(AI_TEXT_10))
        .italic()
        .text_color(text)
        .child(label.into());

    if clickable {
        message
            .cursor_pointer()
            .hover(|style| style.bg(bg_alpha(tokens, tokens.ui.bg_hover, AI_HOVER_BG_ALPHA)))
    } else {
        message
    }
}

pub fn ai_model_selector_model_row(
    tokens: &ThemeTokens,
    model: impl Into<String>,
    active: bool,
    highlighted: bool,
    check_icon: Option<AnyElement>,
) -> Div {
    let model = model.into();
    let row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_12))
        .cursor_pointer()
        .text_color(if active {
            bg_alpha(
                tokens,
                tokens.ui.text,
                (MODEL_SELECTOR_MODEL_ACTIVE_TEXT_ALPHA * 255.0).round() as u32,
            )
        } else {
            muted_text(tokens, MODEL_SELECTOR_MODEL_INACTIVE_TEXT_ALPHA)
        })
        .bg(if highlighted {
            bg_alpha(tokens, tokens.ui.bg_hover, AI_HOVER_BG_ALPHA)
        } else {
            rgba(0x00000000)
        })
        .hover(|style| {
            // Keyboard highlight and pointer hover use the same Radix menu
            // active-row treatment; active rows keep their checkmark emphasis.
            if active {
                style
            } else {
                style
                    .bg(bg_alpha(tokens, tokens.ui.bg_hover, AI_HOVER_BG_ALPHA))
                    .text_color(muted_text(tokens, AI_MUTED_TEXT_70_ALPHA))
            }
        });

    if active {
        row.font_weight(FontWeight::MEDIUM)
            .when_some(check_icon, |row, icon| {
                row.child(
                    div()
                        .flex_none()
                        .size(px(MODEL_SELECTOR_ACTIVE_CHECK_SIZE))
                        .text_color(rgb(tokens.ui.accent))
                        .child(icon),
                )
            })
            .child(div().min_w_0().truncate().child(model))
    } else {
        row.child(
            div()
                .ml(px(MODEL_SELECTOR_INACTIVE_MODEL_INDENT))
                .min_w_0()
                .truncate()
                .child(model),
        )
    }
}

pub fn ai_model_selector_footer(
    tokens: &ThemeTokens,
    icon: impl IntoElement,
    label: impl Into<String>,
) -> Div {
    div()
        .border_t_1()
        .border_color(bg_alpha(
            tokens,
            tokens.ui.border,
            MODEL_SELECTOR_FOOTER_BORDER_ALPHA,
        ))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.two))
                .px(px(tokens.spacing.three))
                .py(px(tokens.spacing.two))
                .text_size(px(AI_TEXT_12))
                .text_color(rgb(tokens.ui.text_muted))
                .cursor_pointer()
                .hover(|style| {
                    style
                        .bg(rgb(tokens.ui.bg_hover))
                        .text_color(rgb(tokens.ui.text))
                })
                .child(div().size(px(MODEL_SELECTOR_SEARCH_ICON_SIZE)).child(icon))
                .child(label.into()),
        )
}

fn ai_model_selector_active_model_chip(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .min_w_0()
        .truncate()
        .rounded(px(tokens.radii.sm))
        .bg(bg_alpha(
            tokens,
            tokens.ui.bg_hover,
            MODEL_SELECTOR_ACTIVE_CHIP_BG_ALPHA,
        ))
        .px(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_9))
        .text_color(muted_text(tokens, MODEL_SELECTOR_ACTIVE_CHIP_TEXT_ALPHA))
        .child(label.into())
}

fn ai_model_selector_status_label(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    state: AiModelSelectorProviderState,
    dot: bool,
) -> Div {
    let color = match state {
        AiModelSelectorProviderState::Ready => tone_color(tokens, AiTone::Green),
        AiModelSelectorProviderState::MissingKey => tone_color(tokens, AiTone::Amber),
        AiModelSelectorProviderState::Offline => tokens.ui.text_muted,
    };
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_9))
        .text_color(rgb(color))
        .when(dot, |status| {
            status.child(
                div()
                    .size(px(MODEL_SELECTOR_STATUS_DOT_SIZE))
                    .rounded_full()
                    .bg(rgb(color)),
            )
        })
        .child(label.into())
}
