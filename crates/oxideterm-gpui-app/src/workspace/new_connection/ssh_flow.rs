use std::{
    future::Future,
    pin::Pin,
    result::Result as StdResult,
    sync::{Arc, mpsc},
    time::Duration,
};

use gpui::{Context, Window};
use oxideterm_connections::{
    SaveConnectionRequest, SaveSerialProfileRequest, SaveTelnetProfileRequest,
    SavedUpstreamProxyProtocol, first_available_default_key_path,
};
use oxideterm_ssh::{
    AuthMethod, ConnectionConsumer, ConnectionState, HostKeyStatus,
    KeyboardInteractivePromptRequest, KeyboardInteractiveResponses, NodeId, NodeReadiness,
    NodeTreeExpansion, ProxyHopConfig, SshConfig, SshPromptError, SshPromptHandler,
    SshTransportClient, UpstreamProxyAuth, UpstreamProxyProtocol,
    check_host_key_with_upstream_proxy,
};
use tokio::sync::oneshot;

use super::{
    form_state::{
        NewConnectionForm, NewConnectionFormMode, NewConnectionProxyHop, NewConnectionSubmitAction,
        NewConnectionTransport, NewConnectionUpstreamProxyAuth, NewConnectionUpstreamProxyPolicy,
        SavedConnectionPromptAction, SshAuthTab, new_connection_form_mode,
    },
    host_key_dialog::HostKeyChallenge,
    session_tree_plan::{
        NativeSessionTreeConnectAction, NativeSessionTreeConnectEndpoint,
        NativeSessionTreeConnectPlan, NativeSessionTreeConnectStep,
    },
};
use crate::workspace::{
    NativeProxyConnectRun, WorkspaceApp,
    session_manager::{
        duplicate_connection_template_name, form_from_saved_connection, save_request_from_form,
        save_request_from_form_with_existing_auth, upstream_proxy_config_from_form,
    },
};
use oxideterm_session_adapter::{
    managed_key_resolver_from_store, proxy_chain_config_from_saved_connection,
    ssh_config_from_saved_connection,
};
use oxideterm_terminal::{SerialSessionConfig, TelnetSessionConfig};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SshConnectionIntent {
    Test,
    Connect,
    ConnectSaved(String),
    DrillDown(NodeId),
}

pub(in crate::workspace) enum SshConnectionWorkerResult {
    Preflight {
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        status: HostKeyStatus,
    },
    SessionTreePreflight {
        run: NativeProxyConnectRun,
        status: HostKeyStatus,
    },
    Test {
        result: StdResult<(), String>,
    },
    KeyboardInteractivePrompt {
        request: KeyboardInteractivePromptRequest,
        response_tx: oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>,
    },
}

#[derive(Clone)]
pub(in crate::workspace) struct NativeSshPromptHandler {
    tx: mpsc::Sender<SshConnectionWorkerResult>,
}

impl NativeSshPromptHandler {
    pub(in crate::workspace) fn new(tx: mpsc::Sender<SshConnectionWorkerResult>) -> Self {
        Self { tx }
    }
}

impl SshPromptHandler for NativeSshPromptHandler {
    fn keyboard_interactive(
        &self,
        request: KeyboardInteractivePromptRequest,
    ) -> Pin<
        Box<dyn Future<Output = Result<KeyboardInteractiveResponses, SshPromptError>> + Send + '_>,
    > {
        Box::pin(async move {
            let (response_tx, response_rx) = oneshot::channel();
            self.tx
                .send(SshConnectionWorkerResult::KeyboardInteractivePrompt {
                    request,
                    response_tx,
                })
                .map_err(|_| {
                    SshPromptError::Failed("native SSH prompt UI is unavailable".into())
                })?;
            response_rx
                .await
                .map_err(|_| SshPromptError::Failed("native SSH prompt UI was closed".into()))?
        })
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn saved_connection_form_source_id(&self) -> Option<&str> {
        self.editing_saved_connection_id
            .as_deref()
            .or(self.duplicating_saved_connection_id.as_deref())
    }

    pub(in crate::workspace) fn saved_connection_form_uses_unloaded_secret(&self) -> bool {
        self.saved_connection_form_source_id().is_some()
            && self.saved_connection_prompt_action.is_none()
    }

    pub(in crate::workspace) fn open_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary();
        self.new_connection_form = Some(NewConnectionForm {
            group: self.i18n.t("ssh.form.ungrouped"),
            agent_available: detect_ssh_agent_available(),
            save_connection: self
                .settings_store
                .settings()
                .new_connection
                .save_connection,
            ..NewConnectionForm::default()
        });
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(in crate::workspace) fn open_serial_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_new_connection_form(window, cx);
        if let Some(form) = self.new_connection_form.as_mut() {
            form.transport = NewConnectionTransport::Serial;
            form.focused_field = super::form_state::NewConnectionField::SerialPortPath;
            form.field_focused = false;
        }
        self.refresh_serial_ports(cx);
    }

    pub(in crate::workspace) fn open_telnet_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_new_connection_form(window, cx);
        if let Some(form) = self.new_connection_form.as_mut() {
            form.transport = NewConnectionTransport::Telnet;
            form.port = super::form_state::TELNET_DEFAULT_PORT_TEXT.to_string();
            form.focused_field = super::form_state::NewConnectionField::Host;
            form.field_focused = false;
        }
    }

    pub(in crate::workspace) fn open_drill_down_form(
        &mut self,
        parent_node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parent_ready = self
            .node_runtime_store
            .snapshot(&parent_node_id)
            .is_some_and(|snapshot| snapshot.state.readiness == NodeReadiness::Ready);
        if !parent_ready {
            self.session_manager.status = Some(format!(
                "{}: {}",
                self.i18n.t("sessions.tree.actions.drill_in"),
                self.i18n.t("ssh.drill_down.parent_not_ready")
            ));
            cx.notify();
            return;
        }

        self.prepare_modal_interaction_boundary();
        let mut form = NewConnectionForm {
            auth_tab: SshAuthTab::Agent,
            focused_field: super::form_state::NewConnectionField::Host,
            save_connection: false,
            group: self.i18n.t("ssh.form.ungrouped"),
            agent_available: detect_ssh_agent_available(),
            ..NewConnectionForm::default()
        };
        form.username = String::new();
        self.new_connection_form = Some(form);
        self.drill_down_parent_node_id = Some(parent_node_id);
        self.editing_saved_connection_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(in crate::workspace) fn connect_saved_connection_as_next_hop(
        &mut self,
        parent_node_id: NodeId,
        saved_connection_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parent_ready = self
            .node_runtime_store
            .snapshot(&parent_node_id)
            .is_some_and(|snapshot| snapshot.state.readiness == NodeReadiness::Ready);
        if !parent_ready {
            self.report_saved_next_hop_error("sessions.saved_next_hop.parent_not_ready", cx);
            return;
        }

        let Some(connection) = self.connection_store.get(&saved_connection_id).cloned() else {
            self.report_saved_next_hop_error("sessions.saved_next_hop.not_found", cx);
            return;
        };
        let Some(mut config) = ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &connection,
        ) else {
            self.report_saved_next_hop_error("sessions.saved_next_hop.missing_credentials", cx);
            return;
        };
        if let Err(error) = prepare_tree_connect_config(&mut config) {
            self.report_saved_next_hop_message(error, cx);
            return;
        }

        // Saved next-hop reuse still belongs to the native SessionTree path:
        // materialize the saved target under the live parent, then let
        // NodeRouter connect through that ancestry.
        let expansion = match self.expand_saved_connection_tree_under_parent(
            parent_node_id.clone(),
            &saved_connection_id,
            config,
            connection.name.clone(),
        ) {
            Ok(expansion) => expansion,
            Err(error) => {
                let message = format!(
                    "{}: {error}",
                    self.i18n.t("sessions.saved_next_hop.materialize_failed")
                );
                self.report_saved_next_hop_message(message, cx);
                return;
            }
        };

        let target_node_id = expansion.target_node_id.clone();
        if let Some(node) = self.ssh_nodes.get_mut(&target_node_id) {
            node.readiness = NodeReadiness::Connecting;
        }
        self.active_ssh_node_id = Some(target_node_id.clone());
        self.host_key_challenge = None;
        self.new_connection_form = None;
        self.drill_down_parent_node_id = None;
        self.duplicating_saved_connection_id = None;
        self.close_new_connection_select();
        self.session_manager.status = Some(self.i18n.t("ssh.drill_down.connecting"));
        self.ensure_node_connection_started(&target_node_id);
        let _ = self.connection_store.mark_used(&saved_connection_id);
        self.persist_session_tree_snapshot();
        cx.notify();
    }

    fn report_saved_next_hop_error(&mut self, i18n_key: &str, cx: &mut Context<Self>) {
        self.report_saved_next_hop_message(self.i18n.t(i18n_key), cx);
    }

    fn report_saved_next_hop_message(&mut self, message: String, cx: &mut Context<Self>) {
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
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn runtime_proxy_hops_for_parent_path(
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
        self.new_connection_form = None;
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.close_new_connection_select();
        self.host_key_challenge = None;
        self.cancel_active_proxy_connect_run();
        self.cancel_keyboard_interactive_challenge(cx);
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

    fn save_new_connection_without_connecting(
        &mut self,
        drill_down_parent_id: Option<&NodeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.save_current_connection_form(drill_down_parent_id, cx) {
            self.close_new_connection_form(window, cx);
        }
    }

    fn save_current_connection_form(
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

    fn ensure_new_connection_save_name_is_unique(
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

    fn save_request_for_current_form(
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

    fn submit_serial_connection_form(
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

    fn submit_telnet_connection_form(
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
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = Some(action);
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
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
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn submit_saved_connection_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn save_editing_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        match save_request_from_form_with_existing_auth(form, Some(id), existing_auth.as_ref()) {
            Ok(mut request) => {
                if form.proxy_hops.is_empty()
                    && let Some(connection) = existing_connection.as_ref()
                {
                    request.proxy_chain = connection.proxy_chain.clone();
                }
                match self.connection_store.upsert(request) {
                    Ok(_) => {
                        self.new_connection_form = None;
                        self.editing_saved_connection_id = None;
                        self.duplicating_saved_connection_id = None;
                        self.close_new_connection_select();
                        self.session_manager.status =
                            Some(self.i18n.t("sessionManager.edit_properties.save"));
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

    fn save_duplicate_connection_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let worker_config = config.clone();
        let worker_title = title.clone();
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

    fn save_after_open_request_for_connect_intent(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<Option<SaveConnectionRequest>, ()> {
        let mode = new_connection_form_mode(
            self.editing_saved_connection_id.as_deref(),
            self.duplicating_saved_connection_id.as_deref(),
            self.saved_connection_prompt_action,
        );
        if !mode.stores_connection_on_connect()
            || !self
                .new_connection_form
                .as_ref()
                .is_some_and(|form| form.save_connection)
        {
            return Ok(None);
        }

        match self
            .new_connection_form
            .as_ref()
            .map(|form| save_request_from_form(form, None))
        {
            Some(Ok(request)) => Ok(Some(request)),
            Some(Err(error)) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(error.to_string());
                }
                cx.notify();
                Err(())
            }
            None => Ok(None),
        }
    }

    fn build_new_connection_config(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(SshConfig, String)> {
        let Some(form) = self.new_connection_form.as_mut() else {
            return None;
        };
        let host = form.host.trim().to_string();
        let username = form.username.trim().to_string();
        let port = form.port.trim().parse::<u16>().ok();
        if host.is_empty() || username.is_empty() || port.is_none() {
            form.error = Some(self.i18n.t("ssh.form.validation_required"));
            cx.notify();
            return None;
        }
        if !form.proxy_hops.is_empty() && form.auth_tab == SshAuthTab::TwoFactor {
            form.error = Some(self.i18n.t("ssh.form.proxy_chain_kbi_unsupported"));
            cx.notify();
            return None;
        }
        if form
            .proxy_hops
            .iter()
            .any(|hop| hop.complete() && hop.auth_tab == SshAuthTab::TwoFactor)
        {
            form.error = Some(
                self.i18n
                    .t("sessionManager.toast.proxy_hop_kbi_unsupported"),
            );
            cx.notify();
            return None;
        }

        let auth = match form.auth_tab {
            SshAuthTab::Password => {
                // UI inputs own plain String drafts; crossing into SSH auth moves a
                // zeroizing clone so backend tasks never retain a normal String password.
                AuthMethod::password_secret(zeroizing_secret_clone(&form.password))
            }
            SshAuthTab::Agent => AuthMethod::Agent,
            SshAuthTab::DefaultKey => {
                AuthMethod::key_secret("", zeroizing_non_empty_secret(&form.passphrase))
            }
            SshAuthTab::SshKey => {
                if form.key_path.trim().is_empty() {
                    form.error = Some(self.i18n.t("ssh.form.key_path_required"));
                    cx.notify();
                    return None;
                }
                AuthMethod::key_secret(
                    form.key_path.trim().to_string(),
                    zeroizing_non_empty_secret(&form.passphrase),
                )
            }
            SshAuthTab::ManagedKey => {
                if form.managed_key_id.trim().is_empty() {
                    form.error = Some(self.i18n.t("ssh.form.managed_key_required"));
                    cx.notify();
                    return None;
                }
                // The connection config carries only the managed-key reference; the
                // private key remains owned by the local managed keychain resolver.
                AuthMethod::managed_key_secret(
                    form.managed_key_id.trim().to_string(),
                    zeroizing_non_empty_secret(&form.passphrase),
                )
            }
            SshAuthTab::Certificate => {
                if form.key_path.trim().is_empty() || form.cert_path.trim().is_empty() {
                    form.error = Some(self.i18n.t("ssh.form.certificate_paths_required"));
                    cx.notify();
                    return None;
                }
                AuthMethod::certificate_secret(
                    form.key_path.trim().to_string(),
                    form.cert_path.trim().to_string(),
                    zeroizing_non_empty_secret(&form.passphrase),
                )
            }
            SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
        };
        let proxy_chain = match proxy_chain_from_form(form) {
            Ok(proxy_chain) => proxy_chain,
            Err(error) => {
                form.error = Some(error);
                cx.notify();
                return None;
            }
        };
        let upstream_proxy = match upstream_proxy_config_from_form(
            &self.connection_store,
            self.settings_store.settings(),
            form,
        ) {
            Ok(upstream_proxy) => upstream_proxy,
            Err(error) => {
                form.error = Some(error.to_string());
                cx.notify();
                return None;
            }
        };
        let config = SshConfig {
            host: host.clone(),
            port: port.unwrap_or(22),
            username: username.clone(),
            auth,
            agent_forwarding: form.agent_forwarding,
            proxy_chain,
            upstream_proxy,
            strict_host_key_checking: true,
            post_connect_command: (!form.post_connect_command.trim().is_empty())
                .then(|| form.post_connect_command.trim().to_string()),
            ..SshConfig::default()
        };
        let title = if form.name.trim().is_empty() {
            format!("{username}@{host}")
        } else {
            form.name.trim().to_string()
        };
        Some((config, title))
    }

    pub(in crate::workspace) fn poll_ssh_worker_results(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut results = Vec::new();
        loop {
            match self.ssh_worker_rx.try_recv() {
                Ok(result) => results.push(result),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
            }
        }

        for result in results {
            match result {
                SshConnectionWorkerResult::Preflight {
                    config,
                    title,
                    intent,
                    status,
                } => self.handle_ssh_preflight_result(config, title, intent, status, window, cx),
                SshConnectionWorkerResult::SessionTreePreflight { run, status } => {
                    self.handle_session_tree_preflight_result(run, status, window, cx)
                }
                SshConnectionWorkerResult::Test { result } => {
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.pending = false;
                        form.error = Some(match result {
                            Ok(()) => self.i18n.t("ssh.form.test_success"),
                            Err(error) => error,
                        });
                    } else {
                        self.session_manager.status = Some(match result {
                            Ok(()) => self.i18n.t("sessionManager.toast.test_success"),
                            Err(error) => format!(
                                "{}: {error}",
                                self.i18n.t("sessionManager.toast.test_failed")
                            ),
                        });
                    }
                    cx.notify();
                }
                SshConnectionWorkerResult::KeyboardInteractivePrompt {
                    request,
                    response_tx,
                } => {
                    self.open_keyboard_interactive_challenge(request, response_tx, window, cx);
                }
            }
        }
    }

    fn handle_ssh_preflight_result(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        status: HostKeyStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = None;
        }

        match status {
            HostKeyStatus::Verified => {
                self.continue_verified_ssh_flow(config, title, intent, window, cx)
            }
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                self.prepare_modal_interaction_boundary();
                let host = config.host.clone();
                let port = config.port;
                self.host_key_challenge = Some(HostKeyChallenge {
                    config,
                    title,
                    status,
                    intent,
                    session_tree_challenge: None,
                    host,
                    port,
                });
                self.needs_active_pane_focus = false;
                cx.notify();
            }
            HostKeyStatus::Error { message } => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(message);
                } else {
                    self.session_manager.status = Some(message);
                }
                cx.notify();
            }
        }
    }

    fn start_proxy_session_tree_connect(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        save_after_open: Option<SaveConnectionRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_proxy_connect_run.is_some() {
            self.report_proxy_session_tree_error(
                "CHAIN_LOCK_BUSY: Another connection chain is in progress".to_string(),
                cx,
            );
            return;
        }
        let upstream_proxy = config.upstream_proxy.clone();
        let endpoints = proxy_session_tree_endpoints(&config);
        let expansion_id = match &intent {
            SshConnectionIntent::ConnectSaved(id) => id.clone(),
            _ => format!("manual-{}", self.next_ssh_node_id),
        };
        let expansion =
            match self.expand_saved_connection_tree(&expansion_id, config, title.clone()) {
                Ok(expansion) => expansion,
                Err(error) => {
                    self.report_proxy_session_tree_error(error.to_string(), cx);
                    return;
                }
            };
        let cleanup_node_id = Some(expansion.target_node_id.clone());
        let plan = match NativeSessionTreeConnectPlan::from_expansion(
            &expansion,
            endpoints,
            cleanup_node_id,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                self.report_proxy_session_tree_error(error, cx);
                return;
            }
        };
        self.active_proxy_connect_run = Some(NativeProxyConnectRun {
            plan,
            title,
            intent,
            save_after_open,
            upstream_proxy,
        });
        self.continue_active_proxy_session_tree_connect(window, cx);
    }

    pub(in crate::workspace) fn start_existing_session_tree_connect(
        &mut self,
        target_node_id: NodeId,
        title: String,
        intent: SshConnectionIntent,
        save_after_open: Option<SaveConnectionRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.active_proxy_connect_run.is_some() || self.active_connection_chain.is_some() {
            self.report_proxy_session_tree_error(
                "CHAIN_LOCK_BUSY: Another connection chain is in progress".to_string(),
                cx,
            );
            return true;
        }

        let Ok(path_node_ids) = self.node_runtime_store.path_to_node(&target_node_id) else {
            self.report_proxy_session_tree_error(
                format!("Node path not found for {}", target_node_id.0),
                cx,
            );
            return true;
        };
        if path_node_ids.len() <= 1 {
            return false;
        }

        let start_index = path_node_ids
            .iter()
            .position(|candidate| !self.connection_trace_node_is_ready(candidate))
            .unwrap_or(path_node_ids.len());
        let nodes_to_connect = path_node_ids[start_index..].to_vec();
        if nodes_to_connect.is_empty() {
            return false;
        }
        if nodes_to_connect
            .iter()
            .any(|node_id| self.connecting_node_locks.contains(node_id))
        {
            self.report_proxy_session_tree_error(
                "NODE_LOCK_BUSY: Node is already connecting".to_string(),
                cx,
            );
            return true;
        }

        let mut endpoints = Vec::with_capacity(nodes_to_connect.len());
        for node_id in &nodes_to_connect {
            let Some(node) = self.ssh_nodes.get(node_id) else {
                self.report_proxy_session_tree_error(
                    format!("SSH node {} not found", node_id.0),
                    cx,
                );
                return true;
            };
            endpoints.push(NativeSessionTreeConnectEndpoint::new(
                node.config.host.clone(),
                node.config.port,
            ));
        }

        let upstream_proxy = nodes_to_connect.first().and_then(|node_id| {
            self.node_runtime_store
                .snapshot(node_id)
                .filter(|snapshot| snapshot.parent_id.is_none())
                .and_then(|_| {
                    self.ssh_nodes
                        .get(node_id)
                        .and_then(|node| node.config.upstream_proxy.clone())
                })
        });
        let expansion = NodeTreeExpansion {
            target_node_id: target_node_id.clone(),
            path_node_ids: nodes_to_connect,
            chain_depth: path_node_ids.len() as u32,
        };
        let plan = match NativeSessionTreeConnectPlan::from_expansion(&expansion, endpoints, None) {
            Ok(plan) => plan,
            Err(error) => {
                self.report_proxy_session_tree_error(error, cx);
                return true;
            }
        };

        self.active_proxy_connect_run = Some(NativeProxyConnectRun {
            plan,
            title,
            intent,
            save_after_open,
            upstream_proxy,
        });
        self.continue_active_proxy_session_tree_connect(window, cx);
        true
    }

    fn handle_session_tree_preflight_result(
        &mut self,
        run: NativeProxyConnectRun,
        status: HostKeyStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.active_proxy_connect_result_is_current(&run) {
            return;
        }
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = None;
        }

        match status {
            HostKeyStatus::Verified => {
                self.mark_current_proxy_connect_step_verified(cx);
                self.continue_active_proxy_session_tree_connect(window, cx);
            }
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                let Some(active_run) = self.active_proxy_connect_run.as_ref() else {
                    return;
                };
                let Ok(challenge) = active_run.plan.challenge_for_current_step(status) else {
                    return;
                };
                let title = active_run.title.clone();
                let intent = active_run.intent.clone();
                self.prepare_modal_interaction_boundary();
                self.host_key_challenge = Some(HostKeyChallenge {
                    config: SshConfig::default(),
                    title,
                    status: challenge.status.clone(),
                    intent,
                    session_tree_challenge: Some(challenge.clone()),
                    host: challenge.step.host,
                    port: challenge.step.port,
                });
                self.needs_active_pane_focus = false;
                cx.notify();
            }
            HostKeyStatus::Error { message } => {
                self.cancel_active_proxy_connect_run();
                self.report_proxy_session_tree_error(message, cx);
            }
        }
    }

    pub(in crate::workspace) fn continue_active_proxy_session_tree_connect(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self.active_proxy_connect_run.clone() else {
            return;
        };
        match run.plan.next_action() {
            NativeSessionTreeConnectAction::Preflight { step } => {
                self.start_session_tree_step_preflight(run, step, cx);
            }
            NativeSessionTreeConnectAction::Connect { step } => {
                self.connect_session_tree_step(step, window, cx);
            }
            NativeSessionTreeConnectAction::Complete { target_node_id } => {
                self.finish_proxy_session_tree_connect(target_node_id, run, window, cx);
            }
        }
    }

    pub(in crate::workspace) fn continue_active_proxy_session_tree_preflight_only(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self.active_proxy_connect_run.clone() else {
            return;
        };
        match run.plan.next_action() {
            NativeSessionTreeConnectAction::Preflight { step } => {
                self.start_session_tree_step_preflight(run, step, cx);
            }
            _ => self.report_proxy_session_tree_error(
                "proxy connect plan is not waiting for host-key preflight".to_string(),
                cx,
            ),
        }
    }

    fn start_session_tree_step_preflight(
        &mut self,
        run: NativeProxyConnectRun,
        step: NativeSessionTreeConnectStep,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = true;
            form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
        } else {
            self.session_manager.status = Some(self.i18n.t("ssh.form.checking_host_key"));
        }
        let tx = self.ssh_worker_tx.clone();
        let router = self.node_router.clone();
        let runtime_store = self.node_runtime_store.clone();
        std::thread::spawn(move || {
            let root_upstream_proxy = run.upstream_proxy.clone();
            let status = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime.block_on(async move {
                    match runtime_store
                        .snapshot(&step.node_id)
                        .and_then(|snapshot| snapshot.parent_id)
                    {
                        Some(parent_id) => {
                            let consumer = ConnectionConsumer::NodeRouter(format!(
                                "{}:preflight",
                                step.node_id.0
                            ));
                            match router
                                .acquire_connection_wait(
                                    &parent_id,
                                    consumer.clone(),
                                    Duration::from_secs(30),
                                )
                                .await
                            {
                                Ok(parent) => {
                                    let connection_id = parent.connection_id.clone();
                                    let status = parent
                                        .handle
                                        .preflight_host_key_via_direct_tcpip(
                                            &step.host, step.port, 10,
                                        )
                                        .await;
                                    router.release_consumer(&connection_id, &consumer);
                                    status
                                }
                                Err(error) => HostKeyStatus::Error {
                                    message: error.to_string(),
                                },
                            }
                        }
                        None => {
                            // The root ProxyJump step uses the same initial TCP outlet
                            // as the eventual SSH connection; child steps keep using
                            // parent direct-tcpip streams.
                            check_host_key_with_upstream_proxy(
                                &step.host,
                                step.port,
                                10,
                                root_upstream_proxy.as_ref(),
                            )
                            .await
                        }
                    }
                }),
                Err(error) => HostKeyStatus::Error {
                    message: format!("failed to initialize SSH runtime: {error}"),
                },
            };
            let _ = tx.send(SshConnectionWorkerResult::SessionTreePreflight { run, status });
        });
        cx.notify();
    }

    fn connect_session_tree_step(
        &mut self,
        step: NativeSessionTreeConnectStep,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.connection_trace_node_is_ready(&step.node_id) {
            if let Some(run) = self.active_proxy_connect_run.as_mut() {
                run.plan.advance_after_connected_step();
            }
            self.continue_active_proxy_session_tree_connect(_window, cx);
            cx.notify();
            return;
        }

        self.apply_session_tree_step_host_key_options(&step);
        if !self.ensure_node_connection_started_without_ancestors(&step.node_id) {
            self.cancel_active_proxy_connect_run();
            self.report_proxy_session_tree_error(
                format!("failed to start SSH node {}", step.node_id.0),
                cx,
            );
        }
    }

    fn finish_proxy_session_tree_connect(
        &mut self,
        target_node_id: NodeId,
        run: NativeProxyConnectRun,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_proxy_connect_run = None;
        self.host_key_challenge = None;
        self.release_proxy_session_tree_locks(&run.plan);
        let Some(target_config) = self
            .node_runtime_store
            .snapshot(&target_node_id)
            .map(|snapshot| snapshot.config)
        else {
            self.report_proxy_session_tree_error(
                "target node was not materialized".to_string(),
                cx,
            );
            return;
        };

        match run.intent {
            SshConnectionIntent::Connect => {
                self.new_connection_form = None;
                self.duplicating_saved_connection_id = None;
                self.close_new_connection_select();
                let post_connect_command = target_config.post_connect_command.clone();
                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    target_node_id,
                    post_connect_command,
                    target_config,
                    run.title,
                    None,
                    None,
                    run.save_after_open,
                    window,
                    cx,
                );
            }
            SshConnectionIntent::ConnectSaved(id) => {
                if self.saved_connection_prompt_action.is_some() {
                    self.new_connection_form = None;
                    self.editing_saved_connection_id = None;
                    self.duplicating_saved_connection_id = None;
                    self.saved_connection_prompt_action = None;
                    self.close_new_connection_select();
                }
                self.session_manager.status = None;
                let post_connect_command = target_config.post_connect_command.clone();
                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    target_node_id,
                    post_connect_command,
                    target_config,
                    run.title,
                    Some(id.clone()),
                    Some(id),
                    None,
                    window,
                    cx,
                );
            }
            SshConnectionIntent::Test | SshConnectionIntent::DrillDown(_) => {}
        }
    }

    fn active_proxy_connect_result_is_current(&self, run: &NativeProxyConnectRun) -> bool {
        self.active_proxy_connect_run
            .as_ref()
            .is_some_and(|active| {
                active.plan.target_node_id == run.plan.target_node_id
                    && active.plan.current_index == run.plan.current_index
            })
    }

    pub(in crate::workspace) fn active_proxy_connect_waits_for_node(
        &self,
        node_id: &NodeId,
    ) -> bool {
        self.active_proxy_connect_run
            .as_ref()
            .and_then(|run| run.plan.steps.get(run.plan.current_index))
            .is_some_and(|step| &step.node_id == node_id)
    }

    pub(in crate::workspace) fn advance_active_proxy_connect_after_node_connected(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.active_proxy_connect_waits_for_node(node_id) {
            return;
        }
        if let Some(run) = self.active_proxy_connect_run.as_mut() {
            run.plan.advance_after_connected_step();
        }
        self.continue_active_proxy_session_tree_connect(window, cx);
    }

    pub(in crate::workspace) fn fail_active_proxy_connect_for_node(
        &mut self,
        node_id: &NodeId,
        error: String,
        cx: &mut Context<Self>,
    ) {
        if self.active_proxy_connect_waits_for_node(node_id) {
            self.cancel_active_proxy_connect_run();
            self.report_proxy_session_tree_error(error, cx);
        }
    }

    pub(in crate::workspace) fn accept_active_proxy_connect_host_key(
        &mut self,
        persist: bool,
        fingerprint: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self.active_proxy_connect_run.as_mut() else {
            return;
        };
        if let Err(error) = run.plan.accept_current_host_key(persist, fingerprint) {
            self.report_proxy_session_tree_error(error, cx);
            return;
        }
        self.continue_active_proxy_session_tree_connect(window, cx);
    }

    fn mark_current_proxy_connect_step_verified(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.active_proxy_connect_run.as_mut() else {
            return;
        };
        if let Err(error) = run.plan.mark_current_preflight_verified() {
            self.report_proxy_session_tree_error(error, cx);
        }
    }

    fn apply_session_tree_step_host_key_options(&mut self, step: &NativeSessionTreeConnectStep) {
        let Some(trust_host_key) = step.trust_host_key else {
            return;
        };
        let Some(fingerprint) = step.expected_host_key_fingerprint.clone() else {
            return;
        };
        if let Some(node) = self.ssh_nodes.get_mut(&step.node_id) {
            node.config.strict_host_key_checking = true;
            node.config.trust_host_key = Some(trust_host_key);
            node.config.expected_host_key_fingerprint = Some(fingerprint);
            let origin = self
                .node_runtime_store
                .snapshot(&step.node_id)
                .map(|snapshot| snapshot.origin)
                .unwrap_or_default();
            // Tauri passes host-key acceptance as connectNode options. Native
            // stores the same one-step options on the node config immediately
            // before starting connect_tree_node.
            self.node_runtime_store.upsert_node_with_origin(
                step.node_id.clone(),
                node.config.clone(),
                origin,
            );
        }
    }

    pub(in crate::workspace) fn cancel_active_proxy_connect_run(&mut self) {
        let Some(run) = self.active_proxy_connect_run.take() else {
            return;
        };
        self.cleanup_proxy_session_tree_run(&run);
    }

    fn cleanup_proxy_session_tree_run(&mut self, run: &NativeProxyConnectRun) {
        self.cleanup_proxy_session_tree_plan(&run.plan);
    }

    fn cleanup_proxy_session_tree_plan(&mut self, plan: &NativeSessionTreeConnectPlan) {
        self.release_proxy_session_tree_locks(plan);
        let Some(cleanup_root) = plan.cleanup_root_node_id() else {
            return;
        };
        let nodes_to_cleanup = self.node_runtime_store.subtree_postorder(&cleanup_root);
        for node_id in &nodes_to_cleanup {
            self.cancel_connection_trace_for_node(node_id);
            self.connecting_node_locks.remove(node_id);
            self.remove_pending_ssh_terminal_opens_for_node(node_id);
            if let Some(connection_id) = self.node_router.connection_id_for_node(node_id) {
                let node_consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
                self.ssh_registry.release(&connection_id, &node_consumer);
                self.release_parent_ref_for_child_connection(node_id, &connection_id);
                if let Some(handle) = self.ssh_registry.get(&connection_id) {
                    let runtime = self.forwarding_runtime.clone();
                    runtime.spawn(async move {
                        handle.clear_physical().await;
                    });
                }
                let _ = self
                    .ssh_registry
                    .mark_state(&connection_id, ConnectionState::Disconnected);
                self.node_router.emitter().unregister(&connection_id);
                let _ = self.ssh_registry.retire_connection(&connection_id);
            }
        }

        // Tauri removes the temporary manual proxy expansion on cancel/failure.
        // Native stores that expansion in both NodeRuntimeStore and the
        // UI-facing node maps, so cleanup has to remove both owners.
        let removed_nodes = self.node_router.remove_runtime_subtree(&cleanup_root);
        for node_id in removed_nodes {
            self.ssh_nodes.remove(&node_id);
            self.expanded_ssh_nodes.remove(&node_id);
            self.saved_ssh_nodes
                .retain(|_saved_id, saved_node_id| saved_node_id != &node_id);
        }
        self.persist_session_tree_snapshot();
    }

    fn release_proxy_session_tree_locks(&mut self, plan: &NativeSessionTreeConnectPlan) {
        for step in &plan.steps {
            self.connecting_node_locks.remove(&step.node_id);
        }
    }

    fn report_proxy_session_tree_error(&mut self, error: String, cx: &mut Context<Self>) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = Some(error);
        } else {
            self.session_manager.status = Some(error);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn start_ssh_test_flow(
        &mut self,
        mut config: SshConfig,
        title: String,
        cx: &mut Context<Self>,
    ) {
        if config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
        {
            prepare_proxy_chain_test_config(&mut config);
            self.start_ssh_test(config, cx);
            return;
        }

        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = true;
            form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
        } else {
            self.session_manager.status = Some(self.i18n.t("ssh.form.checking_host_key"));
        }
        self.start_ssh_preflight(config, title, SshConnectionIntent::Test);
        cx.notify();
    }

    pub(in crate::workspace) fn continue_verified_ssh_flow(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match intent {
            SshConnectionIntent::Connect => {
                let mode = new_connection_form_mode(
                    self.editing_saved_connection_id.as_deref(),
                    self.duplicating_saved_connection_id.as_deref(),
                    self.saved_connection_prompt_action,
                );
                let save_after_open = if mode.stores_connection_on_connect()
                    && self
                        .new_connection_form
                        .as_ref()
                        .is_some_and(|form| form.save_connection)
                {
                    match self
                        .new_connection_form
                        .as_ref()
                        .map(|form| save_request_from_form(form, None))
                    {
                        Some(Ok(request)) => Some(request),
                        Some(Err(error)) => {
                            if let Some(form) = self.new_connection_form.as_mut() {
                                form.error = Some(error.to_string());
                            }
                            cx.notify();
                            return;
                        }
                        None => return,
                    }
                } else {
                    None
                };
                self.new_connection_form = None;
                self.duplicating_saved_connection_id = None;
                self.host_key_challenge = None;
                self.close_new_connection_select();
                if config
                    .proxy_chain
                    .as_ref()
                    .is_some_and(|chain| !chain.is_empty())
                {
                    let expansion_id = format!("manual-{}", self.next_ssh_node_id);
                    match self.expand_saved_connection_tree(&expansion_id, config, title.clone()) {
                        Ok(expansion) => {
                            if let Some(target_config) = self
                                .node_runtime_store
                                .snapshot(&expansion.target_node_id)
                                .map(|snapshot| snapshot.config)
                            {
                                let post_connect_command =
                                    target_config.post_connect_command.clone();
                                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                                    expansion.target_node_id,
                                    post_connect_command,
                                    target_config,
                                    title,
                                    None,
                                    None,
                                    save_after_open,
                                    window,
                                    cx,
                                );
                            }
                        }
                        Err(error) => {
                            self.session_manager.status = Some(error.to_string());
                        }
                    }
                    return;
                }
                let node_id = self.materialize_ssh_root_node(config.clone(), title.clone(), None);
                let post_connect_command = config.post_connect_command.clone();
                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    node_id,
                    post_connect_command,
                    config,
                    title,
                    None,
                    None,
                    save_after_open,
                    window,
                    cx,
                );
            }
            SshConnectionIntent::ConnectSaved(id) => {
                self.host_key_challenge = None;
                if self.saved_connection_prompt_action.is_some() {
                    self.new_connection_form = None;
                    self.editing_saved_connection_id = None;
                    self.duplicating_saved_connection_id = None;
                    self.saved_connection_prompt_action = None;
                    self.close_new_connection_select();
                }
                self.session_manager.status = None;
                let _ = self.open_or_create_saved_ssh_terminal_tab(id, config, title, window, cx);
            }
            SshConnectionIntent::DrillDown(parent_id) => {
                self.host_key_challenge = None;
                let child_id = match self
                    .node_router
                    .drill_down_node(parent_id.clone(), config.clone())
                {
                    Ok(child_id) => child_id,
                    Err(error) => {
                        if let Some(form) = self.new_connection_form.as_mut() {
                            form.pending = false;
                            form.error = Some(error.to_string());
                        } else {
                            self.session_manager.status = Some(error.to_string());
                        }
                        cx.notify();
                        return;
                    }
                };
                self.ssh_nodes.insert(
                    child_id.clone(),
                    crate::workspace::WorkspaceSshNode {
                        saved_connection_id: None,
                        config,
                        title,
                        terminal_ids: Vec::new(),
                        readiness: NodeReadiness::Connecting,
                    },
                );
                self.expanded_ssh_nodes.insert(parent_id);
                self.expanded_ssh_nodes.insert(child_id.clone());
                self.active_ssh_node_id = Some(child_id.clone());
                self.new_connection_form = None;
                self.drill_down_parent_node_id = None;
                self.duplicating_saved_connection_id = None;
                self.close_new_connection_select();
                self.session_manager.status = Some(self.i18n.t("ssh.drill_down.connecting"));
                self.ensure_node_connection_started(&child_id);
                self.persist_session_tree_snapshot();
            }
            SshConnectionIntent::Test => self.start_ssh_test(config, cx),
        }
    }

    pub(in crate::workspace) fn start_ssh_test(
        &mut self,
        config: SshConfig,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = true;
            form.error = Some(self.i18n.t("ssh.form.test_running"));
        } else {
            self.session_manager.status = Some(self.i18n.t("ssh.form.test_running"));
        }
        let tx = self.ssh_worker_tx.clone();
        let managed_key_resolver = managed_key_resolver_from_store(&self.connection_store);
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(runtime) => {
                    let prompt_handler = Arc::new(NativeSshPromptHandler::new(tx.clone()));
                    runtime
                        .block_on(
                            SshTransportClient::new(config)
                                .with_prompt_handler(prompt_handler)
                                .with_managed_key_resolver(managed_key_resolver)
                                .test_connection(),
                        )
                        .map_err(|error| error.to_string())
                }
                Err(error) => Err(format!("failed to initialize SSH runtime: {error}")),
            };
            let _ = tx.send(SshConnectionWorkerResult::Test { result });
        });
        cx.notify();
    }
}

fn detect_ssh_agent_available() -> Option<bool> {
    let sock = std::env::var_os("SSH_AUTH_SOCK")?;
    Some(!sock.is_empty() && std::path::Path::new(&sock).exists())
}

fn proxy_chain_from_form(
    form: &mut NewConnectionForm,
) -> Result<Option<Vec<ProxyHopConfig>>, String> {
    if form.proxy_hops.is_empty() {
        return Ok(None);
    }

    let mut chain = Vec::new();
    for hop in form.proxy_hops.iter().filter(|hop| hop.complete()) {
        if hop.auth_tab == SshAuthTab::TwoFactor {
            return Err("Proxy hop does not support keyboard-interactive/2FA".to_string());
        }
        if hop.auth_tab == SshAuthTab::ManagedKey && hop.managed_key_id.trim().is_empty() {
            return Err("Proxy hop managed key is required".to_string());
        }
        chain.push(ProxyHopConfig {
            host: hop.host.trim().to_string(),
            port: hop.port.trim().parse::<u16>().unwrap_or(22),
            username: hop.username.trim().to_string(),
            auth: auth_method_from_proxy_hop(hop),
            agent_forwarding: hop.agent_forwarding,
            strict_host_key_checking: true,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
        });
    }

    Ok(Some(chain))
}

fn proxy_session_tree_endpoints(config: &SshConfig) -> Vec<NativeSessionTreeConnectEndpoint> {
    let mut endpoints = config
        .proxy_chain
        .as_ref()
        .map(|chain| {
            chain
                .iter()
                .map(|hop| NativeSessionTreeConnectEndpoint::new(hop.host.clone(), hop.port))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    endpoints.push(NativeSessionTreeConnectEndpoint::new(
        config.host.clone(),
        config.port,
    ));
    endpoints
}

fn prepare_proxy_chain_test_config(config: &mut SshConfig) {
    config.strict_host_key_checking = true;
    config.trust_host_key = Some(false);
    config.expected_host_key_fingerprint = None;

    if let Some(chain) = config.proxy_chain.as_mut() {
        for hop in chain {
            hop.strict_host_key_checking = true;
            hop.trust_host_key = Some(false);
            hop.expected_host_key_fingerprint = None;
        }
    }
}

fn prepare_tree_connect_config(config: &mut SshConfig) -> Result<(), String> {
    // Tauri resolves `default_key` to the first existing default key before
    // adding/connecting SessionTree nodes, while test_connection keeps its own
    // dynamic loader. Native mirrors that split here.
    resolve_default_key_for_tree_auth(&mut config.auth)?;
    if let Some(chain) = config.proxy_chain.as_mut() {
        for hop in chain {
            resolve_default_key_for_tree_auth(&mut hop.auth)?;
        }
    }
    Ok(())
}

fn resolve_default_key_for_tree_auth(auth: &mut AuthMethod) -> Result<(), String> {
    match auth {
        AuthMethod::Key { key_path, .. } if key_path.trim().is_empty() => {
            *key_path = first_available_default_key_path().map_err(|error| error.to_string())?;
            Ok(())
        }
        _ => Ok(()),
    }
}

fn auth_method_from_proxy_hop(hop: &NewConnectionProxyHop) -> AuthMethod {
    match hop.auth_tab {
        SshAuthTab::Password => AuthMethod::password_secret(zeroizing_secret_clone(&hop.password)),
        SshAuthTab::DefaultKey => {
            AuthMethod::key_secret("", zeroizing_non_empty_secret(&hop.passphrase))
        }
        SshAuthTab::SshKey => AuthMethod::key_secret(
            hop.key_path.trim().to_string(),
            zeroizing_non_empty_secret(&hop.passphrase),
        ),
        SshAuthTab::ManagedKey => AuthMethod::managed_key_secret(
            hop.managed_key_id.trim().to_string(),
            zeroizing_non_empty_secret(&hop.passphrase),
        ),
        SshAuthTab::Certificate => AuthMethod::certificate_secret(
            hop.key_path.trim().to_string(),
            hop.cert_path.trim().to_string(),
            zeroizing_non_empty_secret(&hop.passphrase),
        ),
        SshAuthTab::Agent => AuthMethod::Agent,
        SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
    }
}

fn form_from_runtime_config(
    config: &SshConfig,
    title: Option<&str>,
    default_group: String,
) -> NewConnectionForm {
    let auth_fields = runtime_auth_form_fields(&config.auth);
    let mut form = NewConnectionForm {
        name: title
            .filter(|title| !title.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}@{}", config.username, config.host)),
        host: config.host.clone(),
        port: config.port.to_string(),
        username: config.username.clone(),
        auth_tab: auth_fields.auth_tab,
        password: auth_fields.password,
        key_path: auth_fields.key_path,
        managed_key_id: auth_fields.managed_key_id,
        cert_path: auth_fields.cert_path,
        passphrase: auth_fields.passphrase,
        group: default_group,
        post_connect_command: config.post_connect_command.clone().unwrap_or_default(),
        agent_forwarding: config.agent_forwarding,
        save_password: auth_fields.save_password,
        ..NewConnectionForm::default()
    };

    if let Some(chain) = &config.proxy_chain {
        form.proxy_hops = chain
            .iter()
            .cloned()
            .map(proxy_hop_form_from_runtime_config)
            .collect();
        form.proxy_chain_expanded = !form.proxy_hops.is_empty();
    }
    if let Some(proxy) = &config.upstream_proxy {
        form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Custom;
        form.upstream_proxy_protocol = match proxy.protocol {
            UpstreamProxyProtocol::Socks5 => SavedUpstreamProxyProtocol::Socks5,
            UpstreamProxyProtocol::HttpConnect => SavedUpstreamProxyProtocol::HttpConnect,
        };
        form.upstream_proxy_host = proxy.host.clone();
        form.upstream_proxy_port = proxy.port.to_string();
        form.upstream_proxy_remote_dns = proxy.remote_dns;
        form.upstream_proxy_no_proxy = proxy.no_proxy.clone();
        if let UpstreamProxyAuth::Password { username, password } = &proxy.auth {
            form.upstream_proxy_auth = NewConnectionUpstreamProxyAuth::Password;
            form.upstream_proxy_username = username.clone();
            form.upstream_proxy_password = password.as_str().to_string();
        }
    }
    form
}

fn proxy_hop_form_from_runtime_config(config: ProxyHopConfig) -> NewConnectionProxyHop {
    let auth_fields = runtime_auth_form_fields(&config.auth);
    NewConnectionProxyHop {
        host: config.host,
        port: config.port.to_string(),
        username: config.username,
        auth_tab: auth_fields.auth_tab,
        key_path: auth_fields.key_path,
        managed_key_id: auth_fields.managed_key_id,
        cert_path: auth_fields.cert_path,
        // Dynamic drill-down save-as must persist a usable proxy chain. Runtime
        // secrets are copied only after the user explicitly asks to save this
        // live path; the connection store then moves them into the keychain.
        password: auth_fields.password,
        passphrase: auth_fields.passphrase,
        agent_forwarding: config.agent_forwarding,
    }
}

struct RuntimeAuthFormFields {
    auth_tab: SshAuthTab,
    password: String,
    key_path: String,
    managed_key_id: String,
    cert_path: String,
    passphrase: String,
    save_password: bool,
}

fn runtime_auth_form_fields(auth: &AuthMethod) -> RuntimeAuthFormFields {
    match auth {
        AuthMethod::Password { password } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::Password,
            password: password.as_str().to_string(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: true,
        },
        AuthMethod::Key {
            key_path,
            passphrase,
        } if key_path.trim().is_empty() => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::DefaultKey,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: passphrase
                .as_ref()
                .map(|value| value.as_str().to_string())
                .unwrap_or_default(),
            save_password: false,
        },
        AuthMethod::Key {
            key_path,
            passphrase,
        } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::SshKey,
            password: String::new(),
            key_path: key_path.clone(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: passphrase
                .as_ref()
                .map(|value| value.as_str().to_string())
                .unwrap_or_default(),
            save_password: false,
        },
        AuthMethod::ManagedKey { key_id, passphrase } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::ManagedKey,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: key_id.clone(),
            cert_path: String::new(),
            passphrase: passphrase
                .as_ref()
                .map(|value| value.as_str().to_string())
                .unwrap_or_default(),
            save_password: false,
        },
        AuthMethod::Certificate {
            key_path,
            cert_path,
            passphrase,
        } => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::Certificate,
            password: String::new(),
            key_path: key_path.clone(),
            managed_key_id: String::new(),
            cert_path: cert_path.clone(),
            passphrase: passphrase
                .as_ref()
                .map(|value| value.as_str().to_string())
                .unwrap_or_default(),
            save_password: false,
        },
        AuthMethod::Agent => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::Agent,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: false,
        },
        AuthMethod::KeyboardInteractive => RuntimeAuthFormFields {
            auth_tab: SshAuthTab::TwoFactor,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: false,
        },
    }
}

#[cfg(test)]
mod runtime_save_tests {
    use super::*;
    use zeroize::Zeroizing;

    #[test]
    fn runtime_proxy_hop_form_preserves_password_for_save_as() {
        let hop = proxy_hop_form_from_runtime_config(ProxyHopConfig {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: AuthMethod::password_secret(Zeroizing::new("jump-secret".to_string())),
            agent_forwarding: true,
            strict_host_key_checking: true,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
        });

        assert_eq!(hop.auth_tab, SshAuthTab::Password);
        assert_eq!(hop.password, "jump-secret");
        assert!(hop.agent_forwarding);
    }

    #[test]
    fn runtime_proxy_hop_form_preserves_key_passphrase_for_save_as() {
        let hop = proxy_hop_form_from_runtime_config(ProxyHopConfig {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: AuthMethod::key_secret(
                "/home/ops/.ssh/id_ed25519",
                Some(Zeroizing::new("key-secret".to_string())),
            ),
            agent_forwarding: false,
            strict_host_key_checking: true,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
        });

        assert_eq!(hop.auth_tab, SshAuthTab::SshKey);
        assert_eq!(hop.key_path, "/home/ops/.ssh/id_ed25519");
        assert_eq!(hop.passphrase, "key-secret");
    }

    #[test]
    fn runtime_target_form_marks_password_for_persistence() {
        let form = form_from_runtime_config(
            &SshConfig {
                host: "target.example.com".to_string(),
                port: 22,
                username: "deploy".to_string(),
                auth: AuthMethod::password_secret(Zeroizing::new("target-secret".to_string())),
                ..SshConfig::default()
            },
            None,
            "Ungrouped".to_string(),
        );

        assert_eq!(form.auth_tab, SshAuthTab::Password);
        assert_eq!(form.password, "target-secret");
        assert!(form.save_password);
    }

    #[test]
    fn runtime_form_preserves_upstream_proxy_password_for_save_as() {
        let form = form_from_runtime_config(
            &SshConfig {
                host: "target.example.com".to_string(),
                port: 22,
                username: "deploy".to_string(),
                auth: AuthMethod::Agent,
                upstream_proxy: Some(oxideterm_ssh::UpstreamProxyConfig {
                    protocol: UpstreamProxyProtocol::Socks5,
                    host: "127.0.0.1".to_string(),
                    port: 1080,
                    auth: UpstreamProxyAuth::Password {
                        username: "proxy-user".to_string(),
                        password: Zeroizing::new("proxy-secret".to_string()),
                    },
                    remote_dns: true,
                    no_proxy: String::new(),
                }),
                ..SshConfig::default()
            },
            None,
            "Ungrouped".to_string(),
        );

        assert_eq!(
            form.upstream_proxy_auth,
            NewConnectionUpstreamProxyAuth::Password
        );
        assert_eq!(form.upstream_proxy_username, "proxy-user");
        assert_eq!(form.upstream_proxy_password, "proxy-secret");
    }
}

fn serial_profile_name_or_port(name: &str, port_path: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        port_path.to_string()
    } else {
        name.to_string()
    }
}

fn telnet_profile_name_or_endpoint(name: &str, host: &str, port: u16) -> String {
    let name = name.trim();
    if name.is_empty() {
        format!("{}:{}", host.trim(), port)
    } else {
        name.to_string()
    }
}

fn serial_profile_group_from_form(group: &str, i18n: &oxideterm_i18n::I18n) -> Option<String> {
    let group = group.trim();
    if group.is_empty()
        || group == "Ungrouped"
        || group == "未分组"
        || group == i18n.t("ssh.form.ungrouped")
        || group == i18n.t("sessionManager.edit_properties.ungrouped")
    {
        None
    } else {
        Some(group.to_string())
    }
}

fn serial_profile_parity_from_terminal(
    parity: oxideterm_terminal::SerialParity,
) -> oxideterm_connections::SerialParity {
    match parity {
        oxideterm_terminal::SerialParity::None => oxideterm_connections::SerialParity::None,
        oxideterm_terminal::SerialParity::Odd => oxideterm_connections::SerialParity::Odd,
        oxideterm_terminal::SerialParity::Even => oxideterm_connections::SerialParity::Even,
    }
}

fn serial_profile_flow_from_terminal(
    flow: oxideterm_terminal::SerialFlowControl,
) -> oxideterm_connections::SerialFlowControl {
    match flow {
        oxideterm_terminal::SerialFlowControl::None => {
            oxideterm_connections::SerialFlowControl::None
        }
        oxideterm_terminal::SerialFlowControl::Software => {
            oxideterm_connections::SerialFlowControl::Software
        }
        oxideterm_terminal::SerialFlowControl::Hardware => {
            oxideterm_connections::SerialFlowControl::Hardware
        }
    }
}

fn zeroizing_secret_clone(value: &str) -> zeroize::Zeroizing<String> {
    zeroize::Zeroizing::new(value.to_string())
}

fn zeroizing_non_empty_secret(value: &str) -> Option<zeroize::Zeroizing<String>> {
    (!value.is_empty()).then(|| zeroizing_secret_clone(value))
}
