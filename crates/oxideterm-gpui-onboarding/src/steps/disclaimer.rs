// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{AnyElement, FontWeight, SharedString, Styled, div, prelude::*, px, rgb, rgba};
use oxideterm_gpui_ui::checkbox;

use crate::{
    OnboardingFrame, OnboardingRequest, OnboardingViewBindings,
    widgets::{
        ONBOARDING_ACCENT_BORDER_ALPHA, ONBOARDING_ACCENT_SUBTLE_ALPHA, ONBOARDING_CARD_BORDER_ALPHA,
        card_bg, grouped_surface, render_icon, section_header,
    },
};

const DISCLAIMER_CARD_MAX_WIDTH: f32 = 560.0;

pub(crate) fn render_disclaimer_step(
    frame: &OnboardingFrame,
    bindings: &OnboardingViewBindings,
) -> AnyElement {
    let theme = frame.tokens.ui;
    let accepted = frame.disclaimer_accepted;
    let dispatch_legal = bindings.dispatch.clone();

    let notice_rows = vec![
        disclaimer_row(
            frame,
            "lucide/alert-triangle.svg",
            theme.warning,
            "onboarding.disclaimer_no_warranty",
            "onboarding.disclaimer_no_warranty_text",
        ),
        disclaimer_row(
            frame,
            "lucide/shield.svg",
            theme.text_muted,
            "onboarding.disclaimer_data_security",
            "onboarding.disclaimer_data_security_text",
        ),
        disclaimer_row(
            frame,
            "lucide/brain.svg",
            theme.text_muted,
            "onboarding.disclaimer_ai",
            "onboarding.disclaimer_ai_text",
        ),
        div()
            .id("onboarding-disclaimer-gpl-link")
            .w_full()
            .px(px(14.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(theme.bg_hover)))
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                dispatch_legal(OnboardingRequest::OpenLegalNotice, window, cx);
                cx.stop_propagation();
            })
            .child(render_icon("lucide/external-link.svg", 13.0, rgb(theme.accent)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(frame.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(frame.i18n.t("onboarding.disclaimer_gpl_note")),
            )
            .into_any_element(),
    ];

    let dispatch_accept = bindings.dispatch.clone();
    let accept_row = div()
        .id("onboarding-disclaimer-accept-row")
        .debug_selector(|| "onboarding-disclaimer-accept-row".into())
        .w_full()
        .rounded(px(frame.tokens.radii.md))
        .border_1()
        .border_color(if accepted {
            rgba((theme.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
        } else {
            rgba((theme.border << 8) | ONBOARDING_CARD_BORDER_ALPHA)
        })
        .bg(if accepted {
            rgba((theme.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
        } else {
            card_bg(&frame.tokens, frame.has_background)
        })
        .hover(move |style| style.bg(rgb(theme.bg_hover)))
        .px(px(14.0))
        .py(px(10.0))
        .cursor_pointer()
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            dispatch_accept(OnboardingRequest::ToggleDisclaimer, window, cx);
            cx.stop_propagation();
        })
        .child(checkbox::checkbox(
            &frame.tokens,
            SharedString::from("onboarding-disclaimer-accept"),
            frame.i18n.t("onboarding.disclaimer_accept"),
            accepted,
        ));

    div()
        .w_full()
        .flex()
        .justify_center()
        .child(
            div()
                .w_full()
                .max_w(px(DISCLAIMER_CARD_MAX_WIDTH))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(section_header(
                            &frame.tokens,
                            frame.i18n.t("onboarding.disclaimer_title"),
                            Some(
                                div()
                                    .text_size(px(frame.tokens.metrics.ui_text_2xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child("GPL-3.0")
                                    .into_any_element(),
                            ),
                        ))
                        .child(
                            div()
                                .text_size(px(frame.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(frame.i18n.t("onboarding.disclaimer_desc")),
                        ),
                )
                .child(grouped_surface(&frame.tokens, frame.has_background, notice_rows))
                .child(accept_row),
        )
        .into_any_element()
}

fn disclaimer_row(
    frame: &OnboardingFrame,
    icon: &'static str,
    icon_color: u32,
    title_key: &str,
    detail_key: &str,
) -> AnyElement {
    let theme = frame.tokens.ui;
    div()
        .w_full()
        .px(px(14.0))
        .py(px(11.0))
        .flex()
        .items_start()
        .gap(px(10.0))
        .child(
            div()
                .pt(px(2.0))
                .flex_none()
                .child(render_icon(icon, 14.0, rgb(icon_color))),
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
                        .line_height(px(frame.tokens.metrics.ui_text_xs * 1.45))
                        .text_color(rgb(theme.text_muted))
                        .child(frame.i18n.t(detail_key)),
                ),
        )
        .into_any_element()
}
