use super::*;

pub(in crate::workspace) const SETTINGS_NETWORK_MAX_WIDTH: f32 = 672.0; // Tauri max-w-2xl
pub(in crate::workspace) const SETTINGS_NETWORK_FIELD_WIDTH: f32 = 320.0; // Desktop preference for normal proxy fields.
pub(in crate::workspace) const SETTINGS_NETWORK_PORT_FIELD_WIDTH: f32 = 140.0; // Ports should stay compact instead of sharing a full row.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NetworkProxyAuthMode {
    None,
    Password,
}

pub(in crate::workspace) fn default_settings_upstream_proxy_config() -> SettingsUpstreamProxyConfig
{
    SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::Socks5,
        host: "127.0.0.1".to_string(),
        port: 1080,
        auth: SettingsUpstreamProxyAuth::None,
        remote_dns: true,
        no_proxy: String::new(),
    }
}

pub(in crate::workspace) fn network_proxy_protocol_label(
    protocol: SettingsUpstreamProxyProtocol,
    i18n: &I18n,
) -> String {
    match protocol {
        SettingsUpstreamProxyProtocol::Socks5 => i18n.t("settings_view.network.protocol_socks5"),
        SettingsUpstreamProxyProtocol::HttpConnect => {
            i18n.t("settings_view.network.protocol_http_connect")
        }
    }
}

pub(in crate::workspace) fn network_proxy_auth_label(
    mode: NetworkProxyAuthMode,
    i18n: &I18n,
) -> String {
    match mode {
        NetworkProxyAuthMode::None => i18n.t("settings_view.network.auth_none"),
        NetworkProxyAuthMode::Password => i18n.t("settings_view.network.auth_password"),
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_network_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let proxy = settings.network.upstream_proxy.as_ref();
        match section_index {
            0 => self.plain_settings_card(vec![
                self.network_checkbox_row(
                    "settings_view.network.disclaimer",
                    "settings_view.network.disclaimer_hint",
                    settings.network.upstream_proxy_disclaimer_accepted,
                    true,
                    Self::toggle_settings_network_disclaimer,
                    cx,
                ),
                self.card_separator(),
                self.network_checkbox_row(
                    "settings_view.network.enabled",
                    "settings_view.network.enabled_hint",
                    proxy.is_some(),
                    settings.network.upstream_proxy_disclaimer_accepted,
                    Self::toggle_settings_network_enabled,
                    cx,
                ),
            ]),
            1 => {
                self.settings_card(
                    "settings_view.network.proxy",
                    "settings_view.network.protocol_hint",
                    vec![
                        div()
                            .w_full()
                            .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.metrics.settings_row_gap))
                            // Keep the card structure visible while its controls are disabled.
                            .opacity(if proxy.is_some() { 1.0 } else { 0.4 })
                            .child(
                                div()
                                    .w_full()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_wrap()
                                    .items_start()
                                    .gap(px(32.0))
                                    .child(self.network_responsive_field(
                                        SETTINGS_NETWORK_FIELD_WIDTH,
                                        self.network_select_field(
                                            "settings_view.network.protocol",
                                            "settings_view.network.protocol_hint",
                                            SettingsSelect::NetworkProxyProtocol,
                                            network_proxy_protocol_label(
                                                proxy.map(|proxy| proxy.protocol).unwrap_or(
                                                    SettingsUpstreamProxyProtocol::Socks5,
                                                ),
                                                &self.i18n,
                                            ),
                                            proxy.is_some(),
                                            cx,
                                        ),
                                    ))
                                    .child(
                                        self.network_responsive_field(
                                            SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                                            self.network_input_field(
                                                "settings_view.network.port",
                                                "settings_view.network.port_hint",
                                                SettingsInput::NetworkProxyPort,
                                                proxy
                                                    .map(|proxy| proxy.port.to_string())
                                                    .unwrap_or_else(|| "1080".to_string()),
                                                "1080".to_string(),
                                                SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                                                proxy.is_some(),
                                                cx,
                                            ),
                                        ),
                                    ),
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
                            .child(
                                self.network_full_width_input(
                                    "settings_view.network.no_proxy",
                                    "settings_view.network.no_proxy_hint",
                                    SettingsInput::NetworkProxyNoProxy,
                                    proxy
                                        .map(|proxy| proxy.no_proxy.clone())
                                        .unwrap_or_default(),
                                    "localhost,127.0.0.1,*.internal".to_string(),
                                    proxy.is_some(),
                                    cx,
                                ),
                            )
                            .child(self.network_checkbox_row(
                                "settings_view.network.remote_dns",
                                "settings_view.network.remote_dns_hint",
                                proxy.map(|proxy| proxy.remote_dns).unwrap_or(true),
                                proxy.is_some(),
                                Self::toggle_settings_network_remote_dns,
                                cx,
                            ))
                            .into_any_element(),
                    ],
                )
            }
            2 => self.settings_network_auth_section(proxy, cx),
            3 => self.settings_network_test_section(proxy.is_some(), cx),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn settings_network_auth_section(
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
            .w_full()
            .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .opacity(if proxy.is_some() { 1.0 } else { 0.4 })
            .child(
                div()
                    .w_full()
                    .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(32.0))
                    .child(self.network_responsive_field(
                        SETTINGS_NETWORK_FIELD_WIDTH,
                        self.network_select_field(
                            "settings_view.network.auth",
                            "settings_view.network.auth_hint",
                            SettingsSelect::NetworkProxyAuth,
                            network_proxy_auth_label(auth_mode, &self.i18n),
                            proxy.is_some(),
                            cx,
                        ),
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

        self.settings_card(
            "settings_view.network.auth",
            "settings_view.network.auth_hint",
            vec![section.into_any_element()],
        )
    }

    pub(in crate::workspace) fn settings_network_test_section(
        &self,
        proxy_enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host_value = if self.focused_settings_input == Some(SettingsInput::NetworkProxyTestHost)
        {
            self.settings_input_draft.clone()
        } else {
            self.settings_network_proxy_test_host.clone()
        };
        let port_value = if self.focused_settings_input == Some(SettingsInput::NetworkProxyTestPort)
        {
            self.settings_input_draft.clone()
        } else {
            self.settings_network_proxy_test_port.clone()
        };
        let test_disabled = !proxy_enabled
            || self.settings_network_proxy_test_pending
            || host_value.trim().is_empty()
            || port_value.trim().parse::<u16>().is_err();

        let content = div()
            .w_full()
            .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .opacity(if proxy_enabled { 1.0 } else { 0.4 })
            .child(
                div()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.network.test_hint")),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(16.0))
                    .child(self.network_responsive_field(
                        SETTINGS_NETWORK_FIELD_WIDTH,
                        self.network_input_field(
                            "settings_view.network.test_host",
                            "settings_view.network.host_hint",
                            SettingsInput::NetworkProxyTestHost,
                            host_value,
                            "server.example.com".to_string(),
                            SETTINGS_NETWORK_FIELD_WIDTH,
                            proxy_enabled,
                            cx,
                        ),
                    ))
                    .child(self.network_responsive_field(
                        SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                        self.network_input_field(
                            "settings_view.network.test_port",
                            "settings_view.network.port_hint",
                            SettingsInput::NetworkProxyTestPort,
                            port_value,
                            "22".to_string(),
                            SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                            proxy_enabled,
                            cx,
                        ),
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_none()
                            .child(self.workspace_toolbar_action_button(
                                if self.settings_network_proxy_test_pending {
                                    self.i18n.t("settings_view.network.testing")
                                } else {
                                    self.i18n.t("settings_view.network.test_button")
                                },
                                None,
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Default,
                                        radius: ButtonRadius::Md,
                                        disabled: test_disabled,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.start_settings_network_proxy_test(cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    )
                    .when_some(
                        self.settings_network_proxy_test_status.clone(),
                        |row, status| {
                            row.child(
                                div()
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(status),
                            )
                        },
                    ),
            )
            .into_any_element();

        self.settings_card(
            "settings_view.network.test_title",
            "settings_view.network.test_hint",
            vec![content],
        )
    }

    pub(in crate::workspace) fn network_responsive_field(
        &self,
        preferred_width: f32,
        field: AnyElement,
    ) -> AnyElement {
        // Field slots own width. Controls fill the slot; the slot itself wraps
        // or shrinks instead of growing into the right sidebar.
        div()
            .w(px(preferred_width))
            .max_w_full()
            .min_w(px(0.0))
            .flex_initial()
            .child(field)
            .into_any_element()
    }

    pub(in crate::workspace) fn network_checkbox_row(
        &self,
        label_key: &'static str,
        hint_key: &'static str,
        checked: bool,
        enabled: bool,
        on_toggle: fn(&mut WorkspaceApp, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut control =
            checkbox(&self.tokens, String::new(), checked).opacity(if enabled { 1.0 } else { 0.5 });
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
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    // Checkbox rows keep the label as the flexible item so the
                    // fixed checkbox can wrap inside narrow settings panes.
                    .flex_basis(px(SETTINGS_NETWORK_FIELD_WIDTH))
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
            .child(div().flex_none().child(control.into_any_element()))
            .into_any_element()
    }

    pub(in crate::workspace) fn network_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .max_w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(self.settings_select_control(select_id, value, !enabled, None, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn network_input_field(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        control_width: f32,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .max_w_full()
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(
                self.settings_text_input_control(input, value, placeholder, control_width, cx)
                    .into_any_element(),
            )
            .when(!enabled, |field| field.opacity(0.5))
            .into_any_element()
    }

    pub(in crate::workspace) fn network_full_width_input(
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
            .w_full()
            .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(self.network_field_label(label_key, hint_key))
            .child(self.network_full_width_text_input_control(input, value, placeholder, cx))
            .when(!enabled, |field| field.opacity(0.5))
            .into_any_element()
    }

    pub(in crate::workspace) fn network_full_width_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w_full()
            .min_w(px(0.0))
            // Full-width proxy fields must size from their parent column, not
            // from the desktop max width used by other settings controls.
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(
                |this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                },
            )),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn network_password_field(
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
            .w_full()
            .max_w(px(SETTINGS_NETWORK_MAX_WIDTH))
            .min_w(px(0.0))
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
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex_basis(px(SETTINGS_NETWORK_FIELD_WIDTH))
                            .child(self.settings_secret_text_input_control(
                                password_input,
                                String::new(),
                                if has_saved_password {
                                    self.i18n
                                        .t("settings_view.network.password_saved_placeholder")
                                } else {
                                    String::new()
                                },
                                SETTINGS_NETWORK_FIELD_WIDTH,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
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
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
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
                    ),
            )
            .when(!enabled, |field| field.opacity(0.5));

        if let Some(status) = self.settings_network_proxy_password_status.clone() {
            row = row.child(
                div()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(status),
            );
        }

        row.into_any_element()
    }

    pub(in crate::workspace) fn network_field_label(
        &self,
        label_key: &str,
        hint_key: &str,
    ) -> AnyElement {
        div()
            .min_w(px(0.0))
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

    pub(in crate::workspace) fn toggle_settings_network_disclaimer(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            |settings| {
                settings.network.upstream_proxy_disclaimer_accepted =
                    !settings.network.upstream_proxy_disclaimer_accepted;
            },
            cx,
        );
    }

    pub(in crate::workspace) fn toggle_settings_network_enabled(&mut self, cx: &mut Context<Self>) {
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

    pub(in crate::workspace) fn toggle_settings_network_remote_dns(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            |settings| {
                if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                    proxy.remote_dns = !proxy.remote_dns;
                }
            },
            cx,
        );
    }

    pub(in crate::workspace) fn save_settings_network_proxy_password(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let password = self.settings_input_draft.clone();
        if password.is_empty() {
            return;
        }
        let secret = SecretString::new(password);
        match self
            .connection_store
            .save_global_upstream_proxy_password(&secret)
        {
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
                self.settings_network_proxy_password_status = Some(
                    self.i18n
                        .t("settings_view.network.password_saved_placeholder"),
                );
            }
            Err(error) => {
                self.settings_network_proxy_password_status = Some(error.to_string());
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn remove_settings_network_proxy_password(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        match self
            .connection_store
            .delete_global_upstream_proxy_password()
        {
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

    pub(in crate::workspace) fn start_settings_network_proxy_test(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let host = self.settings_network_proxy_test_host.trim().to_string();
        let Ok(port) = self.settings_network_proxy_test_port.trim().parse::<u16>() else {
            self.settings_network_proxy_test_status = Some(
                self.i18n
                    .t("settings_view.network.test_error")
                    .replace("{{error}}", "invalid port"),
            );
            cx.notify();
            return;
        };
        let Some(proxy) = self
            .settings_store
            .settings()
            .network
            .upstream_proxy
            .as_ref()
            .cloned()
        else {
            self.settings_network_proxy_test_status = Some(
                self.i18n
                    .t("settings_view.network.test_error")
                    .replace("{{error}}", "proxy is disabled"),
            );
            cx.notify();
            return;
        };
        let Ok(upstream_proxy) = self.runtime_global_upstream_proxy_config(proxy) else {
            self.settings_network_proxy_test_status = Some(
                self.i18n
                    .t("settings_view.network.test_error")
                    .replace("{{error}}", "proxy password is not available"),
            );
            cx.notify();
            return;
        };
        self.settings_network_proxy_test_pending = true;
        self.settings_network_proxy_test_status = None;
        let started_at = std::time::Instant::now();

        cx.spawn(async move |weak, cx| {
            let status =
                check_host_key_with_upstream_proxy(&host, port, 10, Some(&upstream_proxy)).await;
            let elapsed = started_at.elapsed().as_millis();
            let _ = weak.update(cx, move |this, cx| {
                this.settings_network_proxy_test_pending = false;
                this.settings_network_proxy_test_status = Some(match status {
                    HostKeyStatus::Error { message } => this
                        .i18n
                        .t("settings_view.network.test_error")
                        .replace("{{error}}", &message),
                    _ => this
                        .i18n
                        .t("settings_view.network.test_success")
                        .replace("{{elapsed}}", &elapsed.to_string()),
                });
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn runtime_global_upstream_proxy_config(
        &self,
        proxy: SettingsUpstreamProxyConfig,
    ) -> anyhow::Result<UpstreamProxyConfig> {
        let auth = match proxy.auth {
            SettingsUpstreamProxyAuth::None => UpstreamProxyAuth::None,
            SettingsUpstreamProxyAuth::Password {
                username,
                keychain_id,
            } => {
                let password = self
                    .connection_store
                    .get_global_upstream_proxy_password(keychain_id.as_deref().unwrap_or_default())?
                    .into_zeroizing();
                // The test route uses the same hydrated runtime proxy boundary as
                // real SSH preflight and never writes the secret into UI state.
                UpstreamProxyAuth::Password { username, password }
            }
        };
        Ok(UpstreamProxyConfig {
            protocol: match proxy.protocol {
                SettingsUpstreamProxyProtocol::Socks5 => UpstreamProxyProtocol::Socks5,
                SettingsUpstreamProxyProtocol::HttpConnect => UpstreamProxyProtocol::HttpConnect,
            },
            host: proxy.host,
            port: proxy.port,
            auth,
            remote_dns: proxy.remote_dns,
            no_proxy: proxy.no_proxy,
        })
    }
}
