use std::{
    ops::{Deref, DerefMut},
    path::Path,
    sync::Arc,
};

use gpui::{App, Context, Task};
use oxideterm_gpui_terminal::{TerminalNotice, TerminalNoticeVariant};
use oxideterm_public_mcp::{
    ApprovalStatus, ClientApprovalMode, ClientRef, DomainRequest, PublicToolCall,
    ToolEnvelope, ToolGroup,
};
use oxideterm_public_mcp_host::{
    CONNECTION_KEY_DESKTOP_PREFIX, CONNECTION_KEY_MOSH_PREFIX, CONNECTION_KEY_SERIAL_PREFIX,
    CONNECTION_KEY_SSH_PREFIX, CONNECTION_KEY_TELNET_PREFIX, PublicMcpBackground,
    PublicMcpClientChange, PublicMcpClientEffect, PublicMcpDesktopRecord, PublicMcpHost,
    PublicMcpHostEffect, PublicMcpPendingTerminalOpen, PublicMcpTerminalRecord,
    PublicMcpWindowRequest, finish_serialized,
};
use oxideterm_session_adapter::ssh_config_from_saved_connection;
use serde_json::json;

use super::WorkspaceApp;

mod addons;
mod cloud_sync;
mod connections;
pub(in crate::workspace) mod desktops;
mod forward_port;
mod history;
mod quick_commands;
mod recordings;
pub(in crate::workspace) mod terminals;

pub(in crate::workspace) struct PublicMcpWorkspaceBridge {
    host: PublicMcpHost,
    delivery_task: Option<Task<()>>,
    background_pump: Option<tokio::task::JoinHandle<()>>,
    background: Option<PublicMcpBackground>,
}

impl PublicMcpWorkspaceBridge {
    pub(in crate::workspace) fn start(
        settings_path: &Path,
        runtime: &tokio::runtime::Handle,
    ) -> Self {
        Self {
            host: PublicMcpHost::start(settings_path, runtime),
            delivery_task: None,
            background_pump: None,
            background: None,
        }
    }

    fn background(&self) -> &PublicMcpBackground {
        self.background
            .as_ref()
            .expect("Public MCP delivery is initialized with the workspace")
    }
}

impl Deref for PublicMcpWorkspaceBridge {
    type Target = PublicMcpHost;
    fn deref(&self) -> &Self::Target {
        &self.host
    }
}

impl DerefMut for PublicMcpWorkspaceBridge {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.host
    }
}

impl Drop for PublicMcpWorkspaceBridge {
    fn drop(&mut self) {
        if let Some(pump) = self.background_pump.take() {
            pump.abort();
        }
        self.delivery_task.take();
    }
}

pub(in crate::workspace) enum PublicMcpNodeWindowEffect {
    Disconnect(DomainRequest),
}

impl PublicMcpNodeWindowEffect {
    pub(in crate::workspace) fn finish_without_window(self) {
        let Self::Disconnect(request) = self;
        request.finish(ToolEnvelope::failed(
            "A live OxideTerm window is required to disconnect a node",
        ));
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn start_public_mcp_delivery(&mut self, cx: &mut Context<Self>) {
        let Some(receiver) = self.public_mcp.take_receiver() else {
            return;
        };
        let runtime = self.forwarding_runtime.handle();
        let (effects, mut effect_rx) = tokio::sync::mpsc::unbounded_channel();
        let background = PublicMcpBackground::new(
            &self.public_mcp,
            self.node_router.clone(),
            self.sftp_transfer_manager.clone(),
            runtime.clone(),
            Arc::new(self.forwarding_service.clone()),
            effects,
        );
        self.public_mcp.set_suspended(self.app_lock.locked);
        let (task, mut window_rx) = background.start_delivery(receiver);
        self.public_mcp.background = Some(background);
        self.public_mcp.background_pump = Some(task);
        self.public_mcp.delivery_task = Some(cx.spawn(async move |workspace, cx| {
            loop {
                tokio::select! {
                    message = window_rx.recv() => {
                        let Some(message) = message else { break; };
                        if workspace.update(cx, |workspace, cx| match message {
                            PublicMcpWindowDelivery::Request { request, accepted } => {
                                workspace.handle_public_mcp_request(request, cx);
                                if let Some(accepted) = accepted {
                                    let _ = accepted.send(());
                                }
                            }
                            PublicMcpWindowDelivery::StateChanged => {
                                workspace.notify_public_mcp_approval(cx);
                                cx.notify();
                            }
                        }).is_err() { break; }
                    }
                    Some(effect) = effect_rx.recv() => {
                        if workspace.update(cx, |workspace, cx| match effect {
                            PublicMcpHostEffect::SavedForwardsChanged => workspace.queue_cloud_sync_dirty_refresh(cx),
                        }).is_err() { break; }
                    }
                }
            }
        }));
    }

    fn notify_public_mcp_approval(&self, cx: &App) {
        let Some(approval) = self
            .public_mcp
            .approvals()
            .into_iter()
            .rev()
            .find(|approval| approval.status == ApprovalStatus::Pending)
        else {
            return;
        };
        let client_label = self
            .public_mcp
            .clients()
            .into_iter()
            .find(|client| client.client_ref == approval.client_ref)
            .map_or_else(|| approval.client_ref.to_string(), |client| client.label);
        let description = self
            .i18n
            .t("settings_view.network.approval_notice_description")
            .replace("{{client}}", &client_label)
            .replace("{{tool}}", &approval.tool_name);
        self.push_workspace_notice(
            TerminalNotice {
                title: self.i18n.t("settings_view.network.approval_notice_title"),
                description: Some(description),
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Warning,
            },
            cx,
        );
    }

    pub(in crate::workspace) fn set_public_mcp_client_enabled(
        &mut self,
        client: &ClientRef,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.change_public_mcp_client(client, PublicMcpClientChange::Enabled(enabled), cx)
    }

    pub(in crate::workspace) fn set_public_mcp_client_approval_mode(
        &mut self,
        client: &ClientRef,
        mode: ClientApprovalMode,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.change_public_mcp_client(client, PublicMcpClientChange::ApprovalMode(mode), cx)
    }

    pub(in crate::workspace) fn set_public_mcp_client_tool_group(
        &mut self,
        client: &ClientRef,
        group: ToolGroup,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.change_public_mcp_client(client, PublicMcpClientChange::ToolGroup(group, enabled), cx)
    }

    pub(in crate::workspace) fn remove_public_mcp_client(
        &mut self,
        client: &ClientRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.change_public_mcp_client(client, PublicMcpClientChange::Remove, cx)
    }

    fn change_public_mcp_client(
        &mut self,
        client: &ClientRef,
        change: PublicMcpClientChange,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let admission = self.public_mcp.admission();
        let _dispatch = admission.lock();
        let background = self.public_mcp.background().clone();
        let effects = self.public_mcp.change_client(&background, client, change)?;
        self.apply_public_mcp_client_effects(client, effects, cx);
        Ok(())
    }

    fn apply_public_mcp_client_effects(
        &mut self,
        client: &ClientRef,
        effects: Vec<PublicMcpClientEffect>,
        cx: &mut Context<Self>,
    ) {
        use PublicMcpClientEffect::*;
        for effect in effects {
            match effect {
                RevokeRecordings => self.revoke_public_mcp_client_recordings(client, cx),
                StopRecordings => self.stop_public_mcp_client_recordings(client, cx),
                CloseTerminals => self.revoke_public_mcp_client_terminals(client, cx),
                CloseDesktops => self.revoke_public_mcp_client_desktops(client, cx),
                SetDesktopObservation(enabled) => {
                    self.set_public_mcp_client_desktop_observation(client, enabled, cx)
                }
                ReleaseDesktopInputs => self.release_public_mcp_client_desktop_inputs(client, cx),
                ClearDesktopClipboard => {
                    self.revoke_public_mcp_client_desktop_clipboard_content(client, cx)
                }
            }
        }
    }

    pub(in crate::workspace) fn public_mcp_target_label(
        &self,
        client_ref: &ClientRef,
        target: &str,
        cx: &App,
    ) -> String {
        let plugin_registry = self.plugin_entity.read(cx).registry_snapshot();
        self.public_mcp.target_label(
            client_ref,
            target,
            &self.connection_store,
            &self.node_router,
            &plugin_registry,
            &self.forwarding_service,
        )
    }

    pub(in crate::workspace) fn suspend_public_mcp_runtime(&mut self, cx: &mut Context<Self>) {
        self.public_mcp.set_suspended(true);
        for client in self.public_mcp.clients() {
            let effects = self
                .public_mcp
                .suspend_client_policy(self.public_mcp.background(), &client.client_ref);
            self.apply_public_mcp_client_effects(&client.client_ref, effects, cx);
        }
    }

    fn handle_public_mcp_request(
        &mut self,
        delivery: PublicMcpWindowRequest,
        cx: &mut Context<Self>,
    ) {
        let request = match &delivery {
            PublicMcpWindowRequest::Call(request)
            | PublicMcpWindowRequest::ConnectionDirectory(request)
            | PublicMcpWindowRequest::InstallAddon { request, .. } => request,
        };
        let admission = self.public_mcp.admission();
        let _dispatch = admission.lock();
        let audit_context = request.audit_context.clone();
        oxideterm_audit::AuditContext::with_sync_request(audit_context.as_ref(), || {
            if request.is_cancelled() {
                return;
            }
            if self.app_lock.locked {
                delivery
                    .into_request()
                    .finish(ToolEnvelope::failed("The OxideTerm workspace is locked"));
                return;
            }
            let request = match delivery {
                PublicMcpWindowRequest::ConnectionDirectory(request) => {
                    self.handle_public_mcp_connection_directory(*request);
                    return;
                }
                PublicMcpWindowRequest::InstallAddon { request, package } => {
                    self.handle_public_mcp_addons_install(*request, package, cx);
                    return;
                }
                PublicMcpWindowRequest::Call(request) => *request,
            };
            match &request.call {
                PublicToolCall::RequestAccess(_) | PublicToolCall::RevokeAccess(_) => {
                    let background = self.public_mcp.background().clone();
                    match self
                        .public_mcp
                        .change_requested_access(&background, &request)
                    {
                        Ok((effects, response)) => {
                            self.apply_public_mcp_client_effects(&request.client_ref, effects, cx);
                            finish_serialized(request, response);
                            cx.notify();
                        }
                        Err(error) => request.finish(ToolEnvelope::failed(error)),
                    }
                }
                PublicToolCall::SaveConnection(_) => {
                    self.handle_public_mcp_save_connection(request, cx)
                }
                PublicToolCall::RemoveConnection(_) => {
                    self.handle_public_mcp_remove_connection(request, cx)
                }
                PublicToolCall::CredentialStatus(_) => {
                    self.handle_public_mcp_credential_status(request)
                }
                PublicToolCall::StoreCredential(_) => {
                    self.handle_public_mcp_store_credential(request, cx)
                }
                PublicToolCall::ForgetCredential(_) => {
                    self.handle_public_mcp_forget_credential(request, cx)
                }
                PublicToolCall::SyncStatus(_) => self.handle_public_mcp_sync_status(request, cx),
                PublicToolCall::SyncPullPreview(_) => {
                    self.handle_public_mcp_sync_pull_preview(request, cx)
                }
                PublicToolCall::SyncPublishPreview(_) => {
                    self.handle_public_mcp_sync_publish_preview(request, cx)
                }
                PublicToolCall::SyncApplyPlan(_) => {
                    self.handle_public_mcp_sync_apply_plan(request, cx)
                }
                PublicToolCall::SyncRestore(_) => self.handle_public_mcp_sync_restore(request, cx),
                PublicToolCall::ConnectNode(_) => self.handle_public_mcp_connect_node(request, cx),
                PublicToolCall::DisconnectNode(_) => {
                    self.handle_public_mcp_disconnect_node(request, cx)
                }
                PublicToolCall::OpenTerminal(_) => {
                    self.handle_public_mcp_terminal_open(request, cx)
                }
                PublicToolCall::TerminalState(_) => {
                    self.handle_public_mcp_terminal_state(request, cx)
                }
                PublicToolCall::TerminalHistory(_) => {
                    self.handle_public_mcp_terminal_history(request, cx)
                }
                PublicToolCall::ReadTerminal(_) => {
                    self.handle_public_mcp_terminal_read(request, cx)
                }
                PublicToolCall::FindTerminal(_) => {
                    self.handle_public_mcp_terminal_find(request, cx)
                }
                PublicToolCall::SubmitTerminal(_) => {
                    self.handle_public_mcp_terminal_submit(request, cx)
                }
                PublicToolCall::ResizeTerminal(_) => {
                    self.handle_public_mcp_terminal_resize(request, cx)
                }
                PublicToolCall::ControlTerminal(_) => {
                    self.handle_public_mcp_terminal_control(request, cx)
                }
                PublicToolCall::CloseTerminal(_) => {
                    self.handle_public_mcp_terminal_close(request, cx)
                }
                PublicToolCall::RecordingsControl(_) => {
                    self.handle_public_mcp_recordings_control(request, cx)
                }
                PublicToolCall::RecordingsStatus(_) => {
                    self.handle_public_mcp_recordings_status(request, cx)
                }
                PublicToolCall::OpenDesktop(_) => self.handle_public_mcp_desktop_open(request, cx),
                PublicToolCall::DesktopState(_) => {
                    self.handle_public_mcp_desktop_state(request, cx)
                }
                PublicToolCall::DesktopFrame(_) => {
                    self.handle_public_mcp_desktop_frame(request, cx)
                }
                PublicToolCall::DesktopInput(_) => {
                    self.handle_public_mcp_desktop_input(request, cx)
                }
                PublicToolCall::ResizeDesktop(_) => {
                    self.handle_public_mcp_desktop_resize(request, cx)
                }
                PublicToolCall::ReadDesktopClipboard(_) => {
                    self.handle_public_mcp_desktop_clipboard_read(request, cx)
                }
                PublicToolCall::WriteDesktopClipboard(_) => {
                    self.handle_public_mcp_desktop_clipboard_write(request, cx)
                }
                PublicToolCall::ReconnectDesktop(_) => {
                    self.handle_public_mcp_desktop_reconnect(request, cx)
                }
                PublicToolCall::CloseDesktop(_) => {
                    self.handle_public_mcp_desktop_close(request, cx)
                }
                PublicToolCall::QuickCommandsSave(_) => {
                    self.handle_public_mcp_quick_commands_save(request, cx)
                }
                PublicToolCall::QuickCommandsRemove(_) => {
                    self.handle_public_mcp_quick_commands_remove(request, cx)
                }
                PublicToolCall::QuickCommandsRun(_) => {
                    self.handle_public_mcp_quick_commands_run(request)
                }
                PublicToolCall::AddonsList(_) => self.handle_public_mcp_addons_list(request, cx),
                PublicToolCall::AddonsSetEnabled(_) => {
                    self.handle_public_mcp_addons_set_enabled(request, cx)
                }
                PublicToolCall::AddonsRemove(_) => {
                    self.handle_public_mcp_addons_remove(request, cx)
                }
                PublicToolCall::WorkspaceMount(_) => {
                    let owner = self
                        .ide_workspace
                        .read(cx)
                        .automation_file_system()
                        .scoped_owner();
                    self.public_mcp
                        .background()
                        .handle_public_mcp_workspace_mount(request, owner);
                }
                _ => request.finish(ToolEnvelope::failed("The request has no window handler")),
            }
        });
    }

    fn handle_public_mcp_connect_node(&mut self, request: DomainRequest, cx: &mut Context<Self>) {
        let PublicToolCall::ConnectNode(args) = &request.call else {
            return;
        };
        let Some(connection_id) = self.public_mcp.connection_id(
            &request.client_ref,
            &args.connection_ref,
            &self.connection_store,
        ) else {
            request.finish(ToolEnvelope::failed("The connection handle is unavailable"));
            return;
        };
        if !self.public_mcp.node_capacity_available(&request.client_ref) {
            request.finish(ToolEnvelope::failed(
                "The retained SSH node lease limit has been reached",
            ));
            return;
        }
        let Some(connection) = self.connection_store.get(&connection_id).cloned() else {
            request.finish(ToolEnvelope::failed(
                "The saved connection no longer exists",
            ));
            return;
        };
        let Some(config) = ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &connection,
        ) else {
            request.finish(ToolEnvelope::failed(
                "The saved connection requires credentials that are not available",
            ));
            return;
        };
        // An approved MCP attempt participates in the normal recent-connection ordering.
        let _ = self.connection_store.mark_used(&connection_id);
        let node_id = if config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
        {
            match self.expand_saved_connection_tree(&connection_id, config, connection.name) {
                Ok(expansion) => expansion.target_node_id,
                Err(_) => {
                    request.finish(ToolEnvelope::failed(
                        "The saved SSH route could not be prepared",
                    ));
                    return;
                }
            }
        } else {
            self.materialize_ssh_root_node(config, connection.name, Some(connection_id.clone()))
        };
        if !self.ensure_node_connection_started(&node_id, cx) {
            request.finish(ToolEnvelope::failed(
                "The SSH node could not start connecting",
            ));
            return;
        }

        self.public_mcp
            .background()
            .acquire_node_lease(request, node_id, connection_id);
    }

    fn handle_public_mcp_disconnect_node(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        if request.is_cancelled() {
            return;
        }
        self.enqueue_public_mcp_node_window_effect(
            PublicMcpNodeWindowEffect::Disconnect(request),
            cx,
        );
    }

    pub(in crate::workspace) fn apply_public_mcp_node_window_effect(
        &mut self,
        effect: PublicMcpNodeWindowEffect,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            PublicMcpNodeWindowEffect::Disconnect(request) => {
                self.apply_public_mcp_disconnect_node(request, window, cx)
            }
        }
    }

    fn apply_public_mcp_disconnect_node(
        &mut self,
        request: DomainRequest,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if request.is_cancelled() {
            return;
        }
        let PublicToolCall::DisconnectNode(args) = &request.call else {
            return;
        };
        let Some(lease) = self
            .public_mcp
            .node_lease(&request.client_ref, &args.node_ref)
        else {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let mut disconnected = self.node_router.subtree_postorder(&lease.node_id);
        if disconnected.is_empty() {
            disconnected.push(lease.node_id.clone());
        }
        // Reuse the product disconnect path so visible tabs, forwarding owners,
        // runtime tasks, and the physical NodeRouter subtree close together.
        self.disconnect_ssh_node(&lease.node_id, window, cx);
        self.public_mcp
            .background()
            .finish_node_disconnect(request, disconnected);
    }
}
