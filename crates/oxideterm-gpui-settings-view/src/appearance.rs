// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Presentational builders for the settings appearance page.
//!
//! This module deliberately accepts already-resolved labels, icons, and callback
//! targets from the app crate. It owns reusable GPUI structure and sizing, while
//! workspace state transitions stay in `oxideterm-gpui-app`.

use gpui::{
    AnyElement, Div, IntoElement, ObjectFit, ParentElement, SharedString, Styled, div, img,
    prelude::*, px, relative, rgb, rgba,
};
use oxideterm_gpui_ui::text_input::{TextInputView, text_input};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use oxideterm_theme::{AppUiColors, TerminalTheme, ThemeTokens};

const SETTINGS_BG_ACTIVE_SURFACE_ALPHA: u32 = 0x66;
const THEME_EDITOR_CHROME_DOT_SIZE: f32 = 10.0;
const THEME_EDITOR_STATUS_DOT_SIZE: f32 = 8.0;
const THEME_EDITOR_PREVIEW_CURSOR_WIDTH: f32 = 8.0;
const THEME_EDITOR_PREVIEW_CURSOR_HEIGHT: f32 = 16.0;
const THEME_EDITOR_SWATCH_LABEL_SIZE: f32 = 10.0;
const THEME_EDITOR_INLINE_COLOR_INPUT_WIDTH: f32 = 60.0;
const THEME_EDITOR_INLINE_COLOR_INPUT_HEIGHT: f32 = 20.0;
const THEME_EDITOR_INLINE_COLOR_INPUT_PADDING_X: f32 = 4.0;
const BACKGROUND_THUMBNAIL_ASPECT_RATIO: f32 = 16.0 / 9.0;
const BACKGROUND_GALLERY_COLUMNS: f32 = 4.0;
pub const SETTINGS_THEME_EDITOR_INPUT_HEIGHT: f32 = 32.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeEditorTextInputKind {
    // The theme name participates in the normal settings form layout.
    Form,
    // Color values edit in place and should stay close to their read-only size.
    InlineColor,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThemePreviewPage {
    Connections,
    #[default]
    Terminal,
    Sftp,
    NewConnection,
}

pub fn settings_appearance_card_shell(
    tokens: &ThemeTokens,
    background_active: bool,
    header: AnyElement,
    rows: Vec<AnyElement>,
) -> AnyElement {
    // Appearance cards share the same Tauri card surface treatment as the rest
    // of settings, including translucent mode when the settings background is active.
    let card = div()
        .w_full()
        .min_w(px(0.0))
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .p(px(tokens.metrics.settings_card_padding))
        .flex()
        .flex_col()
        .gap(px(tokens.metrics.settings_card_gap))
        .child(header)
        .children(rows);
    oxideterm_gpui_ui::theme_card_surface(
        card,
        tokens,
        background_active,
        SETTINGS_BG_ACTIVE_SURFACE_ALPHA,
    )
    .into_any_element()
}

pub fn settings_appearance_card_title(
    tokens: &ThemeTokens,
    title: String,
    icon: Option<AnyElement>,
) -> AnyElement {
    // The title builder takes a rendered icon so the app crate can keep owning
    // its Lucide asset enum without leaking that type into this view crate.
    let title_el = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgb(tokens.ui.text))
        .when_some(icon, |title, icon| title.child(icon));
    title_el.child(title.to_uppercase()).into_any_element()
}

pub fn settings_appearance_card_header(
    tokens: &ThemeTokens,
    title: String,
    icon: Option<AnyElement>,
    actions: Option<AnyElement>,
) -> AnyElement {
    // Header layout is presentational; callers supply rendered icons/actions
    // so app-specific asset and event types do not cross this crate boundary.
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(settings_appearance_card_title(tokens, title, icon))
        .when_some(actions, |header, actions| header.child(actions))
        .into_any_element()
}

pub fn settings_appearance_row(
    tokens: &ThemeTokens,
    i18n: &I18n,
    label_key: &str,
    hint_key: &str,
    control: AnyElement,
) -> AnyElement {
    // Rows are pure label/hint/control layout; callers decide which control
    // handles focus, mutation, or async work.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(tokens.metrics.settings_row_gap))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(i18n.t(label_key)),
                )
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(i18n.t(hint_key)),
                ),
        )
        .child(control)
        .into_any_element()
}

pub fn settings_appearance_radius_control(
    tokens: &ThemeTokens,
    radius: i64,
    slider: AnyElement,
) -> AnyElement {
    // Radius preview is display-only. The app supplies the slider because drag
    // state and settings mutation live at the workspace boundary.
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .size(px(28.0))
                .rounded(px(radius as f32))
                .border_1()
                .border_color(rgb(tokens.ui.border))
                .bg(rgb(tokens.ui.bg_secondary)),
        )
        .child(slider)
        .child(
            div()
                .w(px(48.0))
                .text_align(gpui::TextAlign::Right)
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .child(format!("{radius}px")),
        )
        .into_any_element()
}

pub fn settings_appearance_theme_preview(
    tokens: &ThemeTokens,
    settings: &PersistedSettings,
    application_name: String,
    terminal_name: String,
    description: String,
    i18n: &I18n,
    page: ThemePreviewPage,
    background: Option<AnyElement>,
    background_scope: oxideterm_settings::BackgroundScope,
    on_select: impl Fn(ThemePreviewPage, &mut gpui::Window, &mut gpui::App) + Clone + 'static,
) -> AnyElement {
    // The caller resolves the candidate application and terminal palettes independently.
    // Sharing one frame makes their contrast visible without changing the live workspace.
    let ui = tokens.ui;
    let window_background =
        background.is_some() && background_scope == oxideterm_settings::BackgroundScope::Window;
    let (window_layer, content_layer) = if window_background {
        (background, None)
    } else {
        (None, background)
    };
    let sidebar_item = |id: &'static str, key: &str, target: ThemePreviewPage| {
        let on_select = on_select.clone();
        oxideterm_gpui_ui::select::select_inline_option_row(tokens, page == target, false)
            .id(id)
            .debug_selector(move || id.into())
            .on_click(move |_, window, cx| {
                on_select(target, window, cx);
                cx.stop_propagation();
            })
            .child(div().min_w_0().truncate().child(i18n.t(key)))
    };
    let tab = |id: &'static str, label: String, target: ThemePreviewPage| {
        let on_select = on_select.clone();
        oxideterm_gpui_ui::tabs::tabs_trigger(tokens, label, page == target)
            .id(id)
            .debug_selector(move || id.into())
            .min_w_0()
            .truncate()
            .text_size(px(tokens.metrics.ui_text_xs))
            .on_click(move |_, window, cx| {
                on_select(target, window, cx);
                cx.stop_propagation();
            })
    };
    let content = match page {
        ThemePreviewPage::Terminal => {
            settings_terminal_theme_sample(tokens, settings, i18n, content_layer, window_background)
        }
        _ => {
            settings_application_theme_sample(tokens, page, i18n, content_layer, window_background)
        }
    };
    div()
        .relative()
        .w_full()
        .min_w_0()
        .flex_none()
        .mt(px(tokens.metrics.settings_font_preview_margin_top))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(ui.border))
        // GPUI overflow masks are rectangular. Only the outer shell paints the
        // panel background; the contrasting content owns its bottom-right radius.
        .bg(rgb(ui.bg_panel))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(ui.text))
        .overflow_hidden()
        .flex()
        .flex_col()
        .when_some(window_layer, |preview, background| {
            preview.child(background)
        })
        .child(
            div()
                .relative()
                .px(px(tokens.metrics.settings_theme_preview_padding))
                .py(px(tokens.spacing.two))
                .border_b_1()
                .border_color(rgb(ui.border))
                .flex()
                .flex_col()
                .gap(px(tokens.spacing.one))
                .child(div().min_w_0().truncate().child(application_name))
                .child(div().text_color(rgb(ui.text_muted)).child(description)),
        )
        .child(
            div()
                .relative()
                .flex()
                .child(
                    div()
                        .w(relative(0.28))
                        .flex_none()
                        .min_w_0()
                        .p(px(tokens.spacing.two))
                        .border_r_1()
                        .border_color(rgb(ui.border))
                        .flex()
                        .flex_col()
                        .gap(px(tokens.spacing.one))
                        .child(sidebar_item(
                            "theme-preview-connections",
                            "settings_view.tabs.connections",
                            ThemePreviewPage::Connections,
                        ))
                        .child(sidebar_item(
                            "theme-preview-terminal",
                            "settings_view.tabs.terminal",
                            ThemePreviewPage::Terminal,
                        ))
                        .child(sidebar_item(
                            "theme-preview-sftp",
                            "settings_view.tabs.sftp",
                            ThemePreviewPage::Sftp,
                        ))
                        .child(div().flex_1())
                        .child(
                            oxideterm_gpui_ui::button::toolbar_button(
                                tokens,
                                String::new(),
                                None,
                                oxideterm_gpui_ui::button::ToolbarButtonOptions {
                                    button: oxideterm_gpui_ui::button::ButtonOptions {
                                        variant: oxideterm_gpui_ui::button::ButtonVariant::Default,
                                        size: oxideterm_gpui_ui::button::ButtonSize::Sm,
                                        ..Default::default()
                                    },
                                    show_label: false,
                                    background: Some(rgb(tokens.ui.accent)),
                                    text_color: Some(rgb(tokens.ui.accent_text)),
                                    hover_background: Some(rgb(tokens.ui.accent_hover)),
                                    ..Default::default()
                                },
                            )
                            .id("theme-preview-new-connection")
                            .debug_selector(|| "theme-preview-new-connection".into())
                            .max_w_full()
                            .min_w_0()
                            .on_click({
                                let on_select = on_select.clone();
                                move |_, window, cx| {
                                    on_select(ThemePreviewPage::NewConnection, window, cx);
                                    cx.stop_propagation();
                                }
                            })
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .child(i18n.t("layout.empty.new_connection")),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .when(
                            matches!(page, ThemePreviewPage::Terminal | ThemePreviewPage::Sftp),
                            |body| {
                                body.child(
                                    oxideterm_gpui_ui::tabs::tabs_list(tokens)
                                        .rounded_none()
                                        .border_b_1()
                                        .border_color(rgb(ui.border))
                                        .child(tab(
                                            "theme-preview-terminal-tab",
                                            terminal_name,
                                            ThemePreviewPage::Terminal,
                                        ))
                                        .child(tab(
                                            "theme-preview-sftp-tab",
                                            i18n.t("settings_view.tabs.sftp"),
                                            ThemePreviewPage::Sftp,
                                        )),
                                )
                            },
                        )
                        .child(content),
                ),
        )
        .into_any_element()
}

fn settings_application_theme_sample(
    tokens: &ThemeTokens,
    page: ThemePreviewPage,
    i18n: &I18n,
    background: Option<AnyElement>,
    window_background: bool,
) -> AnyElement {
    let (title, header, rows) = match page {
        ThemePreviewPage::Connections => (
            "settings_view.tabs.connections",
            Some(("sessionManager.table.host", "sessionManager.table.username")),
            vec![("example.com", "demo"), ("build.example.com", "builder")],
        ),
        ThemePreviewPage::NewConnection => (
            "layout.empty.new_connection",
            None,
            vec![
                ("ssh.form.host", "example.com"),
                ("ssh.form.username", "demo"),
                ("ssh.form.port", "22"),
            ],
        ),
        ThemePreviewPage::Sftp => (
            "settings_view.tabs.sftp",
            Some(("sftp.file_list.col_name", "sftp.file_list.col_size")),
            vec![("src/", "—"), ("theme.toml", "2.4 KB")],
        ),
        ThemePreviewPage::Terminal => unreachable!(),
    };
    div()
        .relative()
        .id("theme-preview-application-sample")
        .debug_selector(move || match page {
            ThemePreviewPage::Connections => "theme-preview-connections-sample".into(),
            ThemePreviewPage::Sftp => "theme-preview-sftp-sample".into(),
            ThemePreviewPage::NewConnection => "theme-preview-new-connection-sample".into(),
            ThemePreviewPage::Terminal => unreachable!(),
        })
        .flex_1()
        .min_w_0()
        .p(px(tokens.metrics.settings_theme_preview_padding))
        .rounded_br(px(oxideterm_gpui_ui::modal::rounded_shell_child_radius(
            tokens.radii.md,
        )))
        .bg(if window_background {
            rgba((tokens.ui.bg << 8) | (tokens.metrics.panel_vibrancy_alpha * 255.0) as u32)
        } else {
            rgb(tokens.ui.bg)
        })
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.two))
        .when_some(background, |sample, background| sample.child(background))
        .child(
            div()
                .relative()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(i18n.t(title)),
        )
        .when_some(header, |sample, (left, right)| {
            sample.child(
                div()
                    .relative()
                    .flex()
                    .justify_between()
                    .gap(px(tokens.spacing.two))
                    .pb(px(tokens.spacing.one))
                    .border_b_1()
                    .border_color(rgb(tokens.ui.border))
                    .text_color(rgb(tokens.ui.text_muted))
                    .child(i18n.t(left))
                    .child(i18n.t(right)),
            )
        })
        .children(rows.into_iter().map(|(key, value)| {
            div()
                .relative()
                .flex()
                .justify_between()
                .gap(px(tokens.spacing.two))
                .child(div().text_color(rgb(tokens.ui.text_muted)).child(
                    if page == ThemePreviewPage::NewConnection {
                        i18n.t(key)
                    } else {
                        key.to_owned()
                    },
                ))
                .child(div().min_w_0().truncate().child(value))
        }))
        .into_any_element()
}

pub fn settings_application_palette_swatch(tokens: &ThemeTokens, ui: AppUiColors) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(3.0))
        .rounded(px(tokens.radii.xs))
        .border_1()
        .border_color(rgb(ui.border))
        .bg(rgb(ui.bg))
        .px(px(5.0))
        .py(px(2.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(ui.text))
                .child("Aa"),
        )
        .child(
            div()
                .px(px(5.0))
                .rounded(px(tokens.radii.xs))
                .bg(rgb(ui.accent))
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(ui.accent_text))
                .child("Aa"),
        )
        .children([ui.bg_panel, ui.border, ui.text_muted].map(|color| {
            div()
                .flex_none()
                .size(px(9.0))
                .rounded(px(tokens.radii.xs))
                .bg(rgb(color))
        }))
}

fn settings_terminal_theme_sample(
    tokens: &ThemeTokens,
    settings: &PersistedSettings,
    i18n: &I18n,
    background: Option<AnyElement>,
    window_background: bool,
) -> AnyElement {
    // This is a static terminal sample, so it can live outside WorkspaceApp
    // without knowing anything about panes, sessions, or live terminal state.
    let terminal = tokens.terminal;
    div()
        .relative()
        .id("theme-preview-terminal-sample")
        .debug_selector(|| "theme-preview-terminal-sample".into())
        .flex_1()
        .min_w_0()
        .rounded_br(px(oxideterm_gpui_ui::modal::rounded_shell_child_radius(
            tokens.radii.md,
        )))
        .bg(if window_background {
            rgba(
                (terminal.background << 8)
                    | (tokens.metrics.terminal_vibrancy_alpha * 255.0) as u32,
            )
        } else {
            rgb(terminal.background)
        })
        .p(px(tokens.metrics.settings_theme_preview_padding))
        .flex()
        .flex_col()
        .when_some(background, |sample, background| sample.child(background))
        .child(
            div()
                .relative()
                .font_family(
                    settings
                        .terminal
                        .font_family
                        .terminal_family_name(&settings.terminal.custom_font_family),
                )
                .text_size(px(tokens.metrics.ui_text_xs))
                .line_height(px(tokens.metrics.settings_theme_preview_line_height))
                .text_color(rgb(terminal.foreground))
                .whitespace_nowrap()
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(6.0))
                        .child(div().text_color(rgb(terminal.cyan)).child("~/project"))
                        .child(div().text_color(rgb(terminal.magenta)).child("git"))
                        .child(div().text_color(rgb(terminal.blue)).child("status")),
                )
                .child(
                    div()
                        .text_color(rgb(terminal.green))
                        .child(i18n.t("settings_view.appearance.theme_preview_added")),
                )
                .child(
                    div()
                        .text_color(rgb(terminal.yellow))
                        .child(i18n.t("settings_view.appearance.theme_preview_modified")),
                )
                .child(
                    div()
                        .text_color(rgb(terminal.red))
                        .child(i18n.t("settings_view.appearance.theme_preview_deleted")),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(4.0))
                        .child(">")
                        .child(div().w(px(9.0)).h(px(18.0)).bg(rgb(terminal.cursor))),
                ),
        )
        .into_any_element()
}

pub fn settings_theme_palette_swatch(tokens: &ThemeTokens, terminal: TerminalTheme) -> Div {
    let mut swatch = div()
        .flex()
        .items_center()
        .gap(px(3.0))
        .rounded(px(tokens.radii.xs))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(terminal.background))
        .px(px(5.0))
        .py(px(2.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(terminal.foreground))
                .child("Aa"),
        );
    for color in [
        terminal.red,
        terminal.green,
        terminal.yellow,
        terminal.blue,
        terminal.magenta,
        terminal.cyan,
    ] {
        swatch = swatch.child(
            div()
                .flex_none()
                .size(px(9.0))
                .rounded(px(tokens.radii.xs))
                .bg(rgb(color)),
        );
    }
    swatch
}

pub fn settings_theme_editor_preview(
    tokens: &ThemeTokens,
    editor_name: &str,
    terminal: TerminalTheme,
    ui: AppUiColors,
    mono_font_family: SharedString,
    background: Option<AnyElement>,
    background_scope: oxideterm_settings::BackgroundScope,
) -> AnyElement {
    // The editor preview renders candidate colors directly from the draft
    // palette; save/delete/cancel behavior remains in the app modal wrapper.
    let window_background =
        background.is_some() && background_scope == oxideterm_settings::BackgroundScope::Window;
    div()
        .relative()
        // The scroll body may contain a much taller UI palette. Keep the
        // preview at its intrinsic height instead of letting flexbox collapse it.
        .flex_none()
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(terminal.background))
        .overflow_hidden()
        .flex()
        .flex_col()
        .when_some(background, |preview, background| preview.child(background))
        .child(
            div()
                .relative()
                .px(px(12.0))
                .py(px(6.0))
                .bg(if window_background {
                    rgba((ui.bg_panel << 8) | (tokens.metrics.panel_vibrancy_alpha * 255.0) as u32)
                } else {
                    rgb(ui.bg_panel)
                })
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .child(settings_appearance_preview_dot(
                    terminal.red,
                    THEME_EDITOR_CHROME_DOT_SIZE,
                ))
                .child(settings_appearance_preview_dot(
                    terminal.yellow,
                    THEME_EDITOR_CHROME_DOT_SIZE,
                ))
                .child(settings_appearance_preview_dot(
                    terminal.green,
                    THEME_EDITOR_CHROME_DOT_SIZE,
                ))
                .child(
                    div()
                        .ml(px(8.0))
                        .text_size(px(THEME_EDITOR_SWATCH_LABEL_SIZE))
                        .text_color(rgb(ui.text_muted))
                        .child(format!("Terminal - {editor_name}")),
                ),
        )
        .child(
            div()
                .relative()
                .p(px(12.0))
                .font_family(mono_font_family)
                .text_size(px(tokens.metrics.ui_text_xs))
                .line_height(px(20.0))
                .text_color(rgb(terminal.foreground))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .flex()
                        .child(div().text_color(rgb(terminal.green)).child("user@oxide"))
                        .child(":")
                        .child(div().text_color(rgb(terminal.blue)).child("~/projects"))
                        .child("$ ")
                        .child(div().text_color(rgb(terminal.magenta)).child("git"))
                        .child(" status"),
                )
                .child(
                    div()
                        .text_color(rgb(terminal.yellow))
                        .child("On branch main"),
                )
                .child(
                    div()
                        .text_color(rgb(terminal.cyan))
                        .child("Changes not staged for commit:"),
                )
                .child(
                    div()
                        .flex()
                        .child(div().text_color(rgb(terminal.red)).child("  modified: "))
                        .child("src/main.rs"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(div().text_color(rgb(terminal.green)).child("user@oxide"))
                        .child(":")
                        .child(div().text_color(rgb(terminal.blue)).child("~"))
                        .child("$ ")
                        .child(
                            div()
                                .w(px(THEME_EDITOR_PREVIEW_CURSOR_WIDTH))
                                .h(px(THEME_EDITOR_PREVIEW_CURSOR_HEIGHT))
                                .bg(rgb(terminal.cursor)),
                        ),
                ),
        )
        .child(
            div()
                .relative()
                .px(px(12.0))
                .py(px(6.0))
                .border_t_1()
                .border_color(rgb(ui.border))
                .bg(if window_background {
                    rgba((ui.bg << 8) | (tokens.metrics.panel_vibrancy_alpha * 255.0) as u32)
                } else {
                    rgb(ui.bg)
                })
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_theme_editor_preview_badge(
                    tokens,
                    "Active",
                    ui.accent,
                    ui.accent_text,
                    false,
                ))
                .child(settings_theme_editor_preview_badge(
                    tokens,
                    "Hover",
                    ui.bg_hover,
                    ui.text_muted,
                    false,
                ))
                .child(settings_theme_editor_preview_badge(
                    tokens,
                    "Panel",
                    ui.bg_panel,
                    ui.text,
                    true,
                ))
                .child(
                    div()
                        .ml_auto()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(settings_appearance_preview_dot(
                            ui.success,
                            THEME_EDITOR_STATUS_DOT_SIZE,
                        ))
                        .child(settings_appearance_preview_dot(
                            ui.warning,
                            THEME_EDITOR_STATUS_DOT_SIZE,
                        ))
                        .child(settings_appearance_preview_dot(
                            ui.error,
                            THEME_EDITOR_STATUS_DOT_SIZE,
                        ))
                        .child(settings_appearance_preview_dot(
                            ui.info,
                            THEME_EDITOR_STATUS_DOT_SIZE,
                        )),
                ),
        )
        .into_any_element()
}

pub fn settings_theme_editor_label(tokens: &ThemeTokens, label: String) -> AnyElement {
    // Modal labels share the small-medium shadcn form label treatment.
    div()
        .text_size(px(tokens.metrics.ui_text_xs))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgb(tokens.ui.text))
        .child(label)
        .into_any_element()
}

pub fn settings_theme_editor_name_duplicate_row(
    name_label: AnyElement,
    name_input: AnyElement,
    duplicate_row: Option<AnyElement>,
) -> AnyElement {
    // The name/base-theme row is pure form layout. The app owns the input focus
    // and select overlay behavior supplied through the child elements.
    div()
        .flex()
        .flex_row()
        .items_end()
        .gap(px(12.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(name_label)
                .child(name_input),
        )
        .when_some(duplicate_row, |row, duplicate| row.child(duplicate))
        .into_any_element()
}

pub fn settings_theme_editor_duplicate_row(label: AnyElement, select: AnyElement) -> AnyElement {
    // Base-theme selection has a fixed width in Tauri; app-side select handling
    // still owns opening, anchors, and mutation.
    div()
        .w(px(180.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(label)
        .child(select)
        .into_any_element()
}

pub fn settings_theme_editor_text_input(
    tokens: &ThemeTokens,
    view: TextInputView,
    kind: ThemeEditorTextInputKind,
    mono_font_family: SharedString,
) -> Div {
    // This builds only the visual text-input primitive. Mouse/IME handlers and
    // anchor probes are attached by the app around the returned control.
    let control = text_input(tokens, view)
        .h(px(SETTINGS_THEME_EDITOR_INPUT_HEIGHT))
        .cursor(gpui::CursorStyle::IBeam);
    match kind {
        ThemeEditorTextInputKind::Form => control.w_full(),
        ThemeEditorTextInputKind::InlineColor => control
            .w(px(THEME_EDITOR_INLINE_COLOR_INPUT_WIDTH))
            .h(px(THEME_EDITOR_INLINE_COLOR_INPUT_HEIGHT))
            .px(px(THEME_EDITOR_INLINE_COLOR_INPUT_PADDING_X))
            .rounded(px(tokens.radii.sm))
            .text_size(px(THEME_EDITOR_SWATCH_LABEL_SIZE))
            .font_family(mono_font_family),
    }
}

pub fn settings_theme_editor_color_grid(cells: Vec<AnyElement>) -> AnyElement {
    // Terminal color editing uses one flat four-column grid, matching Tauri's
    // ThemeEditorModal color matrix.
    div()
        .w_full()
        .grid()
        .grid_cols(4)
        .gap_x(px(16.0))
        .gap_y(px(12.0))
        .children(cells)
        .into_any_element()
}

pub fn settings_theme_editor_color_section(
    tokens: &ThemeTokens,
    title: String,
    cells: Vec<AnyElement>,
) -> AnyElement {
    // UI colors are grouped under section headers, but individual cells still
    // use the same grid contract as terminal colors.
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .pb(px(4.0))
                .border_b_1()
                .border_color(rgba((tokens.ui.border << 8) | 0x66))
                .text_size(px(11.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text_muted))
                .child(title.to_uppercase()),
        )
        .child(settings_theme_editor_color_grid(cells))
        .into_any_element()
}

pub fn settings_theme_editor_color_swatch(tokens: &ThemeTokens, color: u32) -> Div {
    // The swatch is visual chrome only; app-side handlers decide whether it
    // focuses an input, opens a picker, or does nothing.
    div()
        .size(px(28.0))
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(rgba((tokens.ui.border << 8) | 0x99))
        .bg(rgb(color))
        .shadow_sm()
        .cursor_pointer()
}

pub fn settings_theme_editor_color_value(
    tokens: &ThemeTokens,
    color: String,
    mono_font_family: SharedString,
) -> Div {
    // Non-focused color values are rendered as clickable monospace text. The
    // app attaches focus behavior so IME state remains out of this view crate.
    div()
        .text_size(px(THEME_EDITOR_SWATCH_LABEL_SIZE))
        .line_height(px(12.0))
        .font_family(mono_font_family)
        .text_color(rgba((tokens.ui.text << 8) | 0xb3))
        .cursor(gpui::CursorStyle::IBeam)
        .hover(|hex| hex.text_color(rgb(tokens.ui.accent)))
        .child(color)
}

pub fn settings_theme_editor_color_cell(
    tokens: &ThemeTokens,
    label: String,
    swatch: AnyElement,
    value_control: AnyElement,
) -> AnyElement {
    // Color cells own the stable two-column layout. The caller supplies the
    // interactive swatch/value controls so focus and mutation stay app-local.
    div()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(swatch)
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(THEME_EDITOR_SWATCH_LABEL_SIZE))
                        .line_height(px(12.0))
                        .text_color(rgb(tokens.ui.text_muted))
                        .truncate()
                        .child(label),
                )
                .child(value_control),
        )
        .into_any_element()
}

pub fn settings_background_thumbnails_layout(thumbnails: Vec<AnyElement>) -> AnyElement {
    // GPUI's grid row measurement can overestimate aspect-ratio children from
    // full container width, so this mirrors Tauri's quarter-width slot manually.
    let mut layout = div().w_full().flex().flex_row().flex_wrap().gap(px(8.0));
    for thumbnail in thumbnails {
        layout = layout.child(
            div()
                .w(relative(1.0 / BACKGROUND_GALLERY_COLUMNS))
                .flex_none()
                .child(thumbnail),
        );
    }
    layout.into_any_element()
}

pub fn settings_background_gallery(
    tokens: &ThemeTokens,
    title: String,
    actions: AnyElement,
    thumbnails: AnyElement,
) -> AnyElement {
    // The gallery shell owns spacing and title/action placement. File picking
    // and clear-all behavior stay in the app-provided action elements.
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(title),
                )
                .child(actions),
        )
        .child(thumbnails)
        .into_any_element()
}

pub fn settings_background_empty_hint(tokens: &ThemeTokens, label: String) -> AnyElement {
    // Empty gallery text is just display state; the app decides when no
    // background image exists.
    div()
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text_muted))
        .child(label)
        .into_any_element()
}

pub fn settings_background_thumbnail_frame(
    tokens: &ThemeTokens,
    image_path: &str,
    image_source: gpui::ImageSource,
    active: bool,
    active_label: String,
    format_label: String,
    image_fallback_icon: impl Fn() -> AnyElement + 'static,
) -> Div {
    // The frame owns crop, fallback, active border, and badge. Select/remove
    // mouse handlers are intentionally attached by the app after construction.
    let fallback_label = std::path::Path::new(image_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(image_path)
        .to_string();
    let fallback_text_size = tokens.metrics.ui_text_xs;
    let fallback_text_color = tokens.ui.text_muted;
    let fallback_bg = tokens.ui.bg_sunken;
    let thumbnail_radius = tokens.radii.md;
    let image = img(image_source)
        .w_full()
        .h_full()
        .object_fit(ObjectFit::Cover)
        // Tauri uses `rounded-md overflow-hidden` on the thumbnail wrapper.
        // GPUI does not clip image children through that wrapper reliably, so
        // the image owns the same rounded mask as the frame.
        .rounded(px(thumbnail_radius))
        .with_fallback(move || {
            div()
                .w_full()
                .h_full()
                .rounded(px(thumbnail_radius))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .bg(rgb(fallback_bg))
                .child(image_fallback_icon())
                .child(
                    div()
                        .max_w_full()
                        .px(px(8.0))
                        .text_size(px(fallback_text_size))
                        .text_color(rgb(fallback_text_color))
                        .truncate()
                        .child(fallback_label.clone()),
                )
                .into_any_element()
        });

    let mut thumbnail = div()
        .relative()
        .w_full()
        .rounded(px(thumbnail_radius))
        .overflow_hidden()
        .border_2()
        .border_color(rgb(if active {
            tokens.ui.accent
        } else {
            tokens.ui.border
        }))
        .cursor_pointer()
        // Keep the crop owned by the wrapper so the rounded border and image
        // match the browser BackgroundImageSection thumbnail.
        .child(image)
        .child(
            oxideterm_gpui_ui::badge::status_pill(
                tokens,
                format_label,
                oxideterm_gpui_ui::badge::StatusPillOptions::new(
                    oxideterm_gpui_ui::badge::StatusTone::Neutral,
                )
                .compact(),
            )
            .absolute()
            .top(px(6.0))
            .left(px(6.0))
            .rounded(px(tokens.radii.sm))
            .bg(rgba((tokens.ui.bg_elevated << 8) | 0xe6))
            .text_color(rgb(tokens.ui.text)),
        );
    thumbnail.style().aspect_ratio = Some(BACKGROUND_THUMBNAIL_ASPECT_RATIO);
    thumbnail.when(active, |thumb| {
        thumb.child(
            div()
                .absolute()
                .bottom(px(8.0))
                .left(px(8.0))
                .rounded(px(tokens.radii.sm))
                .bg(rgb(tokens.ui.accent))
                .px(px(tokens.metrics.settings_background_badge_padding_x))
                .py(px(tokens.metrics.settings_background_badge_padding_y))
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.accent_text))
                .child(active_label),
        )
    })
}

pub fn settings_background_clear_all_button(
    tokens: &ThemeTokens,
    label: String,
    icon: AnyElement,
) -> Div {
    // Clear-all uses destructive styling, but the actual destructive mutation
    // is attached by the app after this visual builder returns.
    div()
        .h(px(tokens.metrics.settings_appearance_action_height))
        .px(px(10.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .rounded(px(tokens.radii.md))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.error))
        .cursor_pointer()
        .hover(|style| style.bg(rgba((tokens.ui.error << 8) | 0x14)))
        .child(icon)
        .child(label)
}

pub fn settings_background_thumbnail_remove_button(
    tokens: &ThemeTokens,
    close_icon: AnyElement,
) -> Div {
    // The close button is visual chrome only; callers attach the destructive
    // intent so file and gallery state remain in the owning settings Entity.
    div()
        .absolute()
        .top(px(6.0))
        .right(px(6.0))
        .p(px(3.0))
        .rounded(px(tokens.radii.sm))
        .bg(rgba(0x00000099))
        .text_color(rgb(tokens.ui.text))
        .child(close_icon)
}

pub fn settings_background_tabs_section(
    tokens: &ThemeTokens,
    title: String,
    hint: String,
    pills: Vec<AnyElement>,
) -> AnyElement {
    // The tabs section owns the three-column visual layout. Callers build each
    // pill with app-owned toggle handlers.
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(hint),
                ),
        )
        .child(
            div()
                .w_full()
                .grid()
                .grid_cols(3)
                .gap(px(10.0))
                .children(pills),
        )
        .into_any_element()
}

pub fn settings_background_tab_pill(
    tokens: &ThemeTokens,
    label: String,
    icon: AnyElement,
    enabled: bool,
) -> Div {
    // Background tab pills are static option rows. The app supplies translated
    // labels and rendered icons, then wires click-to-toggle behavior.
    div()
        .h(px(40.0))
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(if enabled {
            tokens.ui.accent
        } else {
            tokens.ui.border
        }))
        .bg(if enabled {
            rgba((tokens.ui.accent << 8) | 0x1a)
        } else {
            rgba(0x00000000)
        })
        .px(px(14.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(if enabled {
            tokens.ui.accent
        } else {
            tokens.ui.text_muted
        }))
        .cursor_pointer()
        .child(icon)
        .child(div().truncate().child(label))
}

fn settings_appearance_preview_dot(color: u32, size: f32) -> AnyElement {
    div()
        .size(px(size))
        .rounded_full()
        .bg(rgb(color))
        .into_any_element()
}

fn settings_theme_editor_preview_badge(
    tokens: &ThemeTokens,
    label: &'static str,
    background: u32,
    text: u32,
    bordered: bool,
) -> AnyElement {
    div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(tokens.radii.sm))
        .text_size(px(9.0))
        .text_color(rgb(text))
        .bg(rgb(background))
        .when(bordered, |badge| {
            badge.border_1().border_color(rgb(tokens.ui.border))
        })
        .child(label)
        .into_any_element()
}
