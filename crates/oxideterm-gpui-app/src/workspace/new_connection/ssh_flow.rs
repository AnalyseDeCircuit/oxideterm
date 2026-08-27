use std::{
    collections::HashMap, future::Future, pin::Pin, result::Result as StdResult, sync::Arc,
    time::Duration,
};

use gpui::{App, Context, Window};
use oxideterm_connections::{
    ConnectionTerminalOptions, ConnectionX11ForwardingMode, ConnectionX11ForwardingOptions,
    DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS, MoshIpFamily as SavedMoshIpFamily, MoshPredictionMode,
    MoshUdpPortSelection as SavedMoshUdpPortSelection, SaveConnectionRequest,
    SaveMoshProfileRequest, SaveRemoteDesktopProfileRequest, SaveSerialProfileRequest,
    SaveStandaloneSftpProfileRequest, SaveTelnetProfileRequest, SavedConnectionRuntimeSecrets,
    SavedMoshProfileRuntimeSecrets, SavedProxyCommand, SavedUpstreamProxyAuth,
    SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy, SavedUpstreamProxyProtocol, SecretString,
    SshChannelStrategy, first_available_default_key_path,
};
use oxideterm_mosh::{MoshBootstrapConfig, MoshBootstrapContext};
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopEndpoint, RemoteDesktopProtocol,
    RemoteDesktopSecret,
};
use oxideterm_ssh::{
    AuthMethod, ConnectionConsumer, HostKeyStatus, KeyboardInteractivePromptRequest,
    KeyboardInteractiveResponses, NativeSessionTreeConnectAction, NativeSessionTreeConnectEndpoint,
    NativeSessionTreeConnectPlan, NativeSessionTreeConnectStep, NodeId, NodeReadiness,
    NodeTreeExpansion, ProxyHopConfig, SshConfig, SshPromptError, SshPromptHandler,
    SshTransportClient, UpstreamProxyAuth, UpstreamProxyConfig, UpstreamProxyProtocol,
    X11ForwardPolicy, X11ForwardTrust, check_host_key_with_route,
    check_host_key_with_upstream_proxy,
};
use tokio::sync::oneshot;

use super::{
    ConnectionFormState, NativeProxyConnectRun, ProxyConnectPreflightContext,
    form_state::{
        NewConnectionField, NewConnectionForm, NewConnectionFormMode, NewConnectionProxyHop,
        NewConnectionSubmitAction, NewConnectionTransport, NewConnectionUpstreamProxyAuth,
        NewConnectionUpstreamProxyPolicy, SavedConnectionPromptAction, SshAuthTab,
        StandaloneSftpSecondaryForm, connection_timeout_drafts_valid, identity_agent_from_form,
        identity_agent_selector, ssh_auth_tab_from_saved_auth,
    },
    host_key_dialog::HostKeyChallenge,
};
use crate::workspace::{
    WorkspaceApp, WorkspaceSshNode,
    delivery::ActiveDeliverySender,
    session_manager::{
        RuntimeSecretHandoff, duplicate_connection_template_name, form_from_saved_connection,
        restore_legacy_jump_host_in_form, save_request_from_form_with_existing_auth,
        save_request_from_form_with_proxy_hop_prefix, upstream_proxy_config_from_form,
    },
};
use oxideterm_session_adapter::{
    auth_method_from_saved_auth, managed_key_resolver_from_store,
    proxy_chain_config_from_saved_connection, proxy_command_from_value,
    ssh_config_from_saved_connection, ssh_config_from_saved_connection_with_auth,
    ssh_config_from_saved_connection_with_runtime_secrets,
    ssh_config_from_standalone_sftp_endpoint_with_runtime_secrets,
    ssh_config_from_standalone_sftp_profile_with_runtime_secrets,
};
use oxideterm_terminal::{MoshTerminalConfig, SerialSessionConfig, TelnetSessionConfig};

mod connect;
mod conversion;
mod save;

use conversion::*;
pub(in crate::workspace) use save::mosh_options_from_profile;

fn x11_forward_policy(options: ConnectionX11ForwardingOptions) -> Option<X11ForwardPolicy> {
    if !options.enabled {
        return None;
    }
    match options.mode {
        ConnectionX11ForwardingMode::Trusted => Some(X11ForwardPolicy::trusted()),
        ConnectionX11ForwardingMode::Untrusted if options.untrusted_timeout_seconds == 0 => {
            Some(X11ForwardPolicy::untrusted().without_timeout())
        }
        ConnectionX11ForwardingMode::Untrusted => Some(
            X11ForwardPolicy::untrusted()
                .with_timeout_millis(u64::from(options.untrusted_timeout_seconds) * 1_000),
        ),
    }
}

fn connection_x11_options(policy: Option<X11ForwardPolicy>) -> ConnectionX11ForwardingOptions {
    let Some(policy) = policy else {
        return ConnectionX11ForwardingOptions::default();
    };
    let mode = match policy.trust {
        X11ForwardTrust::Trusted => ConnectionX11ForwardingMode::Trusted,
        X11ForwardTrust::Untrusted => ConnectionX11ForwardingMode::Untrusted,
    };
    let default_timeout = ConnectionX11ForwardingOptions::default().untrusted_timeout_seconds;
    let untrusted_timeout_seconds = match (policy.trust, policy.timeout_millis) {
        (X11ForwardTrust::Untrusted, None) => 0,
        (_, Some(value)) => u32::try_from(value / 1_000)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(default_timeout),
        (X11ForwardTrust::Trusted, None) => default_timeout,
    };
    ConnectionX11ForwardingOptions {
        enabled: true,
        mode,
        untrusted_timeout_seconds,
    }
}

/// Carries the original zeroizing allocations from persistence into one runtime start.
struct SavedConnectionRuntimeHandoff {
    connection_id: String,
    secrets: SavedConnectionRuntimeSecrets,
    auth_override: Option<AuthMethod>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct SshTerminalConnectionOptions {
    pub(in crate::workspace) terminal: ConnectionTerminalOptions,
    pub(in crate::workspace) dedicated_new_terminal_connection: bool,
    pub(in crate::workspace) ssh_channel_strategy: SshChannelStrategy,
}

impl SshTerminalConnectionOptions {
    pub(in crate::workspace) fn from_form(form: &NewConnectionForm) -> Self {
        // Keep the SSH session ownership policy separate from terminal protocol overrides.
        Self {
            terminal: form.terminal.clone(),
            dedicated_new_terminal_connection: form.dedicated_new_terminal_connection,
            ssh_channel_strategy: form.ssh_channel_strategy,
        }
    }
}

impl Default for SshTerminalConnectionOptions {
    fn default() -> Self {
        Self {
            terminal: ConnectionTerminalOptions::default(),
            dedicated_new_terminal_connection: false,
            ssh_channel_strategy: SshChannelStrategy::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SshConnectionIntent {
    Test,
    TestStandaloneSftp,
    Connect(SshTerminalConnectionOptions),
    ConnectSaved(String),
    DrillDown {
        parent_id: NodeId,
        saved_connection_id: Option<String>,
        terminal_options: SshTerminalConnectionOptions,
    },
    Mosh(MoshConnectionOptions),
    StandaloneSftp {
        saved_profile_id: Option<String>,
        initial_remote_path: Option<String>,
        pair_launch_token: Option<String>,
    },
    StandaloneSftpSecondary {
        pair_launch_token: String,
    },
}

impl SshConnectionIntent {
    pub(in crate::workspace) fn standalone_sftp_pair_launch_token(&self) -> Option<&str> {
        match self {
            Self::StandaloneSftp {
                pair_launch_token: Some(token),
                ..
            }
            | Self::StandaloneSftpSecondary {
                pair_launch_token: token,
            } => Some(token),
            _ => None,
        }
    }
}

/// Owns both secret-bearing endpoint configs only until host-key checks finish.
pub(in crate::workspace) struct PendingStandaloneSftpPairLaunch {
    pub(in crate::workspace) saved_profile_id: String,
    pub(in crate::workspace) title: String,
    pub(in crate::workspace) primary_initial_remote_path: Option<String>,
    pub(in crate::workspace) secondary_initial_remote_path: Option<String>,
    pub(in crate::workspace) primary_config: Option<SshConfig>,
    pub(in crate::workspace) secondary_config: Option<SshConfig>,
}

/// Non-secret Mosh launch settings travel through SSH host-key preflight.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct MoshConnectionOptions {
    pub(in crate::workspace) saved_profile_id: Option<String>,
    pub(in crate::workspace) server_executable: String,
    pub(in crate::workspace) udp_host_override: Option<String>,
    pub(in crate::workspace) udp_port: SavedMoshUdpPortSelection,
    pub(in crate::workspace) ip_family: SavedMoshIpFamily,
    pub(in crate::workspace) prediction: MoshPredictionMode,
    pub(in crate::workspace) locale: Option<String>,
    pub(in crate::workspace) terminal: ConnectionTerminalOptions,
    // Correlates an asynchronous verified Mosh launch without exposing a GPUI identity.
    pub(in crate::workspace) public_mcp_open_token: Option<String>,
    // Reconnect binds the new terminal surface to an existing logical connection record.
    pub(in crate::workspace) runtime_connection_attempt_id: Option<String>,
}

pub(in crate::workspace) enum SshConnectionWorkerResult {
    Preflight {
        config: SshConfig,
        upstream_proxy: Option<UpstreamProxyConfig>,
        title: String,
        intent: SshConnectionIntent,
        host: String,
        port: u16,
        status: HostKeyStatus,
    },
    SessionTreePreflight {
        generation: u64,
        step_index: usize,
        upstream_proxy: Option<UpstreamProxyConfig>,
        status: HostKeyStatus,
    },
    Test {
        result: StdResult<(), String>,
    },
    StandaloneSftpConnected {
        endpoint_id: String,
        saved_profile_id: Option<String>,
        title: String,
        initial_remote_path: Option<String>,
        consumer: ConnectionConsumer,
        result: StdResult<oxideterm_ssh::SshConnectionHandle, String>,
    },
    StandaloneSftpPairConnected {
        saved_profile_id: String,
        title: String,
        primary_endpoint_id: String,
        secondary_endpoint_id: String,
        primary_initial_remote_path: Option<String>,
        secondary_initial_remote_path: Option<String>,
        primary_consumer: ConnectionConsumer,
        secondary_consumer: ConnectionConsumer,
        result: StdResult<
            (
                oxideterm_ssh::SshConnectionHandle,
                oxideterm_ssh::SshConnectionHandle,
            ),
            String,
        >,
    },
    KeyboardInteractivePrompt {
        request: KeyboardInteractivePromptRequest,
        response_tx: oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>,
    },
}

#[derive(Clone)]
pub(in crate::workspace) struct NativeSshPromptHandler {
    tx: ActiveDeliverySender<SshConnectionWorkerResult>,
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
    pub(in crate::workspace) fn new(tx: ActiveDeliverySender<SshConnectionWorkerResult>) -> Self {
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
    pub(in crate::workspace) fn ensure_kerberos_credentials_availability(
        &self,
        cx: &mut Context<Self>,
    ) {
        let should_check = self.update_connection_form_state(cx, |state| {
            let Some(form) = state.form.as_mut() else {
                return false;
            };
            let kerberos_enabled = form.gssapi_enabled
                || form.standalone_sftp_secondary.gssapi_enabled
                || form
                    .jump_server_form
                    .as_ref()
                    .is_some_and(|jump| jump.gssapi_enabled);
            if !kerberos_enabled
                || form.gssapi_credentials_available.is_some()
                || form.gssapi_credentials_check_pending
            {
                return false;
            }
            form.gssapi_credentials_check_pending = true;
            true
        });
        if !should_check {
            return;
        }

        let runtime = self.forwarding_runtime.handle().clone();
        cx.spawn(async move |weak, cx| {
            // Platform credential discovery may call GSSAPI or SSPI and must stay off GPUI.
            let available = runtime
                .spawn_blocking(oxideterm_ssh::kerberos_credentials_available)
                .await
                .unwrap_or(false);
            let _ = weak.update(cx, |this, cx| {
                this.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.gssapi_credentials_check_pending = false;
                        form.gssapi_credentials_available = Some(available);
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// Borrows the entity-owned form state without copying secret drafts.
    pub(in crate::workspace) fn connection_form_state<'a>(
        &self,
        cx: &'a App,
    ) -> &'a super::ConnectionFormState {
        &self.connection_flow.read(cx).form
    }

    /// Applies one UI transition inside the form entity and schedules its repaint.
    pub(in crate::workspace) fn update_connection_form_state<R>(
        &self,
        cx: &mut App,
        update: impl FnOnce(&mut super::ConnectionFormState) -> R,
    ) -> R {
        self.connection_flow.update(cx, |connection_flow, cx| {
            let result = update(&mut connection_flow.form);
            cx.notify();
            result
        })
    }

    /// Moves the secret-bearing form through one synchronous root adapter without cloning it.
    pub(in crate::workspace) fn with_connection_form_mut<R>(
        &mut self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut Self, Option<&mut NewConnectionForm>, &mut Context<Self>) -> R,
    ) -> R {
        let mut form = self.connection_flow.update(cx, |connection_flow, cx| {
            let form = connection_flow.form.form.take();
            cx.notify();
            form
        });
        let result = update(self, form.as_mut(), cx);
        self.connection_flow.update(cx, |connection_flow, cx| {
            debug_assert!(
                connection_flow.form.form.is_none(),
                "synchronous form adapter must remain the only draft owner"
            );
            connection_flow.form.form = form;
            cx.notify();
        });
        result
    }

    pub(in crate::workspace) fn saved_connection_form_uses_unloaded_secret(
        &self,
        cx: &App,
    ) -> bool {
        let state = self.connection_form_state(cx);
        state.saved_connection_source_id().is_some()
            && state.saved_connection_prompt_action.is_none()
    }

    pub(in crate::workspace) fn open_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary(cx);
        let mut form = NewConnectionForm::default();
        form.group = self.i18n.t("ssh.form.ungrouped");
        form.agent_available = detect_ssh_agent_available(&form.identity_agent);
        form.save_connection = self
            .settings_store
            .settings()
            .new_connection
            .save_connection;
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_serial_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_new_connection_form(window, cx);
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.transport = NewConnectionTransport::Serial;
                form.focused_field = super::form_state::NewConnectionField::SerialPortPath;
                form.field_focused = false;
            }
        });
        self.refresh_serial_ports(cx);
    }

    pub(in crate::workspace) fn open_telnet_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_new_connection_form(window, cx);
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.transport = NewConnectionTransport::Telnet;
                form.port = super::form_state::TELNET_DEFAULT_PORT_TEXT.to_string();
                form.focused_field = super::form_state::NewConnectionField::Host;
                form.field_focused = false;
            }
        });
    }

    pub(in crate::workspace) fn open_drill_down_form(
        &mut self,
        parent_node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parent_ready = self
            .node_router
            .node_metadata(&parent_node_id)
            .is_some_and(|snapshot| snapshot.readiness == NodeReadiness::Ready);
        if !parent_ready {
            let message = format!(
                "{}: {}",
                self.i18n.t("sessions.tree.actions.drill_in"),
                self.i18n.t("ssh.drill_down.parent_not_ready")
            );
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
            return;
        }

        self.prepare_modal_interaction_boundary(cx);
        let mut form = NewConnectionForm::default();
        form.auth_tab = SshAuthTab::Agent;
        form.focused_field = super::form_state::NewConnectionField::Host;
        form.save_connection = false;
        form.group = self.i18n.t("ssh.form.ungrouped");
        form.agent_available = detect_ssh_agent_available(&form.identity_agent);
        form.username = String::new();
        self.update_connection_form_state(cx, |state| {
            state.replace_with_new_form(form);
            state.drill_down_parent_node_id = Some(parent_node_id);
        });
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
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
            .node_router
            .node_metadata(&parent_node_id)
            .is_some_and(|snapshot| snapshot.readiness == NodeReadiness::Ready);
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
        self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.clear_host_key_challenge(cx);
        });
        self.update_connection_form_state(cx, ConnectionFormState::clear);
        let message = self.i18n.t("ssh.drill_down.connecting");
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.set_status(Some(message), cx);
        });
        self.ensure_node_connection_started(&target_node_id, cx);
        let _ = self.connection_store.mark_used(&saved_connection_id);
        self.persist_session_tree_snapshot();
        cx.notify();
    }
}
