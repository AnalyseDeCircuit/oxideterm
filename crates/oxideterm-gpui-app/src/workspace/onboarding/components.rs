use super::*;
use oxideterm_gpui_ui::checkbox;

impl WorkspaceApp {
    pub(in crate::workspace) fn onboarding_card_bg(&self) -> Rgba {
        let theme = self.tokens.ui;
        let base = if theme.is_light() {
            theme.bg_panel
        } else {
            theme.bg_card
        };
        if self.window_background_preferences().is_some() {
            rgba((base << 8) | oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens))
        } else {
            rgb(base)
        }
    }

    pub(in crate::workspace) fn onboarding_sunken_bg(&self) -> Rgba {
        let theme = self.tokens.ui;
        if self.window_background_preferences().is_some() {
            self.workspace_chrome_background(theme.bg_sunken)
        } else {
            rgb(theme.bg_sunken)
        }
    }

    pub(in crate::workspace) fn onboarding_step_heading(
        &self,
        title_key: &str,
        desc_key: &str,
    ) -> AnyElement {
        div()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
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
                    .line_height(px(self.tokens.metrics.ui_text_xs * 1.45))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(desc_key)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_section(
        &self,
        title_key: &str,
        hint_key: Option<&str>,
        body: AnyElement,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child(self.i18n.t(title_key)),
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

    pub(in crate::workspace) fn onboarding_hairline(&self) -> AnyElement {
        div()
            .h(px(1.0))
            .w_full()
            .bg(rgba(
                (self.tokens.ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_feature_tile(
        &self,
        icon: LucideIcon,
        key: &'static str,
    ) -> AnyElement {
        div()
            .min_w(px(0.0))
            .flex()
            .items_start()
            .gap(px(10.0))
            .py(px(4.0))
            .child(
                div()
                    .size(px(28.0))
                    .mt(px(1.0))
                    .flex_none()
                    .rounded(px(self.tokens.radii.md))
                    .bg(rgba(
                        (self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA,
                    ))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::render_lucide_icon(
                        icon,
                        14.0,
                        rgb(self.tokens.ui.accent),
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
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child(self.i18n.t(&format!("onboarding.{key}"))),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(self.tokens.metrics.ui_text_xs * 1.45))
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
        let mut grid = div().grid().grid_cols(2).gap_x(px(12.0)).gap_y(px(6.0));
        for (icon, key) in items {
            grid = grid.child(self.onboarding_capability_chip(*icon, key));
        }
        self.onboarding_group_label(title_key, grid.into_any_element())
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
        self.onboarding_group_label(title_key, list.into_any_element())
    }

    fn onboarding_group_label(&self, title_key: &str, body: AnyElement) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(title_key)),
            )
            .child(body)
            .into_any_element()
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
            .py(px(2.0))
            .child(
                div()
                    .pt(px(2.0))
                    .flex_none()
                    .child(Self::render_lucide_icon(
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
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if checked {
                rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
            } else {
                rgba((self.tokens.ui.border << 8) | ONBOARDING_CARD_BORDER_ALPHA)
            })
            .bg(if checked {
                rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
            } else {
                self.onboarding_card_bg()
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

    pub(in crate::workspace) fn onboarding_info_row(
        &self,
        icon: LucideIcon,
        color: u32,
        title_key: &str,
        detail_key: &str,
    ) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(10.0))
            .py(px(6.0))
            .child(
                div()
                    .pt(px(2.0))
                    .flex_none()
                    .child(Self::render_lucide_icon(icon, 14.0, rgb(color))),
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
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(detail_key)),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_inline_link(
        &self,
        icon: LucideIcon,
        text: String,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .px(px(6.0))
            .py(px(4.0))
            .rounded(px(self.tokens.radii.sm))
            .hover(move |style| style.bg(rgb(theme.bg_hover)))
            .cursor(CursorStyle::PointingHand)
            .child(Self::render_lucide_icon(icon, 13.0, rgb(theme.accent)))
            .child(
                div()
                    .flex_1()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text))
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

    pub(in crate::workspace) fn onboarding_quiet_note(
        &self,
        key: &str,
        replacements: &[(&str, String)],
    ) -> AnyElement {
        let text = self.onboarding_i18n_with(key, replacements);
        let note_color = if self.window_background_preferences().is_some() {
            self.tokens.ui.text
        } else {
            self.tokens.ui.text_muted
        };
        div()
            .flex()
            .items_start()
            .gap(px(6.0))
            .pt(px(2.0))
            .child(
                div()
                    .pt(px(2.0))
                    .flex_none()
                    .child(Self::render_lucide_icon(
                        LucideIcon::Info,
                        13.0,
                        rgb(note_color),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(note_color))
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
