use super::*;
use oxideterm_connections::{ConnectionTerminalBellPolicy, ConnectionTerminalSessionLogPolicy};

struct JumpServerRenderSnapshot {
    totp_credential_id: Option<String>,
    totp_picker_expanded: bool,
    saved_connection_id: String,
    host: String,
    port: String,
    username: String,
    auth_tab: SshAuthTab,
    empty_password: bool,
    key_path: String,
    managed_key_id: String,
    cert_path: String,
    identity_agent: String,
    gssapi_enabled: bool,
    gssapi_server_identity: String,
    gssapi_delegate_credentials: bool,
    agent_forwarding: bool,
    legacy_ssh_compatibility: bool,
    complete: bool,
}

impl JumpServerRenderSnapshot {
    fn from_hop(hop: &NewConnectionProxyHop) -> Self {
        Self {
            totp_credential_id: hop.totp_credential_id.clone(),
            totp_picker_expanded: hop.totp_picker_expanded,
            saved_connection_id: hop.saved_connection_id.clone(),
            host: hop.host.clone(),
            port: hop.port.clone(),
            username: hop.username.clone(),
            auth_tab: hop.auth_tab,
            empty_password: hop.empty_password,
            key_path: hop.key_path.clone(),
            managed_key_id: hop.managed_key_id.clone(),
            cert_path: hop.cert_path.clone(),
            identity_agent: hop.identity_agent.clone(),
            gssapi_enabled: hop.gssapi_enabled,
            gssapi_server_identity: hop.gssapi_server_identity.clone(),
            gssapi_delegate_credentials: hop.gssapi_delegate_credentials,
            agent_forwarding: hop.agent_forwarding,
            legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
            complete: hop.complete(),
        }
    }
}

struct ProxyHopSummarySnapshot {
    saved_connection_name: Option<String>,
    host: String,
    port: String,
    username: String,
    auth_tab: SshAuthTab,
    gssapi_enabled: bool,
}

impl ConnectionFormView {
    pub(in crate::views) fn render_new_connection_select_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // New-connection dropdowns mount and unmount with their logical state;
        // no render-only payload is retained for an exit transition.
        let select_id = self.connection_form_state(cx).open_select?;
        let anchor_id = Self::new_connection_select_anchor_id(select_id);
        let anchor = self.flow.read(cx).select_anchor_store().get(anchor_id)?;
        let width =
            f32::from(anchor.bounds.size.width).max(self.tokens.metrics.ui_select_min_width);
        let viewport_height = f32::from(window.viewport_size().height);
        let popup_gap = self.tokens.metrics.settings_select_popup_gap;
        let below = viewport_height - f32::from(anchor.bounds.bottom()) - popup_gap;
        let above = f32::from(anchor.bounds.top()) - popup_gap;
        let opens_above = below < self.tokens.metrics.ui_select_max_height && above > below;
        let max_height = if opens_above { above } else { below }
            .max(self.tokens.metrics.ui_control_height)
            .min(self.tokens.metrics.ui_select_max_height);

        let mut popup = select_overlay_popup_with_max_height(&self.tokens, width, max_height);
        match select_id {
            NewConnectionSelect::Totp | NewConnectionSelect::JumpTotp => {
                let current = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| {
                        if select_id == NewConnectionSelect::JumpTotp {
                            form.jump_server_form
                                .as_ref()
                                .and_then(|hop| hop.totp_credential_id.clone())
                        } else {
                            form.totp_credential_id.clone()
                        }
                    });
                let choices = std::iter::once((None, self.i18n.t("settings_view.totp.none")))
                    .chain(
                        self.environment
                            .totp
                            .iter()
                            .map(|entry| (Some(entry.id.clone()), entry.name.clone())),
                    );
                for (id, label) in choices {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, id == current),
                        false,
                        false,
                        cx.listener(move |this, _, _, cx| {
                            this.close_new_connection_select(cx);
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = &mut state.form {
                                    if select_id == NewConnectionSelect::JumpTotp {
                                        if let Some(hop) = &mut form.jump_server_form {
                                            hop.totp_credential_id = id.clone();
                                        }
                                    } else {
                                        form.totp_credential_id = id.clone();
                                    }
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
            }
            NewConnectionSelect::Group => {
                let current_group = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.group.as_str())
                    .unwrap_or_default();
                let ungrouped_label = self.connection_form_ungrouped_label();
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        ungrouped_label.clone(),
                        self.connection_form_group_is_ungrouped(current_group),
                    ),
                    false,
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        this.set_new_connection_group(ungrouped_label.clone(), cx);
                        cx.stop_propagation();
                    }),
                ));

                let groups = self.connection_form_group_options(current_group);
                for group in groups.iter().cloned() {
                    let selected = group == current_group;
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, group.clone(), selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_group(group.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                if groups.is_empty() {
                    popup = popup.child(
                        div()
                            .relative()
                            .flex()
                            .w_full()
                            .items_center()
                            .rounded(px(self.tokens.radii.xs))
                            .py(px(self.tokens.metrics.ui_menu_item_padding_y))
                            .px(px(self.tokens.metrics.ui_menu_item_padding_x))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .opacity(0.65)
                            .child(self.i18n.t("ssh.form.create_groups_hint")),
                    );
                }
            }
            NewConnectionSelect::TunnelHost => {
                let selected_id = self
                    .environment
                    .tunnel
                    .as_ref()
                    .and_then(|tunnel| tunnel.host_id.as_deref());
                let connections = self.environment.connections.clone();
                if connections.is_empty() {
                    popup = popup.child(
                        div()
                            .p_2()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("ssh_tunnel.choose_host")),
                    );
                }
                for host in connections {
                    let selected = selected_id == Some(host.id.as_str());
                    let label = format!(
                        "{} · {}@{}:{}",
                        host.name, host.username, host.host, host.port
                    );
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected),
                        false,
                        false,
                        cx.listener(move |this, _, _, cx| {
                            this.close_new_connection_select(cx);
                            this.request(
                                ConnectionsWindowRequest::SelectTunnelHost(host.id.clone()),
                                None,
                                cx,
                            );
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
            }
            NewConnectionSelect::JumpSavedConnection => {
                let selected_connection_id = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| form.jump_server_form.as_ref())
                    .map(|jump_form| jump_form.saved_connection_id.as_str())
                    .unwrap_or_default();
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n.t("ssh.form.proxy_jump_saved_connection_custom"),
                        selected_connection_id.is_empty(),
                    ),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        this.clear_new_connection_jump_saved_connection(cx);
                        cx.stop_propagation();
                    }),
                ));
                for connection in self.environment.connections.clone() {
                    let selected = connection.id == selected_connection_id;
                    let connection_id = connection.id.clone();
                    let label = format!(
                        "{} · {}@{}:{}",
                        connection.name, connection.username, connection.host, connection.port
                    );
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_jump_saved_connection(
                                connection_id.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::RemoteDesktopSshGateway => {
                let selected_connection_id = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| form.remote_desktop_ssh_gateway_connection_id.as_deref());
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n
                            .t("modals.new_connection.remote_desktop_ssh_gateway_direct"),
                        selected_connection_id.is_none(),
                    ),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        this.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                form.remote_desktop_ssh_gateway_connection_id = None;
                            }
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ));
                for connection in self.environment.connections.clone() {
                    let selected = selected_connection_id == Some(connection.id.as_str());
                    let connection_id = connection.id.clone();
                    let label = format!(
                        "{} · {}@{}:{}",
                        connection.name, connection.username, connection.host, connection.port
                    );
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.remote_desktop_ssh_gateway_connection_id =
                                        Some(connection_id.clone());
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
            }
            NewConnectionSelect::KeyAuthSource
            | NewConnectionSelect::StandaloneSftpSecondaryKeyAuthSource
            | NewConnectionSelect::JumpKeyAuthSource => {
                let context = match select_id {
                    NewConnectionSelect::KeyAuthSource => {
                        self.current_main_auth_selector_context(cx)
                    }
                    NewConnectionSelect::JumpKeyAuthSource => AuthSelectorContext::Jump,
                    NewConnectionSelect::StandaloneSftpSecondaryKeyAuthSource => {
                        AuthSelectorContext::StandaloneSftpSecondary
                    }
                    _ => unreachable!("matched only key auth source selects"),
                };
                let active_tab = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| match select_id {
                        NewConnectionSelect::KeyAuthSource => Some(form.auth_tab),
                        NewConnectionSelect::StandaloneSftpSecondaryKeyAuthSource => {
                            Some(form.standalone_sftp_secondary.auth_tab)
                        }
                        NewConnectionSelect::JumpKeyAuthSource => form
                            .jump_server_form
                            .as_ref()
                            .map(|jump_form| jump_form.auth_tab),
                        _ => None,
                    })
                    .unwrap_or(SshAuthTab::SshKey);
                let active_source = Self::normalized_key_source_for_context(active_tab, context);
                for source in Self::key_auth_source_choices(context).iter().copied() {
                    let label = self.key_auth_source_label(source);
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, source == active_source),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_key_auth_source(select_id, source, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::ManagedKey
            | NewConnectionSelect::StandaloneSftpSecondaryManagedKey
            | NewConnectionSelect::JumpManagedKey => {
                let current_key_id = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| match select_id {
                        NewConnectionSelect::ManagedKey => Some(form.managed_key_id.as_str()),
                        NewConnectionSelect::StandaloneSftpSecondaryManagedKey => {
                            Some(form.standalone_sftp_secondary.managed_key_id.as_str())
                        }
                        NewConnectionSelect::JumpManagedKey => form
                            .jump_server_form
                            .as_ref()
                            .map(|jump_form| jump_form.managed_key_id.as_str()),
                        _ => None,
                    })
                    .unwrap_or_default();
                for key in self.environment.keys.clone() {
                    let selected = key.id == current_key_id;
                    let key_id = key.id.clone();
                    let label = format!("{} · {}", key.name, key.fingerprint);
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_managed_key(select_id, key_id.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::UpstreamProxyPolicy
            | NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyPolicy => {
                let secondary =
                    select_id == NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyPolicy;
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| {
                        if secondary {
                            form.standalone_sftp_secondary.upstream_proxy_policy
                        } else {
                            form.upstream_proxy_policy
                        }
                    })
                    .unwrap_or(NewConnectionUpstreamProxyPolicy::UseGlobal);
                for (policy, label_key) in [
                    (
                        NewConnectionUpstreamProxyPolicy::UseGlobal,
                        "modals.upstream_proxy.use_global",
                    ),
                    (
                        NewConnectionUpstreamProxyPolicy::Direct,
                        "modals.upstream_proxy.direct",
                    ),
                    (
                        NewConnectionUpstreamProxyPolicy::Custom,
                        "modals.upstream_proxy.custom",
                    ),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(label_key), policy == selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_upstream_proxy_policy(policy, secondary, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::UpstreamProxyProtocol
            | NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyProtocol => {
                let secondary =
                    select_id == NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyProtocol;
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| {
                        if secondary {
                            form.standalone_sftp_secondary.upstream_proxy_protocol
                        } else {
                            form.upstream_proxy_protocol
                        }
                    })
                    .unwrap_or(SavedUpstreamProxyProtocol::Socks5);
                for (protocol, label_key) in [
                    (
                        SavedUpstreamProxyProtocol::Socks5,
                        "settings_view.network.protocol_socks5",
                    ),
                    (
                        SavedUpstreamProxyProtocol::HttpConnect,
                        "settings_view.network.protocol_http_connect",
                    ),
                ] {
                    if protocol == SavedUpstreamProxyProtocol::HttpConnect
                        && self
                            .connection_form_state(cx)
                            .form
                            .as_ref()
                            .is_some_and(|form| form.transport == NewConnectionTransport::Rdp)
                    {
                        continue;
                    }
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(label_key), protocol == selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_upstream_proxy_protocol(
                                protocol, secondary, cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::UpstreamProxyAuth
            | NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyAuth => {
                let secondary =
                    select_id == NewConnectionSelect::StandaloneSftpSecondaryUpstreamProxyAuth;
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| {
                        if secondary {
                            form.standalone_sftp_secondary.upstream_proxy_auth
                        } else {
                            form.upstream_proxy_auth
                        }
                    })
                    .unwrap_or(NewConnectionUpstreamProxyAuth::None);
                for (auth, label_key) in [
                    (
                        NewConnectionUpstreamProxyAuth::None,
                        "settings_view.network.auth_none",
                    ),
                    (
                        NewConnectionUpstreamProxyAuth::Password,
                        "settings_view.network.auth_password",
                    ),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(label_key), auth == selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_upstream_proxy_auth(auth, secondary, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::TerminalEncoding => {
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| form.terminal.encoding);
                let default_label = self
                    .i18n
                    .t("ssh.form.terminal_use_application_default")
                    .replace(
                        "{{value}}",
                        &terminal_encoding_label(self.environment.terminal.terminal_encoding),
                    );
                popup = popup.child(select_option_action(
                    select_option(&self.tokens, default_label, selected.is_none()),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        this.set_new_connection_terminal_encoding(None, cx);
                        cx.stop_propagation();
                    }),
                ));
                for encoding in [
                    ConnectionTerminalEncoding::Utf8,
                    ConnectionTerminalEncoding::Gbk,
                    ConnectionTerminalEncoding::Gb18030,
                    ConnectionTerminalEncoding::Big5,
                    ConnectionTerminalEncoding::ShiftJis,
                    ConnectionTerminalEncoding::EucJp,
                    ConnectionTerminalEncoding::EucKr,
                    ConnectionTerminalEncoding::Windows1252,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            connection_terminal_encoding_label(encoding).to_string(),
                            selected == Some(encoding),
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_terminal_encoding(Some(encoding), cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::TerminalBackspaceSequence => {
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| form.terminal.backspace_sequence);
                let default_label = self
                    .i18n
                    .t("ssh.form.terminal_use_application_default")
                    .replace(
                        "{{value}}",
                        terminal_backspace_sequence_label(
                            self.environment.terminal.backspace_sequence,
                        ),
                    );
                popup = popup.child(select_option_action(
                    select_option(&self.tokens, default_label, selected.is_none()),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        this.set_new_connection_terminal_backspace_sequence(None, cx);
                        cx.stop_propagation();
                    }),
                ));
                for sequence in [
                    ConnectionTerminalBackspaceSequence::Delete,
                    ConnectionTerminalBackspaceSequence::ControlH,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            connection_terminal_backspace_sequence_label(sequence).to_string(),
                            selected == Some(sequence),
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_terminal_backspace_sequence(Some(sequence), cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::TerminalDeleteSequence => {
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| form.terminal.delete_sequence);
                let default_label = self
                    .i18n
                    .t("ssh.form.terminal_use_application_default")
                    .replace(
                        "{{value}}",
                        terminal_delete_sequence_label(self.environment.terminal.delete_sequence),
                    );
                popup = popup.child(select_option_action(
                    select_option(&self.tokens, default_label, selected.is_none()),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        this.set_new_connection_terminal_delete_sequence(None, cx);
                        cx.stop_propagation();
                    }),
                ));
                for sequence in [
                    ConnectionTerminalDeleteSequence::Csi3Tilde,
                    ConnectionTerminalDeleteSequence::Delete,
                    ConnectionTerminalDeleteSequence::ControlH,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            connection_terminal_delete_sequence_label(sequence).to_string(),
                            selected == Some(sequence),
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_terminal_delete_sequence(Some(sequence), cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::TerminalSemanticScheme => {
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| form.terminal.semantic_scheme.clone());
                let terminal = &self.environment.terminal;
                let default_name = terminal
                    .active_custom_semantic_scheme()
                    .map(|scheme| scheme.name.clone())
                    .unwrap_or_else(|| match terminal.semantic_scheme {
                        oxideterm_settings::TerminalSemanticScheme::Balanced => self
                            .i18n
                            .t("settings_view.terminal.highlight_rules.semantic_scheme_balanced"),
                        oxideterm_settings::TerminalSemanticScheme::Conservative => self.i18n.t(
                            "settings_view.terminal.highlight_rules.semantic_scheme_conservative",
                        ),
                    });
                let default_label = self
                    .i18n
                    .t("ssh.form.terminal_use_application_default")
                    .replace("{{value}}", &default_name);
                popup = popup.child(select_option_action(
                    select_option(&self.tokens, default_label, selected.is_none()),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        this.set_new_connection_terminal_semantic_scheme(None, cx);
                        cx.stop_propagation();
                    }),
                ));
                let mut schemes = vec![
                    (
                        "balanced".to_string(),
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.semantic_scheme_balanced"),
                    ),
                    (
                        "conservative".to_string(),
                        self.i18n.t(
                            "settings_view.terminal.highlight_rules.semantic_scheme_conservative",
                        ),
                    ),
                ];
                schemes.extend(
                    terminal
                        .custom_semantic_schemes
                        .iter()
                        .map(|scheme| (scheme.id.clone(), scheme.name.clone())),
                );
                for (id, name) in schemes {
                    let is_selected = selected.as_deref() == Some(id.as_str());
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, name, is_selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_terminal_semantic_scheme(Some(id.clone()), cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::TerminalHighlightRuleSet => {
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| form.terminal.highlight_rule_set.clone());
                let terminal = &self.environment.terminal;
                let default_name = terminal
                    .default_highlight_rule_set_name()
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.rule_set_global_base")
                    });
                let default_label = self
                    .i18n
                    .t("ssh.form.terminal_use_application_default")
                    .replace("{{value}}", &default_name);
                popup = popup.child(select_option_action(
                    select_option(&self.tokens, default_label, selected.is_none()),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        this.set_new_connection_terminal_highlight_rule_set(None, cx);
                        cx.stop_propagation();
                    }),
                ));
                for rule_set in &terminal.highlight_rule_sets {
                    let id = rule_set.id.clone();
                    let is_selected = selected.as_deref() == Some(id.as_str());
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, rule_set.name.clone(), is_selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_terminal_highlight_rule_set(
                                Some(id.clone()),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::TerminalSessionLogPolicy => {
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.terminal.session_log_policy)
                    .unwrap_or_default();
                let inherited_mode = if self.environment.terminal.session_log.automatic {
                    self.i18n.t("ssh.form.terminal_session_log_automatic")
                } else {
                    self.i18n.t("ssh.form.terminal_session_log_manual")
                };
                for (policy, label) in [
                    (
                        ConnectionTerminalSessionLogPolicy::Inherit,
                        self.i18n
                            .t("ssh.form.terminal_use_application_default")
                            .replace("{{value}}", &inherited_mode),
                    ),
                    (
                        ConnectionTerminalSessionLogPolicy::Automatic,
                        self.i18n.t("ssh.form.terminal_session_log_automatic"),
                    ),
                    (
                        ConnectionTerminalSessionLogPolicy::Manual,
                        self.i18n.t("ssh.form.terminal_session_log_manual"),
                    ),
                    (
                        ConnectionTerminalSessionLogPolicy::Disabled,
                        self.i18n.t("ssh.form.terminal_session_log_disabled"),
                    ),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected == policy),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_terminal_session_log_policy(policy, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::TerminalBellPolicy => {
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.terminal.bell_policy)
                    .unwrap_or_default();
                let inherited_style = match self.environment.terminal.bell_style {
                    oxideterm_settings::TerminalBellStyle::Off => {
                        self.i18n.t("settings_view.terminal.bell_style_off")
                    }
                    oxideterm_settings::TerminalBellStyle::SoftFlash => {
                        self.i18n.t("settings_view.terminal.bell_style_soft_flash")
                    }
                    oxideterm_settings::TerminalBellStyle::EdgeGlow => {
                        self.i18n.t("settings_view.terminal.bell_style_edge_glow")
                    }
                    oxideterm_settings::TerminalBellStyle::CursorPulse => self
                        .i18n
                        .t("settings_view.terminal.bell_style_cursor_pulse"),
                    oxideterm_settings::TerminalBellStyle::HeaderStrip => self
                        .i18n
                        .t("settings_view.terminal.bell_style_header_strip"),
                };
                for (policy, label) in [
                    (
                        ConnectionTerminalBellPolicy::Inherit,
                        self.i18n
                            .t("ssh.form.terminal_use_application_default")
                            .replace("{{value}}", &inherited_style),
                    ),
                    (
                        ConnectionTerminalBellPolicy::Silent,
                        self.i18n.t("ssh.form.terminal_bell_silent"),
                    ),
                    (
                        ConnectionTerminalBellPolicy::TabHighlight,
                        self.i18n.t("ssh.form.terminal_bell_tab_highlight"),
                    ),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected == policy),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_terminal_bell_policy(policy, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::LocalShell => {
                let selected_shell_id = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| form.local_shell_id.as_deref());
                let resolved_shell_id = selected_shell_id;
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n.t("local_session.inherit_shell"),
                        selected_shell_id.is_none(),
                    ),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        this.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                form.local_shell_id = None;
                                form.error = None;
                            }
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ));
                let default_shell_id = self.environment.default_shell_id.as_deref();
                let default_label = self.i18n.t("settings_view.local_terminal.default");

                // The modal uses the same select surface as the other connection
                // fields; the empty selection inherits application defaults.
                for shell in self.environment.local_shells.iter().cloned() {
                    let shell_id = shell.id.clone();
                    let label = if default_shell_id == Some(shell.id.as_str()) {
                        format!("{} · {default_label}", shell.label)
                    } else {
                        shell.label
                    };
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            resolved_shell_id == Some(shell.id.as_str()),
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.local_shell_id = Some(shell_id.clone());
                                    form.field_focused = false;
                                    clear_connection_selection(form);
                                    form.error = None;
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
            }
            NewConnectionSelect::BleAdapter => {
                let selected_index = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.ble.selected_adapter_index)
                    .unwrap_or_default();
                let adapters = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.ble.adapters.clone())
                    .unwrap_or_default();
                if adapters.is_empty() {
                    let label = if self
                        .connection_form_state(cx)
                        .form
                        .as_ref()
                        .is_some_and(|form| form.ble.adapters_loading)
                    {
                        self.i18n.t("modals.new_connection.ble_loading_adapters")
                    } else {
                        self.i18n.t("modals.new_connection.ble_no_adapters")
                    };
                    popup = popup.child(select_option(&self.tokens, label, false));
                } else {
                    for (index, adapter) in adapters.iter().enumerate() {
                        let selected = index == selected_index;
                        popup = popup.child(select_option_action(
                            select_option(&self.tokens, adapter.display_label(), selected),
                            false,
                            false,
                            cx.listener(move |this, _event, _window, cx| {
                                this.close_new_connection_select(cx);
                                this.set_new_connection_ble_adapter(index, cx);
                                cx.stop_propagation();
                            }),
                        ));
                    }
                }
            }
            NewConnectionSelect::SerialPort => {
                let selected_port = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.serial_port_path.as_str())
                    .unwrap_or_default();
                let ports = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.serial_ports.clone())
                    .unwrap_or_default();
                for port in ports {
                    let selected = port.port_path == selected_port;
                    let port_path = port.port_path.clone();
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, serial_port_display_label(&port), selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_serial_port(port_path.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::SerialDataBits | NewConnectionSelect::SerialStopBits => {
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| match select_id {
                        NewConnectionSelect::SerialDataBits => form.serial_data_bits,
                        NewConnectionSelect::SerialStopBits => form.serial_stop_bits,
                        _ => 0,
                    })
                    .unwrap_or_default();
                let choices: &[(u8, &str)] = match select_id {
                    NewConnectionSelect::SerialDataBits => {
                        &[(5, "5"), (6, "6"), (7, "7"), (8, "8")]
                    }
                    NewConnectionSelect::SerialStopBits => &[(1, "1"), (2, "2")],
                    _ => &[],
                };
                for (value, label) in choices.iter().copied() {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label.to_string(), value == selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_serial_u8(select_id, value, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::SerialParity => {
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.serial_parity)
                    .unwrap_or(oxideterm_terminal::SerialParity::None);
                for parity in [
                    oxideterm_terminal::SerialParity::None,
                    oxideterm_terminal::SerialParity::Odd,
                    oxideterm_terminal::SerialParity::Even,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.serial_parity_label(parity),
                            parity == selected,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_serial_parity(parity, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
            NewConnectionSelect::TelnetInputLineEnding
            | NewConnectionSelect::TelnetOutputLineEnding
            | NewConnectionSelect::BleInputLineEnding
            | NewConnectionSelect::BleOutputLineEnding => {
                use oxideterm_connections::SerialLineEnding;
                // Telnet and BLE share one line-ending popup: the select id
                // alone picks the form family and the edited direction.
                let output = matches!(
                    select_id,
                    NewConnectionSelect::TelnetOutputLineEnding
                        | NewConnectionSelect::BleOutputLineEnding
                );
                let ble = matches!(
                    select_id,
                    NewConnectionSelect::BleInputLineEnding
                        | NewConnectionSelect::BleOutputLineEnding
                );
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| match (ble, output) {
                        (false, false) => form.telnet_input_line_ending,
                        (false, true) => form.telnet_output_line_ending,
                        (true, false) => form.ble.input_line_ending,
                        (true, true) => form.ble.output_line_ending,
                    })
                    .unwrap_or_default();
                for value in [
                    SerialLineEnding::None,
                    SerialLineEnding::Lf,
                    SerialLineEnding::CrLf,
                    SerialLineEnding::Cr,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.connection_line_ending_label(value),
                            selected == value,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _, _, cx| {
                            this.close_new_connection_select(cx);
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    match (ble, output) {
                                        (false, false) => form.telnet_input_line_ending = value,
                                        (false, true) => form.telnet_output_line_ending = value,
                                        (true, false) => form.ble.input_line_ending = value,
                                        (true, true) => form.ble.output_line_ending = value,
                                    }
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
            }
            NewConnectionSelect::BleSendMode => {
                use oxideterm_connections::SerialSendMode;
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.ble.send_mode)
                    .unwrap_or_default();
                for value in [SerialSendMode::Text, SerialSendMode::Hex] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.connection_send_mode_label(value),
                            selected == value,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _, _, cx| {
                            this.close_new_connection_select(cx);
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.ble.send_mode = value;
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
            }
            NewConnectionSelect::BleDisplayMode => {
                use oxideterm_connections::SerialDisplayMode;
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.ble.display_mode)
                    .unwrap_or_default();
                for value in [
                    SerialDisplayMode::Text,
                    SerialDisplayMode::Hex,
                    SerialDisplayMode::Mixed,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.connection_display_mode_label(value),
                            selected == value,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _, _, cx| {
                            this.close_new_connection_select(cx);
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.ble.display_mode = value;
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
            }
            NewConnectionSelect::SerialFlowControl => {
                let selected = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .map(|form| form.serial_flow_control)
                    .unwrap_or(oxideterm_terminal::SerialFlowControl::None);
                for flow in [
                    oxideterm_terminal::SerialFlowControl::None,
                    oxideterm_terminal::SerialFlowControl::Software,
                    oxideterm_terminal::SerialFlowControl::Hardware,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.serial_flow_control_label(flow),
                            flow == selected,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select(cx);
                            this.set_new_connection_serial_flow_control(flow, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
            }
        }

        let (anchor_corner, position, offset_y) = if opens_above {
            (
                Corner::BottomLeft,
                point(anchor.bounds.left(), anchor.bounds.top()),
                -popup_gap,
            )
        } else {
            (Corner::TopLeft, anchor.bounds.bottom_left(), popup_gap)
        };
        let popup_offset_y = if opens_above { 4.0 } else { -4.0 };
        let popup_origin = gpui::point(0.5, if opens_above { 1.0 } else { 0.0 });
        let popup = oxideterm_gpui_ui::motion::popover_zoom_enter(
            &self.tokens,
            "new-connection-select-popup",
            div().child(popup),
            popup_offset_y,
            popup_origin,
        );

        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(anchor_corner)
                            .position(position)
                            .offset(point(px(0.0), px(offset_y)))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(popup),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_SELECT_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    pub(in crate::views) fn render_add_jump_server_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((jump_form, gssapi_credentials_available, is_editing, footer_error)) =
            self.connection_form_state(cx).form.as_ref().and_then(|form| {
                form.jump_server_form.as_ref().map(|hop| {
                    (
                        JumpServerRenderSnapshot::from_hop(hop),
                        form.gssapi_credentials_available,
                        form.jump_server_edit_index.is_some(),
                        form.error.clone(),
                    )
                })
            })
        else {
            return div().into_any_element();
        };
        if jump_form.gssapi_enabled && gssapi_credentials_available.is_none() {
            self.ensure_kerberos_credentials_availability(cx);
        }
        let theme = self.tokens.ui;
        let light = theme.is_light();
        let top_radius = oxideterm_gpui_ui::modal::rounded_shell_child_radius(self.tokens.radii.lg);
        let modal_shadows = vec![
            gpui::BoxShadow {
                color: rgba(if light { 0x00000018 } else { 0x00000066 }).into(),
                offset: point(px(0.0), px(20.0)),
                blur_radius: px(44.0),
                spread_radius: px(-8.0),
                inset: false,
            },
            gpui::BoxShadow {
                color: rgba(if light { 0x0000000c } else { 0x00000038 }).into(),
                offset: point(px(0.0), px(4.0)),
                blur_radius: px(12.0),
                spread_radius: px(-2.0),
                inset: false,
            },
        ];
        let add_disabled = !jump_form.complete
            || (jump_form.auth_tab == SshAuthTab::ManagedKey
                && jump_form.managed_key_id.trim().is_empty());
        let viewport = window.viewport_size();
        let modal_width = JUMP_STUDIO_MODAL_WIDTH.min((f32::from(viewport.width) - 32.0).max(0.0));
        let modal_max_height = (f32::from(viewport.height)
            * self.tokens.metrics.modal_max_viewport_height_ratio)
            .min(JUMP_STUDIO_MAX_HEIGHT);
        let form_visible = self.connection_form_state(cx).jump_server_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        let show_totp_select =
            jump_form.totp_picker_expanded || jump_form.totp_credential_id.is_some();
        let totp_active = jump_form.totp_credential_id.is_some();
        let totp_toggle_chip = div()
            .id("jump-totp-toggle")
            .h(px(self.tokens.metrics.ui_tabs_list_height))
            .px(px(10.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if totp_active || show_totp_select {
                rgb(theme.accent)
            } else {
                rgb(theme.border)
            })
            .bg(if totp_active || show_totp_select {
                rgba((theme.accent << 8) | 0x18)
            } else {
                rgb(theme.bg_panel)
            })
            .cursor_pointer()
            .hover(|el| el.bg(rgb(theme.bg_hover)))
            .flex()
            .items_center()
            .gap(px(5.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(if totp_active || show_totp_select {
                theme.accent
            } else {
                theme.text_muted
            }))
            .child(Self::render_lucide_icon(
                LucideIcon::KeyRound,
                12.0,
                rgb(if totp_active || show_totp_select {
                    theme.accent
                } else {
                    theme.text_muted
                }),
            ))
            .child("TOTP")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(jump) = state
                            .form
                            .as_mut()
                            .and_then(|form| form.jump_server_form.as_mut())
                        {
                            jump.totp_picker_expanded = !jump.totp_picker_expanded;
                        }
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Overlay clicks dismiss only the jump-server sub-dialog.
                    this.begin_jump_server_form_exit(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "jump-server-form-enter",
                modal_container(&self.tokens)
                    .bg(rgb(theme.bg_elevated))
                    .shadow(modal_shadows)
                    .overflow_hidden()
                    .w(px(modal_width))
                    .max_h(px(modal_max_height))
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .flex_none()
                            .w_full()
                            .rounded_t(px(top_radius))
                            .bg(rgb(theme.bg_panel))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .px(px(20.0))
                            .py(px(13.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .child(
                                        div()
                                            .flex_none()
                                            .size(px(28.0))
                                            .rounded(px(self.tokens.radii.md))
                                            .bg(rgba((theme.accent << 8) | 0x18))
                                            .border_1()
                                            .border_color(rgba((theme.accent << 8) | 0x38))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Network,
                                                14.0,
                                                rgb(theme.accent),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(15.0))
                                            .line_height(px(20.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(theme.text_heading))
                                            .child(self.i18n.t("ssh.form.proxy_jump_title")),
                                    ),
                            )
                            .child(self.workspace_icon_action_button(
                                LucideIcon::X,
                                14.0,
                                rgb(theme.text_muted),
                                IconButtonOptions::compact(24.0),
                                |this, _event, _window, cx| {
                                    this.begin_jump_server_form_exit(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .id("new-connection-jump-server-body-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .px(px(22.0))
                            .py(px(18.0))
                            .overflow_y_scroll()
                            .track_scroll(&self.selectable_text_scroll_handle(
                                "new-connection-jump-server-body-scroll",
                            ))
                            .on_scroll(cx.listener(|this, _event, _window, cx| {
                                // Closing anchored popups on body scroll prevents detached dropdowns.
                                if this.connection_form_state(cx).open_select.is_some() {
                                    this.close_new_connection_select(cx);
                                    this.clear_new_connection_select_anchor(cx);
                                    cx.notify();
                                }
                            }))
                            .flex()
                            .flex_col()
                            .gap(px(14.0))
                            .child(self.render_jump_saved_connection_select(
                                &jump_form.saved_connection_id,
                                cx,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap(px(12.0))
                                    .child(div().flex_1().min_w_0().child(
                                        self.render_connection_field(
                                            self.i18n.t("ssh.form.proxy_jump_host"),
                                            &jump_form.host,
                                            self.i18n.t("ssh.form.proxy_jump_host_placeholder"),
                                            NewConnectionField::JumpHost,
                                            false,
                                            cx,
                                        ),
                                    ))
                                    .child(div().w(px(88.0)).flex_none().child(
                                        self.render_connection_field(
                                            self.i18n.t("ssh.form.proxy_jump_port"),
                                            &jump_form.port,
                                            "22".to_string(),
                                            NewConnectionField::JumpPort,
                                            false,
                                            cx,
                                        ),
                                    ))
                                    .child(div().w(px(156.0)).flex_none().child(
                                        self.render_connection_field(
                                            self.i18n.t("ssh.form.proxy_jump_username"),
                                            &jump_form.username,
                                            self.i18n.t("ssh.form.proxy_jump_username_placeholder"),
                                            NewConnectionField::JumpUsername,
                                            false,
                                            cx,
                                        ),
                                    )),
                            )
                            .child(div().w_full().h(px(1.0)).bg(rgb(theme.border)))
                            .child(
                                div()
                                    .flex()
                                    .items_end()
                                    .gap(px(10.0))
                                    .child(div().flex_1().min_w_0().child(
                                        self.render_auth_selector(
                                            jump_form.auth_tab,
                                            AuthSelectorContext::Jump,
                                            true,
                                            cx,
                                        ),
                                    ))
                                    .child(totp_toggle_chip),
                            )
                            .when(show_totp_select, |content| {
                                content.child(self.render_connection_totp_select(true, cx))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::DefaultKey, |content| {
                                content.child(self.render_connection_hint(
                                    self.i18n.t("ssh.form.default_key_desc"),
                                ))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::SshKey, |content| {
                                content
                                    .child(self.render_connection_field_with_browse(
                                        self.i18n.t("ssh.form.proxy_jump_key_path"),
                                        &jump_form.key_path,
                                        self.i18n.t("ssh.form.proxy_jump_key_path_placeholder"),
                                        NewConnectionField::JumpKeyPath,
                                        cx,
                                    ))
                                    .child(self.render_connection_secret_field(
                                        self.i18n.t("ssh.form.passphrase"),
                                        String::new(),
                                        NewConnectionField::JumpPassphrase,
                                        cx,
                                    ))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::ManagedKey, |content| {
                                content
                                    .child(self.render_managed_key_select(
                                        self.i18n.t("ssh.form.managed_key"),
                                        &jump_form.managed_key_id,
                                        true,
                                        cx,
                                    ))
                                    .child(self.render_connection_secret_field(
                                        self.i18n.t("ssh.form.passphrase"),
                                        self.i18n.t("ssh.form.passphrase_placeholder"),
                                        NewConnectionField::JumpPassphrase,
                                        cx,
                                    ))
                                    .child(self.render_connection_hint(
                                        self.i18n.t("ssh.form.managed_key_hint"),
                                    ))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::Certificate, |content| {
                                content
                                    .child(self.render_connection_field_with_browse(
                                        self.i18n.t("ssh.form.private_key"),
                                        &jump_form.key_path,
                                        self.i18n.t("ssh.form.proxy_jump_key_path_placeholder"),
                                        NewConnectionField::JumpKeyPath,
                                        cx,
                                    ))
                                    .child(self.render_connection_field_with_browse(
                                        self.i18n.t("ssh.form.certificate"),
                                        &jump_form.cert_path,
                                        "~/.ssh/id_ed25519-cert.pub".to_string(),
                                        NewConnectionField::JumpCertPath,
                                        cx,
                                    ))
                                    .child(self.render_connection_secret_field(
                                        self.i18n.t("ssh.form.passphrase"),
                                        String::new(),
                                        NewConnectionField::JumpPassphrase,
                                        cx,
                                    ))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::Password, |content| {
                                content
                                    .child(self.render_connection_secret_field(
                                        self.i18n.t("ssh.form.password"),
                                        String::new(),
                                        NewConnectionField::JumpPassword,
                                        cx,
                                    ))
                                    .child(self.render_connection_checkbox(
                                        "jump-empty-password",
                                        self.i18n.t("ssh.form.use_empty_password"),
                                        jump_form.empty_password,
                                        |form| {
                                            if let Some(jump) = form.jump_server_form.as_mut() {
                                                jump.empty_password = !jump.empty_password;
                                                if jump.empty_password {
                                                    jump.password.zeroize();
                                                }
                                            }
                                            form.field_focused = false;
                                        },
                                        cx,
                                    ))
                            })
                            .when(jump_form.auth_tab == SshAuthTab::Agent, |content| {
                                content
                                    .child(self.render_connection_hint(
                                        self.i18n.t("ssh.form.proxy_jump_agent_desc"),
                                    ))
                                    .child(self.render_connection_field(
                                        self.i18n.t("ssh.form.agent_endpoint"),
                                        &jump_form.identity_agent,
                                        self.i18n.t("ssh.form.agent_endpoint_placeholder"),
                                        NewConnectionField::JumpIdentityAgent,
                                        false,
                                        cx,
                                    ))
                                    .child(self.render_connection_hint(
                                        self.i18n.t("ssh.form.agent_endpoint_hint"),
                                    ))
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(12.0))
                                    .pt(px(12.0))
                                    .border_t_1()
                                    .border_color(rgb(theme.border))
                                    .child(self.render_connection_checkbox(
                                        "jump-kerberos",
                                        self.i18n.t("ssh.form.kerberos_preferred"),
                                        jump_form.gssapi_enabled,
                                        |form| {
                                            if let Some(jump) = form.jump_server_form.as_mut() {
                                                jump.gssapi_enabled = !jump.gssapi_enabled;
                                            }
                                        },
                                        cx,
                                    ))
                                    .when(jump_form.gssapi_enabled, |content| {
                                        content
                                            .child(self.render_connection_hint(
                                                self.i18n.t("ssh.form.gssapi_desc"),
                                            ))
                                            .child(self.render_connection_field(
                                                self.i18n.t("ssh.form.gssapi_server_identity"),
                                                &jump_form.gssapi_server_identity,
                                                self.i18n.t(
                                                    "ssh.form.gssapi_server_identity_placeholder",
                                                ),
                                                NewConnectionField::JumpGssapiServerIdentity,
                                                false,
                                                cx,
                                            ))
                                            .child(self.render_kerberos_credentials_status(
                                                gssapi_credentials_available,
                                            ))
                                            .child(self.render_connection_checkbox_with_warning(
                                                "jump-kerberos-delegation-help",
                                                "jump-kerberos-delegation-tooltip",
                                                "ssh.form.gssapi_delegate_credentials",
                                                "ssh.form.gssapi_delegation_warning",
                                                jump_form.gssapi_delegate_credentials,
                                                |form| {
                                                    if let Some(jump) =
                                                        form.jump_server_form.as_mut()
                                                    {
                                                        jump.gssapi_delegate_credentials =
                                                            !jump.gssapi_delegate_credentials;
                                                    }
                                                },
                                                cx,
                                            ))
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(20.0))
                                    .pt(px(12.0))
                                    .border_t_1()
                                    .border_color(rgb(theme.border))
                                    .child(self.render_connection_checkbox(
                                        "jump-agent-forwarding",
                                        self.i18n.t("ssh.form.agent_forwarding"),
                                        jump_form.agent_forwarding,
                                        |form| {
                                            if let Some(jump_form) = form.jump_server_form.as_mut()
                                            {
                                                jump_form.agent_forwarding =
                                                    !jump_form.agent_forwarding;
                                            }
                                        },
                                        cx,
                                    ))
                                    .child(self.render_connection_checkbox(
                                        "jump-legacy-ssh",
                                        self.i18n.t("ssh.form.legacy_ssh_compatibility"),
                                        jump_form.legacy_ssh_compatibility,
                                        |form| {
                                            if let Some(jump_form) = form.jump_server_form.as_mut()
                                            {
                                                jump_form.legacy_ssh_compatibility =
                                                    !jump_form.legacy_ssh_compatibility;
                                            }
                                        },
                                        cx,
                                    )),
                            ),
                    )
                    .child(
                        modal_footer(&self.tokens)
                            .flex_none()
                            .bg(rgb(theme.bg_panel))
                            .px(px(22.0))
                            .py(px(13.0))
                            .justify_between()
                            .gap(px(12.0))
                            .child(div().flex_1().min_w_0().when_some(
                                footer_error,
                                |left, error| {
                                    left.flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(theme.error))
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::AlertCircle,
                                            13.0,
                                            rgb(theme.error),
                                        ))
                                        .child(div().min_w_0().truncate().child(error))
                                },
                            ))
                            .child(
                                div()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(self.render_jump_cancel_button(cx))
                                    .child(self.render_jump_add_button(
                                        add_disabled,
                                        is_editing,
                                        cx,
                                    )),
                            ),
                    ),
                form_visible,
            ))
            .when(!form_visible, |backdrop| {
                backdrop.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .on_scroll(|_event, _window, cx| cx.stop_propagation()),
                )
            })
            .into_any_element()
    }

    pub(super) fn begin_jump_server_form_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.begin_jump_server_form_exit_with_commit(false, cx)
    }

    fn begin_jump_server_form_exit_with_commit(
        &mut self,
        commit: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        let began_exit = self.flow.update(cx, |connection_flow, cx| {
            connection_flow.begin_jump_server_form_exit(commit, delay, cx)
        });
        if !began_exit {
            return false;
        }
        self.request(ConnectionsWindowRequest::ClearComposition, None, cx);
        cx.notify();
        true
    }

    pub(super) fn render_proxy_chain_section(
        &self,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (hop_count, expanded, summaries) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| {
                let (proxy_hops, expanded) = if secondary {
                    (
                        &form.standalone_sftp_secondary.proxy_hops,
                        form.standalone_sftp_secondary.proxy_chain_expanded,
                    )
                } else {
                    (&form.proxy_hops, form.proxy_chain_expanded)
                };
                let summaries = proxy_hops
                    .iter()
                    .map(|hop| ProxyHopSummarySnapshot {
                        saved_connection_name: self
                            .environment
                            .connections
                            .iter()
                            .find(|connection| connection.id == hop.saved_connection_id)
                            .map(|connection| connection.name.clone()),
                        host: hop.host.clone(),
                        port: hop.port.clone(),
                        username: hop.username.clone(),
                        auth_tab: hop.auth_tab,
                        gssapi_enabled: hop.gssapi_enabled,
                    })
                    .collect::<Vec<_>>();
                (summaries.len(), expanded, summaries)
            })
            .unwrap_or_default();

        let body = if hop_count == 0 {
            div()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgba((theme.border << 8) | 0x99))
                .bg(rgba((theme.bg_card << 8) | 0x66))
                .px(px(14.0))
                .py(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(theme.text_muted))
                .child(Self::render_lucide_icon(
                    LucideIcon::Network,
                    14.0,
                    rgb(theme.text_muted),
                ))
                .child(self.i18n.t("ssh.form.proxy_chain_empty"))
                .into_any_element()
        } else if !expanded {
            let preview = summaries
                .iter()
                .map(|hop| {
                    hop.saved_connection_name
                        .as_deref()
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or(hop.host.as_str())
                })
                .collect::<Vec<_>>()
                .join(" → ");
            div()
                .id(if secondary {
                    "secondary-proxy-chain-collapsed"
                } else {
                    "proxy-chain-collapsed"
                })
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.bg_card))
                .px(px(12.0))
                .py(px(10.0))
                .cursor_pointer()
                .hover(|el| el.bg(rgb(theme.bg_hover)))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text))
                        .child(preview),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(
                            self.i18n
                                .t("ssh.form.proxy_chain_count")
                                .replace("{{count}}", &hop_count.to_string()),
                        ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                if secondary {
                                    form.standalone_sftp_secondary.proxy_chain_expanded = true;
                                } else {
                                    form.proxy_chain_expanded = true;
                                }
                            }
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .into_any_element()
        } else {
            let mut list = div()
                .id(if secondary {
                    "new-connection-secondary-proxy-chain-scroll"
                } else {
                    "new-connection-proxy-chain-scroll"
                })
                .flex()
                .flex_col()
                .max_h(px(PROXY_CHAIN_LIST_MAX_HEIGHT))
                .overflow_y_scroll()
                .track_scroll(&self.selectable_text_scroll_handle(if secondary {
                    "new-connection-secondary-proxy-chain-scroll"
                } else {
                    "new-connection-proxy-chain-scroll"
                }));
            for (index, hop) in summaries.iter().enumerate() {
                list = list.child(self.render_proxy_hop_summary(
                    index,
                    index + 1 == hop_count,
                    hop,
                    secondary,
                    cx,
                ));
            }
            list.into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .border_t_1()
            .border_color(rgb(theme.border))
            .pt(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.i18n.t("ssh.form.proxy_chain_title")),
                            )
                            .when(hop_count > 0, |row| {
                                row.child(
                                    div()
                                        .px(px(6.0))
                                        .py(px(1.0))
                                        .rounded(px(self.tokens.radii.xs))
                                        .bg(rgba((theme.accent << 8) | 0x1e))
                                        .text_size(px(11.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgb(theme.accent))
                                        .child(hop_count.to_string()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .when(hop_count > 0, |row| {
                                row.child(self.render_proxy_chain_toggle(expanded, secondary, cx))
                            })
                            .child(self.render_add_jump_button(secondary, cx)),
                    ),
            )
            .child(body)
            .into_any_element()
    }

    fn render_proxy_chain_toggle(
        &self,
        expanded: bool,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Proxy-chain expand/collapse is an icon-only toolbar action in the
        // Tauri form. Use the shared primitive so hover and future focus state
        // stay aligned with other new-connection toolbar controls.
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            self.render_animated_chevron(
                (
                    if secondary {
                        "secondary-proxy-chain-chevron"
                    } else {
                        "proxy-chain-chevron"
                    },
                    expanded as usize,
                ),
                expanded,
                16.0,
                rgb(self.tokens.ui.text),
            ),
            IconButtonOptions {
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(
                    self.tokens.metrics.ui_button_sm_height,
                    ButtonRadius::Md,
                )
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        if secondary {
                            let route = &mut form.standalone_sftp_secondary;
                            route.proxy_chain_expanded = !route.proxy_chain_expanded;
                        } else {
                            form.proxy_chain_expanded = !form.proxy_chain_expanded;
                        }
                        form.field_focused = false;
                    }
                });
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_add_jump_button(&self, secondary: bool, cx: &mut Context<Self>) -> AnyElement {
        // The outer "add jump" command is the same small outline action
        // pattern used by settings toolbars, so keep its chrome shared.
        self.workspace_toolbar_action_button(
            self.i18n.t("ssh.form.proxy_chain_add_jump"),
            Some(Self::render_lucide_icon(
                LucideIcon::Plus,
                16.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                background: Some(rgba(0x00000000)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                this.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.jump_server_form = Some(NewConnectionProxyHop::new());
                        form.jump_server_edit_index = None;
                        form.error = None;
                        form.jump_server_target = if secondary {
                            ConnectionRouteTarget::StandaloneSftpSecondary
                        } else {
                            ConnectionRouteTarget::Primary
                        };
                        form.field_focused = true;
                        form.focused_field = NewConnectionField::JumpHost;
                        form.selected_field = None;
                    }
                    state.jump_server_presence.reopen();
                });
                this.close_new_connection_select(cx);
                this.show_active_input_caret(cx);
                this.request(ConnectionsWindowRequest::FocusForm, Some(window), cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_jump_add_button(
        &self,
        disabled: bool,
        is_editing: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if is_editing {
            self.i18n.t("sessionManager.edit_properties.save")
        } else {
            self.i18n.t("ssh.form.proxy_jump_add")
        };
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Default,
                    size: ButtonSize::Default,
                    disabled,
                    ..ButtonOptions::default()
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                this.add_pending_jump_server(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn render_jump_cancel_button(&self, cx: &mut Context<Self>) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("ssh.form.cancel"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Secondary,
                    size: ButtonSize::Default,
                    ..ButtonOptions::default()
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.begin_jump_server_form_exit(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn render_proxy_hop_summary(
        &self,
        index: usize,
        is_last: bool,
        hop: &ProxyHopSummarySnapshot,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let hop_title = hop
            .saved_connection_name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| self.i18n.t("ssh.form.proxy_chain_jump_server"));
        let auth_label = match hop.auth_tab {
            SshAuthTab::DefaultKey => self.i18n.t("ssh.auth.default_key"),
            SshAuthTab::SshKey => self.i18n.t("ssh.auth.ssh_key"),
            SshAuthTab::ManagedKey => self.i18n.t("ssh.auth.managed_key"),
            SshAuthTab::Certificate => self.i18n.t("ssh.auth.certificate"),
            SshAuthTab::Password => self.i18n.t("ssh.auth.password"),
            SshAuthTab::Agent => self.i18n.t("ssh.auth.agent"),
            SshAuthTab::TwoFactor => self.i18n.t("ssh.auth.two_factor"),
        };
        let endpoint = if hop.username.trim().is_empty() {
            format!("{}:{}", hop.host.trim(), hop.port.trim())
        } else {
            format!(
                "{}@{}:{}",
                hop.username.trim(),
                hop.host.trim(),
                hop.port.trim()
            )
        };

        div()
            .flex()
            .items_stretch()
            .gap(px(10.0))
            .child(
                div()
                    .w(px(22.0))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt(px(10.0))
                    .child(
                        div()
                            .flex_none()
                            .size(px(22.0))
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(theme.border_strong))
                            .bg(rgb(theme.bg_panel))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text))
                            .child((index + 1).to_string()),
                    )
                    .when(!is_last, |rail| {
                        rail.child(
                            div()
                                .flex_1()
                                .w(px(1.5))
                                .min_h(px(12.0))
                                .mt(px(4.0))
                                .bg(rgb(theme.border)),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when(!is_last, |card| card.mb(px(8.0)))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_card))
                    .rounded(px(self.tokens.radii.md))
                    .px(px(12.0))
                    .py(px(9.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme.text_heading))
                                            .child(hop_title),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .px(px(6.0))
                                            .py(px(1.0))
                                            .rounded(px(self.tokens.radii.xs))
                                            .bg(rgb(theme.bg_panel))
                                            .border_1()
                                            .border_color(rgb(theme.border))
                                            .text_size(px(10.5))
                                            .text_color(rgb(theme.text_muted))
                                            .child(auth_label),
                                    )
                                    .when(hop.gssapi_enabled, |row| {
                                        row.child(
                                            div()
                                                .flex_none()
                                                .px(px(5.0))
                                                .py(px(1.0))
                                                .rounded(px(self.tokens.radii.xs))
                                                .bg(rgba((theme.accent << 8) | 0x1e))
                                                .text_size(px(10.0))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(theme.accent))
                                                .child("KRB"),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(2.0))
                                    .child(self.render_edit_jump_button(index, secondary, cx))
                                    .child(self.render_remove_jump_button(index, secondary, cx)),
                            ),
                    )
                    .child({
                        let hop_index = index.to_string();
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_text_scoped(
                                "proxy-hop-endpoint",
                                (&hop_index, &endpoint),
                                endpoint.clone(),
                                theme.text_muted,
                                cx,
                            ))
                    }),
            )
            .into_any_element()
    }

    fn render_remove_jump_button(
        &self,
        index: usize,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_icon_action_button(
            LucideIcon::Trash2,
            14.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(24.0, ButtonRadius::Sm)
            },
            move |this, _event, _window, cx| {
                this.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        let proxy_hops = if secondary {
                            &mut form.standalone_sftp_secondary.proxy_hops
                        } else {
                            &mut form.proxy_hops
                        };
                        if index < proxy_hops.len() {
                            proxy_hops.remove(index);
                        }
                    }
                });
                cx.stop_propagation();
                cx.notify();
            },
            cx,
        )
        .into_any_element()
    }

    fn render_edit_jump_button(
        &self,
        index: usize,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_icon_action_button(
            LucideIcon::Pencil,
            14.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(24.0, ButtonRadius::Sm)
            },
            move |this, _event, window, cx| {
                this.update_connection_form_state(cx, |state| {
                    let Some(form) = state.form.as_mut() else {
                        return;
                    };
                    let jump_server = {
                        let proxy_hops = if secondary {
                            &mut form.standalone_sftp_secondary.proxy_hops
                        } else {
                            &mut form.proxy_hops
                        };
                        if index >= proxy_hops.len() {
                            return;
                        }
                        proxy_hops.remove(index)
                    };
                    // Move instead of clone so temporary secret drafts have one owner.
                    form.jump_server_form = Some(jump_server);
                    form.jump_server_edit_index = Some(index);
                    form.error = None;
                    form.jump_server_target = if secondary {
                        ConnectionRouteTarget::StandaloneSftpSecondary
                    } else {
                        ConnectionRouteTarget::Primary
                    };
                    form.field_focused = true;
                    form.focused_field = NewConnectionField::JumpHost;
                    form.selected_field = None;
                    state.jump_server_presence.reopen();
                });
                this.close_new_connection_select(cx);
                this.show_active_input_caret(cx);
                this.request(ConnectionsWindowRequest::FocusForm, Some(window), cx);
                cx.stop_propagation();
                cx.notify();
            },
            cx,
        )
        .into_any_element()
    }

    pub(super) fn add_pending_jump_server(&mut self, cx: &mut Context<Self>) {
        let Some(jump_form) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|form| form.jump_server_form.as_ref())
        else {
            return;
        };
        if !jump_form.complete() {
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.error = Some(self.i18n.t("ssh.form.proxy_jump_required"));
                }
            });
            cx.notify();
            return;
        }
        self.begin_jump_server_form_exit_with_commit(true, cx);
    }
}
