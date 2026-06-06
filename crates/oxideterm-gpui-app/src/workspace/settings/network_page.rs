const SETTINGS_NETWORK_MAX_WIDTH: f32 = 672.0; // Tauri max-w-2xl
const SETTINGS_NETWORK_FIELD_WIDTH: f32 = 320.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NetworkProxyAuthMode {
    None,
    Password,
}

fn default_settings_upstream_proxy_config() -> SettingsUpstreamProxyConfig {
    SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::Socks5,
        host: "127.0.0.1".to_string(),
        port: 1080,
        auth: SettingsUpstreamProxyAuth::None,
        remote_dns: true,
        no_proxy: String::new(),
    }
}

fn network_proxy_protocol_label(protocol: SettingsUpstreamProxyProtocol, i18n: &I18n) -> String {
    match protocol {
        SettingsUpstreamProxyProtocol::Socks5 => i18n.t("settings_view.network.protocol_socks5"),
        SettingsUpstreamProxyProtocol::HttpConnect => {
            i18n.t("settings_view.network.protocol_http_connect")
        }
    }
}

fn network_proxy_auth_label(mode: NetworkProxyAuthMode, i18n: &I18n) -> String {
    match mode {
        NetworkProxyAuthMode::None => i18n.t("settings_view.network.auth_none"),
        NetworkProxyAuthMode::Password => i18n.t("settings_view.network.auth_password"),
    }
}

impl WorkspaceApp {
    fn settings_network_section(&self, section_index: usize, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings_store.settings();
        let proxy = settings.network.upstream_proxy.as_ref();
        match section_index {
            0 => div()
                .w_full()
                .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
                .flex()
                .flex_col()
                .gap(px(20.0))
                .child(self.network_checkbox_row(
                    "settings_view.network.disclaimer",
                    "settings_view.network.disclaimer_hint",
                    settings.network.upstream_proxy_disclaimer_accepted,
                    true,
                    Self::toggle_settings_network_disclaimer,
                    cx,
                ))
                .child(self.network_checkbox_row(
                    "settings_view.network.enabled",
                    "settings_view.network.enabled_hint",
                    proxy.is_some(),
                    settings.network.upstream_proxy_disclaimer_accepted,
                    Self::toggle_settings_network_enabled,
                    cx,
                ))
                .into_any_element(),
            1 => div()
                .flex()
                .flex_col()
                .gap(px(24.0))
                .opacity(if proxy.is_some() { 1.0 } else { 0.4 })
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(self.tokens.ui.text_heading))
                        .child(self.i18n.t("settings_view.network.proxy")),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
                        .flex()
                        .flex_row()
                        .gap(px(32.0))
                        .child(self.network_select_field(
                            "settings_view.network.protocol",
                            "settings_view.network.protocol_hint",
                            SettingsSelect::NetworkProxyProtocol,
                            network_proxy_protocol_label(
                                proxy
                                    .map(|proxy| proxy.protocol)
                                    .unwrap_or(SettingsUpstreamProxyProtocol::Socks5),
                                &self.i18n,
                            ),
                            proxy.is_some(),
                            cx,
                        ))
                        .child(self.network_input_field(
                            "settings_view.network.port",
                            "settings_view.network.port_hint",
                            SettingsInput::NetworkProxyPort,
                            proxy
                                .map(|proxy| proxy.port.to_string())
                                .unwrap_or_else(|| "1080".to_string()),
                            "1080".to_string(),
                            proxy.is_some(),
                            cx,
                        )),
                )
                .child(self.network_full_width_input(
                    "settings_view.network.host",
                    "settings_view.network.host_hint",
                    SettingsInput::NetworkProxyHost,
                    proxy.map(|proxy| proxy.host.clone()).unwrap_or_default(),
                    "127.0.0.1".to_string(),
                    proxy.is_some(),
                    cx,
                ))
                .child(self.network_full_width_input(
                    "settings_view.network.no_proxy",
                    "settings_view.network.no_proxy_hint",
                    SettingsInput::NetworkProxyNoProxy,
                    proxy.map(|proxy| proxy.no_proxy.clone()).unwrap_or_default(),
                    "localhost,127.0.0.1,*.internal".to_string(),
                    proxy.is_some(),
                    cx,
                ))
                .child(self.network_checkbox_row(
                    "settings_view.network.remote_dns",
                    "settings_view.network.remote_dns_hint",
                    proxy.map(|proxy| proxy.remote_dns).unwrap_or(true),
                    proxy.is_some(),
                    Self::toggle_settings_network_remote_dns,
                    cx,
                ))
                .into_any_element(),
            2 => self.settings_network_auth_section(proxy, cx),
            _ => div().into_any_element(),
        }
    }

    fn settings_network_auth_section(
        &self,
        proxy: Option<&SettingsUpstreamProxyConfig>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let auth_mode = proxy
            .map(|proxy| match &proxy.auth {
                SettingsUpstreamProxyAuth::None => NetworkProxyAuthMode::None,
                SettingsUpstreamProxyAuth::Password { .. } => NetworkProxyAuthMode::Password,
            })
            .unwrap_or(NetworkProxyAuthMode::None);
        let auth_username = proxy
            .and_then(|proxy| match &proxy.auth {
                SettingsUpstreamProxyAuth::Password { username, .. } => Some(username.clone()),
                SettingsUpstreamProxyAuth::None => None,
            })
            .unwrap_or_default();
        let auth_has_saved_password = proxy.is_some_and(|proxy| match &proxy.auth {
            SettingsUpstreamProxyAuth::Password { keychain_id, .. } => keychain_id.is_some(),
            SettingsUpstreamProxyAuth::None => false,
        });

        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .opacity(if proxy.is_some() { 1.0 } else { 0.4 })
            .child(
                div()
                    .w_full()
                    .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
                    .flex()
                    .flex_row()
                    .gap(px(32.0))
                    .child(self.network_select_field(
                        "settings_view.network.auth",
                        "settings_view.network.auth_hint",
                        SettingsSelect::NetworkProxyAuth,
                        network_proxy_auth_label(auth_mode, &self.i18n),
                        proxy.is_some(),
                        cx,
                    )),
            );

        if auth_mode == NetworkProxyAuthMode::Password {
            section = section
                .child(self.network_full_width_input(
                    "settings_view.network.username",
                    "settings_view.network.username_hint",
                    SettingsInput::NetworkProxyUsername,
                    auth_username,
                    String::new(),
                    proxy.is_some(),
                    cx,
                ))
                .child(self.network_password_field(auth_has_saved_password, proxy.is_some(), cx));
        }

        section.into_any_element()
    }

    fn network_checkbox_row(
        &self,
        label_key: &'static str,
        hint_key: &'static str,
        checked: bool,
        enabled: bool,
        on_toggle: fn(&mut WorkspaceApp, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut control = checkbox(&self.tokens, String::new(), checked).opacity(if enabled {
            1.0
        } else {
            0.5
        });
        if enabled {
            control = control.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    on_toggle(this, cx);
                    cx.stop_propagation();
                }),
            );
        }
        div()
            .w_full()
            .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .grid()
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
            .child(control.into_any_element())
            .into_any_element()
    }

    fn network_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w(px(SETTINGS_NETWORK_FIELD_WIDTH))
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(self.settings_select_control(select_id, value, !enabled, None, cx))
            .into_any_element()
    }

    fn network_input_field(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w(px(SETTINGS_NETWORK_FIELD_WIDTH))
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(
                self.settings_text_input_control(
                    input,
                    value,
                    placeholder,
                    SETTINGS_NETWORK_FIELD_WIDTH,
                    cx,
                )
                .into_any_element(),
            )
            .when(!enabled, |field| field.opacity(0.5))
            .into_any_element()
    }

    fn network_full_width_input(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(
                self.settings_text_input_control(input, value, placeholder, SETTINGS_NETWORK_MAX_WIDTH, cx),
            )
            .when(!enabled, |field| field.opacity(0.5))
            .into_any_element()
    }

    fn network_password_field(
        &self,
        has_saved_password: bool,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let password_input = SettingsInput::NetworkProxyPassword;
        let current_value = if self.focused_settings_input == Some(password_input) {
            self.settings_input_draft.clone()
        } else {
            String::new()
        };
        let save_disabled = current_value.is_empty() || !enabled;
        let remove_disabled = !has_saved_password && current_value.is_empty();
        let mut row = div()
            .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(
                "settings_view.network.password",
                if has_saved_password {
                    "settings_view.network.password_saved_hint"
                } else {
                    "settings_view.network.password_hint"
                },
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .child(self.settings_secret_text_input_control(
                        password_input,
                        String::new(),
                        if has_saved_password {
                            self.i18n.t("settings_view.network.password_saved_placeholder")
                        } else {
                            String::new()
                        },
                        320.0,
                        cx,
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.save_password"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::KeyRound,
                            16.0,
                            rgb(if save_disabled {
                                self.tokens.ui.text_muted
                            } else {
                                self.tokens.ui.bg
                            }),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Default,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: save_disabled,
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(|this, _event, _window, cx| {
                            this.save_settings_network_proxy_password(cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.remove_password"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::Trash2,
                            16.0,
                            rgb(self.tokens.ui.text),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: remove_disabled,
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(|this, _event, _window, cx| {
                            this.remove_settings_network_proxy_password(cx);
                            cx.stop_propagation();
                        }),
                    )),
            )
            .when(!enabled, |field| field.opacity(0.5));

        if let Some(status) = self.settings_network_proxy_password_status.clone() {
            row = row.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(status),
            );
        }

        row.into_any_element()
    }

    fn network_field_label(&self, label_key: &str, hint_key: &str) -> AnyElement {
        div()
            .grid()
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
            )
            .into_any_element()
    }

    fn toggle_settings_network_disclaimer(&mut self, cx: &mut Context<Self>) {
        self.edit_settings(
            |settings| {
                settings.network.upstream_proxy_disclaimer_accepted =
                    !settings.network.upstream_proxy_disclaimer_accepted;
            },
            cx,
        );
    }

    fn toggle_settings_network_enabled(&mut self, cx: &mut Context<Self>) {
        self.settings_network_proxy_password_status = None;
        self.edit_settings(
            |settings| {
                settings.network.upstream_proxy = if settings.network.upstream_proxy.is_some() {
                    None
                } else {
                    Some(default_settings_upstream_proxy_config())
                };
            },
            cx,
        );
    }

    fn toggle_settings_network_remote_dns(&mut self, cx: &mut Context<Self>) {
        self.edit_settings(
            |settings| {
                if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                    proxy.remote_dns = !proxy.remote_dns;
                }
            },
            cx,
        );
    }

    fn save_settings_network_proxy_password(&mut self, cx: &mut Context<Self>) {
        let password = self.settings_input_draft.clone();
        if password.is_empty() {
            return;
        }
        let secret = SecretString::new(password);
        match self.connection_store.save_global_upstream_proxy_password(&secret) {
            Ok(keychain_id) => {
                self.edit_settings(
                    move |settings| {
                        if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                            if let SettingsUpstreamProxyAuth::Password { username, .. } =
                                &proxy.auth
                            {
                                proxy.auth = SettingsUpstreamProxyAuth::Password {
                                    username: username.clone(),
                                    keychain_id: Some(keychain_id.clone()),
                                };
                            }
                        }
                    },
                    cx,
                );
                // Clear the transient UI draft after the keychain write succeeds.
                zeroize::Zeroize::zeroize(&mut self.settings_input_draft);
                self.settings_input_draft.clear();
                self.focused_settings_input = None;
                self.settings_network_proxy_password_status =
                    Some(self.i18n.t("settings_view.network.password_saved_placeholder"));
            }
            Err(error) => {
                self.settings_network_proxy_password_status = Some(error.to_string());
            }
        }
        cx.notify();
    }

    fn remove_settings_network_proxy_password(&mut self, cx: &mut Context<Self>) {
        match self.connection_store.delete_global_upstream_proxy_password() {
            Ok(()) => {
                self.edit_settings(
                    |settings| {
                        if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                            if let SettingsUpstreamProxyAuth::Password { username, .. } =
                                &proxy.auth
                            {
                                proxy.auth = SettingsUpstreamProxyAuth::Password {
                                    username: username.clone(),
                                    keychain_id: None,
                                };
                            }
                        }
                    },
                    cx,
                );
                zeroize::Zeroize::zeroize(&mut self.settings_input_draft);
                self.settings_input_draft.clear();
                self.focused_settings_input = None;
                self.settings_network_proxy_password_status = None;
            }
            Err(error) => {
                self.settings_network_proxy_password_status = Some(error.to_string());
            }
        }
        cx.notify();
    }
}
