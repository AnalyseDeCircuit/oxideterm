use super::*;
use oxideterm_gpui_ui::checkbox;

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
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(Self::render_lucide_icon(
                        icon,
                        20.0,
                        rgb(self.tokens.ui.accent),
                    )),
            )
            .child(
                div()
                    .pt(px(1.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text))
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
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                icon,
                                ONBOARDING_ICON_SIZE,
                                rgb(self.tokens.ui.accent),
                            ))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t(title_key)),
                            ),
                    )
                    .when_some(hint_key, |row, key| {
                        row.child(
                            div()
                                .text_size(px(10.0))
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
        self.onboarding_info_card(
            Some((icon, self.tokens.ui.accent)),
            &format!("onboarding.{key}"),
            Some(&format!("onboarding.{key}_desc")),
            false,
            _cx,
        )
    }

    pub(in crate::workspace) fn onboarding_feature_card(
        &self,
        icon: LucideIcon,
        key: &str,
        badge: Option<&str>,
        highlight: bool,
    ) -> AnyElement {
        let border = if highlight {
            rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_STRONG_BORDER_ALPHA)
        } else {
            rgb(self.tokens.ui.border)
        };
        div()
            .flex()
            .gap(px(10.0))
            .p(px(14.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(border)
            .bg(if highlight {
                rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
            } else {
                rgb(self.tokens.ui.bg_card)
            })
            .child(Self::render_lucide_icon(
                icon,
                16.0,
                rgb(self.tokens.ui.accent),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t(&format!("onboarding.{key}"))),
                            )
                            .when_some(badge, |row, badge| {
                                row.child(
                                    div()
                                        .rounded(px(self.tokens.radii.xs))
                                        .border_1()
                                        .border_color(rgb(self.tokens.ui.border))
                                        .px(px(4.0))
                                        .py(px(1.0))
                                        .text_size(px(9.0))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(badge.to_string()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(&format!("onboarding.{key}_desc"))),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_timeline_item(
        &self,
        number: usize,
        icon: LucideIcon,
        key: &str,
        has_line: bool,
    ) -> AnyElement {
        div()
            .relative()
            .flex()
            .items_start()
            .gap(px(12.0))
            .when(has_line, |row| {
                row.child(
                    div()
                        .absolute()
                        .left(px(13.0))
                        .top(px(28.0))
                        .w(px(1.0))
                        .h(px(44.0))
                        .bg(rgb(self.tokens.ui.border)),
                )
            })
            .child(
                div()
                    .relative()
                    .mt(px(6.0))
                    .size(px(28.0))
                    .rounded_full()
                    .border_1()
                    .border_color(rgba(
                        (self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA,
                    ))
                    .bg(rgba(
                        (self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA,
                    ))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.accent))
                    .child(number.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .pb(px(16.0))
                    .child(self.onboarding_info_card_with_text(
                        Some((icon, self.tokens.ui.accent)),
                        self.i18n.t(&format!("onboarding.{key}")),
                        self.i18n.t(&format!("onboarding.{key}_desc")),
                        false,
                    )),
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
        let mut list = div().flex().flex_col().gap(px(6.0));
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
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgba((self.tokens.ui.bg_card << 8) | ONBOARDING_CARD_ALPHA))
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.accent),
            ))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text))
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
            .items_center()
            .gap(px(12.0))
            .p(px(14.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if checked {
                rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
            } else {
                rgb(self.tokens.ui.border)
            })
            .bg(if checked {
                rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
            } else {
                rgb(self.tokens.ui.bg_card)
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
            .child(checkbox::checkbox(&self.tokens, String::new(), checked))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(Self::render_lucide_icon(
                if checked {
                    LucideIcon::CheckCircle
                } else {
                    LucideIcon::Circle
                },
                20.0,
                if checked {
                    rgb(self.tokens.ui.accent)
                } else {
                    rgb(self.tokens.ui.text_muted)
                },
            ))
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
        accent: bool,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.i18n.t(title_key);
        let detail = detail_key.map(|key| self.i18n.t(key)).unwrap_or_default();
        self.onboarding_info_card_with_text(icon, title, detail, accent)
    }

    pub(in crate::workspace) fn onboarding_info_card_with_text(
        &self,
        icon: Option<(LucideIcon, u32)>,
        title: String,
        detail: String,
        accent: bool,
    ) -> AnyElement {
        div()
            .flex()
            .gap(px(10.0))
            .p(px(14.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if accent {
                rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
            } else {
                rgb(self.tokens.ui.border)
            })
            .bg(if accent {
                rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
            } else {
                rgba((self.tokens.ui.bg_card << 8) | ONBOARDING_CARD_ALPHA)
            })
            .when_some(icon, |card, (icon, color)| {
                card.child(Self::render_lucide_icon(icon, 16.0, rgb(color)))
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(title),
                    )
                    .when(!detail.is_empty(), |column| {
                        column.child(
                            div()
                                .text_size(px(11.0))
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
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .cursor(CursorStyle::PointingHand)
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(
                div()
                    .flex_1()
                    .text_size(px(11.0))
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
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA,
            ))
            .bg(rgba(
                (self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA,
            ))
            .child(Self::render_lucide_icon(
                LucideIcon::Info,
                14.0,
                rgb(self.tokens.ui.accent),
            ))
            .child(
                div()
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

pub(in crate::workspace) fn traffic_dot(color: u32) -> AnyElement {
    div()
        .size(px(8.0))
        .rounded_full()
        .bg(rgb(color))
        .into_any_element()
}

pub(in crate::workspace) fn format_theme_label(theme_id: &str) -> String {
    theme_id
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
