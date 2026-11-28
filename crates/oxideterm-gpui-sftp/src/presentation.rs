// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{AnyElement, App, MouseButton, div, prelude::*, px, relative, rgb};
use oxideterm_gpui_ui::icons::LucideIcon;
use oxideterm_i18n::I18n;
use oxideterm_settings::SftpPresentationPreference;
use oxideterm_theme::ThemeTokens;
use std::rc::Rc;

use crate::{browser::render_lucide_icon, style::SftpUiMetrics};

pub fn sftp_open_presentation(
    preferred: oxideterm_settings::SftpPresentationPreference,
    existing_tab: bool,
    sftp_tab_active: bool,
) -> oxideterm_settings::SftpPresentationPreference {
    // The sidebar is hidden while an SFTP tab is active and must not take over
    // its shared entity. Existing tabs also retain their detached-window routing.
    if existing_tab || sftp_tab_active {
        oxideterm_settings::SftpPresentationPreference::Tab
    } else {
        preferred
    }
}

pub fn sidebar_sftp_target<T: Clone>(
    opened: Option<&T>,
    pinned: bool,
    focused: Option<T>,
) -> Option<T> {
    let opened = opened?;
    if pinned {
        Some(opened.clone())
    } else {
        focused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sftp_presentation_reuses_tabs_without_changing_first_open_preference() {
        use oxideterm_settings::SftpPresentationPreference::{Ask, Sidebar, Tab};

        for preference in [Ask, Sidebar, Tab] {
            assert_eq!(sftp_open_presentation(preference, false, false), preference);
            // Reuse an existing tab even when another tab or window is focused.
            assert_eq!(sftp_open_presentation(preference, true, false), Tab);
            // A different node must not activate a hidden sidebar either.
            assert_eq!(sftp_open_presentation(preference, false, true), Tab);
        }
    }

    #[test]
    fn sidebar_follows_focus_unless_pinned_and_stays_closed_until_opened() {
        let a = String::from("a");
        let b = String::from("b");
        assert_eq!(
            sidebar_sftp_target(Some(&a), false, Some(b.clone())),
            Some(b.clone())
        );
        assert_eq!(
            sidebar_sftp_target(Some(&a), true, Some(b.clone())),
            Some(a.clone())
        );
        assert_eq!(sidebar_sftp_target(Some(&a), false, None), None);
        assert_eq!(sidebar_sftp_target(Some(&a), true, None), Some(a));
        assert_eq!(sidebar_sftp_target(None, false, Some(b)), None);
    }
}

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
