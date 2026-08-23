use super::*;
use oxideterm_connections::ConnectionTerminalSessionLogPolicy;

struct JumpServerRenderSnapshot {
    saved_connection_id: String,
    host: String,
    port: String,
    username: String,
    auth_tab: SshAuthTab,
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
            saved_connection_id: hop.saved_connection_id.clone(),
            host: hop.host.clone(),
            port: hop.port.clone(),
            username: hop.username.clone(),
            auth_tab: hop.auth_tab,
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
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_new_connection_select_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // New-connection dropdowns mount and unmount with their logical state;
        // no render-only payload is retained for an exit transition.
        let select_id = self.connection_form_state(cx).open_select?;
        let anchor_id = Self::new_connection_select_anchor_id(select_id);
        let anchor = self
            .connection_flow
            .read(cx)
            .select_anchor_store()
            .get(anchor_id)?;
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
                for connection in self.connection_store.connection_infos() {
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
                for connection in self.connection_store.connection_infos() {
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
                for key in self.connection_store.managed_ssh_keys() {
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
                        &terminal_encoding_label(
                            self.settings_store.settings().terminal.terminal_encoding,
                        ),
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
                            self.settings_store.settings().terminal.backspace_sequence,
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
                        terminal_delete_sequence_label(
                            self.settings_store.settings().terminal.delete_sequence,
                        ),
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
                let terminal = &self.settings_store.settings().terminal;
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
                let terminal = &self.settings_store.settings().terminal;
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
                let inherited_mode = if self
                    .settings_store
                    .settings()
                    .terminal
                    .session_log
                    .automatic
                {
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
            NewConnectionSelect::LocalShell => {
                let selected_shell_id = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| form.local_shell_id.as_deref());
                let resolved_shell = self.resolved_local_shell(selected_shell_id);
                let resolved_shell_id = resolved_shell.as_ref().map(|shell| shell.id.as_str());
                let default_shell_id = self
                    .settings_store
                    .settings()
                    .local_terminal
                    .default_shell_id
                    .as_deref();
                let default_label = self.i18n.t("settings_view.local_terminal.default");

                // The modal uses the same select surface as the other connection
                // fields, while the selected shell still controls only this launch.
                for shell in
                    self.effective_local_shells_for_settings(self.settings_store.settings())
                {
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
        let popup = popup.into_any_element();

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

    pub(in crate::workspace) fn render_add_jump_server_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(jump_form) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|form| form.jump_server_form.as_ref())
            .map(JumpServerRenderSnapshot::from_hop)
        else {
            return div().into_any_element();
        };
        let gssapi_credentials_available = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|form| form.gssapi_credentials_available);
        if jump_form.gssapi_enabled && gssapi_credentials_available.is_none() {
            self.ensure_kerberos_credentials_availability(cx);
        }
        let add_disabled = !jump_form.complete
            || (jump_form.auth_tab == SshAuthTab::ManagedKey
                && jump_form.managed_key_id.trim().is_empty());
        let modal_max_height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        let form_visible = self.connection_form_state(cx).jump_server_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri jump-server form is a Dialog child of the new
                    // connection flow; overlay clicks cancel just this subform.
                    this.begin_jump_server_form_exit(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "jump-server-form-enter",
                modal_container(&self.tokens)
                    .w(px(TAURI_JUMP_MODAL_WIDTH))
                    .max_h(px(modal_max_height))
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(modal_header(
                        &self.tokens,
                        self.i18n.t("ssh.form.proxy_jump_title"),
                        String::new(),
                    ))
                    .child(
                        modal_body(&self.tokens)
                            .id("new-connection-jump-server-body-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .selectable_overflow_y_scroll(&self.selectable_text_scroll_handle(
                                "new-connection-jump-server-body-scroll",
                            ))
                            .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                                // Keep native anchored selects aligned with Tauri/Radix:
                                // scrolling the modal body closes popup content tied to a moved trigger.
                                if this.connection_form_state(cx).open_select.is_some() {
                                    this.close_new_connection_select(cx);
                                    this.clear_new_connection_select_anchor(cx);
                                    cx.notify();
                                }
                            }))
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(self.render_jump_saved_connection_select(
                                &jump_form.saved_connection_id,
                                cx,
                            ))
                            .child(self.render_connection_hint(
                                if self.connection_store.connection_infos().is_empty() {
                                    self.i18n.t("ssh.form.proxy_jump_saved_connection_empty")
                                } else {
                                    self.i18n.t("ssh.form.proxy_jump_saved_connection_hint")
                                },
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_4()
                                    .child(div().flex_1().child(self.render_connection_field(
                                        self.i18n.t("ssh.form.proxy_jump_host"),
                                        &jump_form.host,
                                        self.i18n.t("ssh.form.proxy_jump_host_placeholder"),
                                        NewConnectionField::JumpHost,
                                        false,
                                        cx,
                                    )))
                                    .child(div().w(px(self.tokens.metrics.form_port_width)).child(
                                        self.render_connection_field(
                                            self.i18n.t("ssh.form.proxy_jump_port"),
                                            &jump_form.port,
                                            "22".to_string(),
                                            NewConnectionField::JumpPort,
                                            false,
                                            cx,
                                        ),
                                    )),
                            )
                            .child(self.render_connection_field(
                                self.i18n.t("ssh.form.proxy_jump_username"),
                                &jump_form.username,
                                self.i18n.t("ssh.form.proxy_jump_username_placeholder"),
                                NewConnectionField::JumpUsername,
                                false,
                                cx,
                            ))
                            .child(self.render_connection_hint(
                                self.i18n.t("ssh.form.proxy_jump_kbi_hint"),
                            ))
                            .child(self.render_connection_checkbox(
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
                                        self.i18n.t("ssh.form.gssapi_server_identity_placeholder"),
                                        NewConnectionField::JumpGssapiServerIdentity,
                                        false,
                                        cx,
                                    ))
                                    .child(self.render_connection_hint(
                                        self.i18n.t("ssh.form.gssapi_server_identity_hint"),
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
                                            if let Some(jump) = form.jump_server_form.as_mut() {
                                                jump.gssapi_delegate_credentials =
                                                    !jump.gssapi_delegate_credentials;
                                            }
                                        },
                                        cx,
                                    ))
                            })
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("ssh.form.fallback_authentication")),
                            )
                            .child(self.render_auth_selector(
                                jump_form.auth_tab,
                                AuthSelectorContext::Jump,
                                true,
                                cx,
                            ))
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
                                content.child(self.render_connection_secret_field(
                                    self.i18n.t("ssh.form.password"),
                                    String::new(),
                                    NewConnectionField::JumpPassword,
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
                            .child(self.render_connection_checkbox(
                                self.i18n.t("ssh.form.agent_forwarding"),
                                jump_form.agent_forwarding,
                                |form| {
                                    if let Some(jump_form) = form.jump_server_form.as_mut() {
                                        jump_form.agent_forwarding = !jump_form.agent_forwarding;
                                    }
                                },
                                cx,
                            ))
                            .child(self.render_connection_checkbox(
                                self.i18n.t("ssh.form.legacy_ssh_compatibility"),
                                jump_form.legacy_ssh_compatibility,
                                |form| {
                                    if let Some(jump_form) = form.jump_server_form.as_mut() {
                                        jump_form.legacy_ssh_compatibility =
                                            !jump_form.legacy_ssh_compatibility;
                                    }
                                },
                                cx,
                            )),
                    )
                    .child(
                        modal_footer(&self.tokens)
                            .child(self.render_jump_cancel_button(cx))
                            .child(self.render_jump_add_button(add_disabled, cx)),
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
                        .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation()),
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
        let began_exit = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.begin_jump_server_form_exit(commit, delay, cx)
        });
        if !began_exit {
            return false;
        }
        self.ime_marked_text = None;
        cx.notify();
        true
    }

    pub(super) fn render_proxy_chain_section(
        &self,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (hop_count, expanded) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| {
                if secondary {
                    (
                        form.standalone_sftp_secondary.proxy_hops.len(),
                        form.standalone_sftp_secondary.proxy_chain_expanded,
                    )
                } else {
                    (form.proxy_hops.len(), form.proxy_chain_expanded)
                }
            })
            .unwrap_or_default();
        let mut list = div()
            .id(if secondary {
                "new-connection-secondary-proxy-chain-scroll"
            } else {
                "new-connection-proxy-chain-scroll"
            })
            .flex()
            .flex_col()
            .gap_2()
            .max_h(px(TAURI_PROXY_CHAIN_MAX_HEIGHT))
            .selectable_overflow_y_scroll(&self.selectable_text_scroll_handle(if secondary {
                "new-connection-secondary-proxy-chain-scroll"
            } else {
                "new-connection-proxy-chain-scroll"
            }));
        if hop_count == 0 {
            list = list.child(
                div()
                    .py(px(24.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ssh.form.proxy_chain_empty")),
            );
        } else {
            let summaries = self
                .connection_form_state(cx)
                .form
                .as_ref()
                .map(|form| {
                    let proxy_hops = if secondary {
                        &form.standalone_sftp_secondary.proxy_hops
                    } else {
                        &form.proxy_hops
                    };
                    proxy_hops
                        .iter()
                        .map(|hop| ProxyHopSummarySnapshot {
                            saved_connection_name: self
                                .connection_store
                                .get(&hop.saved_connection_id)
                                .map(|connection| connection.name.clone()),
                            host: hop.host.clone(),
                            port: hop.port.clone(),
                            username: hop.username.clone(),
                            auth_tab: hop.auth_tab,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for (index, hop) in summaries.iter().enumerate() {
                list = list.child(self.render_proxy_hop_summary(index, hop, secondary, cx));
            }
        }

        div()
            .flex()
            .flex_col()
            .rounded(px(self.tokens.radii.lg))
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(TAURI_PROXY_CHAIN_SECTION_PADDING))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(TAURI_PROXY_CHAIN_HEADER_MARGIN))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.i18n.t("ssh.form.proxy_chain_title")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(hop_count > 0, |row| {
                                row.child(self.render_proxy_chain_toggle(expanded, secondary, cx))
                            })
                            .child(self.render_add_jump_button(secondary, cx)),
                    ),
            )
            .child(if expanded {
                list.into_any_element()
            } else {
                div()
                    .py(px(24.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(if hop_count == 0 {
                        self.i18n.t("ssh.form.proxy_chain_empty")
                    } else {
                        self.i18n
                            .t("ssh.form.proxy_chain_count")
                            .replace("{{count}}", &hop_count.to_string())
                    })
                    .into_any_element()
            })
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
                window.focus(&this.focus_handle, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_jump_add_button(&self, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        // Jump-server editor actions mirror Tauri Button chrome. Keep add and
        // cancel on the shared toolbar primitive instead of local button_with
        // calls so disabled/focus handling can converge later.
        self.workspace_toolbar_action_button(
            self.i18n.t("ssh.form.proxy_jump_add"),
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
        hop: &ProxyHopSummarySnapshot,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hop_title = hop
            .saved_connection_name
            .clone()
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
        let auth_icon = if matches!(
            hop.auth_tab,
            SshAuthTab::SshKey | SshAuthTab::DefaultKey | SshAuthTab::ManagedKey
        ) {
            LucideIcon::Key
        } else {
            LucideIcon::Lock
        };
        div()
            .relative()
            .child(
                div()
                    .absolute()
                    .left(px(TAURI_PROXY_CHAIN_NODE_SIZE / 2.0))
                    .top_0()
                    .bottom_0()
                    .w(px(TAURI_PROXY_CHAIN_CONNECTOR_THICKNESS))
                    .when(index > 0, |line| {
                        line.child(
                            div()
                                .absolute()
                                .top(px(TAURI_PROXY_CHAIN_NODE_SIZE / 2.0))
                                .w(px(TAURI_PROXY_CHAIN_LINE_WIDTH))
                                .h(px(TAURI_PROXY_CHAIN_CONNECTOR_THICKNESS))
                                .bg(rgb(self.tokens.ui.text_muted)),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .top(px(0.0))
                            .size(px(TAURI_PROXY_CHAIN_NODE_SIZE))
                            .rounded_full()
                            .border_2()
                            .border_color(rgb(self.tokens.ui.border_strong))
                            .bg(rgb(self.tokens.ui.bg))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Self::render_lucide_icon(
                                auth_icon,
                                16.0,
                                rgb(self.tokens.ui.text_muted),
                            )),
                    ),
            )
            .child(
                div().flex().items_start().gap(px(24.0)).pl(px(48.0)).child(
                    div()
                        .flex_1()
                        .border_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .rounded(px(self.tokens.radii.lg))
                        .p(px(TAURI_PROXY_CHAIN_CARD_PADDING))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(format!("{}. {}", index + 1, hop_title)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(self.render_edit_jump_button(index, secondary, cx))
                                        .child(
                                            self.render_remove_jump_button(index, secondary, cx),
                                        ),
                                ),
                        )
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_host"),
                            hop.host.clone(),
                            cx,
                        ))
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_port"),
                            hop.port.clone(),
                            cx,
                        ))
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_username"),
                            hop.username.clone(),
                            cx,
                        ))
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_auth"),
                            auth_label,
                            cx,
                        )),
                ),
            )
            .into_any_element()
    }

    fn render_proxy_hop_line(
        &self,
        label: String,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .child(div().text_color(rgb(self.tokens.ui.text_muted)).child(
                self.render_selectable_text_scoped(
                    "proxy-hop-label",
                    (&label, &value),
                    format!("{label}:"),
                    self.tokens.ui.text_muted,
                    cx,
                ),
            ))
            .child(div().font_weight(gpui::FontWeight::MEDIUM).child(
                self.render_selectable_text_scoped(
                    "proxy-hop-value",
                    (&label, &value),
                    value.clone(),
                    self.tokens.ui.text,
                    cx,
                ),
            ))
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
                window.focus(&this.focus_handle, cx);
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
