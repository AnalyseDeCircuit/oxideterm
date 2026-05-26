// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Presentational builders for the GPUI Cloud Sync panel.
//!
//! The app crate still owns event callbacks, text selection identity, IME, and
//! virtual-list wiring. This module owns the repeatable Cloud Sync chrome so the
//! app can pass already-localized/selectable children without carrying layout
//! details for every row.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, CursorStyle, Div, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent,
    ParentElement, Styled, Window, div, px, relative, rgb, rgba,
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
};
use oxideterm_gpui_ui::select::{
    select_inline_menu, select_inline_option_row, select_inline_trigger_chrome,
    select_option_action,
};
use oxideterm_theme::ThemeTokens;

pub const CLOUD_SYNC_PANEL_PADDING: f32 = 16.0;
pub const CLOUD_SYNC_CARD_PADDING: f32 = 12.0;
pub const CLOUD_SYNC_CARD_GAP: f32 = 12.0;
pub const CLOUD_SYNC_GRID_GAP: f32 = 8.0;
pub const CLOUD_SYNC_STAT_PADDING: f32 = 8.0;
pub const CLOUD_SYNC_BG_MIX_ALPHA: u32 = 0x80;
pub const CLOUD_SYNC_LIST_BORDER_ALPHA: u32 = 0xA6;
pub const CLOUD_SYNC_LIST_BG_ALPHA: u32 = 0x8C;

pub fn cloud_sync_card(tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    div()
        .w_full()
        .min_w(px(0.0))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.bg_panel))
        .p(px(CLOUD_SYNC_CARD_PADDING))
        .flex()
        .flex_col()
        .gap(px(10.0))
}

pub fn cloud_sync_section_item(
    tokens: &ThemeTokens,
    index: usize,
    section_count: usize,
    child: AnyElement,
) -> AnyElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .px(px(CLOUD_SYNC_PANEL_PADDING))
        .pb(px(CLOUD_SYNC_CARD_GAP))
        .when(index == 0, |item| item.pt(px(CLOUD_SYNC_PANEL_PADDING)))
        .when(index + 1 == section_count, |item| {
            item.pb(px(CLOUD_SYNC_PANEL_PADDING))
        })
        .child(child)
        .text_size(px(tokens.metrics.ui_text_sm))
        .into_any_element()
}

pub fn cloud_sync_header(
    tokens: &ThemeTokens,
    title: AnyElement,
    status: impl Into<gpui::SharedString>,
) -> AnyElement {
    let theme = tokens.ui;
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text))
                .child(title),
        )
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(theme.text_muted))
                .child(status.into()),
        )
        .into_any_element()
}

pub fn cloud_sync_sidebar_empty(
    tokens: &ThemeTokens,
    icon: AnyElement,
    title: AnyElement,
    subtitle: AnyElement,
) -> AnyElement {
    let theme = tokens.ui;
    div()
        .flex_1()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(px(tokens.metrics.empty_sidebar_padding_x))
        .text_color(rgb(theme.text_muted))
        .child(div().mb_3().child(icon))
        .child(
            div()
                .w_full()
                .text_center()
                .text_size(px(tokens.metrics.empty_sidebar_title_font_size))
                .text_color(rgb(theme.text_muted))
                .child(title),
        )
        .child(
            div()
                .mt_1()
                .w_full()
                .text_center()
                .text_size(px(tokens.metrics.empty_sidebar_subtitle_font_size))
                .text_color(rgb(theme.text_muted))
                .child(subtitle),
        )
        .into_any_element()
}

pub struct CloudSyncGuideExampleElements {
    pub label: AnyElement,
    pub value: AnyElement,
}

/// Builds the Cloud Sync quick-start card while the app supplies localized text nodes.
pub fn cloud_sync_guide_card(
    tokens: &ThemeTokens,
    title: AnyElement,
    heading: AnyElement,
    description: AnyElement,
    steps: AnyElement,
    examples_title: Option<AnyElement>,
    examples: impl IntoIterator<Item = CloudSyncGuideExampleElements>,
    warning: Option<AnyElement>,
    mono_font_family: gpui::SharedString,
) -> AnyElement {
    let theme = tokens.ui;
    let mut card = cloud_sync_card(tokens)
        .child(title)
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text_heading))
                .child(heading),
        )
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .line_height(px(20.0))
                .text_color(rgb(theme.text_muted))
                .child(description),
        )
        .child(steps);

    let examples = examples.into_iter().collect::<Vec<_>>();
    if let Some(examples_title) = examples_title.filter(|_| !examples.is_empty()) {
        let mut example_card = div()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg << 8) | CLOUD_SYNC_BG_MIX_ALPHA))
            .p(px(CLOUD_SYNC_CARD_PADDING))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(tokens.metrics.ui_text_sm))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_heading))
                    .child(examples_title),
            );
        for example in examples {
            example_card = example_card.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(tokens.metrics.ui_text_sm))
                    .line_height(px(20.0))
                    .text_color(rgb(theme.text_muted))
                    .child(example.label)
                    .child(
                        div()
                            .font_family(mono_font_family.clone())
                            .text_color(rgb(theme.accent))
                            .child(example.value),
                    ),
            );
        }
        card = card.child(example_card);
    }

    if let Some(warning) = warning {
        card = card.child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .line_height(px(20.0))
                .text_color(rgb(theme.accent))
                .child(warning),
        );
    }
    card.into_any_element()
}

pub fn cloud_sync_section_title(tokens: &ThemeTokens, title: AnyElement) -> AnyElement {
    div()
        .text_size(px(tokens.metrics.ui_text_xs))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(tokens.ui.text_heading))
        .child(title)
        .into_any_element()
}

pub fn cloud_sync_action_grid(children: impl IntoIterator<Item = AnyElement>) -> AnyElement {
    children
        .into_iter()
        .fold(
            div()
                .w_full()
                .min_w(px(0.0))
                .grid()
                .grid_cols(2)
                .gap(px(CLOUD_SYNC_GRID_GAP)),
            |grid, child| grid.child(child),
        )
        .into_any_element()
}

pub fn cloud_sync_button_options(variant: ButtonVariant, disabled: bool) -> ToolbarButtonOptions {
    ToolbarButtonOptions {
        button: ButtonOptions {
            variant,
            size: ButtonSize::Sm,
            radius: ButtonRadius::Md,
            disabled,
        },
        ..ToolbarButtonOptions::default()
    }
}

pub fn cloud_sync_inline_button_options(tokens: &ThemeTokens) -> ToolbarButtonOptions {
    let theme = tokens.ui;
    ToolbarButtonOptions {
        button: ButtonOptions {
            variant: ButtonVariant::Outline,
            size: ButtonSize::Sm,
            radius: ButtonRadius::Md,
            disabled: false,
        },
        background: Some(rgb(theme.bg_panel)),
        border: Some(rgb(theme.border)),
        text_color: Some(rgb(theme.text_muted)),
        hover_background: Some(rgb(theme.bg_hover)),
        hover_text_color: Some(rgb(theme.text)),
        height: Some(36.0),
        padding_x: Some(12.0),
        font_size: Some(tokens.metrics.ui_text_xs),
        ..ToolbarButtonOptions::default()
    }
}

pub fn cloud_sync_status_card(
    tokens: &ThemeTokens,
    progress: Option<AnyElement>,
    error: Option<AnyElement>,
    facts: AnyElement,
    meta: AnyElement,
) -> AnyElement {
    cloud_sync_card(tokens)
        .gap(px(10.0))
        .when_some(progress, |card, progress| card.child(progress))
        .when_some(error, |card, error| card.child(error))
        .child(facts)
        .child(meta)
        .into_any_element()
}

pub fn cloud_sync_fact_grid(children: impl IntoIterator<Item = AnyElement>) -> AnyElement {
    children
        .into_iter()
        .fold(
            div()
                .w_full()
                .min_w(px(0.0))
                .grid()
                .grid_cols(2)
                .gap(px(CLOUD_SYNC_GRID_GAP)),
            |grid, child| grid.child(child),
        )
        .into_any_element()
}

pub fn cloud_sync_fact_card(
    tokens: &ThemeTokens,
    label: AnyElement,
    value: AnyElement,
    value_uses_mono: bool,
    mono_font: Option<gpui::SharedString>,
) -> AnyElement {
    let theme = tokens.ui;
    div()
        .min_w(px(0.0))
        .rounded(px(tokens.radii.md))
        .bg(rgba((theme.bg << 8) | CLOUD_SYNC_BG_MIX_ALPHA))
        .p(px(CLOUD_SYNC_STAT_PADDING))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(theme.text_muted))
                .child(label),
        )
        .child(
            div()
                .min_w(px(0.0))
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(theme.text))
                .when(value_uses_mono, |item| {
                    item.font_family(mono_font.unwrap_or_else(|| "monospace".into()))
                })
                .child(value),
        )
        .into_any_element()
}

pub fn cloud_sync_progress_view(
    tokens: &ThemeTokens,
    stage: AnyElement,
    count: AnyElement,
    ratio: f32,
) -> AnyElement {
    let theme = tokens.ui;
    div()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.bg_panel))
        .p(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .justify_between()
                .text_size(px(tokens.metrics.ui_text_sm))
                .text_color(rgb(theme.text))
                .child(stage)
                .child(count),
        )
        .child(
            div()
                .h(px(4.0))
                .w_full()
                .rounded(px(999.0))
                .bg(rgb(theme.bg_hover))
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .w(relative(ratio.clamp(0.0, 1.0)))
                        .rounded(px(999.0))
                        .bg(rgb(theme.accent)),
                ),
        )
        .into_any_element()
}

pub fn cloud_sync_error_view(
    tokens: &ThemeTokens,
    message: impl Into<gpui::SharedString>,
) -> AnyElement {
    let theme = tokens.ui;
    div()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(theme.error))
        .bg(rgba((theme.error << 8) | 0x14))
        .px(px(12.0))
        .py(px(10.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .line_height(px(20.0))
        .text_color(rgb(theme.error))
        .child(message.into())
        .into_any_element()
}

pub fn cloud_sync_meta_block(
    tokens: &ThemeTokens,
    lines: impl IntoIterator<Item = AnyElement>,
) -> AnyElement {
    lines
        .into_iter()
        .fold(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .text_size(px(tokens.metrics.ui_text_xs))
                .line_height(px(18.0))
                .text_color(rgb(tokens.ui.text_muted)),
            |block, line| block.child(line),
        )
        .into_any_element()
}

pub fn cloud_sync_meta_line(content: AnyElement) -> AnyElement {
    div()
        .min_w(px(0.0))
        .overflow_hidden()
        .child(content)
        .into_any_element()
}

pub fn cloud_sync_preview_card(
    tokens: &ThemeTokens,
    title: AnyElement,
    fact_rows: impl IntoIterator<Item = AnyElement>,
    warning: Option<String>,
    body: impl IntoIterator<Item = AnyElement>,
    actions: AnyElement,
) -> AnyElement {
    let theme = tokens.ui;
    let card = fact_rows.into_iter().fold(
        cloud_sync_card(tokens).gap(px(8.0)).child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text_heading))
                .child(title),
        ),
        |card, row| card.child(row),
    );
    let card = warning.into_iter().fold(card, |card, warning| {
        card.child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .line_height(px(20.0))
                .text_color(rgb(theme.accent))
                .child(warning),
        )
    });
    body.into_iter()
        .fold(card, |card, child| card.child(child))
        .child(actions)
        .into_any_element()
}

pub fn cloud_sync_preview_block(tokens: &ThemeTokens, title: AnyElement) -> Div {
    let theme = tokens.ui;
    div()
        .w_full()
        .min_w(px(0.0))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgba((theme.bg << 8) | CLOUD_SYNC_BG_MIX_ALPHA))
        .p(px(CLOUD_SYNC_STAT_PADDING))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text_heading))
                .child(title),
        )
}

pub fn cloud_sync_list_item(
    tokens: &ThemeTokens,
    title: AnyElement,
    meta: Option<AnyElement>,
    title_uses_mono: bool,
    mono_font: Option<gpui::SharedString>,
) -> AnyElement {
    let theme = tokens.ui;
    let title_el = div()
        .min_w(px(0.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(theme.text))
        .when(title_uses_mono, |item| {
            item.font_family(mono_font.unwrap_or_else(|| "monospace".into()))
        })
        .child(title);
    div()
        .w_full()
        .min_w(px(0.0))
        .py(px(4.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(title_el)
        .when_some(meta, |item, meta| {
            item.child(
                div()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(meta),
            )
        })
        .into_any_element()
}

pub fn cloud_sync_list_more(
    tokens: &ThemeTokens,
    label: impl Into<gpui::SharedString>,
) -> AnyElement {
    div()
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text_muted))
        .child(label.into())
        .into_any_element()
}

pub fn cloud_sync_check_row(
    tokens: &ThemeTokens,
    checked: bool,
    disabled: bool,
    label: AnyElement,
    meta: Option<AnyElement>,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = tokens.ui;
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .cursor(if disabled {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        })
        .opacity(if disabled { 0.5 } else { 1.0 })
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            if !disabled {
                listener(event, window, cx);
            }
        })
        .child(
            div()
                .size(px(16.0))
                .rounded(px(999.0))
                .border_1()
                .border_color(if checked {
                    rgb(theme.accent)
                } else {
                    rgb(theme.border)
                })
                .when(checked, |mark| mark.bg(rgb(theme.accent)))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.0))
                .text_color(rgb(theme.bg))
                .child(if checked { "✓" } else { "" }),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .text_color(if disabled {
                            rgb(theme.text_muted)
                        } else {
                            rgb(theme.text)
                        })
                        .child(label),
                )
                .when_some(meta, |row, meta| {
                    row.child(div().text_color(rgb(theme.text_muted)).child(meta))
                }),
        )
        .into_any_element()
}

pub fn cloud_sync_rollback_backup_row(
    tokens: &ThemeTokens,
    created_at: AnyElement,
    summary: AnyElement,
    action: AnyElement,
) -> AnyElement {
    let theme = tokens.ui;
    div()
        .pb(px(8.0))
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .rounded(px(tokens.radii.md))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgba((theme.bg << 8) | CLOUD_SYNC_BG_MIX_ALPHA))
                .p(px(CLOUD_SYNC_STAT_PADDING))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(tokens.metrics.ui_text_sm))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(theme.text))
                                .child(created_at),
                        )
                        .child(
                            div()
                                .text_size(px(tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(summary),
                        ),
                )
                .child(action),
        )
        .into_any_element()
}

pub fn cloud_sync_history_card(
    tokens: &ThemeTokens,
    title: AnyElement,
    body: AnyElement,
) -> AnyElement {
    cloud_sync_card(tokens)
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text_heading))
                .child(title),
        )
        .child(body)
        .into_any_element()
}

pub fn cloud_sync_history_empty(tokens: &ThemeTokens, label: AnyElement) -> AnyElement {
    div()
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text_muted))
        .child(label)
        .into_any_element()
}

pub fn cloud_sync_history_entry(
    tokens: &ThemeTokens,
    action: AnyElement,
    summary: AnyElement,
    error: Option<AnyElement>,
) -> AnyElement {
    let theme = tokens.ui;
    div()
        .min_w(px(0.0))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgba((theme.border << 8) | CLOUD_SYNC_LIST_BORDER_ALPHA))
        .bg(rgba((theme.bg << 8) | CLOUD_SYNC_LIST_BG_ALPHA))
        .p(px(10.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .text_size(px(tokens.metrics.ui_text_xs))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text))
                .child(action),
        )
        .child(
            div()
                .line_height(px(18.0))
                .text_color(rgb(theme.text_muted))
                .child(summary),
        )
        .when_some(error, |item, error| {
            item.child(
                div()
                    .line_height(px(18.0))
                    .text_color(rgb(theme.error))
                    .child(error),
            )
        })
        .into_any_element()
}

pub fn cloud_sync_notes_card(
    tokens: &ThemeTokens,
    title: AnyElement,
    body: impl Into<gpui::SharedString>,
) -> AnyElement {
    let theme = tokens.ui;
    cloud_sync_card(tokens)
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text_heading))
                .child(title),
        )
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .line_height(px(20.0))
                .text_color(rgb(theme.text_muted))
                .child(body.into()),
        )
        .into_any_element()
}

pub fn cloud_sync_field_row(label: AnyElement, control: AnyElement) -> AnyElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(label)
        .child(control)
        .into_any_element()
}

pub fn cloud_sync_secret_row(input: AnyElement, action: Option<AnyElement>) -> AnyElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .gap(px(8.0))
        .items_end()
        .child(div().flex_1().min_w(px(0.0)).child(input))
        .when_some(action, |row, action| row.child(action))
        .into_any_element()
}

pub fn cloud_sync_toggle(
    tokens: &ThemeTokens,
    label: AnyElement,
    checked: bool,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = tokens.ui;
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .py(px(2.0))
        .text_size(px(tokens.metrics.ui_text_xs))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(theme.text_muted))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, listener)
        .child(label)
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .rounded(px(2.0))
                .border_1()
                .border_color(if checked {
                    rgb(theme.accent)
                } else {
                    rgb(theme.border)
                })
                .bg(if checked {
                    rgb(theme.accent)
                } else {
                    rgba(0x00000000)
                })
                .flex()
                .items_center()
                .justify_center()
                .when(checked, |box_el| {
                    box_el.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.bg))
                            .child("✓"),
                    )
                }),
        )
        .into_any_element()
}

pub fn cloud_sync_select_field(
    tokens: &ThemeTokens,
    label: AnyElement,
    trigger: AnyElement,
    menu: Option<AnyElement>,
) -> AnyElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text_muted))
                .child(label),
        )
        .child(trigger)
        .children(menu)
        .into_any_element()
}

pub fn cloud_sync_select_trigger(
    tokens: &ThemeTokens,
    open: bool,
    focused: bool,
    focus_visible: bool,
    value: AnyElement,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    select_inline_trigger_chrome(tokens, open, focused, focus_visible)
        .on_mouse_down(MouseButton::Left, listener)
        .child(value)
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .text_color(rgb(tokens.ui.text_muted))
                .child("⌄"),
        )
        .into_any_element()
}

pub fn cloud_sync_select_menu(
    tokens: &ThemeTokens,
    options: impl IntoIterator<Item = AnyElement>,
) -> AnyElement {
    let mut menu = select_inline_menu(tokens);
    for option in options {
        menu = menu.child(option);
    }
    menu.into_any_element()
}

pub fn cloud_sync_select_option(
    tokens: &ThemeTokens,
    selected: bool,
    highlighted: bool,
    label: AnyElement,
    on_mouse_move: impl Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static,
    on_select: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let option_row = select_inline_option_row(tokens, selected, highlighted)
        .on_mouse_move(on_mouse_move)
        .child(label)
        .when(selected, |row| row.child("✓"));
    select_option_action(option_row, false, false, on_select).into_any_element()
}
