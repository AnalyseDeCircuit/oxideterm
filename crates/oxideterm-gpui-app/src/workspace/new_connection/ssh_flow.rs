use std::{
    collections::HashMap,
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
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopEndpoint, RemoteDesktopProtocol,
    RemoteDesktopSecret,
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
    NativeProxyConnectRun, WorkspaceApp, WorkspaceSshNode,
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

mod connect;
mod conversion;
mod save;

use conversion::*;

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

fn sync_saved_connection_node_title_for_nodes(
    ssh_nodes: &mut HashMap<NodeId, WorkspaceSshNode>,
    saved_connection_id: &str,
    title: &str,
) -> bool {
    let mut changed = false;
    for node in ssh_nodes.values_mut() {
        if node.saved_connection_id.as_deref() != Some(saved_connection_id) {
            continue;
        }
        if node.title == title {
            continue;
        }
        // Only mirror saved display metadata. The live runtime config keeps
        // describing the already-created SSH node until the user reconnects.
        node.title = title.to_string();
        changed = true;
    }
    changed
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
        self.editing_saved_connection_connect_after_save_node_id = None;
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
        self.editing_saved_connection_connect_after_save_node_id = None;
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
            parent_node_id,
            &saved_connection_id,
            config,
            connection.name,
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

        let target_node_id = expansion.target_node_id;
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
}
