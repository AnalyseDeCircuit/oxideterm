use std::{future::Future, pin::Pin, result::Result as StdResult, sync::Arc, sync::mpsc};

use gpui::{Context, Window};
use oxideterm_ssh::{
    AuthMethod, HostKeyStatus, KeyboardInteractivePromptRequest, NodeId, NodeReadiness,
    ProxyChainPreflightChallenge, ProxyHopConfig, SshConfig, SshPromptError, SshPromptHandler,
    SshTransportClient, check_host_key,
};
use tokio::sync::oneshot;

use super::{
    form_state::{
        NewConnectionForm, NewConnectionFormMode, NewConnectionProxyHop,
        SavedConnectionPromptAction, SshAuthTab, new_connection_form_mode,
    },
    host_key_dialog::{HostKeyChallenge, SshProxyPreflightPlan},
};
use crate::workspace::{
    WorkspaceApp,
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
    ProxyPreflight {
        plan: SshProxyPreflightPlan,
        challenge: Option<ProxyChainPreflightChallenge>,
        error: Option<String>,
    },
    Test {
        result: StdResult<(), String>,
    },
    KeyboardInteractivePrompt {
        request: KeyboardInteractivePromptRequest,
        response_tx: oneshot::Sender<Result<Vec<String>, SshPromptError>>,
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
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, SshPromptError>> + Send + '_>> {
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
    pub(in crate::workspace) fn open_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_connection_form = Some(NewConnectionForm {
            group: self.i18n.t("ssh.form.ungrouped"),
            ..NewConnectionForm::default()
        });
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.open_new_connection_select = None;
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

        let mut form = NewConnectionForm {
            auth_tab: SshAuthTab::Agent,
            focused_field: super::form_state::NewConnectionField::Host,
            save_connection: false,
            group: self.i18n.t("ssh.form.ungrouped"),
            ..NewConnectionForm::default()
        };
        form.username = String::new();
        self.new_connection_form = Some(form);
        self.drill_down_parent_node_id = Some(parent_node_id);
        self.editing_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.open_new_connection_select = None;
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
        self.saved_connection_prompt_action = None;
        self.open_new_connection_select = None;
        self.host_key_challenge = None;
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
            self.saved_connection_prompt_action,
        ) {
            NewConnectionFormMode::SavedConnectionPrompt => {
                self.submit_saved_connection_prompt(window, cx);
            }
            NewConnectionFormMode::EditProperties => {
                self.save_editing_connection(window, cx);
            }
            NewConnectionFormMode::NewConnection => {
                self.start_new_connection_flow(SshConnectionIntent::Connect, window, cx);
            }
        }
    }

    pub(in crate::workspace) fn start_new_connection_flow(
        &mut self,
        intent: SshConnectionIntent,
        _window: &mut Window,
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
        if let SshConnectionIntent::DrillDown(parent_id) = intent {
            // Tauri DrillDownDialog calls tree_drill_down and then
            // connect_tree_node; it does not run a local direct host-key
            // preflight because the child may only be reachable through the
            // parent tunnel. Native keeps that node-only path here.
            self.continue_verified_ssh_flow(
                config,
                title,
                SshConnectionIntent::DrillDown(parent_id),
                _window,
                cx,
            );
            return;
        }
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = true;
            form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
        }

        if config.proxy_chain.is_some() {
            self.start_proxy_chain_preflight(SshProxyPreflightPlan {
                config,
                title,
                intent,
                current_index: 0,
            });
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
        self.new_connection_form = Some(form_from_saved_connection(&conn, error));
        self.editing_saved_connection_id = Some(id.to_string());
        self.saved_connection_prompt_action = Some(action);
        self.open_new_connection_select = None;
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
        self.new_connection_form = Some(form_from_saved_connection(&conn, error));
        self.editing_saved_connection_id = Some(id.to_string());
        self.saved_connection_prompt_action = None;
        self.open_new_connection_select = None;
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
                        self.open_new_connection_select = None;
                        self.session_manager.status =
                            Some(self.i18n.t("sessionManager.edit_properties.save"));
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
        config: SshConfig,
        title: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session_manager.status = Some(self.i18n.t("ssh.form.checking_host_key"));
        if config.proxy_chain.is_some() {
            self.start_proxy_chain_preflight(SshProxyPreflightPlan {
                config,
                title,
                intent: SshConnectionIntent::ConnectSaved(id),
                current_index: 0,
            });
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

        let auth = match form.auth_tab {
            SshAuthTab::Password => AuthMethod::password(form.password.clone()),
            SshAuthTab::Agent => AuthMethod::Agent,
            SshAuthTab::DefaultKey => AuthMethod::key(
                "",
                (!form.passphrase.is_empty()).then(|| form.passphrase.clone()),
            ),
            SshAuthTab::SshKey => {
                if form.key_path.trim().is_empty() {
                    form.error = Some(self.i18n.t("ssh.form.key_path_required"));
                    cx.notify();
                    return None;
                }
                AuthMethod::key(
                    form.key_path.trim().to_string(),
                    (!form.passphrase.is_empty()).then(|| form.passphrase.clone()),
                )
            }
            SshAuthTab::Certificate => {
                if form.key_path.trim().is_empty() || form.cert_path.trim().is_empty() {
                    form.error = Some(self.i18n.t("ssh.form.certificate_paths_required"));
                    cx.notify();
                    return None;
                }
                AuthMethod::certificate(
                    form.key_path.trim().to_string(),
                    form.cert_path.trim().to_string(),
                    (!form.passphrase.is_empty()).then(|| form.passphrase.clone()),
                )
            }
            SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
        };
        let proxy_chain = proxy_chain_from_form(form);
        let config = SshConfig {
            host: host.clone(),
            port: port.unwrap_or(22),
            username: username.clone(),
            auth,
            agent_forwarding: form.agent_forwarding,
            proxy_chain,
            strict_host_key_checking: true,
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
                SshConnectionWorkerResult::ProxyPreflight {
                    plan,
                    challenge,
                    error,
                } => self.handle_proxy_chain_preflight_result(plan, challenge, error, window, cx),
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
                let host = config.host.clone();
                let port = config.port;
                self.host_key_challenge = Some(HostKeyChallenge {
                    config,
                    title,
                    status,
                    intent,
                    proxy_plan: None,
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

    pub(in crate::workspace) fn start_proxy_chain_preflight(&self, plan: SshProxyPreflightPlan) {
        let tx = self.ssh_worker_tx.clone();
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime
                    .block_on(SshTransportClient::new(plan.config.clone()).preflight_proxy_chain())
                    .map_err(|error| error.to_string()),
                Err(error) => Err(format!("failed to initialize SSH runtime: {error}")),
            };
            let (challenge, error) = match result {
                Ok(challenge) => (challenge, None),
                Err(error) => (None, Some(error)),
            };
            let _ = tx.send(SshConnectionWorkerResult::ProxyPreflight {
                plan,
                challenge,
                error,
            });
        });
    }

    fn handle_proxy_chain_preflight_result(
        &mut self,
        mut plan: SshProxyPreflightPlan,
        challenge: Option<ProxyChainPreflightChallenge>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = None;
        }

        if let Some(error) = error {
            if let Some(form) = self.new_connection_form.as_mut() {
                form.error = Some(error);
            } else {
                self.session_manager.status = Some(error);
            }
            cx.notify();
            return;
        }

        let Some(challenge) = challenge else {
            self.continue_verified_ssh_flow(plan.config, plan.title, plan.intent, window, cx);
            return;
        };

        match challenge.status {
            HostKeyStatus::Verified => {
                self.continue_verified_ssh_flow(plan.config, plan.title, plan.intent, window, cx)
            }
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                plan.current_index = challenge.step_index;
                self.host_key_challenge = Some(HostKeyChallenge {
                    config: plan.config.clone(),
                    title: plan.title.clone(),
                    status: challenge.status,
                    intent: plan.intent.clone(),
                    proxy_plan: Some(plan),
                    host: challenge.host,
                    port: challenge.port,
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
                    self.saved_connection_prompt_action,
                );
                if mode.stores_connection_on_connect()
                    && self
                        .new_connection_form
                        .as_ref()
                        .is_some_and(|form| form.save_connection)
                    && let Some(form) = self.new_connection_form.as_ref()
                    && let Ok(request) = save_request_from_form(form, None)
                    && let Err(error) = self.connection_store.upsert(request)
                    && let Some(form) = self.new_connection_form.as_mut()
                {
                    form.error = Some(error.to_string());
                    cx.notify();
                    return;
                }
                self.new_connection_form = None;
                self.host_key_challenge = None;
                self.open_new_connection_select = None;
                let _ = self.create_ssh_terminal_tab(config, title, window, cx);
            }
            SshConnectionIntent::ConnectSaved(id) => {
                self.host_key_challenge = None;
                if self.saved_connection_prompt_action.is_some() {
                    self.new_connection_form = None;
                    self.editing_saved_connection_id = None;
                    self.saved_connection_prompt_action = None;
                    self.open_new_connection_select = None;
                }
                let _ = self.connection_store.mark_used(&id);
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
                self.open_new_connection_select = None;
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

fn proxy_chain_from_form(form: &mut NewConnectionForm) -> Option<Vec<ProxyHopConfig>> {
    if form.proxy_hops.is_empty() {
        return None;
    }
    Some(
        form.proxy_hops
            .iter()
            .filter(|hop| hop.complete())
            .map(|hop| ProxyHopConfig {
                host: hop.host.trim().to_string(),
                port: hop.port.trim().parse::<u16>().unwrap_or(22),
                username: hop.username.trim().to_string(),
                auth: auth_method_from_proxy_hop(hop),
                agent_forwarding: hop.agent_forwarding,
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            })
            .collect(),
    )
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

fn auth_method_from_proxy_hop(hop: &NewConnectionProxyHop) -> AuthMethod {
    match hop.auth_tab {
        SshAuthTab::Password => AuthMethod::password(hop.password.clone()),
        SshAuthTab::DefaultKey => AuthMethod::key(
            "",
            (!hop.passphrase.is_empty()).then(|| hop.passphrase.clone()),
        ),
        SshAuthTab::SshKey => AuthMethod::key(
            hop.key_path.trim().to_string(),
            (!hop.passphrase.is_empty()).then(|| hop.passphrase.clone()),
        ),
        SshAuthTab::Certificate => AuthMethod::certificate(
            hop.key_path.trim().to_string(),
            hop.cert_path.trim().to_string(),
            (!hop.passphrase.is_empty()).then(|| hop.passphrase.clone()),
        ),
        SshAuthTab::Agent | SshAuthTab::TwoFactor => AuthMethod::Agent,
    }
}
