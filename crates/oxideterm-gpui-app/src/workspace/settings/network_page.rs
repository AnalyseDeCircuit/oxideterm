use super::*;

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

pub(in crate::workspace) fn network_application_proxy_mode_label(
    mode: SettingsApplicationProxyMode,
    i18n: &oxideterm_i18n::I18n,
) -> String {
    match mode {
        SettingsApplicationProxyMode::System => {
            i18n.t("settings_view.network.application_mode_system")
        }
        SettingsApplicationProxyMode::Direct => {
            i18n.t("settings_view.network.application_mode_direct")
        }
        SettingsApplicationProxyMode::Shared => {
            i18n.t("settings_view.network.application_mode_shared")
        }
    }
}

fn schedule_settings_network_proxy_test(
    runtime: &tokio::runtime::Runtime,
    host: String,
    port: u16,
    upstream_proxy: UpstreamProxyConfig,
) -> tokio::sync::oneshot::Receiver<HostKeyStatus> {
    let (status_tx, status_rx) = tokio::sync::oneshot::channel();
    // The workspace runtime owns network I/O; the GPUI task receives only the
    // resulting status and never polls Tokio sockets on the UI executor.
    runtime.spawn(async move {
        let status = match probe_upstream_proxy_route(&host, port, 10, &upstream_proxy).await {
            Ok(()) => HostKeyStatus::Verified,
            Err(error) => HostKeyStatus::Error {
                message: error.to_string(),
            },
        };
        let _ = status_tx.send(status);
    });
    status_rx
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
            0 => self.settings_network_shared_proxy_section(proxy, cx),
            1 => self.settings_network_routing_section(cx),
            _ => div().into_any_element(),
        }
    }

    fn settings_network_shared_proxy_section(
        &self,
        proxy: Option<&SettingsUpstreamProxyConfig>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(proxy) = proxy else {
            let disclaimer_accepted = self
                .settings_store
                .settings()
                .network
                .upstream_proxy_disclaimer_accepted;
            let empty_state = div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.settings_card_gap))
                .child(self.network_checkbox_row(
                    "settings_view.network.disclaimer",
                    "settings_view.network.disclaimer_hint",
                    disclaimer_accepted,
                    true,
                    Self::toggle_settings_network_disclaimer,
                    cx,
                ))
                .child(
                    div()
                        .flex_none()
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.network.add_shared_proxy"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Default,
                                    size: ButtonSize::Default,
                                    radius: ButtonRadius::Md,
                                    disabled: !disclaimer_accepted,
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(|this, _event, _window, cx| {
                                this.toggle_settings_network_enabled(cx);
                                cx.stop_propagation();
                            }),
                        )),
                )
                .into_any_element();
            return self.settings_card(
                "settings_view.network.shared_proxy",
                "settings_view.network.shared_proxy_empty_hint",
                vec![empty_state],
            );
        };

        let content =
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.settings_card_gap))
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
                                network_proxy_protocol_label(proxy.protocol, &self.i18n),
                                true,
                                cx,
                            ),
                        ))
                        .child(self.network_compact_field(
                            SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                            self.network_input_field(
                                "settings_view.network.port",
                                "settings_view.network.port_hint",
                                SettingsInput::NetworkProxyPort,
                                proxy.port.to_string(),
                                "1080".to_string(),
                                true,
                                cx,
                            ),
                        )),
                )
                .child(self.network_full_width_input(
                    "settings_view.network.host",
                    "settings_view.network.host_hint",
                    SettingsInput::NetworkProxyHost,
                    proxy.host.clone(),
                    "127.0.0.1".to_string(),
                    true,
                    cx,
                ))
                .child(self.network_full_width_input(
                    "settings_view.network.no_proxy",
                    "settings_view.network.no_proxy_hint",
                    SettingsInput::NetworkProxyNoProxy,
                    proxy.no_proxy.clone(),
                    "localhost,127.0.0.1,*.internal".to_string(),
                    true,
                    cx,
                ))
                .child(self.network_checkbox_row(
                    "settings_view.network.remote_dns",
                    "settings_view.network.remote_dns_hint",
                    proxy.remote_dns,
                    true,
                    Self::toggle_settings_network_remote_dns,
                    cx,
                ))
                .child(self.card_separator())
                .child(self.settings_network_auth_content(Some(proxy), cx))
                .child(self.card_separator())
                .child(self.settings_network_test_content(true, cx))
                .child(self.card_separator())
                .child(div().w_full().flex().justify_end().child(
                    self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.network.remove_shared_proxy"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::Trash2,
                            16.0,
                            rgb(self.tokens.ui.error),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(|this, _event, _window, cx| {
                            this.toggle_settings_network_enabled(cx);
                            cx.stop_propagation();
                        }),
                    ),
                ))
                .into_any_element();

        self.settings_card(
            "settings_view.network.shared_proxy",
            "settings_view.network.shared_proxy_hint",
            vec![content],
        )
    }

    fn settings_network_auth_content(
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
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_row_gap))
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

        section.into_any_element()
    }

    fn settings_network_test_content(
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

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .opacity(if proxy_enabled { 1.0 } else { 0.4 })
            .child(self.network_subsection_heading(
                "settings_view.network.test_title",
                "settings_view.network.test_hint",
            ))
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
                            "settings_view.network.test_host_hint",
                            SettingsInput::NetworkProxyTestHost,
                            host_value,
                            "server.example.com".to_string(),
                            proxy_enabled,
                            cx,
                        ),
                    ))
                    .child(self.network_compact_field(
                        SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                        self.network_input_field(
                            "settings_view.network.test_port",
                            "settings_view.network.test_port_hint",
                            SettingsInput::NetworkProxyTestPort,
                            port_value,
                            "22".to_string(),
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
            .into_any_element()
    }

    fn settings_network_routing_section(&self, cx: &mut Context<Self>) -> AnyElement {
        // Routing policy is distinct from the reusable proxy definition above.
        // SSH remains connection-owned while app and updater routes are global.
        let settings = self.settings_store.settings();
        let application_mode = settings.network.application_proxy_mode;
        let update_proxy = &settings.general.update_proxy;
        let mut content = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(self.network_static_route_row(
                "settings_view.network.ssh_route",
                "settings_view.network.ssh_route_hint",
                "settings_view.network.per_connection",
            ))
            .child(self.card_separator())
            .child(self.network_route_select_row(
                "settings_view.network.application_route",
                "settings_view.network.application_route_hint",
                SettingsSelect::NetworkApplicationProxyMode,
                network_application_proxy_mode_label(application_mode, &self.i18n),
                cx,
            ))
            .child(self.card_separator())
            .child(self.network_route_select_row(
                "settings_view.network.update_route",
                "settings_view.network.update_route_hint",
                SettingsSelect::UpdateProxyMode,
                update_proxy_mode_label(update_proxy.mode, &self.i18n),
                cx,
            ));

        if update_proxy.mode == UpdateProxyMode::Custom {
            content = content.child(self.settings_network_custom_update_proxy(cx));
        }

        content = content.child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.network.legal_hint")),
        );

        self.settings_card(
            "settings_view.network.routing",
            "settings_view.network.routing_hint",
            vec![content.into_any_element()],
        )
    }

    fn settings_network_custom_update_proxy(&self, cx: &mut Context<Self>) -> AnyElement {
        let proxy = &self.settings_store.settings().general.update_proxy;
        div()
            .w_full()
            .min_w(px(0.0))
            .pt(px(self.tokens.metrics.settings_row_gap))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .child(self.network_subsection_heading(
                "settings_view.network.custom_update_proxy",
                "settings_view.network.custom_update_proxy_hint",
            ))
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
                            "settings_view.network.update_protocol",
                            "settings_view.network.update_protocol_hint",
                            SettingsSelect::UpdateProxyProtocol,
                            update_proxy_protocol_label(proxy.protocol, &self.i18n),
                            true,
                            cx,
                        ),
                    ))
                    .child(self.network_compact_field(
                        SETTINGS_NETWORK_PORT_FIELD_WIDTH,
                        self.network_input_field(
                            "settings_view.network.update_port",
                            "settings_view.network.update_port_hint",
                            SettingsInput::UpdateProxyPort,
                            self.current_settings_input_value(SettingsInput::UpdateProxyPort),
                            "7890".to_string(),
                            true,
                            cx,
                        ),
                    )),
            )
            .child(self.network_full_width_input(
                "settings_view.network.update_host",
                "settings_view.network.update_host_hint",
                SettingsInput::UpdateProxyHost,
                self.current_settings_input_value(SettingsInput::UpdateProxyHost),
                "127.0.0.1".to_string(),
                true,
                cx,
            ))
            .child(self.network_full_width_input(
                "settings_view.network.update_no_proxy",
                "settings_view.network.update_no_proxy_hint",
                SettingsInput::UpdateProxyNoProxy,
                self.current_settings_input_value(SettingsInput::UpdateProxyNoProxy),
                "localhost,127.0.0.1".to_string(),
                true,
                cx,
            ))
            .into_any_element()
    }

    fn network_subsection_heading(&self, label_key: &str, hint_key: &str) -> AnyElement {
        self.network_field_label(label_key, hint_key)
    }

    fn network_static_route_row(
        &self,
        label_key: &str,
        hint_key: &str,
        value_key: &str,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex_basis(px(SETTINGS_NETWORK_FIELD_WIDTH))
                    .child(self.network_field_label(label_key, hint_key)),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(value_key)),
            )
            .into_any_element()
    }

    fn network_route_select_row(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex_basis(px(SETTINGS_NETWORK_FIELD_WIDTH))
                    .child(self.network_field_label(label_key, hint_key)),
            )
            .child(
                div()
                    .w(px(240.0))
                    .max_w_full()
                    .flex_none()
                    .child(self.settings_select_control(select_id, value, false, None, cx)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn network_responsive_field(
        &self,
        preferred_width: f32,
        field: AnyElement,
    ) -> AnyElement {
        // Field slots grow with the card and wrap once their preferred width
        // no longer fits, avoiding a fixed-width cluster on wide panes.
        div()
            .max_w_full()
            .min_w(px(0.0))
            .flex_1()
            .flex_basis(px(preferred_width))
            .child(field)
            .into_any_element()
    }

    pub(in crate::workspace) fn network_compact_field(
        &self,
        preferred_width: f32,
        field: AnyElement,
    ) -> AnyElement {
        // Compact numeric fields keep their natural width while larger peers
        // consume the remaining row; max-width still permits narrow panes.
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
                self.settings_text_input_control_fill(input, value, placeholder, cx)
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
                    window.focus(&this.focus_handle, cx);
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
                            .child(self.settings_secret_text_input_control_fill(
                                password_input,
                                String::new(),
                                if has_saved_password {
                                    self.i18n
                                        .t("settings_view.network.password_saved_placeholder")
                                } else {
                                    String::new()
                                },
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
        let removes_saved_password = self
            .settings_store
            .settings()
            .network
            .upstream_proxy
            .as_ref()
            .is_some_and(|proxy| {
                matches!(
                    &proxy.auth,
                    SettingsUpstreamProxyAuth::Password {
                        keychain_id: Some(_),
                        ..
                    }
                )
            });
        if removes_saved_password
            && let Err(error) = self
                .connection_store
                .delete_global_upstream_proxy_password()
        {
            self.settings_network_proxy_password_status = Some(error.to_string());
            cx.notify();
            return;
        }
        // Disabling the proxy also clears any transient credential draft.
        self.clear_settings_input_draft(SettingsInput::NetworkProxyPassword);
        self.edit_settings(
            |settings| {
                if settings.network.upstream_proxy.is_some() {
                    settings.network.application_proxy_mode = SettingsApplicationProxyMode::System;
                }
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
                                    keychain_id: Some(keychain_id),
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
        let Ok(upstream_proxy) =
            upstream_proxy_config_from_global_settings(&self.connection_store, &proxy)
        else {
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
        let status_rx = schedule_settings_network_proxy_test(
            self.forwarding_runtime.as_ref(),
            host,
            port,
            upstream_proxy,
        );

        cx.spawn(async move |weak, cx| {
            let status = status_rx.await.unwrap_or_else(|_| HostKeyStatus::Error {
                message: "proxy route test task stopped unexpectedly".to_string(),
            });
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_ssh::{UpstreamProxyAuth, UpstreamProxyProtocol};

    #[test]
    fn proxy_route_test_enters_workspace_tokio_runtime() {
        let proxy_listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("bind a proxy test listener");
        let proxy_port = proxy_listener
            .local_addr()
            .expect("read proxy address")
            .port();
        let proxy_server = std::thread::spawn(move || {
            // Closing the accepted socket forces a deterministic SOCKS handshake error.
            let (stream, _) = proxy_listener.accept().expect("accept proxy test client");
            drop(stream);
        });

        let runtime = tokio::runtime::Runtime::new().expect("create workspace runtime");
        let status_rx = schedule_settings_network_proxy_test(
            &runtime,
            "proxy-test-target.invalid".to_string(),
            22,
            UpstreamProxyConfig {
                protocol: UpstreamProxyProtocol::Socks5,
                host: std::net::Ipv4Addr::LOCALHOST.to_string(),
                port: proxy_port,
                auth: UpstreamProxyAuth::None,
                remote_dns: true,
                no_proxy: String::new(),
            },
        );

        let status = status_rx
            .blocking_recv()
            .expect("receive the proxy test result outside Tokio");
        proxy_server.join().expect("join proxy test server");
        assert!(matches!(status, HostKeyStatus::Error { .. }));
    }
}
