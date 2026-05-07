impl WorkspaceApp {
    fn render_settings_select_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open_select = self.open_settings_select?;
        let anchor = *self.select_anchors.get(&open_select.anchor_id())?;
        let width =
            f32::from(anchor.bounds.size.width).max(self.tokens.metrics.ui_select_min_width);
        let settings = self.settings_store.settings();

        let popup = match (self.active_settings_tab, open_select) {
            (SettingsTab::General, SettingsSelect::Language) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for language in language_options() {
                    let label = self.language_label(language);
                    popup = popup.child(
                        select_option(&self.tokens, label, language == settings.general.language)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.open_settings_select = None;
                                    this.edit_settings(
                                        |settings| settings.general.language = language,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceTheme) => {
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width,
                    self.tokens.metrics.settings_theme_select_popup_max_height,
                );

                popup = popup.child(select_label(
                    &self.tokens,
                    self.i18n.t("settings_view.appearance.theme_group_oxide"),
                ));
                for &theme_id in OXIDE_THEME_IDS {
                    if !built_in_theme_exists(theme_id) {
                        continue;
                    }
                    let next_theme = theme_id.to_string();
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            theme_display_name(theme_id),
                            theme_id == settings.terminal.theme.as_str(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.theme = next_theme.clone(),
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }

                popup = popup
                    .child(select_separator(&self.tokens))
                    .child(select_label(
                        &self.tokens,
                        self.i18n.t("settings_view.appearance.theme_group_classic"),
                    ));
                let mut classic_themes: Vec<_> = BUILT_IN_THEMES
                    .iter()
                    .filter(|theme| !is_oxide_theme(theme.id))
                    .collect();
                classic_themes.sort_by_key(|theme| theme.id);
                for theme in classic_themes {
                    let theme_id = theme.id.to_string();
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            theme_display_name(theme.id),
                            theme.id == settings.terminal.theme.as_str(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.theme = theme_id.clone(),
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceDensity) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &density in density_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            density_label(density, &self.i18n),
                            density == settings.appearance.ui_density,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.appearance.ui_density = density,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceAnimation) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &speed in animation_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            animation_label(speed, &self.i18n),
                            speed == settings.appearance.animation_speed,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.appearance.animation_speed = speed,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceRenderProfile) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &profile in render_profile_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            render_profile_label(profile, &self.i18n),
                            profile == settings.appearance.render_profile,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.appearance.render_profile = profile,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceFrostedGlass) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &native_mode in oxideterm_gpui_platform::vibrancy::available_modes() {
                    let mode = frosted_glass_mode_from_native(native_mode);
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            frosted_glass_label(mode, &self.i18n),
                            native_vibrancy_mode(settings.appearance.frosted_glass) == native_mode,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.appearance.frosted_glass = mode,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceBackgroundFit) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &fit in background_fit_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            background_fit_label(fit, &self.i18n),
                            fit == settings.terminal.background_fit,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.background_fit = fit,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalFontFamily) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &family in font_family_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            font_family_label(family),
                            family == settings.terminal.font_family,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.font_family = family,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalEncoding) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &encoding in terminal_encoding_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            terminal_encoding_label(encoding),
                            encoding == settings.terminal.terminal_encoding,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.terminal_encoding = encoding,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalAdaptiveRenderer) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &mode in adaptive_renderer_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            adaptive_renderer_label(mode, &self.i18n),
                            mode == settings.terminal.adaptive_renderer,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.adaptive_renderer = mode,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalCursorStyle) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &style in cursor_style_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            cursor_style_label(style, &self.i18n),
                            style == settings.terminal.cursor_style,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.cursor_style = style,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::HighlightPreset) => {
                let mut popup = select_overlay_popup(&self.tokens, width.max(288.0));
                for (group_index, group) in
                    highlight_preset_groups(&self.i18n).into_iter().enumerate()
                {
                    if group_index > 0 {
                        popup = popup.child(select_separator(&self.tokens));
                    }
                    popup = popup.child(select_label(&self.tokens, group.label));
                    for preset in group.items {
                        popup = popup.child(
                            select_option(&self.tokens, preset.label.clone(), false).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.open_settings_select = None;
                                    this.add_highlight_preset(preset.rules.clone(), cx);
                                    cx.stop_propagation();
                                }),
                            ),
                        );
                    }
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::HighlightRenderMode(index)) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let selected = settings
                    .terminal
                    .highlight_rules
                    .get(index)
                    .map(|rule| rule.render_mode)
                    .unwrap_or_default();
                for &mode in highlight_render_mode_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            highlight_render_mode_label(mode, &self.i18n),
                            mode == selected,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_highlight_rule(index, |rule| rule.render_mode = mode, cx);
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Local, SettingsSelect::LocalShell) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let selected = settings.local_terminal.default_shell_id.as_deref();
                for shell in self.local_shells.clone() {
                    let shell_id = shell.id.clone();
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            shell.label,
                            selected == Some(shell_id.as_str()),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| {
                                        settings.local_terminal.default_shell_id =
                                            Some(shell_id.clone())
                                    },
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ConnectionIdleTimeout) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for (seconds, label) in connection_idle_timeout_options(&self.i18n) {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            label,
                            seconds == settings.connection_pool.idle_timeout_secs,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| {
                                        settings.connection_pool.idle_timeout_secs = seconds
                                    },
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpConcurrent) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &count in sftp_concurrent_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            sftp_transfer_count_label(&self.i18n, count),
                            count == settings.sftp.max_concurrent_transfers,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.sftp.max_concurrent_transfers = count,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpDirectoryParallelism) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &count in sftp_directory_parallelism_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            sftp_transfer_count_label(&self.i18n, count),
                            count == settings.sftp.directory_parallelism,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.sftp.directory_parallelism = count,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            _ => None,
        }?;

        Some(
            deferred(
                anchored()
                    .anchor(Corner::TopLeft)
                    .position(anchor.bounds.bottom_left())
                    .offset(point(
                        px(0.0),
                        px(self.tokens.metrics.settings_select_popup_gap),
                    ))
                    .position_mode(AnchoredPositionMode::Window)
                    .child(popup),
            )
            .with_priority(100)
            .into_any_element(),
        )
    }

    fn language_select_row(&self, selected: Language, cx: &mut Context<Self>) -> AnyElement {
        let control_width = self.tokens.metrics.settings_select_width;
        let anchor_id = SettingsSelect::Language.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, self.language_label(selected), false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select =
                        if this.open_settings_select == Some(SettingsSelect::Language) {
                            None
                        } else {
                            Some(SettingsSelect::Language)
                        };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        let control = div()
            .relative()
            .w(px(control_width))
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ));

        self.setting_row(
            "settings_view.general.language",
            "settings_view.general.language_hint",
            control.into_any_element(),
        )
    }

    fn select_setting_row(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, value, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        let control = div().relative().w(px(width)).child(select_anchor_probe(
            anchor_id,
            trigger,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        ));

        self.setting_row(label_key, hint_key, control.into_any_element())
    }

    fn count_row(&self, label_key: &str, hint_key: &str, count: usize) -> AnyElement {
        self.value_row(label_key, hint_key, count.to_string())
    }

    fn bool_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        self.setting_row(
            label_key,
            hint_key,
            checkbox(&self.tokens, label, checked)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(|settings| setter(settings, !checked), cx);
                    }),
                )
                .into_any_element(),
        )
    }

    fn number_row(
        &self,
        label_key: &str,
        hint_key: &str,
        value: i64,
        step: i64,
        min: i64,
        max: i64,
        setter: fn(&mut PersistedSettings, i64),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = div()
            .h(px(self.tokens.metrics.ui_control_height))
            .w(px(112.0))
            .px(px(self.tokens.metrics.ui_control_padding_x))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text))
            .cursor_pointer()
            .child(value.to_string())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    let next = if value >= max { min } else { value + step };
                    this.edit_settings(|settings| setter(settings, next.clamp(min, max)), cx);
                }),
            )
            .into_any_element();
        self.setting_row(label_key, hint_key, control)
    }

    fn setting_row(&self, label_key: &str, hint_key: &str, control: AnyElement) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(control)
            .into_any_element()
    }

}
