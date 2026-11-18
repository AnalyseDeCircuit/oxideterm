// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{AnyElement, FontWeight, ObjectFit, Styled, div, prelude::*, px, rgb, rgba};

use crate::{
    OnboardingFrame,
    widgets::{
        ONBOARDING_CARD_BORDER_ALPHA, ONBOARDING_HAIRLINE_ALPHA, grouped_surface, hairline,
        render_icon, section_header, sunken_bg,
    },
};

const OVERVIEW_ICON_SIZE: f32 = 42.0;
const OVERVIEW_ICON_RADIUS: f32 = 10.0;

pub(crate) fn render_overview_step(frame: &OnboardingFrame, wide: bool) -> AnyElement {
    let theme = frame.tokens.ui;
    let wordmark_size = frame.tokens.metrics.welcome_wordmark_font_size;

    let brand_badges = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .child(pillar_pill(
            frame,
            "lucide/code-2.svg",
            "onboarding.badge_open_source",
        ))
        .child(pillar_pill(
            frame,
            "lucide/cpu.svg",
            "onboarding.badge_pure_rust",
        ))
        .child(pillar_pill(
            frame,
            "lucide/zap.svg",
            "onboarding.badge_no_webview",
        ));

    let hero = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .w_full()
                .flex()
                .when(wide, |row| row.flex_row().items_center().justify_between())
                .when(!wide, |col| col.flex_col().items_start())
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .size(px(OVERVIEW_ICON_SIZE))
                                .relative()
                                .flex_none()
                                .rounded(px(OVERVIEW_ICON_RADIUS))
                                .overflow_hidden()
                                .child(
                                    gpui::img(frame.app_icon_source.clone())
                                        .size(px(OVERVIEW_ICON_SIZE))
                                        .object_fit(ObjectFit::Contain),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .rounded(px(OVERVIEW_ICON_RADIUS))
                                        .border_1()
                                        .border_color(rgba(
                                            (theme.border << 8) | ONBOARDING_CARD_BORDER_ALPHA,
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(wordmark_size))
                                        .line_height(px(wordmark_size + 4.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(theme.text_heading))
                                        .child("OxideTerm"),
                                )
                                .child(
                                    div()
                                        .text_size(px(frame.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(frame.i18n.t("onboarding.subtitle")),
                                ),
                        ),
                )
                .child(brand_badges),
        )
        .child(
            div()
                .text_size(px(frame.tokens.metrics.ui_text_xs))
                .line_height(px(frame.tokens.metrics.ui_text_xs * 1.5))
                .text_color(rgb(theme.text))
                .child(frame.i18n.t("onboarding.overview_intro")),
        );

    let connections_group = feature_category(
        frame,
        "onboarding.cat_connections",
        [
            ("lucide/network.svg", "multiplexing"),
            ("lucide/refresh-cw.svg", "reconnect"),
            ("lucide/arrow-left-right.svg", "port_forwarding"),
            ("lucide/monitor.svg", "remote_desktop"),
        ],
    );

    let files_dev_group = feature_category(
        frame,
        "onboarding.cat_files_dev",
        [
            ("lucide/folder-sync.svg", "sftp"),
            ("lucide/folder-open.svg", "local_file_manager"),
            ("lucide/code-2.svg", "remote_ide"),
            ("lucide/cable.svg", "serial_console"),
        ],
    );

    let ops_ai_group = feature_category(
        frame,
        "onboarding.cat_ops_ai",
        [
            ("lucide/activity.svg", "host_tools"),
            ("lucide/sparkles.svg", "ai_chat"),
            ("lucide/book-open.svg", "knowledge_base"),
            ("lucide/zap.svg", "quick_commands"),
        ],
    );

    let security_ext_group = feature_category(
        frame,
        "onboarding.cat_security_ext",
        [
            ("lucide/lock.svg", "security"),
            ("lucide/cloud.svg", "cloud_sync"),
            ("lucide/package.svg", "portable_oxide"),
            ("lucide/puzzle.svg", "plugin_system"),
        ],
    );

    let columns = div()
        .w_full()
        .grid()
        .grid_cols(if wide { 2 } else { 1 })
        .gap_x(px(if wide { 20.0 } else { 14.0 }))
        .gap_y(px(14.0))
        .child(connections_group)
        .child(ops_ai_group)
        .child(files_dev_group)
        .child(security_ext_group);

    div()
        .id("onboarding-overview-page")
        .debug_selector(|| "onboarding-overview-page".into())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(hero)
        .child(hairline(&frame.tokens))
        .child(columns)
        .into_any_element()
}

fn pillar_pill(frame: &OnboardingFrame, icon: &'static str, label_key: &str) -> AnyElement {
    let theme = frame.tokens.ui;
    div()
        .px(px(8.0))
        .py(px(4.0))
        .rounded(px(frame.tokens.radii.md))
        .border_1()
        .border_color(rgba((theme.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
        .bg(sunken_bg(&frame.tokens, frame.has_background))
        .flex()
        .items_center()
        .gap(px(5.0))
        .child(render_icon(icon, 11.5, rgb(theme.accent)))
        .child(
            div()
                .text_size(px(frame.tokens.metrics.ui_text_2xs))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text_heading))
                .child(frame.i18n.t(label_key)),
        )
        .into_any_element()
}

fn feature_category(
    frame: &OnboardingFrame,
    title_key: &str,
    items: [(&'static str, &'static str); 4],
) -> AnyElement {
    let theme = frame.tokens.ui;
    let rows = items
        .into_iter()
        .map(|(icon, key)| {
            div()
                .w_full()
                .flex_1()
                .px(px(12.0))
                .py(px(7.5))
                .flex()
                .items_start()
                .gap(px(10.0))
                .child(
                    div()
                        .pt(px(2.0))
                        .flex_none()
                        .child(render_icon(icon, 13.5, rgb(theme.accent))),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(1.5))
                        .child(
                            div()
                                .text_size(px(frame.tokens.metrics.ui_text_xs))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(theme.text_heading))
                                .child(frame.i18n.t(&format!("onboarding.{key}"))),
                        )
                        .child(
                            div()
                                .text_size(px(frame.tokens.metrics.ui_text_2xs))
                                .line_height(px(frame.tokens.metrics.ui_text_2xs * 1.4))
                                .text_color(rgb(theme.text_muted))
                                .child(frame.i18n.t(&format!("onboarding.{key}_desc"))),
                        ),
                )
                .into_any_element()
        })
        .collect::<Vec<_>>();

    div()
        .w_full()
        .h_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(section_header(&frame.tokens, frame.i18n.t(title_key), None))
        .child(grouped_surface(&frame.tokens, frame.has_background, rows).flex_1())
        .into_any_element()
}
