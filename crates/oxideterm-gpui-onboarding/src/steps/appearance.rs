// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{
    AnyElement, App, FontWeight, SharedString, Styled, WeakEntity, div, prelude::*, px, rgb, rgba,
    svg,
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with,
};
use oxideterm_gpui_ui::motion::{self, UserSegmentedControlMotionState};
use oxideterm_settings::{AnimationSpeed, AppearanceScheme, CornerStyle, FontFamily};
use oxideterm_theme::UiMotionProfile;

use crate::{
    ONBOARDING_FONT_OPTIONS, OnboardingFrame, OnboardingRequest, OnboardingThemeTarget,
    OnboardingView, OnboardingViewBindings,
    widgets::{
        ONBOARDING_ACCENT_BORDER_ALPHA, ONBOARDING_ACCENT_SUBTLE_ALPHA,
        ONBOARDING_CARD_BORDER_ALPHA, ONBOARDING_HAIRLINE_ALPHA, compact_pill_bar,
        compact_pill_item, grouped_surface, render_icon, section_header, sunken_bg,
    },
};

const ONBOARDING_MODE_CONTROL_ID: &str = "onboarding-mode-control";
const ONBOARDING_SCHEME_CONTROL_ID: &str = "onboarding-scheme-control";
const ONBOARDING_CORNER_CONTROL_ID: &str = "onboarding-corner-control";
const ONBOARDING_ANIMATION_CONTROL_ID: &str = "onboarding-animation-control";

fn motion_profile_for_speed(speed: AnimationSpeed) -> UiMotionProfile {
    match speed {
        AnimationSpeed::Off => UiMotionProfile::Off,
        AnimationSpeed::Reduced => UiMotionProfile::Reduced,
        AnimationSpeed::Normal => UiMotionProfile::Normal,
        AnimationSpeed::Fast => UiMotionProfile::Fast,
    }
}

pub(crate) fn render_appearance_step(
    frame: &OnboardingFrame,
    bindings: &OnboardingViewBindings,
    control_motion: &UserSegmentedControlMotionState,
    view: WeakEntity<OnboardingView>,
    wide: bool,
    cx: &mut App,
) -> AnyElement {
    let settings = &frame.settings;
    let theme = frame.tokens.ui;

    let mode_options = [
        (
            Some(AppearanceScheme::Light),
            "settings_view.appearance.mode_light",
        ),
        (
            Some(AppearanceScheme::Dark),
            "settings_view.appearance.mode_dark",
        ),
        (None, "settings_view.appearance.follow_system"),
    ];
    let mode_active_index = if settings.appearance.follow_system_appearance {
        2
    } else if settings.appearance.manual_scheme == Some(AppearanceScheme::Light) {
        0
    } else if settings.appearance.manual_scheme == Some(AppearanceScheme::Dark) {
        1
    } else {
        2
    };
    let mode_row = setting_stacked_row(
        frame,
        "settings_view.appearance.mode",
        compact_pill_bar(
            &frame.tokens,
            ONBOARDING_MODE_CONTROL_ID,
            frame.has_background,
            mode_active_index,
            control_motion,
            mode_options
                .into_iter()
                .enumerate()
                .map(|(index, (scheme, key))| {
                    let selected = match scheme {
                        None => settings.appearance.follow_system_appearance,
                        Some(scheme) => {
                            !settings.appearance.follow_system_appearance
                                && settings.appearance.manual_scheme == Some(scheme)
                        }
                    };
                    let dispatch = bindings.dispatch.clone();
                    let view = view.clone();
                    let tokens = frame.tokens;
                    compact_pill_item(
                        &frame.tokens,
                        SharedString::from(format!("onboarding-mode-{index}")),
                        frame.i18n.t(key),
                        selected,
                        move |window, cx| {
                            let _ = view.update(cx, |view, cx| {
                                view.begin_control_motion(
                                    ONBOARDING_MODE_CONTROL_ID,
                                    &tokens,
                                    mode_active_index,
                                    index,
                                    cx,
                                );
                            });
                            dispatch(OnboardingRequest::SetAppearanceMode(scheme), window, cx);
                        },
                    )
                })
                .collect(),
        ),
    );

    let edit_dark = frame.edit_scheme.unwrap_or(frame.system_dark);
    let scheme_active_index = usize::from(edit_dark);
    let mut theme_rows = vec![
        mode_row,
        setting_stacked_row(
            frame,
            "settings_view.appearance.edit_scheme",
            compact_pill_bar(
                &frame.tokens,
                ONBOARDING_SCHEME_CONTROL_ID,
                frame.has_background,
                scheme_active_index,
                control_motion,
                [
                    (false, "settings_view.appearance.system_light"),
                    (true, "settings_view.appearance.system_dark"),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (dark, key))| {
                    let dispatch = bindings.dispatch.clone();
                    let view = view.clone();
                    let tokens = frame.tokens;
                    compact_pill_item(
                        &frame.tokens,
                        SharedString::from(format!("onboarding-scheme-{}", u8::from(dark))),
                        frame.i18n.t(key),
                        edit_dark == dark,
                        move |window, cx| {
                            let _ = view.update(cx, |view, cx| {
                                view.begin_control_motion(
                                    ONBOARDING_SCHEME_CONTROL_ID,
                                    &tokens,
                                    scheme_active_index,
                                    index,
                                    cx,
                                );
                            });
                            dispatch(OnboardingRequest::SetEditScheme(dark), window, cx);
                        },
                    )
                })
                .collect(),
            ),
        ),
        setting_inline_row(
            frame,
            "settings_view.appearance.application_theme",
            (bindings.theme_select)(OnboardingThemeTarget::Application, settings, cx),
            204.0,
        ),
        setting_inline_row(
            frame,
            "settings_view.appearance.terminal_theme",
            (bindings.theme_select)(OnboardingThemeTarget::Terminal, settings, cx),
            204.0,
        ),
    ];

    if !settings.appearance.follow_system_appearance && settings.appearance.manual_scheme.is_none()
    {
        let dispatch = bindings.dispatch.clone();
        theme_rows.push(
            div()
                .w_full()
                .px(px(12.0))
                .py(px(8.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(frame.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(frame.i18n.t("settings_view.appearance.legacy_scheme_hint")),
                )
                .child(
                    button_with(
                        &frame.tokens,
                        frame.i18n.t("settings_view.appearance.use_current_scheme"),
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: false,
                        },
                    )
                    .id("onboarding-assign-scheme")
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        move |_, window, cx| {
                            dispatch(
                                OnboardingRequest::AssignCurrentScheme(edit_dark),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        },
                    ),
                )
                .into_any_element(),
        );
    }

    let mut font_grid = div().grid().grid_cols(2).gap(px(6.0));
    for (family, label, _) in ONBOARDING_FONT_OPTIONS {
        let selected = settings.terminal.font_family == family;
        let label = if family == FontFamily::Custom {
            frame.i18n.t("settings_view.terminal.custom_font")
        } else {
            label.trim_end_matches(" (Subset)").to_string()
        };
        let dispatch = bindings.dispatch.clone();
        font_grid = font_grid.child(
            div()
                .id(SharedString::from(format!("onboarding-font-{family:?}")))
                .min_w(px(0.0))
                .rounded(px(frame.tokens.radii.md))
                .border_1()
                .border_color(if selected {
                    rgba((theme.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
                } else {
                    rgba((theme.border << 8) | ONBOARDING_HAIRLINE_ALPHA)
                })
                .bg(if selected {
                    rgba((theme.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
                } else {
                    sunken_bg(&frame.tokens, frame.has_background)
                })
                .hover(move |style| style.bg(rgb(theme.bg_hover)))
                .px(px(10.0))
                .py(px(6.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(6.0))
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                    dispatch(OnboardingRequest::SetFontFamily(family), window, cx);
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .truncate()
                        .font_family(SharedString::from(family.terminal_family_name("")))
                        .text_size(px(frame.tokens.metrics.ui_text_xs))
                        .when(selected, |text| text.font_weight(FontWeight::MEDIUM))
                        .text_color(rgb(if selected {
                            theme.text_heading
                        } else {
                            theme.text
                        }))
                        .child(label),
                )
                .child(motion::animated_checkmark(
                    &frame.tokens,
                    SharedString::from(format!("onboarding-font-check-{family:?}")),
                    svg()
                        .path("lucide/check.svg")
                        .size(px(12.0))
                        .flex_none()
                        .text_color(rgb(theme.accent)),
                    selected,
                )),
        );
    }

    let font_size = settings.terminal.font_size;
    let custom_font_slot = (settings.terminal.font_family == FontFamily::Custom)
        .then(|| (bindings.custom_font_input)(settings, cx));

    let font_rows = vec![
        div()
            .w_full()
            .px(px(12.0))
            .py(px(10.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(font_grid)
            .child(motion::auto_height(
                &frame.tokens,
                "onboarding-custom-font",
                custom_font_slot,
            ))
            .into_any_element(),
        div()
            .w_full()
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(frame.tokens.metrics.ui_text_xs))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(frame.i18n.t("onboarding.font_size")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(font_size_button(frame, bindings, "dec", "-", -1))
                    .child(
                        div()
                            .min_w(px(44.0))
                            .text_align(gpui::TextAlign::Center)
                            .font_family(SharedString::from(
                                FontFamily::Jetbrains.terminal_family_name(""),
                            ))
                            .text_size(px(frame.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{font_size}px")),
                    )
                    .child(font_size_button(frame, bindings, "inc", "+", 1)),
            )
            .into_any_element(),
    ];

    let selected_corner = settings.appearance.corner_style;
    let selected_radius = settings.appearance.border_radius;
    let selected_anim = settings.appearance.animation_speed;
    let corner_options = [
        (
            CornerStyle::Circular,
            "settings_view.appearance.corner_circular",
        ),
        (
            CornerStyle::Continuous,
            "settings_view.appearance.corner_continuous",
        ),
    ];
    let corner_active_index = corner_options
        .iter()
        .position(|(style, _)| *style == selected_corner)
        .unwrap_or(0);
    let anim_active_index = frame
        .animation_options
        .iter()
        .position(|(speed, _)| *speed == selected_anim)
        .unwrap_or(0);

    let detail_rows = vec![
        setting_stacked_row(
            frame,
            "settings_view.appearance.corner_style",
            compact_pill_bar(
                &frame.tokens,
                ONBOARDING_CORNER_CONTROL_ID,
                frame.has_background,
                corner_active_index,
                control_motion,
                corner_options
                    .into_iter()
                    .enumerate()
                    .map(|(index, (style, key))| {
                        let dispatch = bindings.dispatch.clone();
                        let view = view.clone();
                        let tokens = frame.tokens;
                        compact_pill_item(
                            &frame.tokens,
                            SharedString::from(format!("onboarding-corner-{index}")),
                            frame.i18n.t(key),
                            selected_corner == style,
                            move |window, cx| {
                                let _ = view.update(cx, |view, cx| {
                                    view.begin_control_motion(
                                        ONBOARDING_CORNER_CONTROL_ID,
                                        &tokens,
                                        corner_active_index,
                                        index,
                                        cx,
                                    );
                                });
                                dispatch(OnboardingRequest::SetCornerStyle(style), window, cx);
                            },
                        )
                    })
                    .collect(),
            ),
        ),
        div()
            .w_full()
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(frame.tokens.metrics.ui_text_xs))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(frame.i18n.t("settings_view.appearance.border_radius")),
            )
            .child(
                div()
                    .w(px(204.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child((bindings.radius_slider)(selected_radius, cx)),
                    )
                    .child(
                        div()
                            .min_w(px(32.0))
                            .text_align(gpui::TextAlign::Right)
                            .font_family(SharedString::from(
                                FontFamily::Jetbrains.terminal_family_name(""),
                            ))
                            .text_size(px(frame.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{selected_radius}px")),
                    ),
            )
            .into_any_element(),
        setting_stacked_row(
            frame,
            "settings_view.appearance.animation",
            compact_pill_bar(
                &frame.tokens,
                ONBOARDING_ANIMATION_CONTROL_ID,
                frame.has_background,
                anim_active_index,
                control_motion,
                frame
                    .animation_options
                    .iter()
                    .enumerate()
                    .map(|(index, (speed, label))| {
                        let speed = *speed;
                        let dispatch = bindings.dispatch.clone();
                        let view = view.clone();
                        let mut target_tokens = frame.tokens;
                        // Preview the newly selected speed immediately (e.g. Off -> Normal).
                        target_tokens.apply_motion(motion_profile_for_speed(speed));
                        compact_pill_item(
                            &frame.tokens,
                            SharedString::from(format!("onboarding-anim-{index}")),
                            label.clone(),
                            speed == selected_anim,
                            move |window, cx| {
                                let _ = view.update(cx, |view, cx| {
                                    view.begin_control_motion(
                                        ONBOARDING_ANIMATION_CONTROL_ID,
                                        &target_tokens,
                                        anim_active_index,
                                        index,
                                        cx,
                                    );
                                });
                                dispatch(OnboardingRequest::SetAnimationSpeed(speed), window, cx);
                            },
                        )
                    })
                    .collect(),
            ),
        ),
    ];

    let controls_col = div()
        .when(wide, |col| col.w(px(388.0)).flex_none())
        .when(!wide, |col| col.w_full())
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(section_header(
            &frame.tokens,
            frame.i18n.t("onboarding.appearance_title"),
            None,
        ))
        .child(grouped_surface(
            &frame.tokens,
            frame.has_background,
            theme_rows,
        ))
        .child(section_header(
            &frame.tokens,
            frame.i18n.t("onboarding.select_font"),
            None,
        ))
        .child(grouped_surface(
            &frame.tokens,
            frame.has_background,
            font_rows,
        ))
        .child(grouped_surface(
            &frame.tokens,
            frame.has_background,
            detail_rows,
        ));

    let preview_col = div()
        .min_w(px(0.0))
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(section_header(
            &frame.tokens,
            frame.preview_theme_name.clone(),
            Some(
                div()
                    .font_family(frame.mono_font_family.clone())
                    .text_size(px(frame.tokens.metrics.ui_text_2xs))
                    .text_color(rgb(theme.text_muted))
                    .child(format!(
                        "{} · {}px",
                        settings
                            .terminal
                            .font_family
                            .terminal_family_name(&settings.terminal.custom_font_family),
                        settings.terminal.font_size
                    ))
                    .into_any_element(),
            ),
        ))
        .child(render_unified_workbench_preview(frame))
        .child(render_theme_inspector_card(frame))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(frame.tokens.metrics.ui_text_xs))
                .text_color(rgb(theme.text_muted))
                .child(render_icon("lucide/info.svg", 12.5, rgb(theme.text_muted)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(frame.i18n.t("onboarding.background_image_hint")),
                ),
        );

    div()
        .w_full()
        .flex()
        .when(wide, |row| row.flex_row().gap(px(26.0)))
        .when(!wide, |col| col.flex_col().gap(px(18.0)))
        .child(controls_col)
        .child(preview_col)
        .into_any_element()
}

fn setting_stacked_row(
    frame: &OnboardingFrame,
    label_key: &str,
    control: AnyElement,
) -> AnyElement {
    let theme = frame.tokens.ui;
    div()
        .w_full()
        .px(px(12.0))
        .py(px(8.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .w_full()
                .text_size(px(frame.tokens.metrics.ui_text_xs))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text))
                .child(frame.i18n.t(label_key)),
        )
        .child(control)
        .into_any_element()
}

fn setting_inline_row(
    frame: &OnboardingFrame,
    label_key: &str,
    control: AnyElement,
    control_width: f32,
) -> AnyElement {
    let theme = frame.tokens.ui;
    div()
        .w_full()
        .px(px(12.0))
        .py(px(7.5))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(frame.tokens.metrics.ui_text_xs))
                .line_height(px(frame.tokens.metrics.ui_text_xs * 1.35))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text))
                .child(frame.i18n.t(label_key)),
        )
        .child(div().w(px(control_width)).flex_none().child(control))
        .into_any_element()
}

fn font_size_button(
    frame: &OnboardingFrame,
    bindings: &OnboardingViewBindings,
    id: &str,
    label: &'static str,
    delta: i64,
) -> AnyElement {
    let theme = frame.tokens.ui;
    let dispatch = bindings.dispatch.clone();
    button_with(
        &frame.tokens,
        label.to_string(),
        ButtonOptions {
            variant: ButtonVariant::Outline,
            size: ButtonSize::Icon,
            radius: ButtonRadius::Sm,
            disabled: false,
        },
    )
    .id(SharedString::from(format!("onboarding-font-size-{id}")))
    .size(px(24.0))
    .cursor_pointer()
    .hover(move |style| style.bg(rgb(theme.bg_hover)))
    .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
        dispatch(OnboardingRequest::AdjustFontSize(delta), window, cx);
        cx.stop_propagation();
    })
    .into_any_element()
}

fn render_unified_workbench_preview(frame: &OnboardingFrame) -> AnyElement {
    let ui = frame.preview_tokens.ui;
    let term = frame.preview_tokens.terminal;
    let mono_font = frame.mono_font_family.clone();
    let font_size = (frame.settings.terminal.font_size as f32).clamp(11.0, 18.0);
    let line_height = (font_size * 1.55).round();
    let preview_radius = (frame.settings.appearance.border_radius as f32).clamp(4.0, 16.0);

    let mini_titlebar = div()
        .h(px(32.0))
        .px(px(12.0))
        .bg(rgb(ui.shell_bg()))
        .border_b_1()
        .border_color(rgba((ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .child(window_dot(ui.border))
                        .child(window_dot(ui.border))
                        .child(window_dot(ui.border)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .px(px(9.0))
                                .py(px(3.5))
                                .rounded(px(frame.tokens.radii.sm))
                                .bg(rgb(ui.bg))
                                .border_1()
                                .border_color(rgba((ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
                                .text_size(px(frame.tokens.metrics.ui_text_2xs))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(ui.text_heading))
                                .child("oxide@prod"),
                        )
                        .child(
                            div()
                                .px(px(8.0))
                                .py(px(3.5))
                                .text_size(px(frame.tokens.metrics.ui_text_2xs))
                                .text_color(rgb(ui.text_muted))
                                .child("SFTP"),
                        ),
                ),
        )
        .child(
            div()
                .text_size(px(frame.tokens.metrics.ui_text_2xs))
                .text_color(rgb(ui.text_muted))
                .child("oxide@prod:~/services"),
        );

    let mini_sidebar = div()
        .w(px(114.0))
        .flex_none()
        .h_full()
        .bg(rgb(ui.shell_bg()))
        .border_r_1()
        .border_color(rgba((ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
        .p(px(8.0))
        .flex()
        .flex_col()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(5.0))
                        .rounded(px(frame.tokens.radii.sm))
                        .bg(rgb(ui.bg_active))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().size(px(6.0)).rounded_full().bg(rgb(ui.accent)))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(frame.tokens.metrics.ui_text_2xs))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(ui.text_heading))
                                .child("prod-edge"),
                        ),
                )
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(5.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().size(px(6.0)).rounded_full().bg(rgb(ui.border)))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(frame.tokens.metrics.ui_text_2xs))
                                .text_color(rgb(ui.text_muted))
                                .child("staging-db"),
                        ),
                ),
        )
        .child(
            div()
                .w_full()
                .py(px(5.0))
                .rounded(px(frame.tokens.radii.sm))
                .bg(rgb(ui.accent))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(frame.tokens.metrics.ui_text_2xs))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(ui.accent_text))
                .child(frame.i18n.t("layout.empty.new_connection")),
        );

    let terminal_canvas = div()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .bg(rgb(term.background))
        .p(px(16.0))
        .font_family(mono_font)
        .text_size(px(font_size))
        .line_height(px(line_height))
        .text_color(rgb(term.foreground))
        .flex()
        .flex_col()
        .gap(px(5.0))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.0))
                .child(div().text_color(rgb(term.green)).child("oxide@prod"))
                .child(div().text_color(rgb(term.cyan)).child("~/services"))
                .child(
                    div()
                        .text_color(rgb(term.foreground))
                        .child("ssh edge-01 => status"),
                ),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(10.0))
                .child(div().text_color(rgb(term.bright_green)).child("● active"))
                .child(div().text_color(rgb(term.blue)).child("sftp+tunnel"))
                .child(div().text_color(rgb(term.yellow)).child("12ms RTT"))
                .child(
                    div()
                        .text_color(rgb(term.magenta))
                        .child("Aa 01 => 中文对齐"),
                ),
        )
        .child(div().h(px(6.0)))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.0))
                .child(div().text_color(rgb(term.cyan)).child("~/project"))
                .child(div().text_color(rgb(term.foreground)).child("git status")),
        )
        .child(
            div()
                .text_color(rgb(term.green))
                .child("  new: src/theme.rs"),
        )
        .child(
            div()
                .text_color(rgb(term.yellow))
                .child("  mod: Cargo.toml"),
        )
        .child(
            div()
                .text_color(rgb(term.red))
                .child("  del: old-theme.toml"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(div().text_color(rgb(term.bright_black)).child(">"))
                .child(div().w(px(7.0)).h(px(font_size)).bg(rgb(term.cursor))),
        );

    div()
        .w_full()
        .flex_1()
        .min_h(px(290.0))
        .rounded(px(preview_radius))
        .border_1()
        .border_color(rgba((ui.border << 8) | ONBOARDING_CARD_BORDER_ALPHA))
        .bg(rgb(ui.bg))
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(mini_titlebar)
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h(px(258.0))
                .flex()
                .flex_row()
                .child(mini_sidebar)
                .child(terminal_canvas),
        )
        .into_any_element()
}

fn render_theme_inspector_card(frame: &OnboardingFrame) -> AnyElement {
    let ui = frame.preview_tokens.ui;
    let term = frame.preview_tokens.terminal;
    let theme = frame.tokens.ui;
    let corner_key = match frame.settings.appearance.corner_style {
        CornerStyle::Circular => "settings_view.appearance.corner_circular",
        CornerStyle::Continuous => "settings_view.appearance.corner_continuous",
    };
    let radius = frame.settings.appearance.border_radius;
    let preview_radius = (radius as f32).clamp(2.0, 12.0);
    let ansi_colors = [
        term.black,
        term.red,
        term.green,
        term.yellow,
        term.blue,
        term.magenta,
        term.cyan,
        term.white,
        term.bright_black,
        term.bright_red,
        term.bright_green,
        term.bright_yellow,
        term.bright_blue,
        term.bright_magenta,
        term.bright_cyan,
        term.bright_white,
    ];
    let swatch = |color: u32, size_px: f32, r: f32| {
        div()
            .size(px(size_px))
            .flex_none()
            .rounded(px(r))
            .border_1()
            .border_color(rgba((ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
            .bg(rgb(color))
    };

    let rows = vec![
        div()
            .w_full()
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_none()
                    .text_size(px(frame.tokens.metrics.ui_text_xs))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(frame.i18n.t("settings_view.appearance.color_theme")),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(4.0))
                    .children(
                        ansi_colors
                            .into_iter()
                            .map(|color| swatch(color, 13.0, frame.tokens.radii.xs)),
                    ),
            )
            .into_any_element(),
        div()
            .w_full()
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(frame.tokens.metrics.ui_text_xs))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(frame.i18n.t("settings_view.appearance.corner_style")),
                    )
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(2.5))
                            .rounded(px(preview_radius))
                            .border_1()
                            .border_color(rgba((ui.border << 8) | ONBOARDING_CARD_BORDER_ALPHA))
                            .bg(rgb(ui.bg_sunken))
                            .font_family(frame.mono_font_family.clone())
                            .text_size(px(frame.tokens.metrics.ui_text_2xs))
                            .text_color(rgb(ui.text))
                            .child(format!("{} · {}px", frame.i18n.t(corner_key), radius)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .children(
                        [ui.accent, ui.success, ui.warning, ui.error]
                            .into_iter()
                            .map(|color| swatch(color, 13.0, preview_radius.min(6.0))),
                    ),
            )
            .into_any_element(),
    ];

    grouped_surface(&frame.tokens, frame.has_background, rows).into_any_element()
}

fn window_dot(color: u32) -> AnyElement {
    div()
        .size(px(7.0))
        .rounded_full()
        .bg(rgba((color << 8) | 0x99))
        .into_any_element()
}
