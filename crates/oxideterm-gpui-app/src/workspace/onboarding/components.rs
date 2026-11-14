use super::*;
use oxideterm_gpui_ui::{ActionSlotRowOptions, action_slot_row, checkbox};

impl WorkspaceApp {
    pub(in crate::workspace) fn onboarding_step_heading(
        &self,
        icon: LucideIcon,
        title_key: &str,
        desc_key: &str,
    ) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(
                div()
                    .size(px(ONBOARDING_STEP_ICON_SLOT))
                    .mt(px(1.0))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
                    .bg(rgb(self.tokens.ui.bg_sunken))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(Self::render_lucide_icon(
                        icon,
                        ONBOARDING_ICON_SIZE,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(desc_key)),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_section(
        &self,
        icon: LucideIcon,
        title_key: &str,
        hint_key: Option<&str>,
        body: AnyElement,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .child(Self::render_lucide_icon(
                                icon,
                                14.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child(self.i18n.t(title_key)),
                            ),
                    )
                    .when_some(hint_key, |row, key| {
                        row.child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t(key)),
                        )
                    }),
            )
            .child(body)
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_feature_tile(
        &self,
        icon: LucideIcon,
        key: &'static str,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
            .bg(rgb(self.tokens.ui.bg_panel))
            .p(px(12.0))
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(
                div()
                    .size(px(24.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(self.tokens.ui.bg_sunken))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::render_lucide_icon(
                        icon,
                        13.0,
                        rgb(self.tokens.ui.text_muted),
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
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(&format!("onboarding.{key}"))),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(&format!("onboarding.{key}_desc"))),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_feature_card(
        &self,
        icon: LucideIcon,
        key: &str,
        badge: Option<&str>,
        highlight: bool,
    ) -> AnyElement {
        let trailing = badge
            .map(|badge| {
                div()
                    .rounded(px(self.tokens.radii.xs))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
                    .bg(rgb(self.tokens.ui.bg_sunken))
                    .px(px(5.0))
                    .py(px(1.0))
                    .font_family(SharedString::from(
                        FontFamily::Jetbrains.terminal_family_name(""),
                    ))
                    .text_size(px(self.tokens.metrics.ui_text_2xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(badge.to_string())
                    .into_any_element()
            })
            .into_iter()
            .collect::<Vec<_>>();
        // The shared body slot owns the flexible width so localized titles do
        // not collapse to their minimum-content width inside the two-column grid.
        let content = div()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(&format!("onboarding.{key}"))),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(&format!("onboarding.{key}_desc"))),
            )
            .into_any_element();
        div()
            .py(px(9.0))
            .px(px(11.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if highlight {
                rgb(self.tokens.ui.border)
            } else {
                rgba((self.tokens.ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA)
            })
            .bg(if highlight {
                rgb(self.tokens.ui.bg_sunken)
            } else {
                rgb(self.tokens.ui.bg_panel)
            })
            .child(action_slot_row(
                &self.tokens,
                ActionSlotRowOptions::new().align_start().gap(9.0),
                Some(Self::render_lucide_icon(
                    icon,
                    15.0,
                    rgb(if highlight {
                        self.tokens.ui.text
                    } else {
                        self.tokens.ui.text_muted
                    }),
                )),
                content,
                trailing,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_timeline_item(
        &self,
        number: usize,
        icon: LucideIcon,
        key: &str,
    ) -> AnyElement {
        div()
            .min_w(px(0.0))
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(
                div()
                    .size(px(22.0))
                    .mt(px(1.0))
                    .flex_none()
                    .rounded_full()
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
                    .bg(rgb(self.tokens.ui.bg_sunken))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(SharedString::from(
                        FontFamily::Jetbrains.terminal_family_name(""),
                    ))
                    .text_size(px(self.tokens.metrics.ui_text_2xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(number.to_string()),
            )
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
                            .child(Self::render_lucide_icon(
                                icon,
                                14.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t(&format!("onboarding.{key}"))),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(&format!("onboarding.{key}_desc"))),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_capability_grid(
        &self,
        title_key: &str,
        items: &[(LucideIcon, &str)],
    ) -> AnyElement {
        let mut grid = div().grid().grid_cols(2).gap(px(8.0));
        for (icon, key) in items {
            grid = grid.child(self.onboarding_capability_chip(*icon, key));
        }
        self.onboarding_section(
            LucideIcon::ListChecks,
            title_key,
            None,
            grid.into_any_element(),
        )
    }

    pub(in crate::workspace) fn onboarding_capability_list(
        &self,
        title_key: &str,
        items: &[(LucideIcon, &str)],
    ) -> AnyElement {
        let mut list = div().flex().flex_col().gap(px(4.0));
        for (icon, key) in items {
            list = list.child(self.onboarding_capability_chip(*icon, key));
        }
        self.onboarding_section(LucideIcon::Shield, title_key, None, list.into_any_element())
    }

    pub(in crate::workspace) fn onboarding_capability_chip(
        &self,
        icon: LucideIcon,
        key: &str,
    ) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(7.0))
            .py(px(4.0))
            .child(
                div().pt(px(1.0)).flex_none().child(Self::render_lucide_icon(
                    icon,
                    13.0,
                    rgb(self.tokens.ui.text_muted),
                )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(&format!("onboarding.{key}"))),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_toggle_card(
        &self,
        title_key: &str,
        hint_key: &str,
        checked: bool,
        enabled: bool,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if checked {
                rgb(self.tokens.ui.accent)
            } else {
                rgb(self.tokens.ui.border)
            })
            .bg(if checked {
                rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
            } else {
                rgb(self.tokens.ui.bg_panel)
            })
            .when(enabled, |card| {
                card.hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            })
            .opacity(if enabled {
                1.0
            } else {
                ONBOARDING_DISABLED_OPACITY
            })
            .cursor(if enabled {
                CursorStyle::PointingHand
            } else {
                CursorStyle::OperationNotAllowed
            })
            .child(div().pt(px(1.0)).flex_none().child(checkbox::checkbox(
                &self.tokens,
                title_key.to_owned(),
                String::new(),
                checked,
            )))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if enabled {
                        action(this, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_info_card(
        &self,
        icon: Option<(LucideIcon, u32)>,
        title_key: &str,
        detail_key: Option<&str>,
        _accent: bool,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.i18n.t(title_key);
        let detail = detail_key.map(|key| self.i18n.t(key)).unwrap_or_default();
        self.onboarding_info_card_with_text(icon, title, detail)
    }

    pub(in crate::workspace) fn onboarding_info_card_with_text(
        &self,
        icon: Option<(LucideIcon, u32)>,
        title: String,
        detail: String,
    ) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(10.0))
            .py(px(6.0))
            .when_some(icon, |card, (icon, color)| {
                card.child(div().pt(px(2.0)).flex_none().child(
                    Self::render_lucide_icon(icon, 15.0, rgb(color)),
                ))
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(title),
                    )
                    .when(!detail.is_empty(), |column| {
                        column.child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(detail),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_clickable_card(
        &self,
        icon: LucideIcon,
        text: String,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .cursor(CursorStyle::PointingHand)
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(
                div()
                    .flex_1()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(text),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    action(this, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_tip(
        &self,
        key: &str,
        replacements: &[(&str, String)],
    ) -> AnyElement {
        let text = self.onboarding_i18n_with(key, replacements);
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(9.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .child(div().pt(px(1.0)).flex_none().child(Self::render_lucide_icon(
                LucideIcon::Info,
                14.0,
                rgb(self.tokens.ui.text_muted),
            )))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(text),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_i18n_with(
        &self,
        key: &str,
        replacements: &[(&str, String)],
    ) -> String {
        let mut text = self.i18n.t(key);
        for (name, value) in replacements {
            text = text.replace(&format!("{{{{{name}}}}}"), value);
        }
        text
    }

    pub(in crate::workspace) fn open_onboarding_disclaimer(&mut self, cx: &mut Context<Self>) {
        self.open_help_legal_notice(cx);
    }
}

pub(in crate::workspace) fn platform_cmd(suffix: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{suffix}")
    } else {
        format!("Ctrl+{suffix}")
    }
}
