// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{AnyElement, App, MouseButton, div, prelude::*, px, relative, rgb};
use oxideterm_gpui_ui::icons::LucideIcon;
use oxideterm_i18n::I18n;
use oxideterm_settings::SftpPresentationPreference;
use oxideterm_theme::ThemeTokens;
use std::rc::Rc;

use crate::{browser::render_lucide_icon, style::SftpUiMetrics};

pub fn render_presentation_choice(
    tokens: &ThemeTokens,
    i18n: &I18n,
    dispatch: Rc<dyn Fn(Option<SftpPresentationPreference>, &mut App)>,
) -> AnyElement {
    let ui = SftpUiMetrics::from_tokens(tokens);
    let theme = tokens.ui;
    let choice = |preference, icon, title_key, description_key| {
        div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded(px(tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .cursor_pointer()
            .hover(move |card| card.border_color(rgb(theme.accent)).bg(rgb(theme.bg_hover)))
            .child(render_lucide_icon(icon, 20.0, rgb(theme.accent)))
            .child(
                div()
                    .text_size(px(ui.text_sm))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text))
                    .child(i18n.t(title_key)),
            )
            .child(
                div()
                    .text_size(px(ui.text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(i18n.t(description_key)),
            )
            .on_mouse_down(MouseButton::Left, {
                let dispatch = dispatch.clone();
                move |_, _, cx| {
                    dispatch(Some(preference), cx);
                    cx.stop_propagation();
                }
            })
    };
    {
        oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
            .on_mouse_down(MouseButton::Left, {
                let dispatch = dispatch.clone();
                move |_, _, cx| {
                    dispatch(None, cx);
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .w(px(520.0))
                    .max_w(relative(0.9))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .rounded(px(tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_elevated))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .text_size(px(ui.size(18.0)))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text))
                            .child(i18n.t("sftp.presentation.title")),
                    )
                    .child(
                        div()
                            .text_size(px(ui.text_sm))
                            .text_color(rgb(theme.text_muted))
                            .child(i18n.t("sftp.presentation.description")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(choice(
                                oxideterm_settings::SftpPresentationPreference::Tab,
                                LucideIcon::ExternalLink,
                                "sftp.presentation.tab",
                                "sftp.presentation.tab_description",
                            ))
                            .child(choice(
                                oxideterm_settings::SftpPresentationPreference::Sidebar,
                                LucideIcon::PanelLeft,
                                "sftp.presentation.sidebar",
                                "sftp.presentation.sidebar_description",
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(ui.text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(i18n.t("sftp.presentation.remember_hint")),
                    ),
            )
            .into_any_element()
    }
}
