use super::*;
pub(crate) const PUBLIC_MCP_CLIENTS_FILE: &str = "public-mcp-clients.json";
pub(crate) const PUBLIC_MCP_ENDPOINT_FILE: &str = "public-mcp-endpoint.json";
pub(crate) const PUBLIC_MCP_BROKER_CAPACITY: usize = 64;
pub(crate) const PUBLIC_MCP_COMMAND_TIMEOUT: Duration = Duration::from_secs(5 * 60);
pub(crate) const PUBLIC_MCP_COMMAND_OUTPUT_LIMIT: usize = 1024 * 1024;
pub(crate) const PUBLIC_MCP_OUTPUT_PAGE_LIMIT: usize = 256 * 1024;
pub(crate) const PUBLIC_MCP_COMMAND_RETENTION: Duration = Duration::from_secs(15 * 60);
// Bound command concurrency and retained output independently of node lease lifetime.
pub(crate) const PUBLIC_MCP_COMMAND_CAPACITY: usize = 256;
pub(crate) const PUBLIC_MCP_COMMAND_CAPACITY_PER_CLIENT: usize = 64;
// Node leases include both ready and in-flight connection attempts.
pub const PUBLIC_MCP_NODE_CAPACITY: usize = 128;
pub const PUBLIC_MCP_NODE_CAPACITY_PER_CLIENT: usize = 32;
// Namespace internal profile identifiers so external connection handles remain type-safe.
pub const CONNECTION_KEY_SSH_PREFIX: &str = "ssh:";
pub const CONNECTION_KEY_SERIAL_PREFIX: &str = "serial:";
pub const CONNECTION_KEY_TELNET_PREFIX: &str = "telnet:";
pub const CONNECTION_KEY_MOSH_PREFIX: &str = "mosh:";
pub const CONNECTION_KEY_DESKTOP_PREFIX: &str = "desktop:";
pub const CONNECTION_TYPE_SSH: &str = "ssh";
pub const CONNECTION_TYPE_SERIAL: &str = "serial";
pub const CONNECTION_TYPE_TELNET: &str = "telnet";
pub const CONNECTION_TYPE_MOSH: &str = "mosh";

pub struct PublicMcpHost {
    pub(crate) endpoint_url: Option<String>,
    pub(crate) startup_error: Option<String>,
    pub(crate) server: Option<PublicMcpHttpServer>,
    pub(crate) port_draft: String,
    pub(crate) client_registry_ready: bool,
    pub state: Arc<PublicMcpState>,
    pub settings_path: PathBuf,
    pub(crate) receiver: Option<DomainRequestReceiver>,
    pub(crate) admission: Arc<Mutex<bool>>,
    pub(crate) revealed_credential: Option<ClientCredential>,
    // Public connection references are client-scoped and never encode saved connection IDs.
    pub(crate) connection_refs: HashMap<(ClientRef, String), ConnectionRef>,
    pub(crate) connection_ids: HashMap<ConnectionRef, (ClientRef, String)>,
    pub(crate) quick_commands: Arc<Mutex<quick_commands::QuickCommandDirectory>>,
    pub(crate) addon_refs: HashMap<(ClientRef, String), AddonRef>,
    pub(crate) addon_ids: HashMap<AddonRef, (ClientRef, String)>,
    pub(crate) sync_handles: Arc<Mutex<sync_handles::SyncHandles>>,
    pub(crate) terminals: Arc<Mutex<HashMap<TerminalRef, PublicMcpTerminalRecord>>>,
    pub pending_terminal_opens: HashMap<String, PublicMcpPendingTerminalOpen>,
    pub(crate) recordings: Arc<Mutex<HashMap<RecordingRef, PublicMcpRecordingRecord>>>,
    pub history: Arc<Mutex<oxideterm_public_mcp::history::HistoryStore>>,
    pub(crate) handle_cleanup: Option<tokio::task::JoinHandle<()>>,
    pub(crate) desktops: Arc<Mutex<HashMap<DesktopRef, PublicMcpDesktopRecord>>>,
    pub(crate) runtime_handles: Arc<Mutex<PublicMcpRuntimeHandles>>,
}

#[derive(Default)]
pub(crate) struct PublicMcpRuntimeHandles {
    pub(crate) nodes: HashMap<NodeRef, PublicMcpNodeLease>,
    pub(crate) commands: HashMap<CommandRef, PublicMcpCommandRecord>,
    pub(crate) operations: HashMap<OperationRef, PublicMcpOperationRecord>,
    pub(crate) forwards: HashMap<ForwardRef, PublicMcpForwardRecord>,
    pub(crate) file_sessions: HashMap<FileSessionRef, PublicMcpFileSessionRecord>,
    pub(crate) transfers: HashMap<TransferRef, PublicMcpTransferRecord>,
    pub(crate) workspaces: HashMap<WorkspaceRef, PublicMcpWorkspaceRecord>,
}

#[derive(Clone)]
/// Maps a generic operation handle to a typed domain handle without exposing internal IDs.
pub(crate) enum PublicMcpOperationTarget {
    Command(CommandRef),
    Transfer(TransferRef),
}

#[derive(Clone)]
pub(crate) struct PublicMcpOperationRecord {
    pub(crate) client_ref: ClientRef,
    pub(crate) owner_group: ToolGroup,
    pub(crate) target: PublicMcpOperationTarget,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PublicMcpEndpointState {
    pub(crate) version: u32,
    pub(crate) port: u16,
    // Zero keeps automatic allocation while retaining the last live port for discovery.
    #[serde(default)]
    pub(crate) preferred_port: u16,
}

#[derive(Clone)]
pub struct PublicMcpNodeLease {
    pub client_ref: ClientRef,
    pub node_id: NodeId,
    pub saved_connection_id: Option<String>,
    pub physical_connection_id: Option<String>,
    pub consumer: ConnectionConsumer,
}

#[derive(Clone)]
pub(crate) struct PublicMcpForwardRecord {
    pub(crate) client_ref: ClientRef,
    pub(crate) node_ref: NodeRef,
    pub(crate) node_id: NodeId,
    pub(crate) owner_connection_id: Option<String>,
    pub(crate) forward_id: String,
    pub(crate) created_by_client: bool,
    pub(crate) persisted: bool,
}

/// Keeps the SFTP consumer and canonical root private to one external client.
#[derive(Clone)]
pub(crate) struct PublicMcpFileSessionRecord {
    pub(crate) client_ref: ClientRef,
    pub(crate) node_id: NodeId,
    pub(crate) root: Option<String>,
    pub(crate) session: Option<Arc<tokio::sync::Mutex<oxideterm_sftp::SftpSession>>>,
    pub(crate) physical_connection_id: Option<String>,
    pub(crate) consumer: ConnectionConsumer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublicMcpTransferState {
    Pending,
    Running,
    Completed,
    Cancelled,
    Failed,
}

pub(crate) struct PublicMcpTransferRecord {
    pub(crate) client_ref: ClientRef,
    pub(crate) file_session_ref: FileSessionRef,
    pub(crate) internal_id: String,
    pub(crate) direction: &'static str,
    pub(crate) remote_path: String,
    pub state: PublicMcpTransferState,
    pub(crate) total_bytes: u64,
    pub(crate) transferred_bytes: u64,
    pub(crate) speed_bytes_per_second: u64,
    pub(crate) artifact: Option<oxideterm_public_mcp::ArtifactProjection>,
    pub(crate) error_code: Option<&'static str>,
    pub(crate) remote_residue: Option<&'static str>,
    pub(crate) finished_at: Option<Instant>,
}

/// Keeps one headless IDE owner scoped to an external client and SFTP root.
#[derive(Clone)]
pub(crate) struct PublicMcpWorkspaceRecord {
    pub(crate) client_ref: ClientRef,
    pub(crate) file_session_ref: FileSessionRef,
    pub(crate) node_id: NodeId,
    pub(crate) root: String,
    pub(crate) owner: oxideterm_ide_fs::NodeAgentIdeFileSystem,
    pub(crate) revisions: Arc<Mutex<HashMap<String, PublicMcpWorkspaceRevision>>>,
    pub(crate) cancellation: CancellationToken,
    pub(crate) edit_cancellation: CancellationToken,
}

impl PublicMcpWorkspaceRecord {
    pub(crate) fn revoke(&self) {
        self.cancellation.cancel();
        self.edit_cancellation.cancel();
        self.owner.release_all_ide_consumers();
    }
}

#[derive(Clone)]
pub(crate) struct PublicMcpWorkspaceRevision {
    pub(crate) public_revision: String,
    pub(crate) version: oxideterm_ide_core::SavedFileVersion,
}

#[derive(Clone)]
pub struct PublicMcpTerminalRecord {
    pub client_ref: ClientRef,
    pub session_id: TerminalSessionId,
    pub transport: &'static str,
    pub title: String,
    pub node_ref: Option<NodeRef>,
}

pub struct PublicMcpPendingTerminalOpen {
    pub client_ref: ClientRef,
    pub terminal_ref: TerminalRef,
    pub request: DomainRequest,
    pub cols: u16,
    pub rows: u16,
    pub title: String,
}

pub(crate) struct PublicMcpRecordingRecord {
    pub client_ref: ClientRef,
    pub terminal_ref: TerminalRef,
    pub cols: usize,
    pub rows: usize,
    pub elapsed_ms: u64,
    pub event_count: usize,
    pub active: bool,
    pub truncated: bool,
    pub stopped_at: Option<Instant>,
    pub content: Option<Zeroizing<String>>,
    pub artifact_refs: HashSet<ArtifactRef>,
}

#[derive(Clone)]
pub struct PublicMcpDesktopRecord {
    pub client_ref: ClientRef,
    pub tab_id: TabId,
    pub title: String,
    pub observing_frames: bool,
    pub(crate) frame_artifacts: HashSet<ArtifactRef>,
    pub(crate) clipboard_artifacts: HashSet<ArtifactRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublicMcpCommandState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

pub(crate) struct PublicMcpCommandRecord {
    pub(crate) client_ref: ClientRef,
    pub(crate) node_ref: NodeRef,
    pub(crate) owner_group: ToolGroup,
    pub state: PublicMcpCommandState,
    pub(crate) stdout: Zeroizing<Vec<u8>>,
    pub(crate) stderr: Zeroizing<Vec<u8>>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) truncated: bool,
    pub(crate) error: Option<String>,
    pub(crate) cancellation: CancellationToken,
}

#[derive(Serialize)]
pub(crate) struct PublicConnectionDirectoryEntry {
    pub(crate) connection_ref: ConnectionRef,
    pub(crate) name: String,
    pub(crate) group: Option<String>,
    pub(crate) connection_type: &'static str,
    pub(crate) tags: Vec<String>,
    pub(crate) last_used_at: Option<String>,
}

impl PublicMcpHost {
    pub fn admission(&self) -> Arc<Mutex<bool>> {
        self.admission.clone()
    }

    pub fn set_suspended(&self, suspended: bool) {
        *self.admission.lock() = suspended;
    }

    pub fn node_lease(&self, client: &ClientRef, node: &NodeRef) -> Option<PublicMcpNodeLease> {
        node_lease_for_client(&self.runtime_handles, client, node)
    }

    pub fn node_capacity_available(&self, client: &ClientRef) -> bool {
        self.runtime_handles.lock().node_capacity_available(client)
    }

    pub(crate) fn start_handle_cleanup(&mut self, runtime: &tokio::runtime::Handle) {
        if self.handle_cleanup.is_none() {
            let history = self.history.clone();
            let sync_handles = self.sync_handles.clone();
            self.handle_cleanup = Some(runtime.spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    history.lock().expire(Instant::now());
                    sync_handles.lock().expire_sync_handles();
                }
            }));
        }
    }
}
impl PublicMcpHost {
    pub fn revoke_client_commands(&self, client_ref: &ClientRef) {
        let cancellations = {
            let mut handles = self.runtime_handles.lock();
            let command_refs = handles
                .commands
                .iter()
                .filter_map(|(command_ref, record)| {
                    (&record.client_ref == client_ref).then_some(command_ref.clone())
                })
                .collect::<Vec<_>>();
            remove_command_operations(&mut handles, &command_refs);
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

    pub fn revoke_client_commands_for_group(&self, client_ref: &ClientRef, tool_group: ToolGroup) {
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
            remove_command_operations(&mut handles, &command_refs);
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

    pub fn revoke_client_runtime(&self, client_ref: &ClientRef, node_router: &NodeRouter) {
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
            remove_command_operations(&mut handles, &command_refs);
            let cancellations = command_refs
                .into_iter()
                .filter_map(|command_ref| handles.commands.remove(&command_ref))
                .map(|record| record.cancellation)
                .collect::<Vec<_>>();
            handles
                .operations
                .retain(|_, operation| &operation.client_ref != client_ref);
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

impl PublicMcpRuntimeHandles {
    pub(crate) fn node_capacity_available(&self, client: &ClientRef) -> bool {
        self.nodes.len() < PUBLIC_MCP_NODE_CAPACITY
            && self
                .nodes
                .values()
                .filter(|lease| &lease.client_ref == client)
                .count()
                < PUBLIC_MCP_NODE_CAPACITY_PER_CLIENT
    }

    pub(crate) fn reserve_node(&mut self, node: NodeRef, lease: PublicMcpNodeLease) -> bool {
        if !self.node_capacity_available(&lease.client_ref) {
            return false;
        }
        self.nodes.insert(node, lease);
        true
    }
}
