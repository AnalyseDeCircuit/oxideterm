use std::{
    future::Future,
    pin::Pin,
    result::Result as StdResult,
    sync::{Arc, mpsc},
    time::Duration,
};

use gpui::{Context, Window};
use oxideterm_connections::{SaveConnectionRequest, first_available_default_key_path};
use oxideterm_ssh::{
    AuthMethod, ConnectionConsumer, ConnectionState, HostKeyStatus,
    KeyboardInteractivePromptRequest, KeyboardInteractiveResponses, NodeId, NodeReadiness,
    ProxyHopConfig, SshConfig, SshPromptError, SshPromptHandler, SshTransportClient,
    check_host_key,
};
use tokio::sync::oneshot;

use super::{
    form_state::{
        NewConnectionForm, NewConnectionFormMode, NewConnectionProxyHop,
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
        form_from_saved_connection, proxy_chain_config_from_saved_connection,
        save_request_from_form, save_request_from_form_with_existing_auth,
        ssh_config_from_saved_connection,
    },
};

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
        if let Some(parent_id) = self.drill_down_parent_node_id.clone() {
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
            NewConnectionFormMode::NewConnection => {
                self.start_new_connection_flow(SshConnectionIntent::Connect, window, cx);
            }
        }
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
        let Some(config) = ssh_config_from_saved_connection(&self.connection_store, &conn) else {
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

    fn start_saved_connection_flow(
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
        let worker_config = config.clone();
        let worker_title = title.clone();
        std::thread::spawn(move || {
            let status = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime.block_on(check_host_key(&host, port, 10)),
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
        let config = SshConfig {
            host: host.clone(),
            port: port.unwrap_or(22),
            username: username.clone(),
            auth,
            agent_forwarding: form.agent_forwarding,
            proxy_chain,
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
        let cleanup_node_id = matches!(intent, SshConnectionIntent::Connect)
            .then(|| expansion.target_node_id.clone());
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
        });
        self.continue_active_proxy_session_tree_connect(window, cx);
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
                self.mark_current_proxy_connect_step_accepted(false, None);
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
                        None => check_host_key(&step.host, step.port, 10).await,
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
                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    target_node_id,
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
                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    target_node_id,
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

    fn mark_current_proxy_connect_step_accepted(
        &mut self,
        persist: bool,
        fingerprint: Option<String>,
    ) {
        let Some(run) = self.active_proxy_connect_run.as_mut() else {
            return;
        };
        let Some(step) = run.plan.steps.get_mut(run.plan.current_index) else {
            return;
        };
        step.trust_host_key = Some(persist);
        step.expected_host_key_fingerprint = fingerprint.or_else(|| {
            self.ssh_nodes
                .get(&step.node_id)
                .and_then(|node| node.config.expected_host_key_fingerprint.clone())
        });
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
        if run.plan.cleanup_node_id.is_none() {
            return;
        }
        let cleanup_root = run
            .plan
            .steps
            .first()
            .map(|step| step.node_id.clone())
            .or_else(|| run.plan.cleanup_node_id.clone());
        let Some(cleanup_root) = cleanup_root else {
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
                                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                                    expansion.target_node_id,
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
                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    node_id,
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
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(runtime) => {
                    let prompt_handler = Arc::new(NativeSshPromptHandler::new(tx.clone()));
                    runtime
                        .block_on(
                            SshTransportClient::new(config)
                                .with_prompt_handler(prompt_handler)
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
        SshAuthTab::Certificate => AuthMethod::certificate_secret(
            hop.key_path.trim().to_string(),
            hop.cert_path.trim().to_string(),
            zeroizing_non_empty_secret(&hop.passphrase),
        ),
        SshAuthTab::Agent => AuthMethod::Agent,
        SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
    }
}

fn zeroizing_secret_clone(value: &str) -> zeroize::Zeroizing<String> {
    zeroize::Zeroizing::new(value.to_string())
}

fn zeroizing_non_empty_secret(value: &str) -> Option<zeroize::Zeroizing<String>> {
    (!value.is_empty()).then(|| zeroizing_secret_clone(value))
}
