use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use base64::Engine;
use gpui::{App, Context, Task};
use oxideterm_connections::{ConnectionInfo, ConnectionStore};
use oxideterm_gpui_terminal::{TerminalNotice, TerminalNoticeVariant};
use oxideterm_plugin_registry as plugin_host;
use oxideterm_public_mcp::{
    AddonRef, ApprovalRef, ApprovalStatus, AuditQuery, ClientApprovalMode, ClientCredential,
    ClientProjection, ClientRef, ClientRegistry, CommandRef, ConnectionRef, DomainBroker,
    DomainMessage, DomainRequest, DomainRequestReceiver, ForwardRef, NodeRef, PublicMcpHttpServer,
    PublicMcpState, PublicToolCall, QuickCommandRef, ToolEnvelope, ToolGroup, ToolOutcome,
    start_http_server,
};
use oxideterm_session_adapter::ssh_config_from_saved_connection;
use oxideterm_ssh::{ConnectionConsumer, NodeId, NodeRouter, SshTransportError};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::WorkspaceApp;

mod addons;
mod forwards;
mod host_tools;
mod quick_commands;

const PUBLIC_MCP_CLIENTS_FILE: &str = "public-mcp-clients.json";
const PUBLIC_MCP_ENDPOINT_FILE: &str = "public-mcp-endpoint.json";
const PUBLIC_MCP_BROKER_CAPACITY: usize = 64;
const PUBLIC_MCP_COMMAND_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const PUBLIC_MCP_COMMAND_OUTPUT_LIMIT: usize = 1024 * 1024;
const PUBLIC_MCP_OUTPUT_PAGE_LIMIT: usize = 256 * 1024;
// Bound retained command output even when an authorized client never releases its node lease.
const PUBLIC_MCP_COMMAND_CAPACITY: usize = 256;
const PUBLIC_MCP_COMMAND_CAPACITY_PER_CLIENT: usize = 64;

pub(in crate::workspace) struct PublicMcpWorkspaceBridge {
    endpoint_url: Option<String>,
    startup_error: Option<String>,
    server: Option<PublicMcpHttpServer>,
    state: Arc<PublicMcpState>,
    settings_path: PathBuf,
    receiver: Option<DomainRequestReceiver>,
    delivery_task: Option<Task<()>>,
    revealed_credential: Option<ClientCredential>,
    // Public connection references are client-scoped and never encode saved connection IDs.
    connection_refs: HashMap<(ClientRef, String), ConnectionRef>,
    connection_ids: HashMap<ConnectionRef, (ClientRef, String)>,
    quick_command_refs: HashMap<(ClientRef, String), QuickCommandRef>,
    quick_command_ids: HashMap<QuickCommandRef, (ClientRef, String)>,
    addon_refs: HashMap<(ClientRef, String), AddonRef>,
    addon_ids: HashMap<AddonRef, (ClientRef, String)>,
    runtime_handles: Arc<Mutex<PublicMcpRuntimeHandles>>,
}

#[derive(Default)]
struct PublicMcpRuntimeHandles {
    nodes: HashMap<NodeRef, PublicMcpNodeLease>,
    commands: HashMap<CommandRef, PublicMcpCommandRecord>,
    forwards: HashMap<ForwardRef, PublicMcpForwardRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PublicMcpEndpointState {
    version: u32,
    port: u16,
}

#[derive(Clone)]
struct PublicMcpNodeLease {
    client_ref: ClientRef,
    node_id: NodeId,
    saved_connection_id: Option<String>,
    physical_connection_id: Option<String>,
    consumer: ConnectionConsumer,
}

#[derive(Clone)]
struct PublicMcpForwardRecord {
    client_ref: ClientRef,
    node_ref: NodeRef,
    node_id: NodeId,
    owner_connection_id: Option<String>,
    forward_id: String,
    created_by_client: bool,
    persisted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PublicMcpCommandState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

struct PublicMcpCommandRecord {
    client_ref: ClientRef,
    node_ref: NodeRef,
    owner_group: ToolGroup,
    state: PublicMcpCommandState,
    stdout: Zeroizing<Vec<u8>>,
    stderr: Zeroizing<Vec<u8>>,
    exit_code: Option<i32>,
    truncated: bool,
    error: Option<String>,
    cancellation: CancellationToken,
}

#[derive(Serialize)]
struct PublicConnectionProjection {
    connection_ref: ConnectionRef,
    name: String,
    group: Option<String>,
    host: String,
    port: u16,
    username: String,
    tags: Vec<String>,
    last_used_at: Option<String>,
}

#[derive(Serialize)]
struct PublicConnectionDirectoryEntry {
    connection_ref: ConnectionRef,
    name: String,
    group: Option<String>,
    connection_type: &'static str,
    tags: Vec<String>,
    last_used_at: Option<String>,
}

impl PublicMcpWorkspaceBridge {
    pub(in crate::workspace) fn start(
        settings_path: &Path,
        runtime: &tokio::runtime::Handle,
    ) -> Self {
        let clients_path = settings_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(PUBLIC_MCP_CLIENTS_FILE);
        let endpoint_state_path = settings_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(PUBLIC_MCP_ENDPOINT_FILE);
        let (clients, registry_error) = match ClientRegistry::open(clients_path) {
            Ok(clients) => (Arc::new(clients), None),
            Err(error) => (Arc::new(ClientRegistry::default()), Some(error.to_string())),
        };
        let (broker, receiver) = DomainBroker::channel(PUBLIC_MCP_BROKER_CAPACITY);
        let state = Arc::new(PublicMcpState {
            clients,
            approvals: Arc::default(),
            audit: Arc::new(oxideterm_public_mcp::AuditStore::new(2_048)),
            artifacts: Arc::default(),
            broker,
        });
        let preferred_port = read_endpoint_port(&endpoint_state_path).unwrap_or(0);
        let (server, endpoint_url, server_error) = if registry_error.is_none() {
            let started =
                start_http_server(runtime, state.clone(), preferred_port).or_else(|first_error| {
                    if preferred_port == 0 {
                        Err(first_error)
                    } else {
                        start_http_server(runtime, state.clone(), 0)
                    }
                });
            match started {
                Ok(server) => {
                    let endpoint_url = Some(server.endpoint_url());
                    // A persistence failure must not hide a healthy live endpoint.
                    let _ = persist_endpoint_port(&endpoint_state_path, server.port());
                    (Some(server), endpoint_url, None)
                }
                Err(error) => (None, None, Some(error.to_string())),
            }
        } else {
            (None, None, None)
        };
        Self {
            endpoint_url,
            startup_error: registry_error.or(server_error),
            server,
            state,
            settings_path: settings_path.to_path_buf(),
            receiver: Some(receiver),
            delivery_task: None,
            revealed_credential: None,
            connection_refs: HashMap::new(),
            connection_ids: HashMap::new(),
            quick_command_refs: HashMap::new(),
            quick_command_ids: HashMap::new(),
            addon_refs: HashMap::new(),
            addon_ids: HashMap::new(),
            runtime_handles: Arc::default(),
        }
    }

    pub(in crate::workspace) fn endpoint_url(&self) -> Option<&str> {
        self.endpoint_url.as_deref()
    }

    pub(in crate::workspace) fn startup_error(&self) -> Option<&str> {
        self.startup_error.as_deref()
    }

    pub(in crate::workspace) fn record_action_error(&mut self, error: String) {
        self.startup_error = Some(error);
    }

    pub(in crate::workspace) fn clients(&self) -> Vec<ClientProjection> {
        self.state.clients.list()
    }

    pub(in crate::workspace) fn approvals(&self) -> Vec<oxideterm_public_mcp::ApprovalProjection> {
        self.state.approvals.list()
    }

    pub(in crate::workspace) fn revealed_credential(&self) -> Option<&str> {
        self.revealed_credential
            .as_ref()
            .map(ClientCredential::expose)
    }

    pub(in crate::workspace) fn create_client(
        &mut self,
        label: String,
        approval_mode: ClientApprovalMode,
    ) -> Result<(), String> {
        let registered = self
            .state
            .clients
            .register(label, approval_mode, all_tool_groups())
            .map_err(|error| error.to_string())?;
        self.revealed_credential = Some(registered.credential);
        self.startup_error = None;
        Ok(())
    }

    pub(in crate::workspace) fn dismiss_revealed_credential(&mut self) {
        self.revealed_credential.take();
    }

    pub(in crate::workspace) fn set_client_enabled(
        &self,
        client_ref: &ClientRef,
        enabled: bool,
    ) -> Result<(), String> {
        self.state
            .clients
            .set_enabled(client_ref, enabled)
            .map_err(|error| error.to_string())
    }

    pub(in crate::workspace) fn set_client_approval_mode(
        &self,
        client_ref: &ClientRef,
        approval_mode: ClientApprovalMode,
    ) -> Result<(), String> {
        self.state
            .clients
            .set_approval_mode(client_ref, approval_mode)
            .map_err(|error| error.to_string())
    }

    pub(in crate::workspace) fn set_client_tool_group(
        &self,
        client_ref: &ClientRef,
        tool_group: ToolGroup,
        enabled: bool,
    ) -> Result<(), String> {
        let Some(client) = self.state.clients.get(client_ref) else {
            return Err("The external MCP client no longer exists".to_owned());
        };
        let mut tool_groups = client.tool_groups;
        if enabled {
            tool_groups.insert(tool_group);
        } else if tool_group != ToolGroup::Basic {
            tool_groups.remove(&tool_group);
        }
        self.state
            .clients
            .set_groups(client_ref, tool_groups)
            .map_err(|error| error.to_string())
    }

    pub(in crate::workspace) fn remove_client(&self, client_ref: &ClientRef) -> Result<(), String> {
        self.state
            .clients
            .remove(client_ref)
            .map_err(|error| error.to_string())
    }

    pub(in crate::workspace) fn set_approval_status(
        &mut self,
        approval_ref: &ApprovalRef,
        status: ApprovalStatus,
    ) -> Result<(), String> {
        let result = self
            .state
            .approvals
            .set_status(approval_ref, status)
            .map_err(|error| error.to_string());
        if result.is_ok() {
            self.startup_error = None;
        }
        result
    }

    fn take_receiver(&mut self) -> Option<DomainRequestReceiver> {
        self.receiver.take()
    }

    fn set_delivery_task(&mut self, task: Task<()>) {
        self.delivery_task = Some(task);
    }

    fn connection_id(
        &mut self,
        client_ref: &ClientRef,
        connection_ref: &ConnectionRef,
        store: &ConnectionStore,
    ) -> Option<String> {
        self.sync_connection_refs(client_ref, store);
        self.connection_ids
            .get(connection_ref)
            .filter(|(owner, _)| owner == client_ref)
            .map(|(_, connection_id)| connection_id.clone())
    }

    fn connection_projection(
        &mut self,
        client_ref: &ClientRef,
        info: ConnectionInfo,
    ) -> PublicConnectionProjection {
        let connection_key = (client_ref.clone(), info.id.clone());
        let connection_ref = self
            .connection_refs
            .entry(connection_key)
            .or_default()
            .clone();
        self.connection_ids
            .entry(connection_ref.clone())
            .or_insert_with(|| (client_ref.clone(), info.id.clone()));
        PublicConnectionProjection {
            connection_ref,
            name: info.name,
            group: info.group,
            host: info.host,
            port: info.port,
            username: info.username,
            tags: info.tags,
            last_used_at: info.last_used_at,
        }
    }

    fn connection_directory_entry(
        &mut self,
        client_ref: &ClientRef,
        info: ConnectionInfo,
    ) -> PublicConnectionDirectoryEntry {
        let connection_key = (client_ref.clone(), info.id.clone());
        let connection_ref = self
            .connection_refs
            .entry(connection_key)
            .or_default()
            .clone();
        self.connection_ids
            .entry(connection_ref.clone())
            .or_insert((client_ref.clone(), info.id));
        PublicConnectionDirectoryEntry {
            connection_ref,
            name: info.name,
            group: info.group,
            connection_type: "ssh",
            tags: info.tags,
            last_used_at: info.last_used_at,
        }
    }

    fn sync_connection_refs(&mut self, client_ref: &ClientRef, store: &ConnectionStore) {
        for info in store.connection_infos() {
            let connection_key = (client_ref.clone(), info.id.clone());
            let connection_ref = self
                .connection_refs
                .entry(connection_key)
                .or_default()
                .clone();
            self.connection_ids
                .entry(connection_ref)
                .or_insert((client_ref.clone(), info.id));
        }
    }

    fn remove_client_connection_refs(&mut self, client_ref: &ClientRef) {
        let removed_refs = self
            .connection_refs
            .extract_if(|(owner, _), _| owner == client_ref)
            .map(|(_, connection_ref)| connection_ref)
            .collect::<HashSet<_>>();
        self.connection_ids
            .retain(|connection_ref, _| !removed_refs.contains(connection_ref));
    }

    fn remove_client_quick_command_refs(&mut self, client_ref: &ClientRef) {
        let removed_refs = self
            .quick_command_refs
            .extract_if(|(owner, _), _| owner == client_ref)
            .map(|(_, quickcommand_ref)| quickcommand_ref)
            .collect::<HashSet<_>>();
        self.quick_command_ids
            .retain(|quickcommand_ref, _| !removed_refs.contains(quickcommand_ref));
    }

    fn target_label(
        &self,
        client_ref: &ClientRef,
        target: &str,
        store: &ConnectionStore,
        node_router: &NodeRouter,
        plugin_registry: &plugin_host::NativePluginRegistry,
        forwarding_service: &super::forwards::ForwardingRuntimeService,
    ) -> String {
        if let Some(quickcommand_ref) = target
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<QuickCommandRef>().ok())
            && let Some((owner, command_id)) = self.quick_command_ids.get(&quickcommand_ref)
            && owner == client_ref
            && let Ok(snapshot) = oxideterm_quick_commands::load_snapshot(&self.settings_path)
            && let Some(command) = snapshot
                .commands
                .into_iter()
                .find(|command| &command.id == command_id)
        {
            // The full saved command is shown only in the local approval UI.
            return format!("{} — {}", command.name, command.command);
        }
        if let Ok(connection_ref) = target.parse::<ConnectionRef>()
            && let Some((owner, connection_id)) = self.connection_ids.get(&connection_ref)
            && owner == client_ref
            && let Some(connection) = store.get(connection_id)
        {
            return format!(
                "{} ({}@{}:{})",
                connection.name, connection.username, connection.host, connection.port
            );
        }
        let (addon_target, addon_action) = target.split_once(' ').unwrap_or((target, ""));
        if let Ok(addon_ref) = addon_target.parse::<AddonRef>()
            && let Some((owner, plugin_id)) = self.addon_ids.get(&addon_ref)
            && owner == client_ref
            && let Some(plugin) = plugin_registry
                .plugins()
                .iter()
                .find(|plugin| &plugin.manifest.id == plugin_id)
        {
            return format!(
                "{} ({}) {}",
                plugin.manifest.name, plugin.manifest.id, addon_action
            )
            .trim()
            .to_owned();
        }
        let (forward_target, forward_action) = target.split_once(' ').unwrap_or((target, ""));
        if let Ok(forward_ref) = forward_target.parse::<ForwardRef>()
            && let Some(record) = self
                .runtime_handles
                .lock()
                .forwards
                .get(&forward_ref)
                .filter(|record| record.client_ref == *client_ref)
                .cloned()
            && let Some(rule) = forwarding_service
                .public_mcp_rules_for_node(&record.node_id)
                .into_iter()
                .find(|rule| rule.id == record.forward_id)
        {
            let destination = match rule.forward_type {
                oxideterm_forwarding::ForwardType::Dynamic => "SOCKS".to_owned(),
                oxideterm_forwarding::ForwardType::Local
                | oxideterm_forwarding::ForwardType::Remote => {
                    format!("{}:{}", rule.target_host, rule.target_port)
                }
            };
            return format!(
                "{}:{} → {} {}",
                rule.bind_address, rule.bind_port, destination, forward_action
            )
            .trim()
            .to_owned();
        }
        let node_target = target.split_whitespace().next().unwrap_or(target);
        if let Ok(node_ref) = node_target.parse::<NodeRef>()
            && let Some(lease) = self.runtime_handles.lock().nodes.get(&node_ref).cloned()
            && let Some(metadata) = node_router.node_metadata(&lease.node_id)
        {
            return format!("{}@{}:{}", metadata.username, metadata.host, metadata.port);
        }
        target.to_owned()
    }
}

impl Drop for PublicMcpWorkspaceBridge {
    fn drop(&mut self) {
        self.delivery_task.take();
        self.revealed_credential.take();
        self.server.take();
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn start_public_mcp_delivery(&mut self, cx: &mut Context<Self>) {
        let Some(mut receiver) = self.public_mcp.take_receiver() else {
            return;
        };
        let task = cx.spawn(async move |workspace, cx| {
            while let Some(message) = receiver.recv().await {
                if workspace
                    .update(cx, |workspace, cx| match message {
                        DomainMessage::Request(request) => {
                            workspace.handle_public_mcp_request(*request, cx)
                        }
                        DomainMessage::StateChanged => {
                            workspace.notify_public_mcp_approval(cx);
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.public_mcp.set_delivery_task(task);
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
        client_ref: &ClientRef,
        enabled: bool,
    ) -> Result<(), String> {
        self.public_mcp.set_client_enabled(client_ref, enabled)?;
        if !enabled {
            self.revoke_public_mcp_client_runtime(client_ref);
            self.public_mcp.remove_client_connection_refs(client_ref);
            self.public_mcp.remove_client_quick_command_refs(client_ref);
            self.public_mcp.remove_client_addon_refs(client_ref);
        }
        Ok(())
    }

    pub(in crate::workspace) fn set_public_mcp_client_approval_mode(
        &mut self,
        client_ref: &ClientRef,
        approval_mode: ClientApprovalMode,
    ) -> Result<(), String> {
        self.public_mcp
            .set_client_approval_mode(client_ref, approval_mode)?;
        // A mode transition cannot inherit actions or runtime handles from the old policy.
        self.revoke_public_mcp_client_runtime(client_ref);
        Ok(())
    }

    pub(in crate::workspace) fn set_public_mcp_client_tool_group(
        &mut self,
        client_ref: &ClientRef,
        tool_group: ToolGroup,
        enabled: bool,
    ) -> Result<(), String> {
        self.public_mcp
            .set_client_tool_group(client_ref, tool_group, enabled)?;
        if !enabled {
            self.public_mcp
                .state
                .approvals
                .revoke_client_tool_group(client_ref, tool_group);
            match tool_group {
                ToolGroup::NodeSession => self.revoke_public_mcp_client_runtime(client_ref),
                ToolGroup::CommandExecute => self.public_mcp.revoke_client_commands(client_ref),
                ToolGroup::QuickCommandExecute => self
                    .public_mcp
                    .revoke_client_commands_for_group(client_ref, ToolGroup::QuickCommandExecute),
                ToolGroup::ArtifactTransfer => {
                    self.public_mcp.state.artifacts.revoke_client(client_ref)
                }
                ToolGroup::ForwardManage => self.revoke_public_mcp_client_forwards(client_ref),
                ToolGroup::Basic
                | ToolGroup::ConnectionDirectory
                | ToolGroup::ConnectionRead
                | ToolGroup::CommandObserve
                | ToolGroup::AuditRead
                | ToolGroup::HostToolsObserve
                | ToolGroup::HostToolsOperate
                | ToolGroup::QuickCommandRead
                | ToolGroup::QuickCommandContentRead
                | ToolGroup::QuickCommandManage
                | ToolGroup::AddonRead
                | ToolGroup::AddonManage
                | ToolGroup::ForwardRead => {}
            }
        }
        Ok(())
    }

    pub(in crate::workspace) fn remove_public_mcp_client(
        &mut self,
        client_ref: &ClientRef,
    ) -> Result<(), String> {
        self.public_mcp.remove_client(client_ref)?;
        self.revoke_public_mcp_client_runtime(client_ref);
        self.public_mcp.remove_client_connection_refs(client_ref);
        self.public_mcp.remove_client_quick_command_refs(client_ref);
        self.public_mcp.remove_client_addon_refs(client_ref);
        Ok(())
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

    pub(in crate::workspace) fn suspend_public_mcp_runtime(&self) {
        // Locking the workspace invalidates approvals and releases only MCP-owned consumers.
        for client in self.public_mcp.clients() {
            self.revoke_public_mcp_client_runtime(&client.client_ref);
        }
    }

    fn revoke_public_mcp_client_runtime(&self, client_ref: &ClientRef) {
        self.revoke_public_mcp_client_forwards(client_ref);
        self.public_mcp
            .revoke_client_runtime(client_ref, &self.node_router);
    }

    fn revoke_public_mcp_client_forwards(&self, client_ref: &ClientRef) {
        let records =
            forwards::revoke_client_forwards(&self.public_mcp.runtime_handles, client_ref);
        for record in records {
            let service = self.forwarding_service.clone();
            self.forwarding_runtime.spawn(async move {
                service
                    .public_mcp_revoke_forward(
                        &record.node_id,
                        &record.forward_id,
                        record.persisted,
                    )
                    .await;
            });
        }
    }

    fn handle_public_mcp_request(&mut self, request: DomainRequest, cx: &mut Context<Self>) {
        if request.is_cancelled() {
            return;
        }
        if self.app_lock.locked {
            request.finish(ToolEnvelope::failed("The OxideTerm workspace is locked"));
            return;
        }
        match &request.call {
            PublicToolCall::BrowseConnections(_) => {
                self.handle_public_mcp_browse_connections(request)
            }
            PublicToolCall::DescribeConnection(_) => {
                self.handle_public_mcp_describe_connection(request)
            }
            PublicToolCall::ConnectNode(_) => self.handle_public_mcp_connect_node(request, cx),
            PublicToolCall::InspectNode(_) => self.handle_public_mcp_inspect_node(request),
            PublicToolCall::ReleaseNode(_) => self.handle_public_mcp_release_node(request),
            PublicToolCall::DisconnectNode(_) => {
                self.handle_public_mcp_disconnect_node(request, cx)
            }
            PublicToolCall::StartCommand(_) => self.handle_public_mcp_start_command(request),
            PublicToolCall::CommandState(_) => self.handle_public_mcp_command_state(request),
            PublicToolCall::CommandOutput(_) => self.handle_public_mcp_command_output(request),
            PublicToolCall::CancelCommand(_) => self.handle_public_mcp_cancel_command(request),
            PublicToolCall::StageArtifact(_) => self.handle_public_mcp_stage_artifact(request),
            PublicToolCall::ReadArtifact(_) => self.handle_public_mcp_read_artifact(request),
            PublicToolCall::AuditSearch(_) => self.handle_public_mcp_audit_search(request),
            PublicToolCall::HostToolsCatalog(_) => {
                self.handle_public_mcp_host_tools_catalog(request)
            }
            PublicToolCall::HostToolsCapture(_) => {
                self.handle_public_mcp_host_tools_capture(request)
            }
            PublicToolCall::HostToolsOperate(_) => {
                self.handle_public_mcp_host_tools_operate(request)
            }
            PublicToolCall::QuickCommandsList(_) => {
                self.handle_public_mcp_quick_commands_list(request)
            }
            PublicToolCall::QuickCommandsDescribe(_) => {
                self.handle_public_mcp_quick_commands_describe(request)
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
            PublicToolCall::AddonsInstall(_) => self.handle_public_mcp_addons_install(request, cx),
            PublicToolCall::AddonsSetEnabled(_) => {
                self.handle_public_mcp_addons_set_enabled(request, cx)
            }
            PublicToolCall::AddonsRemove(_) => self.handle_public_mcp_addons_remove(request, cx),
            PublicToolCall::ForwardsList(_) => self.handle_public_mcp_forwards_list(request),
            PublicToolCall::ForwardsOpen(_) => self.handle_public_mcp_forwards_open(request),
            PublicToolCall::ForwardsChange(_) => self.handle_public_mcp_forwards_change(request),
            PublicToolCall::ForwardsStop(_) => self.handle_public_mcp_forwards_stop(request),
            PublicToolCall::ForwardsRestart(_) => self.handle_public_mcp_forwards_restart(request),
            PublicToolCall::ForwardsRemove(_) => self.handle_public_mcp_forwards_remove(request),
            PublicToolCall::ForwardsMetrics(_) => self.handle_public_mcp_forwards_metrics(request),
            PublicToolCall::ForwardsDiscoverPorts(_) => {
                self.handle_public_mcp_forwards_discover_ports(request)
            }
        }
    }

    fn handle_public_mcp_browse_connections(&mut self, request: DomainRequest) {
        let PublicToolCall::BrowseConnections(args) = &request.call else {
            return;
        };
        let query = args
            .query
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let allows_ssh = args.connection_types.is_empty()
            || args
                .connection_types
                .iter()
                .any(|connection_type| connection_type.eq_ignore_ascii_case("ssh"));
        if !allows_ssh {
            finish_serialized(request, json!({ "connections": [] }));
            return;
        }
        let connections = self
            .connection_store
            .connection_infos()
            .into_iter()
            .filter(|connection| connection_directory_matches_query(connection, &query))
            .map(|connection| {
                self.public_mcp
                    .connection_directory_entry(&request.client_ref, connection)
            })
            .collect::<Vec<_>>();
        finish_serialized(request, json!({ "connections": connections }));
    }

    fn handle_public_mcp_describe_connection(&mut self, request: DomainRequest) {
        let PublicToolCall::DescribeConnection(args) = &request.call else {
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
        let Some(info) = self
            .connection_store
            .connection_infos()
            .into_iter()
            .find(|connection| connection.id == connection_id)
        else {
            request.finish(ToolEnvelope::failed(
                "The saved connection no longer exists",
            ));
            return;
        };
        let projection = self
            .public_mcp
            .connection_projection(&request.client_ref, info);
        finish_serialized(request, json!({ "connection": projection }));
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
            match self.expand_saved_connection_tree(&connection_id, config, connection.name.clone())
            {
                Ok(expansion) => expansion.target_node_id,
                Err(_) => {
                    request.finish(ToolEnvelope::failed(
                        "The saved SSH route could not be prepared",
                    ));
                    return;
                }
            }
        } else {
            self.materialize_ssh_root_node(
                config,
                connection.name.clone(),
                Some(connection_id.clone()),
            )
        };
        if !self.ensure_node_connection_started(&node_id, cx) {
            request.finish(ToolEnvelope::failed(
                "The SSH node could not start connecting",
            ));
            return;
        }

        let node_ref = NodeRef::new();
        let consumer = ConnectionConsumer::PublicMcp(node_ref.to_string());
        let lease = PublicMcpNodeLease {
            client_ref: request.client_ref.clone(),
            node_id: node_id.clone(),
            saved_connection_id: Some(connection_id),
            physical_connection_id: None,
            consumer: consumer.clone(),
        };
        self.public_mcp
            .runtime_handles
            .lock()
            .nodes
            .insert(node_ref.clone(), lease);
        let router = self.node_router.clone();
        let handles = self.public_mcp.runtime_handles.clone();
        let request_cancellation = request.cancellation_token();
        self.forwarding_runtime.spawn(async move {
            let acquired = tokio::select! {
                _ = request_cancellation.cancelled() => {
                    handles.lock().nodes.remove(&node_ref);
                    return;
                }
                result = router.acquire_connection_wait(
                    &node_id,
                    consumer.clone(),
                    Duration::from_secs(30),
                ) => result,
            };
            match acquired {
                Ok(resolved) => {
                    let connection_id = resolved.connection_id;
                    if request_cancellation.is_cancelled() {
                        handles.lock().nodes.remove(&node_ref);
                        router.release_consumer(&connection_id, &consumer);
                        return;
                    }
                    let retained = if let Some(lease) = handles.lock().nodes.get_mut(&node_ref) {
                        lease.physical_connection_id = Some(connection_id.clone());
                        true
                    } else {
                        false
                    };
                    if !retained {
                        // Revocation may race an in-flight connection attempt.
                        router.release_consumer(&connection_id, &consumer);
                        request.finish(ToolEnvelope::failed(
                            "The MCP client was revoked while connecting",
                        ));
                        return;
                    }
                    finish_serialized(request, json!({ "node_ref": node_ref, "state": "ready" }));
                }
                Err(_) => {
                    handles.lock().nodes.remove(&node_ref);
                    request.finish(ToolEnvelope::failed("The SSH node did not become ready"));
                }
            }
        });
    }

    fn handle_public_mcp_inspect_node(&self, request: DomainRequest) {
        let PublicToolCall::InspectNode(args) = &request.call else {
            return;
        };
        let Some(lease) = node_lease_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &args.node_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let state = self.node_router.node_state(&lease.node_id);
        let metadata = self.node_router.node_metadata(&lease.node_id);
        let node_ref = args.node_ref.clone();
        match (state, metadata) {
            (Ok(state), Some(metadata)) => finish_serialized(
                request,
                json!({
                    "node_ref": node_ref,
                    "readiness": state.state.readiness,
                    "host": metadata.host,
                    "port": metadata.port,
                    "username": metadata.username,
                }),
            ),
            (Err(_), _) => request.finish(ToolEnvelope::failed("The node state is unavailable")),
            (_, None) => request.finish(ToolEnvelope::failed("The node no longer exists")),
        }
    }

    fn handle_public_mcp_release_node(&self, request: DomainRequest) {
        let PublicToolCall::ReleaseNode(args) = &request.call else {
            return;
        };
        let (lease, cancellations) = {
            let mut handles = self.public_mcp.runtime_handles.lock();
            let owned = handles
                .nodes
                .get(&args.node_ref)
                .is_some_and(|lease| lease.client_ref == request.client_ref);
            if !owned {
                drop(handles);
                request.finish(ToolEnvelope::failed("The node handle is unavailable"));
                return;
            }
            let lease = handles
                .nodes
                .remove(&args.node_ref)
                .expect("node lease ownership was checked");
            let command_refs = handles
                .commands
                .iter()
                .filter_map(|(command_ref, record)| {
                    (record.client_ref == request.client_ref && record.node_ref == args.node_ref)
                        .then_some(command_ref.clone())
                })
                .collect::<Vec<_>>();
            let cancellations = command_refs
                .into_iter()
                .filter_map(|command_ref| handles.commands.remove(&command_ref))
                .map(|record| record.cancellation)
                .collect::<Vec<_>>();
            (lease, cancellations)
        };
        for cancellation in cancellations {
            cancellation.cancel();
        }
        if let Some(connection_id) = lease.physical_connection_id {
            self.node_router
                .release_consumer(&connection_id, &lease.consumer);
        }
        finish_serialized(
            request,
            json!({ "released": true, "physical_node_disconnected": false }),
        );
    }

    fn handle_public_mcp_disconnect_node(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::DisconnectNode(args) = &request.call else {
            return;
        };
        let Some(lease) = node_lease_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &args.node_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let disconnected = self.workspace_runtime.update(cx, |runtime, cx| {
            runtime.disconnect_node_runtime_subtree(&lease.node_id, cx)
        });
        let mut handles = self.public_mcp.runtime_handles.lock();
        let disconnected_node_refs = handles
            .nodes
            .iter()
            .filter_map(|(node_ref, candidate)| {
                disconnected
                    .contains(&candidate.node_id)
                    .then_some(node_ref.clone())
            })
            .collect::<HashSet<_>>();
        for node_ref in &disconnected_node_refs {
            handles.nodes.remove(node_ref);
        }
        let command_refs = handles
            .commands
            .iter()
            .filter_map(|(command_ref, record)| {
                disconnected_node_refs
                    .contains(&record.node_ref)
                    .then_some(command_ref.clone())
            })
            .collect::<Vec<_>>();
        let cancellations = command_refs
            .into_iter()
            .filter_map(|command_ref| handles.commands.remove(&command_ref))
            .map(|record| record.cancellation)
            .collect::<Vec<_>>();
        forwards::invalidate_for_disconnected_nodes(&mut handles, &disconnected);
        drop(handles);
        for cancellation in cancellations {
            cancellation.cancel();
        }
        finish_serialized(
            request,
            json!({
                "disconnected": !disconnected.is_empty(),
                "invalidated_node_handles": disconnected_node_refs.len(),
            }),
        );
    }

    fn handle_public_mcp_start_command(&self, request: DomainRequest) {
        let PublicToolCall::StartCommand(args) = &request.call else {
            return;
        };
        let node_ref = args.node_ref.clone();
        let command = command_for_working_directory(
            &args.command,
            args.working_directory
                .as_ref()
                .map(|directory| directory.as_str()),
        );
        self.start_public_mcp_node_command(request, node_ref, command, ToolGroup::CommandExecute);
    }

    pub(super) fn start_public_mcp_node_command(
        &self,
        request: DomainRequest,
        node_ref: NodeRef,
        command: Zeroizing<String>,
        owner_group: ToolGroup,
    ) {
        let Some(lease) = node_lease_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &node_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let command_ref = CommandRef::new();
        let cancellation = CancellationToken::new();
        let mut handles = self.public_mcp.runtime_handles.lock();
        let client_command_count = handles
            .commands
            .values()
            .filter(|record| record.client_ref == request.client_ref)
            .count();
        if handles.commands.len() >= PUBLIC_MCP_COMMAND_CAPACITY
            || client_command_count >= PUBLIC_MCP_COMMAND_CAPACITY_PER_CLIENT
        {
            drop(handles);
            request.finish(ToolEnvelope::failed(
                "The retained command limit was reached; release an unused node lease first",
            ));
            return;
        }
        handles.commands.insert(
            command_ref.clone(),
            PublicMcpCommandRecord {
                client_ref: request.client_ref.clone(),
                node_ref,
                owner_group,
                state: PublicMcpCommandState::Running,
                stdout: Zeroizing::new(Vec::new()),
                stderr: Zeroizing::new(Vec::new()),
                exit_code: None,
                truncated: false,
                error: None,
                cancellation: cancellation.clone(),
            },
        );
        drop(handles);

        let router = self.node_router.clone();
        let handles = self.public_mcp.runtime_handles.clone();
        let command_ref_for_task = command_ref.clone();
        self.forwarding_runtime.spawn(async move {
            let resolved = tokio::select! {
                _ = cancellation.cancelled() => return,
                result = router.resolve_connection(&lease.node_id) => result,
            };
            let result = match resolved {
                Ok(resolved) => {
                    tokio::select! {
                        _ = cancellation.cancelled() => None,
                        result = resolved.handle.run_secret_command_capture(
                            command.as_str(),
                            PUBLIC_MCP_COMMAND_TIMEOUT,
                            PUBLIC_MCP_COMMAND_OUTPUT_LIMIT,
                        ) => Some(result.map_err(public_command_error)),
                    }
                }
                Err(_) => Some(Err("The SSH node is no longer ready".to_owned())),
            };
            let Some(result) = result else {
                return;
            };
            let mut handles = handles.lock();
            let Some(record) = handles.commands.get_mut(&command_ref_for_task) else {
                return;
            };
            if record.state != PublicMcpCommandState::Running {
                return;
            }
            match result {
                Ok(output) => {
                    record.stdout = output.stdout;
                    record.stderr = output.stderr;
                    record.exit_code = output.exit_code;
                    record.truncated = output.truncated;
                    if output.exit_code == Some(0) {
                        record.state = PublicMcpCommandState::Succeeded;
                    } else {
                        record.state = PublicMcpCommandState::Failed;
                        record.error = Some(match output.exit_code {
                            Some(exit_code) => {
                                format!("Remote command exited with status {exit_code}")
                            }
                            None => "Remote command ended without an exit status".to_owned(),
                        });
                    }
                }
                Err(error) => {
                    record.error = Some(error);
                    record.state = PublicMcpCommandState::Failed;
                }
            }
        });
        finish_serialized(
            request,
            json!({ "command_ref": command_ref, "state": "running" }),
        );
    }

    fn handle_public_mcp_command_state(&self, request: DomainRequest) {
        let PublicToolCall::CommandState(args) = &request.call else {
            return;
        };
        let handles = self.public_mcp.runtime_handles.lock();
        let Some(record) = handles
            .commands
            .get(&args.command_ref)
            .filter(|record| record.client_ref == request.client_ref)
        else {
            request.finish(ToolEnvelope::failed("The command handle is unavailable"));
            return;
        };
        let command_ref = args.command_ref.clone();
        let error = record.error.clone();
        finish_serialized(
            request,
            json!({
                "command_ref": command_ref,
                "state": record.state,
                "exit_code": record.exit_code,
                "truncated": record.truncated,
                "error": error,
            }),
        );
    }

    fn handle_public_mcp_command_output(&self, request: DomainRequest) {
        let PublicToolCall::CommandOutput(args) = &request.call else {
            return;
        };
        let handles = self.public_mcp.runtime_handles.lock();
        let Some(record) = handles
            .commands
            .get(&args.command_ref)
            .filter(|record| record.client_ref == request.client_ref)
        else {
            request.finish(ToolEnvelope::failed("The command handle is unavailable"));
            return;
        };
        let offset = usize::try_from(args.offset).unwrap_or(usize::MAX);
        let limit = usize::try_from(args.limit)
            .unwrap_or(PUBLIC_MCP_OUTPUT_PAGE_LIMIT)
            .min(PUBLIC_MCP_OUTPUT_PAGE_LIMIT);
        let stdout = output_page(&record.stdout, offset, limit);
        let stderr = output_page(&record.stderr, offset, limit);
        let command_ref = args.command_ref.clone();
        finish_serialized(
            request,
            json!({
                "command_ref": command_ref,
                "state": record.state,
                "offset": offset,
                "stdout": stdout,
                "stderr": stderr,
                "stdout_size": record.stdout.len(),
                "stderr_size": record.stderr.len(),
                "truncated": record.truncated,
            }),
        );
    }

    fn handle_public_mcp_cancel_command(&self, request: DomainRequest) {
        let PublicToolCall::CancelCommand(args) = &request.call else {
            return;
        };
        let mut handles = self.public_mcp.runtime_handles.lock();
        let Some(record) = handles
            .commands
            .get_mut(&args.command_ref)
            .filter(|record| record.client_ref == request.client_ref)
        else {
            request.finish(ToolEnvelope::failed("The command handle is unavailable"));
            return;
        };
        let cancelled = record.state == PublicMcpCommandState::Running;
        if cancelled {
            record.cancellation.cancel();
            record.state = PublicMcpCommandState::Cancelled;
        }
        finish_serialized(request, json!({ "cancelled": cancelled }));
    }

    fn handle_public_mcp_stage_artifact(&self, request: DomainRequest) {
        let PublicToolCall::StageArtifact(args) = &request.call else {
            return;
        };
        match self.public_mcp.state.artifacts.stage(
            request.client_ref.clone(),
            &args.bytes,
            args.media_type.clone(),
            args.name.clone(),
        ) {
            Ok(artifact) => finish_serialized(request, json!({ "artifact": artifact })),
            Err(error) => request.finish(ToolEnvelope::failed(error.to_string())),
        }
    }

    fn handle_public_mcp_read_artifact(&self, request: DomainRequest) {
        let PublicToolCall::ReadArtifact(args) = &request.call else {
            return;
        };
        match self.public_mcp.state.artifacts.read(
            &request.client_ref,
            &args.artifact_ref,
            args.offset,
            args.length,
        ) {
            Ok(page) => {
                // Only the requested bounded page crosses the protocol boundary.
                let bytes_base64 =
                    base64::engine::general_purpose::STANDARD.encode(page.bytes.as_slice());
                finish_serialized(
                    request,
                    json!({
                        "artifact": page.projection,
                        "offset": page.offset,
                        "bytes_base64": bytes_base64,
                        "next_offset": page.next_offset,
                    }),
                );
            }
            Err(error) => request.finish(ToolEnvelope::failed(error.to_string())),
        }
    }

    fn handle_public_mcp_audit_search(&self, request: DomainRequest) {
        let PublicToolCall::AuditSearch(args) = &request.call else {
            return;
        };
        let page = self.public_mcp.state.audit.search(
            &request.client_ref,
            AuditQuery {
                after_ms: args.after_ms,
                before_ms: args.before_ms,
                tool_name: args.tool.as_deref(),
                target: args.target_ref.as_deref(),
                cursor: args.cursor.as_ref(),
                limit: args.limit as usize,
            },
        );
        finish_serialized(request, json!(page));
    }
}

impl PublicMcpWorkspaceBridge {
    fn revoke_client_commands(&self, client_ref: &ClientRef) {
        let cancellations = {
            let mut handles = self.runtime_handles.lock();
            let command_refs = handles
                .commands
                .iter()
                .filter_map(|(command_ref, record)| {
                    (&record.client_ref == client_ref).then_some(command_ref.clone())
                })
                .collect::<Vec<_>>();
            command_refs
                .into_iter()
                .filter_map(|command_ref| handles.commands.remove(&command_ref))
                .map(|record| record.cancellation)
                .collect::<Vec<_>>()
        };
        for cancellation in cancellations {
            cancellation.cancel();
        }
    }

    fn revoke_client_commands_for_group(&self, client_ref: &ClientRef, tool_group: ToolGroup) {
        let cancellations = {
            let mut handles = self.runtime_handles.lock();
            let command_refs = handles
                .commands
                .iter()
                .filter_map(|(command_ref, record)| {
                    (&record.client_ref == client_ref && record.owner_group == tool_group)
                        .then_some(command_ref.clone())
                })
                .collect::<Vec<_>>();
            command_refs
                .into_iter()
                .filter_map(|command_ref| handles.commands.remove(&command_ref))
                .map(|record| record.cancellation)
                .collect::<Vec<_>>()
        };
        for cancellation in cancellations {
            cancellation.cancel();
        }
    }

    fn revoke_client_runtime(&self, client_ref: &ClientRef, node_router: &NodeRouter) {
        self.state.approvals.revoke_client(client_ref);
        self.state.artifacts.revoke_client(client_ref);
        let (leases, cancellations) = {
            let mut handles = self.runtime_handles.lock();
            let node_refs = handles
                .nodes
                .iter()
                .filter_map(|(node_ref, lease)| {
                    (&lease.client_ref == client_ref).then_some(node_ref.clone())
                })
                .collect::<Vec<_>>();
            let leases = node_refs
                .into_iter()
                .filter_map(|node_ref| handles.nodes.remove(&node_ref))
                .collect::<Vec<_>>();
            let command_refs = handles
                .commands
                .iter()
                .filter_map(|(command_ref, record)| {
                    (&record.client_ref == client_ref).then_some(command_ref.clone())
                })
                .collect::<Vec<_>>();
            let cancellations = command_refs
                .into_iter()
                .filter_map(|command_ref| handles.commands.remove(&command_ref))
                .map(|record| record.cancellation)
                .collect::<Vec<_>>();
            (leases, cancellations)
        };
        for cancellation in cancellations {
            cancellation.cancel();
        }
        for lease in leases {
            if let Some(connection_id) = lease.physical_connection_id {
                node_router.release_consumer(&connection_id, &lease.consumer);
            }
        }
    }
}

fn node_lease_for_client(
    handles: &Arc<Mutex<PublicMcpRuntimeHandles>>,
    client_ref: &ClientRef,
    node_ref: &NodeRef,
) -> Option<PublicMcpNodeLease> {
    handles
        .lock()
        .nodes
        .get(node_ref)
        .filter(|lease| &lease.client_ref == client_ref)
        .cloned()
}

fn connection_directory_matches_query(connection: &ConnectionInfo, query: &str) -> bool {
    query.is_empty()
        || connection.name.to_lowercase().contains(query)
        || connection
            .group
            .as_deref()
            .is_some_and(|group| group.to_lowercase().contains(query))
        || connection
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(query))
}

fn all_tool_groups() -> BTreeSet<ToolGroup> {
    let mut tool_groups = ToolGroup::selectable()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    tool_groups.insert(ToolGroup::Basic);
    tool_groups
}

fn finish_serialized(request: DomainRequest, value: serde_json::Value) {
    request.finish(ToolEnvelope {
        outcome: ToolOutcome::Completed,
        data: Some(value),
        error: None,
    });
}

fn command_for_working_directory(
    command: &Zeroizing<String>,
    working_directory: Option<&str>,
) -> Zeroizing<String> {
    let Some(working_directory) = working_directory.filter(|directory| !directory.is_empty())
    else {
        return Zeroizing::new(command.to_string());
    };
    let quoted_directory = shell_single_quote(working_directory);
    Zeroizing::new(format!(
        "cd -- {} && {}",
        quoted_directory.as_str(),
        command.as_str()
    ))
}

fn shell_single_quote(value: &str) -> Zeroizing<String> {
    // POSIX shells represent one literal quote by ending, escaping, and reopening the quote.
    let mut quoted = String::with_capacity(value.len().saturating_add(2));
    quoted.push('\'');
    for character in value.chars() {
        if character == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    Zeroizing::new(quoted)
}

fn output_page(bytes: &[u8], offset: usize, limit: usize) -> String {
    let start = offset.min(bytes.len());
    let end = start.saturating_add(limit).min(bytes.len());
    String::from_utf8_lossy(&bytes[start..end]).into_owned()
}

fn public_command_error(error: SshTransportError) -> String {
    // Public errors expose an actionable category without forwarding transport internals.
    match error {
        SshTransportError::Timeout => "The remote command timed out".to_owned(),
        SshTransportError::DnsResolution { .. } => {
            "The SSH host name could not be resolved".to_owned()
        }
        SshTransportError::AuthenticationFailed(_) | SshTransportError::UnsupportedAuth(_) => {
            "SSH authentication is unavailable for this command".to_owned()
        }
        SshTransportError::HostKeyUnknown { .. }
        | SshTransportError::HostKeyChanged { .. }
        | SshTransportError::HostKeyCheckFailed(_) => {
            "SSH host key verification requires attention in OxideTerm".to_owned()
        }
        SshTransportError::AlgorithmNegotiationFailed { .. } => {
            "SSH algorithm negotiation failed".to_owned()
        }
        SshTransportError::ConnectionFailed(_)
        | SshTransportError::PreflightComplete
        | SshTransportError::Channel(_) => "The remote command could not be completed".to_owned(),
    }
}

fn read_endpoint_port(path: &Path) -> Option<u16> {
    let bytes = std::fs::read(path).ok()?;
    let state: PublicMcpEndpointState = serde_json::from_slice(&bytes).ok()?;
    (state.version == 1 && state.port != 0).then_some(state.port)
}

fn persist_endpoint_port(path: &Path, port: u16) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(&PublicMcpEndpointState { version: 1, port })
        .map_err(std::io::Error::other)?;
    oxideterm_atomic_file::durable_write(path, &bytes)
}
