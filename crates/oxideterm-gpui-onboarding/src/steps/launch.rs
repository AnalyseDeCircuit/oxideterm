// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{AnyElement, FontWeight, SharedString, Styled, div, prelude::*, px, rgb, rgba};
use oxideterm_gpui_ui::button::{ButtonVariant};
use oxideterm_gpui_ui::checkbox;

use crate::{
    OnboardingFrame, OnboardingImportStatus, OnboardingRequest, OnboardingViewBindings,
    widgets::{
        ONBOARDING_ACCENT_BORDER_ALPHA, ONBOARDING_CARD_BORDER_ALPHA, ONBOARDING_DISABLED_OPACITY,
        ONBOARDING_HEADER_BORDER_ALPHA, action_button, card_bg, grouped_surface, keycap,
        render_icon, section_header, sunken_bg,
    },
};

pub(crate) fn render_launch_step(
    frame: &OnboardingFrame,
    bindings: &OnboardingViewBindings,
    wide: bool,
) -> AnyElement {
    let theme = frame.tokens.ui;

    let quick_primary = div()
        .w_full()
        .h_full()
        .grid()
        .grid_cols(if wide { 2 } else { 1 })
        .gap(px(10.0))
        .child(primary_action_tile(
            frame,
            bindings,
            "onboarding-launch-new-conn",
            "lucide/plus.svg",
            "onboarding.new_connection",
            "onboarding.new_connection_desc",
            true,
            OnboardingRequest::OpenNewConnection,
        ))
        .child(primary_action_tile(
            frame,
            bindings,
            "onboarding-launch-terminal",
            "lucide/terminal.svg",
            "onboarding.open_terminal",
            "onboarding.open_terminal_desc",
            false,
            OnboardingRequest::OpenTerminal,
        ));

    let import_rows = vec![
        import_ssh_row(frame, bindings),
        utility_action_row(
            frame,
            bindings,
            "onboarding-import-apps",
            "lucide/app-window.svg",
            theme.text_muted,
            frame.i18n.t("onboarding.import_apps"),
            frame.i18n.t("onboarding.import_apps_desc"),
            false,
            OnboardingRequest::OpenConnectionImporters,
        ),
    ];

    let quick_start_section = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(section_header(
            &frame.tokens,
            frame.i18n.t("onboarding.quick_start"),
            None,
        ))
        .child(
            div()
                .w_full()
                .grid()
                .grid_cols(if wide { 2 } else { 1 })
                .gap(px(if wide { 20.0 } else { 12.0 }))
                .child(quick_primary)
                .child(grouped_surface(&frame.tokens, frame.has_background, import_rows).h_full()),
        );

    let ai_rows = vec![
        ai_toggle_row(
            frame,
            bindings,
            "onboarding-ai-enable",
            "onboarding.ai_tools_enable",
            "onboarding.ai_tools_enable_hint",
            frame.ai_opt_in,
            true,
            OnboardingRequest::ToggleAiOptIn,
        ),
        ai_toggle_row(
            frame,
            bindings,
            "onboarding-ai-tools-enable",
            "onboarding.ai_tools_enable_tools",
            "onboarding.ai_tools_enable_tools_hint",
            frame.tool_use_opt_in,
            frame.ai_opt_in,
            OnboardingRequest::ToggleToolUseOptIn,
        ),
        div()
            .w_full()
            .px(px(14.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(render_icon("lucide/shield.svg", 12.5, rgb(theme.text_muted)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(frame.tokens.metrics.ui_text_2xs))
                    .text_color(rgb(theme.text_muted))
                    .child(frame.i18n.t("onboarding.ai_tools_privacy_context")),
            )
            .into_any_element(),
    ];

    let cli_col = render_cli_card(frame, bindings);
    let ai_col = div()
        .h_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(section_header(
            &frame.tokens,
            frame.i18n.t("onboarding.ai_tools_title"),
            None,
        ))
        .child(grouped_surface(&frame.tokens, frame.has_background, ai_rows).flex_1());

    let preferences_band = div()
        .w_full()
        .grid()
        .grid_cols(if wide { 2 } else { 1 })
        .gap(px(if wide { 20.0 } else { 14.0 }))
        .child(ai_col)
        .child(cli_col);

    let mut shortcut_strip = div()
        .pt(px(14.0))
        .w_full()
        .border_t_1()
        .border_color(rgba((theme.border << 8) | ONBOARDING_HEADER_BORDER_ALPHA))
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(px(18.0))
        .gap_y(px(8.0));
    for item in frame.shortcuts.iter().take(6) {
        shortcut_strip = shortcut_strip.child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(keycap(
                    &frame.tokens,
                    frame.has_background,
                    frame.mono_font_family.clone(),
                    item.key_label.clone(),
                ))
                .child(
                    div()
                        .text_size(px(frame.tokens.metrics.ui_text_xs))
                        .text_color(rgb(if frame.has_background {
                            theme.text
                        } else {
                            theme.text_muted
                        }))
                        .child(frame.i18n.t(item.label_key)),
                ),
        );
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(quick_start_section)
        .child(preferences_band)
        .when(!frame.shortcuts.is_empty(), |col| col.child(shortcut_strip))
        .into_any_element()
}

fn primary_action_tile(
    frame: &OnboardingFrame,
    bindings: &OnboardingViewBindings,
    id: &'static str,
    icon: &'static str,
    title_key: &str,
    desc_key: &str,
    primary: bool,
    request: OnboardingRequest,
) -> AnyElement {
    let theme = frame.tokens.ui;
    let dispatch = bindings.dispatch.clone();
    div()
        .id(id)
        .debug_selector(move || id.into())
        .w_full()
        .min_w(px(0.0))
        .min_h(px(92.0))
        .h_full()
        .rounded(px(frame.tokens.radii.lg))
        .border_1()
        .border_color(if primary {
            rgba((theme.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
        } else {
            rgba((theme.border << 8) | ONBOARDING_CARD_BORDER_ALPHA)
        })
        .bg(card_bg(&frame.tokens, frame.has_background))
        .hover(move |style| style.bg(rgb(theme.bg_hover)))
        .px(px(14.0))
        .py(px(12.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .cursor_pointer()
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            dispatch(request, window, cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .size(px(32.0))
                .flex_none()
                .rounded(px(frame.tokens.radii.sm))
                .when(!primary, |badge| {
                    badge
                        .border_1()
                        .border_color(rgba((theme.border << 8) | 0x66))
                        .bg(sunken_bg(&frame.tokens, frame.has_background))
                })
                .when(primary, |badge| badge.bg(rgb(theme.accent)))
                .flex()
                .items_center()
                .justify_center()
                .child(render_icon(
                    icon,
                    15.0,
                    rgb(if primary {
                        theme.accent_text
                    } else {
                        theme.text
                    }),
                )),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(frame.tokens.metrics.ui_text_sm))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(theme.text_heading))
                        .child(frame.i18n.t(title_key)),
                )
                .child(
                    div()
                        .text_size(px(frame.tokens.metrics.ui_text_xs))
                        .line_height(px(frame.tokens.metrics.ui_text_xs * 1.4))
                        .text_color(rgb(theme.text_muted))
                        .child(frame.i18n.t(desc_key)),
                ),
        )
        .into_any_element()
}

fn import_ssh_row(frame: &OnboardingFrame, bindings: &OnboardingViewBindings) -> AnyElement {
    let theme = frame.tokens.ui;
    let (icon, detail, disabled) = match frame.import_status {
        OnboardingImportStatus::Done => (
            "lucide/check.svg",
            frame
                .i18n
                .t("onboarding.import_ssh_done")
                .replace("{{count}}", &frame.imported_count.to_string()),
            true,
        ),
        OnboardingImportStatus::Loading => (
            "lucide/loader-circle.svg",
            frame.i18n.t("onboarding.importing"),
            true,
        ),
        OnboardingImportStatus::Idle => match frame.host_count {
            Some(0) => (
                "lucide/download.svg",
                frame.i18n.t("onboarding.import_ssh_none"),
                true,
            ),
            Some(count) => (
                "lucide/download.svg",
                frame
                    .i18n
                    .t("onboarding.import_ssh_desc")
                    .replace("{{count}}", &count.to_string()),
                false,
            ),
            None => (
                "lucide/loader-circle.svg",
                frame.i18n.t("onboarding.importing"),
                true,
            ),
        },
    };
    let icon_color = if frame.import_status == OnboardingImportStatus::Done {
        theme.success
    } else {
        theme.text_muted
    };
    utility_action_row(
        frame,
        bindings,
        "onboarding-import-ssh",
        icon,
        icon_color,
        frame.i18n.t("onboarding.import_ssh"),
        detail,
        disabled,
        OnboardingRequest::ImportSshHosts,
    )
}

fn utility_action_row(
    frame: &OnboardingFrame,
    bindings: &OnboardingViewBindings,
    id: &'static str,
    icon: &'static str,
    icon_color: u32,
    title: String,
    detail: String,
    disabled: bool,
    request: OnboardingRequest,
) -> AnyElement {
    let theme = frame.tokens.ui;
    let dispatch = bindings.dispatch.clone();
    div()
        .id(id)
        .debug_selector(move || id.into())
        .w_full()
        .min_w(px(0.0))
        .flex_1()
        .min_h(px(50.0))
        .px(px(14.0))
        .py(px(9.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .when(!disabled, |row| {
            row.hover(move |style| style.bg(rgb(theme.bg_hover)))
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                    dispatch(request, window, cx);
                    cx.stop_propagation();
                })
        })
        .opacity(if disabled {
            ONBOARDING_DISABLED_OPACITY
        } else {
            1.0
        })
        .child(render_icon(icon, 14.0, rgb(icon_color)))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(1.5))
                .child(
                    div()
                        .text_size(px(frame.tokens.metrics.ui_text_sm))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(frame.tokens.metrics.ui_text_xs))
                        .line_height(px(frame.tokens.metrics.ui_text_xs * 1.35))
                        .text_color(rgb(theme.text_muted))
                        .child(detail),
                ),
        )
        .child(render_icon("lucide/chevron-right.svg", 13.0, rgb(theme.text_muted)))
        .into_any_element()
}

fn ai_toggle_row(
    frame: &OnboardingFrame,
    bindings: &OnboardingViewBindings,
    id: &'static str,
    title_key: &str,
    hint_key: &str,
    checked: bool,
    enabled: bool,
    request: OnboardingRequest,
) -> AnyElement {
    let theme = frame.tokens.ui;
    let dispatch = bindings.dispatch.clone();
    div()
        .id(id)
        .debug_selector(move || id.into())
        .w_full()
        .flex_1()
        .px(px(14.0))
        .py(px(10.0))
        .flex()
        .items_start()
        .gap(px(10.0))
        .when(enabled, |row| {
            row.hover(move |style| style.bg(rgb(theme.bg_hover)))
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                    dispatch(request, window, cx);
                    cx.stop_propagation();
                })
        })
        .opacity(if enabled {
            1.0
        } else {
            ONBOARDING_DISABLED_OPACITY
        })
        .child(div().pt(px(1.0)).flex_none().child(checkbox::checkbox(
            &frame.tokens,
            SharedString::from(format!("{id}-checkbox")),
            String::new(),
            checked,
        )))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(frame.tokens.metrics.ui_text_sm))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(frame.i18n.t(title_key)),
                )
                .child(
                    div()
                        .text_size(px(frame.tokens.metrics.ui_text_xs))
                        .line_height(px(frame.tokens.metrics.ui_text_xs * 1.4))
                        .text_color(rgb(theme.text_muted))
                        .child(frame.i18n.t(hint_key)),
                ),
        )
        .into_any_element()
}

fn render_cli_card(frame: &OnboardingFrame, bindings: &OnboardingViewBindings) -> AnyElement {
    let theme = frame.tokens.ui;
    let cli = &frame.cli;
    let status_label = if cli.loading {
        frame.i18n.t("settings_view.general.cli_checking")
    } else if cli.installed && cli.needs_reinstall {
        frame.i18n.t("onboarding.cli_step_reinstall_required")
    } else if cli.installed {
        frame.i18n.t("onboarding.cli_step_installed")
    } else if cli.has_status && !cli.bundled {
        frame.i18n.t("onboarding.cli_step_not_bundled")
    } else {
        frame.i18n.t("settings_view.general.cli_not_installed")
    };
    let status_color = if cli.installed && !cli.needs_reinstall {
        theme.success
    } else if cli.loading || cli.needs_reinstall {
        theme.warning
    } else {
        theme.text_muted
    };
    let path = cli.install_path.as_deref().unwrap_or("");
    let dispatch_settings = bindings.dispatch.clone();
    let dispatch_retry = bindings.dispatch.clone();

    let mut rows = vec![
        div()
            .w_full()
            .flex_1()
            .px(px(14.0))
            .py(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(render_icon(
                                if cli.installed {
                                    "lucide/check-circle.svg"
                                } else {
                                    "lucide/alert-triangle.svg"
                                },
                                14.0,
                                rgb(status_color),
                            ))
                            .child(
                                div()
                                    .text_size(px(frame.tokens.metrics.ui_text_sm))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(status_color))
                                    .child(status_label),
                            ),
                    )
                    .when(!path.is_empty(), |col| {
                        col.child(
                            div()
                                .font_family(frame.mono_font_family.clone())
                                .text_size(px(frame.tokens.metrics.ui_text_2xs))
                                .line_height(px(frame.tokens.metrics.ui_text_2xs * 1.4))
                                .text_color(rgb(theme.text_muted))
                                .child(
                                    frame
                                        .i18n
                                        .t("onboarding.cli_step_installed_at")
                                        .replace("{{path}}", path),
                                ),
                        )
                    }),
            )
            .child(action_button(
                &frame.tokens,
                "onboarding-cli-settings",
                frame.i18n.t("settings_view.general.cli_companion"),
                Some(("lucide/settings.svg", true)),
                ButtonVariant::Outline,
                false,
                move |window, cx| {
                    dispatch_settings(OnboardingRequest::OpenCliSettings, window, cx);
                },
            ))
            .into_any_element(),
    ];

    let footer_note = if !cli.installed || cli.needs_reinstall {
        frame.i18n.t("onboarding.cli_step_skip_hint")
    } else {
        frame.i18n.t("onboarding.cli_step_desc")
    };
    rows.push(
        div()
            .w_full()
            .flex_1()
            .px(px(14.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(frame.tokens.metrics.ui_text_xs))
                    .line_height(px(frame.tokens.metrics.ui_text_xs * 1.45))
                    .text_color(rgb(theme.text_muted))
                    .child(footer_note),
            )
            .when(!cli.has_status, |row| {
                row.child(
                    div()
                        .id("onboarding-cli-retry")
                        .px(px(6.0))
                        .py(px(3.0))
                        .rounded(px(frame.tokens.radii.sm))
                        .hover(move |style| style.bg(rgb(theme.bg_hover)))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                            dispatch_retry(OnboardingRequest::RefreshCliStatus, window, cx);
                            cx.stop_propagation();
                        })
                        .child(render_icon("lucide/refresh-cw.svg", 12.0, rgb(theme.accent)))
                        .child(
                            div()
                                .text_size(px(frame.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.accent))
                                .child(frame.i18n.t("settings_view.help.retry")),
                        ),
                )
            })
            .into_any_element(),
    );

    div()
        .h_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(section_header(
            &frame.tokens,
            frame.i18n.t("onboarding.cli_step_title"),
            None,
        ))
        .child(grouped_surface(&frame.tokens, frame.has_background, rows).flex_1())
        .into_any_element()
}
