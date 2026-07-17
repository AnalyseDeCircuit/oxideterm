// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::Timer;

impl WorkspaceApp {
    pub(super) fn report_saved_next_hop_error(&mut self, i18n_key: &str, cx: &mut Context<Self>) {
        self.report_saved_next_hop_message(self.i18n.t(i18n_key), cx);
    }

    pub(super) fn report_saved_next_hop_message(
        &mut self,
        message: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = Some(message);
        } else {
            self.session_manager.status = Some(message);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn open_save_runtime_node_form(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            self.session_manager.status = Some(self.i18n.t("ssh.form.runtime_node_missing"));
            cx.notify();
            return;
        };
        let parent_id = self
            .node_runtime_store
            .snapshot(&node_id)
            .and_then(|snapshot| snapshot.parent_id);
        let proxy_hops = match parent_id
            .as_ref()
            .map(|parent_id| self.runtime_proxy_hops_for_parent_path(parent_id))
            .transpose()
        {
            Ok(hops) => hops.unwrap_or_default(),
            Err(error) => {
                self.session_manager.status = Some(error.to_string());
                cx.notify();
                return;
            }
        };

        self.prepare_modal_interaction_boundary();
        let mut form = form_from_runtime_config(
            &node.config,
            Some(&node.title),
            self.i18n.t("ssh.form.ungrouped"),
        );
        form.proxy_hops = proxy_hops;
        form.proxy_chain_expanded = !form.proxy_hops.is_empty();
        form.agent_available = detect_ssh_agent_available();
        form.save_connection = true;
        self.new_connection_form = Some(form);
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.editing_saved_connection_connect_after_save_node_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
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
            let snapshot = self.node_runtime_store.snapshot(&node_id).ok_or_else(|| {
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
        let Some(generation) = self.new_connection_form_presence.begin_exit() else {
            return;
        };
        self.close_new_connection_select();
        self.focus_active_pane(window, cx);
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.finish_new_connection_form_exit(generation, cx);
            return;
        }
        // Secret-bearing form state stays in its single owner until the inert
        // visual exit completes; no password or passphrase payload is cloned.
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.finish_new_connection_form_exit(generation, cx) {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_new_connection_form_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.new_connection_form_presence.finish_exit(generation) {
            return false;
        }
        self.new_connection_form = None;
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.editing_saved_connection_connect_after_save_node_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.host_key_challenge = None;
        self.cancel_active_proxy_connect_run();
        self.cancel_keyboard_interactive_challenge(cx);
        self.new_connection_form_presence.reopen();
        true
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
        if self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| form.transport == NewConnectionTransport::Serial)
            && self.drill_down_parent_node_id.is_none()
            && matches!(
                new_connection_form_mode(
                    self.editing_saved_connection_id.as_deref(),
                    self.duplicating_saved_connection_id.as_deref(),
                    self.saved_connection_prompt_action,
                ),
                NewConnectionFormMode::NewConnection
            )
        {
            self.submit_serial_connection_form(action, window, cx);
            return;
        }
        if self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| form.transport == NewConnectionTransport::Telnet)
            && self.drill_down_parent_node_id.is_none()
            && matches!(
                new_connection_form_mode(
                    self.editing_saved_connection_id.as_deref(),
                    self.duplicating_saved_connection_id.as_deref(),
                    self.saved_connection_prompt_action,
                ),
                NewConnectionFormMode::NewConnection
            )
        {
            self.submit_telnet_connection_form(action, window, cx);
            return;
        }
        if self
            .new_connection_form
            .as_ref()
            .and_then(|form| remote_desktop_protocol_for_transport(form.transport))
            .is_some()
            && self.drill_down_parent_node_id.is_none()
            && matches!(
                new_connection_form_mode(
                    self.editing_saved_connection_id.as_deref(),
                    self.duplicating_saved_connection_id.as_deref(),
                    self.saved_connection_prompt_action,
                ),
                NewConnectionFormMode::NewConnection
            )
        {
            self.submit_remote_desktop_connection_form(window, cx);
            return;
        }
        if self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| form.transport == NewConnectionTransport::WslGraphics)
            && self.drill_down_parent_node_id.is_none()
            && matches!(
                new_connection_form_mode(
                    self.editing_saved_connection_id.as_deref(),
                    self.duplicating_saved_connection_id.as_deref(),
                    self.saved_connection_prompt_action,
                ),
                NewConnectionFormMode::NewConnection
            )
        {
            self.close_new_connection_form(window, cx);
            self.open_graphics_tab(window, cx);
            return;
        }
        if let Some(parent_id) = self.drill_down_parent_node_id.clone() {
            match action {
                NewConnectionSubmitAction::Save => {
                    self.save_new_connection_without_connecting(Some(&parent_id), window, cx);
                    return;
                }
                NewConnectionSubmitAction::SaveAndConnect => {
                    if !self.save_current_connection_form(Some(&parent_id), cx) {
                        return;
                    }
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.save_connection = false;
                    }
                }
                NewConnectionSubmitAction::Connect => {
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.save_connection = false;
                    }
                }
            }
            self.start_new_connection_flow(SshConnectionIntent::DrillDown(parent_id), window, cx);
            return;
        }
        match new_connection_form_mode(
            self.editing_saved_connection_id.as_deref(),
            self.duplicating_saved_connection_id.as_deref(),
            self.saved_connection_prompt_action,
        ) {
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
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.save_connection = false;
                    }
                    self.start_new_connection_flow(SshConnectionIntent::Connect, window, cx);
                }
                NewConnectionSubmitAction::Save => {
                    self.save_new_connection_without_connecting(None, window, cx);
                }
                NewConnectionSubmitAction::SaveAndConnect => {
                    if !self.save_current_connection_form(None, cx) {
                        return;
                    }
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.save_connection = false;
                    }
                    self.start_new_connection_flow(SshConnectionIntent::Connect, window, cx);
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
        if self.save_current_connection_form(drill_down_parent_id, cx) {
            self.close_new_connection_form(window, cx);
        }
    }

    pub(super) fn save_current_connection_form(
        &mut self,
        drill_down_parent_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.ensure_new_connection_save_name_is_unique(drill_down_parent_id);
        let request = match self.save_request_for_current_form(drill_down_parent_id) {
            Some(Ok(request)) => request,
            Some(Err(error)) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(error.to_string());
                }
                cx.notify();
                return false;
            }
            None => return false,
        };

        // The Save and Save & Connect buttons mean "persist this draft now",
        // so duplicate-name and keychain failures should block connection start.
        match self.connection_store.upsert(request) {
            Ok(_) => {
                self.queue_cloud_sync_dirty_refresh(cx);
                true
            }
            Err(error) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(format!(
                        "{}: {error}",
                        self.i18n.t("modals.new_connection.save_failed")
                    ));
                }
                cx.notify();
                false
            }
        }
    }

    pub(super) fn ensure_new_connection_save_name_is_unique(
        &mut self,
        _drill_down_parent_id: Option<&NodeId>,
    ) {
        let occupied_names: Vec<String> = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.name.clone())
            .collect();
        let Some(form) = self.new_connection_form.as_mut() else {
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
    }

    pub(super) fn save_request_for_current_form(
        &self,
        drill_down_parent_id: Option<&NodeId>,
    ) -> Option<anyhow::Result<SaveConnectionRequest>> {
        let form = self.new_connection_form.as_ref()?;
        let mut form = form.clone();
        if let Some(parent_id) = drill_down_parent_id {
            match self.runtime_proxy_hops_for_parent_path(parent_id) {
                Ok(mut hops) => {
                    hops.extend(form.proxy_hops);
                    form.proxy_hops = hops;
                }
                Err(error) => return Some(Err(error)),
            }
        }
        Some(save_request_from_form(&form, None))
    }

    pub(super) fn submit_serial_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        let port_path = form.serial_port_path.trim().to_string();
        let baud_rate = form.serial_baud_rate.trim().parse::<u32>().ok();
        if port_path.is_empty() {
            form.error = Some(self.i18n.t("modals.new_connection.serial_port_required"));
            cx.notify();
            return;
        }
        let Some(baud_rate) = baud_rate.filter(|baud| *baud > 0) else {
            form.error = Some(
                self.i18n
                    .t("modals.new_connection.serial_invalid_baud_rate"),
            );
            cx.notify();
            return;
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
        let mut save_request = should_save_profile.then(|| SaveSerialProfileRequest {
            id: None,
            name: serial_profile_name_or_port(&form.serial_profile_name, &port_path),
            group: serial_profile_group_from_form(&form.group, &self.i18n),
            port_path: port_path.clone(),
            baud_rate: Some(baud_rate),
            data_bits: Some(form.serial_data_bits),
            stop_bits: Some(form.serial_stop_bits),
            parity: Some(serial_profile_parity_from_terminal(form.serial_parity)),
            flow_control: Some(serial_profile_flow_from_terminal(form.serial_flow_control)),
            connect_on_open: None,
        });
        form.pending = true;
        form.error = None;

        if action == NewConnectionSubmitAction::Save {
            let request =
                save_request.expect("serial save action must build a serial profile request");
            match self.connection_store.upsert_serial_profile(request) {
                Ok(_) => {
                    self.queue_cloud_sync_dirty_refresh(cx);
                    self.new_connection_form = None;
                    self.close_new_connection_select();
                }
                Err(error) => {
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.pending = false;
                        form.error = Some(format!(
                            "{}: {error}",
                            self.i18n.t("modals.new_connection.serial_save_failed")
                        ));
                    }
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
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.pending = false;
                        form.error = Some(format!(
                            "{}: {error}",
                            self.i18n.t("modals.new_connection.serial_save_failed")
                        ));
                    }
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
                            self.session_manager.status = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.serial_save_failed")
                            ));
                        }
                    }
                }
                self.new_connection_form = None;
                self.close_new_connection_select();
            }
            Err(error) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.pending = false;
                    form.error = Some(error.to_string());
                }
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
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        let host = form.host.trim().to_string();
        let port = form.port.trim().parse::<u16>().ok();
        if host.is_empty() {
            form.error = Some(self.i18n.t("modals.new_connection.telnet_host_required"));
            cx.notify();
            return;
        }
        let Some(port) = port else {
            form.error = Some(self.i18n.t("modals.new_connection.telnet_invalid_port"));
            cx.notify();
            return;
        };
        let should_save_profile = action != NewConnectionSubmitAction::Connect;
        let mut save_request = should_save_profile.then(|| SaveTelnetProfileRequest {
            id: None,
            name: telnet_profile_name_or_endpoint(&form.telnet_profile_name, &host, port),
            group: serial_profile_group_from_form(&form.group, &self.i18n),
            host: host.clone(),
            port,
            connect_on_open: None,
        });
        let config = TelnetSessionConfig { host, port };
        form.pending = true;
        form.error = None;

        if action == NewConnectionSubmitAction::Save {
            let request =
                save_request.expect("telnet save action must build a telnet profile request");
            match self.connection_store.upsert_telnet_profile(request) {
                Ok(_) => {
                    self.new_connection_form = None;
                    self.close_new_connection_select();
                }
                Err(error) => {
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.pending = false;
                        form.error = Some(format!(
                            "{}: {error}",
                            self.i18n.t("modals.new_connection.telnet_save_failed")
                        ));
                    }
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
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.pending = false;
                        form.error = Some(format!(
                            "{}: {error}",
                            self.i18n.t("modals.new_connection.telnet_save_failed")
                        ));
                    }
                    cx.notify();
                    return;
                }
            }
        }

        // Telnet is opened as a native local terminal transport. It does not
        // create an SSH node, so SSH-only saved-connection/test flows stay out.
        match self.create_telnet_terminal_tab(config, window, cx) {
            Ok(_) => {
                if let Some(request) = save_request {
                    match self.connection_store.upsert_telnet_profile(request) {
                        Ok(_) => {}
                        Err(error) => {
                            self.session_manager.status = Some(format!(
                                "{}: {error}",
                                self.i18n.t("modals.new_connection.telnet_save_failed")
                            ));
                        }
                    }
                }
                self.new_connection_form = None;
                self.close_new_connection_select();
            }
            Err(error) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.pending = false;
                    form.error = Some(error.to_string());
                }
            }
        }
        cx.notify();
    }

    pub(super) fn submit_remote_desktop_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        let Some(protocol) = remote_desktop_protocol_for_transport(form.transport) else {
            return;
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
                self.i18n
                    .t("modals.new_connection.remote_desktop_host_required"),
            );
            cx.notify();
            return;
        }
        let Some(port) = port else {
            form.error = Some(
                self.i18n
                    .t("modals.new_connection.remote_desktop_invalid_port"),
            );
            cx.notify();
            return;
        };
        if protocol == RemoteDesktopProtocol::Rdp && form.username.trim().is_empty() {
            form.error = Some(
                self.i18n
                    .t("modals.new_connection.remote_desktop_username_required"),
            );
            cx.notify();
            return;
        }
        if protocol == RemoteDesktopProtocol::Rdp && form.password.is_empty() {
            form.error = Some(
                self.i18n
                    .t("modals.new_connection.remote_desktop_password_required"),
            );
            cx.notify();
            return;
        }
        let label = remote_desktop_profile_label(&form.name, protocol, &host, port);
        let username = (protocol == RemoteDesktopProtocol::Rdp)
            .then(|| form.username.trim().to_string())
            .filter(|username| !username.is_empty());
        let password = if !form.password.is_empty() {
            // Remote desktop passwords are runtime-only. Move the UI draft into
            // a zeroizing wrapper before the form is dropped. VNC keeps this
            // optional because servers may advertise unauthenticated access.
            Some(RemoteDesktopSecret::from(std::mem::take(
                &mut form.password,
            )))
        } else {
            None
        };
        let profile = RemoteDesktopConnectionProfile {
            id: format!("new-remote-desktop-{}", uuid::Uuid::new_v4()),
            label,
            protocol,
            endpoint: RemoteDesktopEndpoint::new(host, port),
            username,
            domain: None,
            credential_ref: None,
            read_only: false,
        };

        self.new_connection_form = None;
        self.close_new_connection_select();
        self.open_remote_desktop_connection_tab(profile, password, window, cx);
    }

    pub(in crate::workspace) fn start_new_connection_flow(
        &mut self,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if intent == SshConnectionIntent::Test
            && self
                .new_connection_form
                .as_ref()
                .is_some_and(|form| form.auth_tab == SshAuthTab::TwoFactor)
        {
            if let Some(form) = self.new_connection_form.as_mut() {
                form.error = Some(self.i18n.t("ssh.form.test_not_supported_kbi"));
            }
            cx.notify();
            return;
        }
        let Some((config, title)) = self.build_new_connection_config(cx) else {
            return;
        };
        if intent == SshConnectionIntent::Test {
            self.start_ssh_test_flow(config, title, cx);
            return;
        }
        let mut config = config;
        if let Err(error) = prepare_tree_connect_config(&mut config) {
            if let Some(form) = self.new_connection_form.as_mut() {
                form.error = Some(error);
            } else {
                self.session_manager.status = Some(error);
            }
            cx.notify();
            return;
        }
        if let SshConnectionIntent::DrillDown(parent_id) = intent {
            // Tauri DrillDownDialog calls tree_drill_down and then
            // connect_tree_node; it does not run a local direct host-key
            // preflight because the child may only be reachable through the
            // parent tunnel. Native keeps that node-only path here.
            self.continue_verified_ssh_flow(
                config,
                title,
                SshConnectionIntent::DrillDown(parent_id),
                window,
                cx,
            );
            return;
        }
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = true;
            form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
        }

        if config.proxy_chain.is_some() {
            let save_after_open = match self.save_after_open_request_for_connect_intent(cx) {
                Ok(save_after_open) => save_after_open,
                Err(()) => return,
            };
            self.start_proxy_session_tree_connect(
                config,
                title,
                intent,
                save_after_open,
                window,
                cx,
            );
            cx.notify();
            return;
        }
        self.start_ssh_preflight(config, title, intent);
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_connection(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = self.connection_store.get(id).cloned() else {
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
        self.prepare_modal_interaction_boundary();
        self.new_connection_form = Some(form_from_saved_connection(&conn, error));
        self.editing_saved_connection_id = Some(id.to_string());
        self.editing_saved_connection_connect_after_save_node_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = Some(action);
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
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
        self.prepare_modal_interaction_boundary();
        self.new_connection_form = Some(form_from_saved_connection(&conn, error));
        self.editing_saved_connection_id = Some(id.to_string());
        self.editing_saved_connection_connect_after_save_node_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
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
        if self.editing_saved_connection_id.as_deref() == Some(id) {
            // This marker is consumed after a successful save so normal
            // connection edits keep their existing save-only behavior.
            self.editing_saved_connection_connect_after_save_node_id = Some(node_id);
        }
    }

    pub(in crate::workspace) fn open_runtime_node_reconnect_editor(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            return;
        };
        self.prepare_modal_interaction_boundary();
        let mut form = form_from_runtime_config(
            &node.config,
            Some(&node.title),
            self.i18n.t("ssh.form.ungrouped"),
        );
        form.agent_available = detect_ssh_agent_available();
        form.save_connection = false;
        self.new_connection_form = Some(form);
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.editing_saved_connection_connect_after_save_node_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(super) fn submit_saved_connection_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.saved_connection_prompt_action else {
            return;
        };
        let Some(id) = self.editing_saved_connection_id.clone() else {
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
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.pending = true;
                    form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
                }
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
        let Some(id) = self.editing_saved_connection_id.clone() else {
            return;
        };
        let Some(form) = self.new_connection_form.as_ref() else {
            return;
        };
        let existing_connection = self.connection_store.get(&id).cloned();
        let existing_auth = existing_connection
            .as_ref()
            .map(|connection| connection.auth.clone());
        match save_request_from_form_with_existing_auth(
            form,
            Some(id.clone()),
            existing_auth.as_ref(),
        ) {
            Ok(mut request) => {
                if form.proxy_hops.is_empty()
                    && let Some(connection) = existing_connection.as_ref()
                {
                    request.proxy_chain = connection.proxy_chain.clone();
                }
                match self.connection_store.upsert(request) {
                    Ok(_) => {
                        self.sync_saved_connection_node_title(&id);
                        let connect_after_save_node_id = self
                            .editing_saved_connection_connect_after_save_node_id
                            .take();
                        self.new_connection_form = None;
                        self.editing_saved_connection_id = None;
                        self.duplicating_saved_connection_id = None;
                        self.close_new_connection_select();
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
                            self.session_manager.status =
                                Some(self.i18n.t("sessionManager.edit_properties.save"));
                            self.focus_active_pane(window, cx);
                        }
                    }
                    Err(error) => {
                        if let Some(form) = self.new_connection_form.as_mut() {
                            form.error = Some(error.to_string());
                        }
                    }
                }
            }
            Err(error) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(error.to_string());
                }
            }
        }
        cx.notify();
    }

    pub(super) fn save_duplicate_connection_template(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source_id) = self.duplicating_saved_connection_id.clone() else {
            return;
        };
        let Some(form) = self.new_connection_form.as_ref() else {
            return;
        };
        let source_connection = self.connection_store.get(&source_id).cloned();
        let source_auth = source_connection
            .as_ref()
            .map(|connection| connection.auth.clone());
        match save_request_from_form_with_existing_auth(form, None, source_auth.as_ref()) {
            Ok(mut request) => {
                if form.proxy_hops.is_empty()
                    && let Some(connection) = source_connection.as_ref()
                {
                    // The modal edits the target connection fields only. If the
                    // proxy chain was not expanded into editable rows, keep the
                    // source chain just like Tauri's duplicate draft does.
                    request.proxy_chain = connection.proxy_chain.clone();
                }
                match self.connection_store.upsert(request) {
                    Ok(_) => {
                        self.new_connection_form = None;
                        self.editing_saved_connection_id = None;
                        self.editing_saved_connection_connect_after_save_node_id = None;
                        self.duplicating_saved_connection_id = None;
                        self.close_new_connection_select();
                        self.session_manager.status =
                            Some(self.i18n.t("sessionManager.toast.connection_duplicated"));
                        self.queue_cloud_sync_dirty_refresh(cx);
                        self.focus_active_pane(window, cx);
                    }
                    Err(error) => {
                        if let Some(form) = self.new_connection_form.as_mut() {
                            form.error = Some(error.to_string());
                        }
                    }
                }
            }
            Err(error) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(error.to_string());
                }
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
            if let Some(form) = self.new_connection_form.as_mut() {
                form.error = Some(error);
            } else {
                self.session_manager.status = Some(error);
            }
            cx.notify();
            return;
        }
        self.session_manager.status = Some(self.i18n.t("ssh.form.checking_host_key"));
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
        self.start_ssh_preflight(config, title, SshConnectionIntent::ConnectSaved(id));
        cx.notify();
    }

    pub(in crate::workspace) fn start_ssh_preflight(
        &self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
    ) {
        let tx = self.ssh_worker_tx.clone();
        let host = config.host.clone();
        let port = config.port;
        let upstream_proxy = config.upstream_proxy.clone();
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
                title: worker_title,
                intent,
                status,
            });
        });
    }
}
