// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::workspace::{
    WorkspaceNotificationKind, WorkspaceNotificationScope, WorkspaceNotificationSeverity,
};
use gpui::App;
use oxideterm_connections::{ConnectionStore, SavedConnection};
use oxideterm_gpui_terminal::TerminalNoticeVariant;

impl WorkspaceApp {
    pub(super) fn report_saved_next_hop_error(&mut self, i18n_key: &str, cx: &mut Context<Self>) {
        self.report_saved_next_hop_message(self.i18n.t(i18n_key), cx);
    }

    pub(super) fn report_saved_next_hop_message(
        &mut self,
        message: String,
        cx: &mut Context<Self>,
    ) {
        let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.set_form_feedback(Some(false), Some(message.clone()), cx)
        });
        if !reported_to_form {
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn open_save_runtime_node_form(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self.ssh_nodes.get(&node_id).map(|node| node.title.clone()) else {
            let message = self.i18n.t("ssh.form.runtime_node_missing");
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
            return;
        };
        let Some(runtime_snapshot) = self.node_router.node_runtime_snapshot(&node_id) else {
            let message = self.i18n.t("ssh.form.runtime_node_missing");
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
            return;
        };
        let parent_id = runtime_snapshot.parent_id.clone();
        let proxy_hops = match parent_id
            .as_ref()
            .map(|parent_id| self.runtime_proxy_hops_for_parent_path(parent_id))
            .transpose()
        {
            Ok(hops) => hops.unwrap_or_default(),
            Err(error) => {
                let message = error.to_string();
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(message), cx);
                });
                return;
            }
        };

        self.prepare_modal_interaction_boundary(cx);
        let mut form = form_from_runtime_config(
            runtime_snapshot.config,
            Some(&title),
            self.i18n.t("ssh.form.ungrouped"),
        );
        form.proxy_hops = proxy_hops;
        form.proxy_chain_expanded = !form.proxy_hops.is_empty();
        form.agent_available = detect_ssh_agent_available(&form.identity_agent);
        form.save_connection = true;
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(super) fn runtime_proxy_hops_for_parent_path(
        &self,
        parent_id: &NodeId,
    ) -> anyhow::Result<Vec<NewConnectionProxyHop>> {
        let mut configs = Vec::new();
        let mut cursor = Some(parent_id.clone());
        while let Some(node_id) = cursor {
            let snapshot = self
                .node_router
                .node_runtime_snapshot(&node_id)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "{}: {}",
                        self.i18n.t("ssh.form.runtime_node_missing"),
                        node_id.0
                    )
                })?;
            configs.push(snapshot.config);
            cursor = snapshot.parent_id;
        }
        configs.reverse();

        Ok(configs
            .into_iter()
            .flat_map(|config| {
                let embedded_hops = config.proxy_chain.unwrap_or_default().into_iter();
                embedded_hops
                    .chain(std::iter::once(ProxyHopConfig {
                        host: config.host,
                        port: config.port,
                        username: config.username,
                        auth: config.auth,
                        agent_forwarding: config.agent_forwarding,
                        identity_agent: config.identity_agent,
                        agent_forwarding_socket: config.agent_forwarding_socket,
                        legacy_ssh_compatibility: config.legacy_ssh_compatibility,
                        strict_host_key_checking: true,
                        trust_host_key: None,
                        expected_host_key_fingerprint: None,
                    }))
                    .map(proxy_hop_form_from_runtime_config)
            })
            .collect())
    }

    pub(in crate::workspace) fn close_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if !self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.begin_connection_form_exit(delay, cx)
        }) {
            return;
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn submit_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.submit_new_connection_form_with_action(
            NewConnectionSubmitAction::SaveAndConnect,
            window,
            cx,
        );
    }

    pub(in crate::workspace) fn submit_new_connection_form_with_action(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (transport, drill_down_parent_id, mode) = {
            let state = self.connection_form_state(cx);
            (
                state.form.as_ref().map(|form| form.transport),
                state.drill_down_parent_node_id.clone(),
                state.mode(),
            )
        };
        if transport == Some(NewConnectionTransport::Serial)
            && drill_down_parent_id.is_none()
            && mode == NewConnectionFormMode::NewConnection
        {
            self.submit_serial_connection_form(action, window, cx);
            return;
        }
        if transport == Some(NewConnectionTransport::Telnet)
            && drill_down_parent_id.is_none()
            && mode == NewConnectionFormMode::NewConnection
        {
            self.submit_telnet_connection_form(action, window, cx);
            return;
        }
        if transport
            .and_then(remote_desktop_protocol_for_transport)
            .is_some()
            && drill_down_parent_id.is_none()
            && mode == NewConnectionFormMode::NewConnection
        {
            self.submit_remote_desktop_connection_form(action, window, cx);
            return;
        }
        if transport == Some(NewConnectionTransport::WslGraphics)
            && drill_down_parent_id.is_none()
            && mode == NewConnectionFormMode::NewConnection
        {
            self.close_new_connection_form(window, cx);
            self.open_graphics_tab(window, cx);
            return;
        }
        if let Some(parent_id) = drill_down_parent_id {
            match action {
                NewConnectionSubmitAction::Save => {
                    self.save_new_connection_without_connecting(Some(&parent_id), window, cx);
                    return;
                }
                NewConnectionSubmitAction::SaveAndConnect => {
                    let Some(handoff) = self.save_current_connection_form(Some(&parent_id), cx)
                    else {
                        return;
                    };
                    self.start_saved_form_connection_flow(handoff, Some(parent_id), window, cx);
                    return;
                }
                NewConnectionSubmitAction::Connect => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.save_connection = false;
                        }
                    });
                }
            }
            let terminal_options = self
                .connection_form_state(cx)
                .form
                .as_ref()
                .map(|form| form.terminal)
                .unwrap_or_default();
            self.start_new_connection_flow(
                SshConnectionIntent::DrillDown {
                    parent_id,
                    saved_connection_id: None,
                    terminal_options,
                },
                window,
                cx,
            );
            return;
        }
        match mode {
            NewConnectionFormMode::SavedConnectionPrompt => {
                self.submit_saved_connection_prompt(window, cx);
            }
            NewConnectionFormMode::EditProperties => {
                self.save_editing_connection(window, cx);
            }
            NewConnectionFormMode::DuplicateTemplate => {
                self.save_duplicate_connection_template(window, cx);
            }
            NewConnectionFormMode::NewConnection => match action {
                NewConnectionSubmitAction::Connect => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.save_connection = false;
                        }
                    });
                    let terminal_options = self
                        .connection_form_state(cx)
                        .form
                        .as_ref()
                        .map(|form| form.terminal)
                        .unwrap_or_default();
                    self.start_new_connection_flow(
                        SshConnectionIntent::Connect(terminal_options),
                        window,
                        cx,
                    );
                }
                NewConnectionSubmitAction::Save => {
                    self.save_new_connection_without_connecting(None, window, cx);
                }
                NewConnectionSubmitAction::SaveAndConnect => {
                    let Some(handoff) = self.save_current_connection_form(None, cx) else {
                        return;
                    };
                    self.start_saved_form_connection_flow(handoff, None, window, cx);
                }
            },
        }
    }

    pub(super) fn save_new_connection_without_connecting(
        &mut self,
        drill_down_parent_id: Option<&NodeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .save_current_connection_form(drill_down_parent_id, cx)
            .is_some()
        {
            self.close_new_connection_form(window, cx);
        }
    }

    pub(super) fn save_current_connection_form(
        &mut self,
        drill_down_parent_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) -> Option<SavedConnectionRuntimeHandoff> {
        self.ensure_new_connection_save_name_is_unique(drill_down_parent_id, cx);
        let request = match self.save_request_for_current_form(drill_down_parent_id, cx) {
            Some(Ok(request)) => request,
            Some(Err(error)) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(error.to_string());
                    }
                });
                cx.notify();
                return None;
            }
            None => return None,
        };
        let auth_override = self.with_connection_form_mut(cx, |_this, form, _cx| {
            let form = form?;
            (form.auth_tab == SshAuthTab::Password && !form.save_password)
                .then(|| AuthMethod::password_secret(take_zeroizing_secret(&mut form.password)))
        });

        // The Save and Save & Connect buttons mean "persist this draft now",
        // so duplicate-name and keychain failures should block connection start.
        match self.connection_store.upsert_with_runtime_secrets(request) {
            Ok((connection, secrets)) => {
                self.queue_cloud_sync_dirty_refresh(cx);
                Some(SavedConnectionRuntimeHandoff {
                    connection_id: connection.id,
                    secrets,
                    auth_override,
                })
            }
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(format!(
                            "{}: {error}",
                            self.i18n.t("modals.new_connection.save_failed")
                        ));
                    }
                });
                cx.notify();
                None
            }
        }
    }

    fn start_saved_form_connection_flow(
        &mut self,
        handoff: SavedConnectionRuntimeHandoff,
        drill_down_parent_id: Option<NodeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(connection) = self.connection_store.get(&handoff.connection_id).cloned() else {
            self.report_saved_next_hop_error("modals.new_connection.save_failed", cx);
            return;
        };
        let Some(mut config) = ssh_config_from_saved_connection_with_runtime_secrets(
            &self.connection_store,
            self.settings_store.settings(),
            &connection,
            handoff.secrets,
            handoff.auth_override,
        ) else {
            self.report_saved_next_hop_error("modals.new_connection.save_failed", cx);
            return;
        };
        let intent = if let Some(parent_id) = drill_down_parent_id {
            let prefix_count = match self.runtime_proxy_hops_for_parent_path(&parent_id) {
                Ok(hops) => hops.len(),
                Err(error) => {
                    self.report_saved_next_hop_message(error.to_string(), cx);
                    return;
                }
            };
            if prefix_count > 0 {
                let Some(proxy_chain) = config.proxy_chain.as_mut() else {
                    self.report_saved_next_hop_error("modals.new_connection.save_failed", cx);
                    return;
                };
                if proxy_chain.len() < prefix_count {
                    self.report_saved_next_hop_error("modals.new_connection.save_failed", cx);
                    return;
                }
                // Existing parent nodes already own the persisted prefix path.
                proxy_chain.drain(..prefix_count);
                if proxy_chain.is_empty() {
                    config.proxy_chain = None;
                }
            }
            SshConnectionIntent::DrillDown {
                parent_id,
                saved_connection_id: Some(connection.id.clone()),
                terminal_options: connection.options.terminal,
            }
        } else {
            SshConnectionIntent::ConnectSaved(connection.id.clone())
        };
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.save_connection = false;
            }
        });
        self.start_new_connection_config_flow(config, connection.name, intent, window, cx);
    }

    pub(super) fn ensure_new_connection_save_name_is_unique(
        &mut self,
        _drill_down_parent_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) {
        let occupied_names: Vec<String> = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.name.clone())
            .collect();
        self.update_connection_form_state(cx, |state| {
            let Some(form) = state.form.as_mut() else {
                return;
            };
            let fallback_name = if form.name.trim().is_empty() {
                let host = form.host.trim();
                let username = form.username.trim();
                if host.is_empty() || username.is_empty() {
                    return;
                }
                format!("{username}@{host}")
            } else {
                form.name.trim().to_string()
            };
            let name_exists = occupied_names
                .iter()
                .any(|name| name.trim().eq_ignore_ascii_case(&fallback_name));
            let next_name = if name_exists {
                // New/save-as flows create a fresh connection id, so avoid storing a
                // second indistinguishable row when the draft name already exists.
                duplicate_connection_template_name(
                    &fallback_name,
                    occupied_names.iter().map(String::as_str),
                )
            } else {
                fallback_name
            };
            form.name = next_name;
        });
    }

    pub(super) fn save_request_for_current_form(
        &mut self,
        drill_down_parent_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) -> Option<anyhow::Result<SaveConnectionRequest>> {
        let mut runtime_proxy_hops = match drill_down_parent_id {
            Some(parent_id) => match self.runtime_proxy_hops_for_parent_path(parent_id) {
                Ok(hops) => hops,
                Err(error) => return Some(Err(error)),
            },
            None => Vec::new(),
        };
        self.with_connection_form_mut(cx, |_this, form, _cx| {
            let form = form?;
            Some(save_request_from_form_with_proxy_hop_prefix(
                form,
                &mut runtime_proxy_hops,
                None,
            ))
        })
    }

    pub(super) fn submit_serial_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((config, mut save_request)) =
            self.with_connection_form_mut(cx, |this, form, cx| {
                let form = form?;
                let port_path = form.serial_port_path.trim().to_string();
                let baud_rate = form.serial_baud_rate.trim().parse::<u32>().ok();
                if port_path.is_empty() {
                    form.error = Some(this.i18n.t("modals.new_connection.serial_port_required"));
                    cx.notify();
                    return None;
                }
                let Some(baud_rate) = baud_rate.filter(|baud| *baud > 0) else {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.serial_invalid_baud_rate"),
                    );
                    cx.notify();
                    return None;
                };
                let config = SerialSessionConfig {
                    port_path: port_path.clone(),
                    baud_rate,
                    data_bits: form.serial_data_bits,
                    stop_bits: form.serial_stop_bits,
                    parity: form.serial_parity,
                    flow_control: form.serial_flow_control,
                };
                let should_save_profile = action != NewConnectionSubmitAction::Connect;
                let save_request = should_save_profile.then(|| SaveSerialProfileRequest {
                    id: None,
                    name: serial_profile_name_or_port(&form.serial_profile_name, &port_path),
                    group: serial_profile_group_from_form(&form.group, &this.i18n),
                    icon: asset_icon_from_form(&form.icon),
                    color: asset_color_from_form(&form.color),
                    icon_background_color: asset_color_from_form(&form.icon_background_color),
                    port_path,
                    baud_rate: Some(baud_rate),
                    data_bits: Some(form.serial_data_bits),
                    stop_bits: Some(form.serial_stop_bits),
                    parity: Some(serial_profile_parity_from_terminal(form.serial_parity)),
                    flow_control: Some(serial_profile_flow_from_terminal(form.serial_flow_control)),
                    connect_on_open: None,
                });
                form.pending = true;
                form.error = None;
                Some((config, save_request))
            })
        else {
            return;
        };

        if action == NewConnectionSubmitAction::Save {
            let request =
                save_request.expect("serial save action must build a serial profile request");
            match self.connection_store.upsert_serial_profile(request) {
                Ok(_) => {
                    self.queue_cloud_sync_dirty_refresh(cx);
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.serial_save_failed")
                            ));
                        }
                    });
                }
            }
            cx.notify();
            return;
        }

        if action == NewConnectionSubmitAction::SaveAndConnect {
            let request = save_request
                .take()
                .expect("serial save-and-open action must build a serial profile request");
            match self.connection_store.upsert_serial_profile(request) {
                Ok(_) => self.queue_cloud_sync_dirty_refresh(cx),
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.serial_save_failed")
                            ));
                        }
                    });
                    cx.notify();
                    return;
                }
            }
        }

        match self.create_serial_terminal_tab(config, window, cx) {
            Ok(_) => {
                if let Some(request) = save_request {
                    match self.connection_store.upsert_serial_profile(request) {
                        Ok(_) => self.queue_cloud_sync_dirty_refresh(cx),
                        Err(error) => {
                            let message = format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.serial_save_failed")
                            );
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(message), cx);
                            });
                        }
                    }
                }
                self.update_connection_form_state(cx, ConnectionFormState::clear);
            }
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.pending = false;
                        form.error = Some(error.to_string());
                    }
                });
            }
        }
        cx.notify();
    }

    pub(super) fn submit_telnet_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((config, terminal_options, mut save_request)) =
            self.with_connection_form_mut(cx, |this, form, cx| {
                let form = form?;
                let host = form.host.trim().to_string();
                let port = form.port.trim().parse::<u16>().ok();
                if host.is_empty() {
                    form.error = Some(this.i18n.t("modals.new_connection.telnet_host_required"));
                    cx.notify();
                    return None;
                }
                let Some(port) = port else {
                    form.error = Some(this.i18n.t("modals.new_connection.telnet_invalid_port"));
                    cx.notify();
                    return None;
                };
                let should_save_profile = action != NewConnectionSubmitAction::Connect;
                let save_request = should_save_profile.then(|| SaveTelnetProfileRequest {
                    id: None,
                    name: telnet_profile_name_or_endpoint(&form.telnet_profile_name, &host, port),
                    group: serial_profile_group_from_form(&form.group, &this.i18n),
                    icon: asset_icon_from_form(&form.icon),
                    color: asset_color_from_form(&form.color),
                    icon_background_color: asset_color_from_form(&form.icon_background_color),
                    host: host.clone(),
                    port,
                    terminal: form.terminal,
                    connect_on_open: None,
                });
                let config = TelnetSessionConfig { host, port };
                let terminal_options = form.terminal;
                form.pending = true;
                form.error = None;
                Some((config, terminal_options, save_request))
            })
        else {
            return;
        };

        if action == NewConnectionSubmitAction::Save {
            let request =
                save_request.expect("telnet save action must build a telnet profile request");
            match self.connection_store.upsert_telnet_profile(request) {
                Ok(_) => {
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.telnet_save_failed")
                            ));
                        }
                    });
                }
            }
            cx.notify();
            return;
        }

        if action == NewConnectionSubmitAction::SaveAndConnect {
            let request = save_request
                .take()
                .expect("telnet save-and-open action must build a telnet profile request");
            match self.connection_store.upsert_telnet_profile(request) {
                Ok(_) => {}
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.telnet_save_failed")
                            ));
                        }
                    });
                    cx.notify();
                    return;
                }
            }
        }

        // Telnet is opened as a native local terminal transport. It does not
        // create an SSH node, so SSH-only saved-connection/test flows stay out.
        match self.create_telnet_terminal_tab(config, terminal_options, window, cx) {
            Ok(_) => {
                if let Some(request) = save_request {
                    match self.connection_store.upsert_telnet_profile(request) {
                        Ok(_) => {}
                        Err(error) => {
                            let message = format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.telnet_save_failed")
                            );
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(message), cx);
                            });
                        }
                    }
                }
                self.update_connection_form_state(cx, ConnectionFormState::clear);
            }
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.pending = false;
                        form.error = Some(error.to_string());
                    }
                });
            }
        }
        cx.notify();
    }

    pub(super) fn submit_remote_desktop_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((mut profile, save_request, mut runtime_password)) = self
            .with_connection_form_mut(cx, |this, form, cx| {
                let form = form?;
                let Some(protocol) = remote_desktop_protocol_for_transport(form.transport) else {
                    return None;
                };
                let host = form.host.trim().to_string();
                let port = form
                    .port
                    .trim()
                    .parse::<u16>()
                    .ok()
                    .filter(|port| *port > 0);
                if host.is_empty() {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.remote_desktop_host_required"),
                    );
                    cx.notify();
                    return None;
                }
                let Some(port) = port else {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.remote_desktop_invalid_port"),
                    );
                    cx.notify();
                    return None;
                };
                if protocol == RemoteDesktopProtocol::Rdp && form.username.trim().is_empty() {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.remote_desktop_username_required"),
                    );
                    cx.notify();
                    return None;
                }
                let editing_profile_id = form.remote_desktop_profile_id.clone();
                let existing_profile = editing_profile_id
                    .as_deref()
                    .and_then(|id| this.connection_store.get_remote_desktop_profile(id))
                    .cloned();
                let has_saved_credential = form.saved_password_keychain_id.is_some();
                if protocol == RemoteDesktopProtocol::Rdp
                    && action != NewConnectionSubmitAction::Save
                    && form.password.is_empty()
                    && !has_saved_credential
                {
                    form.error = Some(
                        this.i18n
                            .t("modals.new_connection.remote_desktop_password_required"),
                    );
                    cx.notify();
                    return None;
                }
                let label = remote_desktop_profile_label(&form.name, protocol, &host, port);
                let username = (protocol == RemoteDesktopProtocol::Rdp)
                    .then(|| form.username.trim().to_string())
                    .filter(|username| !username.is_empty());
                let password = if !form.password.is_empty() {
                    // Move the UI draft into a zeroizing type before saving or starting a worker.
                    Some(SecretString::from(std::mem::take(&mut form.password)))
                } else {
                    None
                };
                let save_credential = form.save_password;
                let should_save =
                    editing_profile_id.is_some() || action != NewConnectionSubmitAction::Connect;
                let clear_credential =
                    editing_profile_id.is_some() && has_saved_credential && !save_credential;
                let (credential_to_save, runtime_password) = if should_save && save_credential {
                    // Saving and connecting reloads the protected value below instead of cloning it.
                    (password, None)
                } else {
                    (None, password)
                };
                let domain = existing_profile
                    .as_ref()
                    .and_then(|profile| profile.domain.clone());
                let read_only = existing_profile
                    .as_ref()
                    .is_some_and(|profile| profile.read_only);
                let save_request = should_save.then(|| SaveRemoteDesktopProfileRequest {
                    id: editing_profile_id,
                    name: label.clone(),
                    group: serial_profile_group_from_form(&form.group, &this.i18n),
                    icon: asset_icon_from_form(&form.icon),
                    color: asset_color_from_form(&form.color),
                    icon_background_color: asset_color_from_form(&form.icon_background_color),
                    protocol,
                    host: host.clone(),
                    port,
                    username: username.clone(),
                    domain: domain.clone(),
                    credential_ref: None,
                    credential: credential_to_save,
                    clear_credential,
                    read_only,
                    session_options: form.remote_desktop_session_options,
                });
                let profile = RemoteDesktopConnectionProfile {
                    id: format!("new-remote-desktop-{}", uuid::Uuid::new_v4()),
                    label,
                    protocol,
                    endpoint: RemoteDesktopEndpoint::new(host, port),
                    username,
                    domain,
                    credential_ref: None,
                    read_only,
                    // A reconnect reuses the profile, so keep the user's per-session
                    // redirection choices on the profile instead of rebuilding defaults.
                    session_options: form.remote_desktop_session_options,
                };
                form.pending = true;
                form.error = None;
                Some((profile, save_request, runtime_password))
            })
        else {
            return;
        };

        if let Some(request) = save_request {
            match self.connection_store.upsert_remote_desktop_profile(request) {
                Ok(saved) => {
                    profile.id = saved.id;
                    profile.label = saved.name;
                    profile.credential_ref = saved.credential_ref;
                    self.queue_cloud_sync_dirty_refresh(cx);
                    if action != NewConnectionSubmitAction::Save && runtime_password.is_none() {
                        match self
                            .connection_store
                            .get_remote_desktop_credential(&profile.id)
                        {
                            Ok(password) => runtime_password = password,
                            Err(error) => {
                                self.update_connection_form_state(cx, |state| {
                                    if let Some(form) = state.form.as_mut() {
                                        form.pending = false;
                                        form.error = Some(format!(
                                            "{}: {error}",
                                            self.i18n.t(
                                                "sessionManager.remote_desktop_profiles.open_failed"
                                            )
                                        ));
                                    }
                                });
                                cx.notify();
                                return;
                            }
                        }
                    }
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = false;
                            form.error = Some(format!(
                                "{}: {error}",
                                self.i18n
                                    .t("modals.new_connection.remote_desktop_save_failed")
                            ));
                        }
                    });
                    cx.notify();
                    return;
                }
            }
        }

        self.update_connection_form_state(cx, ConnectionFormState::clear);
        if action != NewConnectionSubmitAction::Save {
            let runtime_password =
                runtime_password.map(|secret| RemoteDesktopSecret::from(secret.into_zeroizing()));
            self.open_remote_desktop_connection_tab(profile, runtime_password, window, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn start_new_connection_flow(
        &mut self,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if intent == SshConnectionIntent::Test
            && self
                .connection_form_state(cx)
                .form
                .as_ref()
                .is_some_and(|form| form.auth_tab == SshAuthTab::TwoFactor)
        {
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.error = Some(self.i18n.t("ssh.form.test_not_supported_kbi"));
                }
            });
            cx.notify();
            return;
        }
        let Some((config, title)) = self.build_new_connection_config(cx) else {
            return;
        };
        self.start_new_connection_config_flow(config, title, intent, window, cx);
    }

    fn start_new_connection_config_flow(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if intent == SshConnectionIntent::Test {
            self.start_ssh_test_flow(config, title, cx);
            return;
        }
        let mut config = config;
        if let Err(error) = prepare_tree_connect_config(&mut config) {
            let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.set_form_feedback(None, Some(error.clone()), cx)
            });
            if !reported_to_form {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(error), cx);
                });
            }
            cx.notify();
            return;
        }
        if matches!(&intent, SshConnectionIntent::DrillDown { .. }) {
            // Tauri DrillDownDialog calls tree_drill_down and then
            // connect_tree_node; it does not run a local direct host-key
            // preflight because the child may only be reachable through the
            // parent tunnel. Native keeps that node-only path here.
            self.continue_verified_ssh_flow(config, title, intent, window, cx);
            return;
        }
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.pending = true;
                form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
            }
        });

        if config.proxy_chain.is_some() {
            self.start_proxy_session_tree_connect(config, title, intent, None, window, cx);
            cx.notify();
            return;
        }
        self.start_ssh_preflight(config, title, intent, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_connection(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = saved_connection_for_open(&self.connection_store, id) else {
            // Saved rows can outlive an external store update. Report the
            // stale reference without exposing its identifier or connection data.
            tracing::warn!("Saved connection lookup failed before opening");
            let title = self.i18n.t("sessionManager.toast.connection_not_found");
            self.push_command_palette_toast(title.clone(), None, TerminalNoticeVariant::Error, cx);
            self.push_notification_entry(
                WorkspaceNotificationKind::Connection,
                WorkspaceNotificationSeverity::Error,
                title,
                None,
                WorkspaceNotificationScope::Global,
                Some("saved-connection-not-found".to_string()),
            );
            cx.notify();
            return;
        };
        let Some(config) = ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &conn,
        ) else {
            if self.try_reuse_active_saved_connection_terminal(id, &conn, window, cx) {
                return;
            }
            self.open_saved_connection_prompt(
                id,
                SavedConnectionPromptAction::Connect,
                Some(
                    self.i18n
                        .t("sessionManager.edit_properties.password_placeholder"),
                ),
                window,
                cx,
            );
            return;
        };
        let title = conn.name.clone();
        self.start_saved_connection_flow(id.to_string(), config, title, window, cx);
    }

    pub(in crate::workspace) fn open_saved_connection_prompt(
        &mut self,
        id: &str,
        action: SavedConnectionPromptAction,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = self.connection_store.get(id).cloned() else {
            return;
        };
        self.prepare_modal_interaction_boundary(cx);
        let form = form_from_saved_connection(&conn, error);
        self.update_connection_form_state(cx, |state| {
            state.replace_with_new_form(form);
            state.editing_saved_connection_id = Some(id.to_string());
            state.saved_connection_prompt_action = Some(action);
        });
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_connection_editor(
        &mut self,
        id: &str,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = self.connection_store.get(id).cloned() else {
            return;
        };
        self.prepare_modal_interaction_boundary(cx);
        let form = form_from_saved_connection(&conn, error);
        self.update_connection_form_state(cx, |state| {
            state.replace_with_new_form(form);
            state.editing_saved_connection_id = Some(id.to_string());
        });
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_connection_reconnect_editor(
        &mut self,
        node_id: NodeId,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_connection_editor(id, None, window, cx);
        if self
            .connection_form_state(cx)
            .editing_saved_connection_id
            .as_deref()
            == Some(id)
        {
            // This marker is consumed after a successful save so normal
            // connection edits keep their existing save-only behavior.
            self.update_connection_form_state(cx, |state| {
                state.editing_saved_connection_connect_after_save_node_id = Some(node_id);
            });
        }
    }

    pub(in crate::workspace) fn open_runtime_node_reconnect_editor(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self.ssh_nodes.get(&node_id).map(|node| node.title.clone()) else {
            return;
        };
        let Some(runtime_snapshot) = self.node_router.node_runtime_snapshot(&node_id) else {
            return;
        };
        self.prepare_modal_interaction_boundary(cx);
        let mut form = form_from_runtime_config(
            runtime_snapshot.config,
            Some(&title),
            self.i18n.t("ssh.form.ungrouped"),
        );
        form.agent_available = detect_ssh_agent_available(&form.identity_agent);
        form.save_connection = false;
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(super) fn submit_saved_connection_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self
            .connection_form_state(cx)
            .saved_connection_prompt_action
        else {
            return;
        };
        let Some(id) = self
            .connection_form_state(cx)
            .editing_saved_connection_id
            .clone()
        else {
            return;
        };
        let Some((mut config, title)) = self.build_new_connection_config(cx) else {
            return;
        };
        if config.proxy_chain.is_none()
            && let Some(conn) = self.connection_store.get(&id)
            && let Some(proxy_chain) =
                proxy_chain_config_from_saved_connection(&self.connection_store, conn)
            && !proxy_chain.is_empty()
        {
            config.proxy_chain = Some(proxy_chain);
            config.strict_host_key_checking = true;
        }

        match action {
            SavedConnectionPromptAction::Connect => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.pending = true;
                        form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
                    }
                });
                self.start_saved_connection_flow(id, config, title, window, cx);
            }
            SavedConnectionPromptAction::Test => {
                self.start_ssh_test_flow(config, title, cx);
            }
        }
    }

    pub(super) fn sync_saved_connection_node_title(&mut self, saved_connection_id: &str) -> bool {
        let Some(title) = self
            .connection_store
            .get(saved_connection_id)
            .map(|connection| connection.name.clone())
        else {
            return false;
        };
        sync_saved_connection_node_title_for_nodes(&mut self.ssh_nodes, saved_connection_id, &title)
    }

    pub(super) fn save_editing_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self
            .connection_form_state(cx)
            .editing_saved_connection_id
            .clone()
        else {
            return;
        };
        let existing_connection = self.connection_store.get(&id).cloned();
        let existing_auth = existing_connection
            .as_ref()
            .map(|connection| connection.auth.clone());
        let Some(save_request) = self.with_connection_form_mut(cx, |_this, form, _cx| {
            let form = form?;
            Some(
                save_request_from_form_with_existing_auth(
                    form,
                    Some(id.clone()),
                    existing_auth.as_ref(),
                )
                .map(|mut request| {
                    if form.proxy_hops.is_empty()
                        && let Some(connection) = existing_connection.as_ref()
                    {
                        request.proxy_chain = connection.proxy_chain.clone();
                    }
                    request
                }),
            )
        }) else {
            return;
        };
        match save_request {
            Ok(request) => {
                match self.connection_store.upsert(request) {
                    Ok(_) => {
                        self.sync_saved_connection_node_title(&id);
                        self.apply_saved_connection_terminal_preferences(&id, cx);
                        let connect_after_save_node_id =
                            self.update_connection_form_state(cx, |state| {
                                let node_id = state
                                    .editing_saved_connection_connect_after_save_node_id
                                    .take();
                                state.clear();
                                node_id
                            });
                        self.queue_cloud_sync_dirty_refresh(cx);
                        if let Some(node_id) = connect_after_save_node_id {
                            if let Some(conn) = self.connection_store.get(&id).cloned()
                                && let Some(config) = ssh_config_from_saved_connection(
                                    &self.connection_store,
                                    self.settings_store.settings(),
                                    &conn,
                                )
                            {
                                let title = conn.name.clone();
                                // Drop the stale failed runtime node before
                                // materializing the edited connection again.
                                self.remove_inactive_session_tree_node(&node_id, window, cx);
                                self.start_saved_connection_flow(id, config, title, window, cx);
                            } else {
                                self.open_saved_connection_prompt(
                                    &id,
                                    SavedConnectionPromptAction::Connect,
                                    Some(
                                        self.i18n.t(
                                            "sessionManager.edit_properties.password_placeholder",
                                        ),
                                    ),
                                    window,
                                    cx,
                                );
                            }
                        } else {
                            let message = self.i18n.t("sessionManager.edit_properties.save");
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(message), cx);
                            });
                            self.focus_active_pane(window, cx);
                        }
                    }
                    Err(error) => {
                        self.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                form.error = Some(error.to_string());
                            }
                        });
                    }
                }
            }
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(error.to_string());
                    }
                });
            }
        }
        cx.notify();
    }

    pub(super) fn save_duplicate_connection_template(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source_id) = self
            .connection_form_state(cx)
            .duplicating_saved_connection_id
            .clone()
        else {
            return;
        };
        let source_connection = self.connection_store.get(&source_id).cloned();
        let source_auth = source_connection
            .as_ref()
            .map(|connection| connection.auth.clone());
        let Some(save_request) = self.with_connection_form_mut(cx, |_this, form, _cx| {
            let form = form?;
            Some(
                save_request_from_form_with_existing_auth(form, None, source_auth.as_ref()).map(
                    |mut request| {
                        if form.proxy_hops.is_empty()
                            && let Some(connection) = source_connection.as_ref()
                        {
                            // Preserve the source chain when it was not expanded for editing.
                            request.proxy_chain = connection.proxy_chain.clone();
                        }
                        request
                    },
                ),
            )
        }) else {
            return;
        };
        match save_request {
            Ok(request) => match self.connection_store.upsert(request) {
                Ok(_) => {
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                    let message = self.i18n.t("sessionManager.toast.connection_duplicated");
                    self.session_manager.update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(message), cx);
                    });
                    self.queue_cloud_sync_dirty_refresh(cx);
                    self.focus_active_pane(window, cx);
                }
                Err(error) => {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.error = Some(error.to_string());
                        }
                    });
                }
            },
            Err(error) => {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(error.to_string());
                    }
                });
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn start_saved_connection_flow(
        &mut self,
        id: String,
        mut config: SshConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = prepare_tree_connect_config(&mut config) {
            let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.set_form_feedback(None, Some(error.clone()), cx)
            });
            if !reported_to_form {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(error), cx);
                });
            }
            cx.notify();
            return;
        }
        let message = self.i18n.t("ssh.form.checking_host_key");
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.set_status(Some(message), cx);
        });
        if config.proxy_chain.is_some() {
            self.start_proxy_session_tree_connect(
                config,
                title,
                SshConnectionIntent::ConnectSaved(id),
                None,
                window,
                cx,
            );
            cx.notify();
            return;
        }
        self.start_ssh_preflight(config, title, SshConnectionIntent::ConnectSaved(id), cx);
        cx.notify();
    }

    pub(in crate::workspace) fn start_ssh_preflight(
        &self,
        mut config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        cx: &App,
    ) {
        let tx = self.ssh_worker_sender(cx);
        let host = config.host.clone();
        let port = config.port;
        let upstream_proxy = config.upstream_proxy.take();
        let worker_config = config;
        let worker_title = title;
        std::thread::spawn(move || {
            let status = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime.block_on(check_host_key_with_upstream_proxy(
                    &host,
                    port,
                    10,
                    upstream_proxy.as_ref(),
                )),
                Err(error) => HostKeyStatus::Error {
                    message: format!("failed to initialize SSH runtime: {error}"),
                },
            };
            let _ = tx.send(SshConnectionWorkerResult::Preflight {
                config: worker_config,
                upstream_proxy,
                title: worker_title,
                intent,
                status,
            });
        });
    }
}

fn saved_connection_for_open(store: &ConnectionStore, id: &str) -> Option<SavedConnection> {
    store.get(id).cloned()
}

#[cfg(test)]
mod saved_connection_open_tests {
    use super::*;

    #[test]
    fn stale_saved_connection_id_is_detected_before_opening() {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-stale-saved-connection-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = ConnectionStore::load(path).expect("empty connection store");

        assert!(saved_connection_for_open(&store, "removed-connection").is_none());
    }
}
