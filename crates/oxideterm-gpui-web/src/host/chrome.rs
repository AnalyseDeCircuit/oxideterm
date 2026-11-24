use super::*;
use oxideterm_gpui_ui::{
    button::{ButtonRadius, IconButtonOptions, icon_button},
    tooltip::tooltip_view,
};
use oxideterm_settings_model::{
    AppearanceSchemeError, apply_appearance_mode, next_appearance_scheme,
};

pub(super) fn glyph(icon: LucideIcon, size: f32, color: u32) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(rgb(color))
        .into_any_element()
}

impl Workspace {
    pub(super) fn chrome_divider(&self) -> Rgba {
        rgba((self.tokens.ui.border << 8) | 0x66)
    }

    pub(super) fn shortcut(&self, id: &str, cx: &App) -> Option<String> {
        let def = self.actions.iter().find(|d| d.id == id)?;
        effective_combo(
            def,
            &self.settings.read(cx).settings(cx).keybindings.overrides,
            self.side,
        )
        .map(|combo| format_combo_for(&combo, self.side))
    }

    pub(super) fn bind_button(
        &self,
        button: Div,
        id: impl Into<ElementId>,
        label: String,
        shortcut: Option<String>,
        cx: &Context<Self>,
        activate: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + Clone + 'static,
    ) -> AnyElement {
        let tokens = self.tokens;
        let owner = cx.weak_entity();
        let accessible = activate.clone();
        button
            .id(id)
            .flex_none()
            .role(Role::Button)
            .aria_label(label.clone())
            .tooltip(move |_, cx| tooltip_view(tokens, label.clone(), shortcut.clone(), cx))
            .on_a11y_action(AccessibleAction::Click, move |_, w, cx| {
                let _ = owner.update(cx, |h, cx| accessible(h, w, cx));
            })
            .on_click(cx.listener(move |h, _, w, cx| {
                activate(h, w, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    }

    pub(super) fn toolbar_button(
        &self,
        action: Action,
        icon: LucideIcon,
        label: &'static str,
        size: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        let t = &self.tokens;
        let shortcut = match action {
            Action::Palette => self.shortcut("app.commandPalette", cx),
            Action::Zen => self.shortcut("app.zenMode", cx),
            Action::Split => self.shortcut("split.horizontal", cx),
            _ => None,
        };
        let button = icon_button(
            t,
            glyph(icon, t.metrics.sidebar_action_icon_size, t.ui.text_muted),
            IconButtonOptions::opaque_toolbar(size, ButtonRadius::Md),
        );
        self.bind_button(
            button,
            label,
            self.i18n.t(label),
            shortcut,
            cx,
            move |h, w, cx| h.action(action, w, cx),
        )
    }

    pub(super) fn titlebar(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let size = t.metrics.titlebar_height - 6.;
        let light = t.ui.is_light();
        div()
            .flex_none()
            .h(px(t.metrics.titlebar_height))
            .flex()
            .items_center()
            .px_2()
            .bg(rgb(t.ui.shell_bg()))
            .border_b_1()
            .border_color(self.chrome_divider())
            .text_size(px(t.metrics.titlebar_label_font_size))
            .text_color(rgb(t.ui.text_muted))
            .gap(px(t.spacing.one))
            .child(
                svg()
                    .path("icons/oxide-mark.svg")
                    .size(px(16.))
                    .text_color(rgb(t.ui.text)),
            )
            .child(self.i18n.t("menu.app"))
            .when_some(self.gateway_host.clone(), |bar, host| {
                bar.child(
                    div()
                        .ml_1()
                        .min_w_0()
                        .truncate()
                        .rounded(px(t.radii.sm))
                        .bg(rgb(t.ui.shell_active_bg()))
                        .px_1()
                        .child(host),
                )
            })
            .when(!self.demo && !self.ready, |bar| {
                bar.child(self.i18n.t(if self.connecting {
                    "common.status_connecting"
                } else {
                    "connections.state.disconnected"
                }))
            })
            .child(div().flex_1())
            .child(self.toolbar_button(
                Action::Palette,
                LucideIcon::Search,
                Action::Palette.key(),
                size,
                cx,
            ))
            .child(self.toolbar_button(
                Action::Zen,
                LucideIcon::EyeOff,
                Action::Zen.key(),
                size,
                cx,
            ))
            .child(self.toolbar_button(
                Action::Theme,
                if light {
                    LucideIcon::Sun
                } else {
                    LucideIcon::Moon
                },
                if light {
                    "sidebar.tooltips.switch_to_dark"
                } else {
                    "sidebar.tooltips.switch_to_light"
                },
                size,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn toggle_theme(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let settings = self.settings.read(cx).settings(cx);
        let system_dark = dark(w);
        let fixed_scheme = settings
            .appearance
            .active_scheme(system_dark)
            .is_none()
            .then(|| oxideterm_settings::AppearanceScheme::from_dark(!self.tokens.ui.is_light()));
        let next = match next_appearance_scheme(settings, system_dark) {
            Ok(scheme) => Ok(scheme),
            Err(AppearanceSchemeError::Unassigned) => {
                let scheme =
                    oxideterm_settings::AppearanceScheme::from_dark(self.tokens.ui.is_light());
                oxideterm_settings_model::validate_appearance_scheme(settings, scheme)
                    .map(|()| scheme)
            }
            Err(e) => Err(e),
        };
        match next {
            Ok(scheme) => self.settings.update(cx, |s, cx| {
                s.edit_preferences(cx, |p| {
                    if let Some(current) = fixed_scheme {
                        // Switching back must restore the user's original fixed palette.
                        oxideterm_settings_model::assign_current_appearance(
                            p,
                            current,
                            system_dark,
                        );
                    }
                    apply_appearance_mode(p, Some(scheme));
                })
            }),
            Err(e) => {
                let message = match e {
                    AppearanceSchemeError::Identical => {
                        self.i18n.t("settings_view.appearance.schemes_identical")
                    }
                    AppearanceSchemeError::Unavailable(scheme) => self
                        .i18n
                        .t("settings_view.appearance.scheme_unavailable")
                        .replace(
                            "{{scheme}}",
                            &self.i18n.t(if scheme.is_dark() {
                                "settings_view.appearance.system_dark"
                            } else {
                                "settings_view.appearance.system_light"
                            }),
                        ),
                    AppearanceSchemeError::Unassigned => {
                        self.i18n.t("settings_view.appearance.legacy_scheme_hint")
                    }
                };
                self.notifications_notice(message, cx);
                self.settings.update(cx, |s, cx| {
                    s.open_section(oxideterm_gpui_settings_view::SettingsTab::Appearance, cx)
                });
                self.action(Action::Settings, w, cx);
            }
        }
    }

    pub(super) fn toggle_zen(&mut self, cx: &mut Context<Self>) {
        let zen = !self.settings.read(cx).settings(cx).sidebar_ui.zen_mode;
        self.settings.update(cx, |s, cx| {
            s.edit_preferences(cx, |p| p.sidebar_ui.zen_mode = zen)
        });
        self.rails.set_zen_mode(zen, self.demo, &self.tokens);
        self.zen_exit.reset();
        self.changed(cx);
    }

    pub(super) fn zen_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.zen_exit
                .reveal(self.settings.read(cx).settings(cx).sidebar_ui.zen_mode);
        } else if let Some(generation) = self.zen_exit.schedule_hide() {
            at_deadline(Instant::now() + zen::HIDE_DELAY, cx, move |h, _, cx| {
                if h.zen_exit.apply_hide(generation) {
                    cx.notify();
                }
            })
            .detach();
        }
    }

    pub(super) fn strip_actions(&self, cx: &Context<Self>) -> (Option<AnyElement>, f32) {
        let size = self.tokens.metrics.tabbar_height;
        let mut actions = Vec::new();
        if self.settings.read(cx).settings(cx).sidebar_ui.zen_mode {
            actions.push(self.toolbar_button(
                Action::Zen,
                LucideIcon::Eye,
                "zen_mode.exit",
                size,
                cx,
            ));
        } else {
            if self.ready && self.active_session().is_some() {
                actions.push(self.toolbar_button(
                    Action::Split,
                    LucideIcon::SplitSquareHorizontal,
                    Action::Split.key(),
                    size,
                    cx,
                ));
            }
            if self.demo {
                actions.push(self.toolbar_button(
                    Action::Quick,
                    LucideIcon::ListChecks,
                    Action::Quick.key(),
                    size,
                    cx,
                ));
            }
        }
        let width = actions.len() as f32 * size;
        (
            (!actions.is_empty()).then(|| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .children(actions)
                    .into_any_element()
            }),
            width,
        )
    }

    pub(super) fn empty_workspace(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let actions = [
            (Action::Terminal, LucideIcon::Terminal, self.ready),
            (Action::Catalog, LucideIcon::LayoutList, self.demo),
            (Action::Palette, LucideIcon::Search, true),
        ];
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .pb_3()
                            .border_b_1()
                            .border_color(self.chrome_divider())
                            .child(
                                svg()
                                    .path("icons/oxide-mark.svg")
                                    .size(px(36.))
                                    .text_color(rgb(t.ui.text_muted)),
                            )
                            .child(
                                div()
                                    .text_size(px(t.metrics.welcome_wordmark_font_size))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(t.ui.text_heading))
                                    .child(self.i18n.t("layout.empty.title")),
                            ),
                    )
                    .children(actions.into_iter().filter(|(_, _, enabled)| *enabled).map(
                        |(action, icon, _)| {
                            let label = self.i18n.t(action.key());
                            let button = div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .py_1()
                                .px_2()
                                .rounded(px(t.radii.md))
                                .cursor_pointer()
                                .hover(move |d| d.bg(rgb(t.ui.bg_hover)))
                                .text_size(px(t.metrics.ui_text_sm))
                                .text_color(rgb(t.ui.text_secondary))
                                .child(glyph(icon, t.metrics.tab_icon_size, t.ui.text_muted))
                                .child(label.clone());
                            self.bind_button(
                                button,
                                ("welcome-action", action as u64),
                                label,
                                None,
                                cx,
                                move |h, w, cx| h.action(action, w, cx),
                            )
                        },
                    )),
            )
            .into_any_element()
    }
}
