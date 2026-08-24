use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use base64::Engine;
use http::{header::AUTHORIZATION, request::Parts};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    model::{
        CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, Implementation,
        JsonObject, ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities,
        ServerInfo, Tool, ToolAnnotations,
    },
    service::RequestContext,
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::{
    approval::ApprovalStore,
    artifact::ArtifactStore,
    audit::{AuditAuthorization, AuditStore},
    auth::{ClientApprovalMode, ClientProjection, ClientRegistry, ToolGroup},
    broker::{BrokerError, DomainBroker},
    calls::{
        AddonsInstallArgs, AddonsListArgs, AddonsRemoveArgs, AddonsSetEnabledArgs, AuditSearchArgs,
        BrowseConnectionsArgs, CancelCommandArgs, CancelOperationArgs, CommandOutputArgs,
        CommandStateArgs, ConnectNodeArgs, CredentialStatusArgs, DescribeConnectionArgs,
        DesktopButtonState, DesktopClipboardImageFormat, DesktopClipboardPayload, DesktopFrameArgs,
        DesktopHandleArgs, DesktopInputArgs, DesktopInputEvent, DisconnectNodeArgs, FilesCloseArgs,
        FilesCompareArgs, FilesListArgs, FilesMoveArgs, FilesOpenArgs, FilesReadArgs,
        FilesRemoveArgs, FilesStatArgs, FilesWriteArgs, ForgetCredentialArgs, ForwardHandleArgs,
        ForwardKind, ForwardsChangeArgs, ForwardsDiscoverPortsArgs, ForwardsListArgs,
        ForwardsOpenArgs, ForwardsRemoveArgs, HostToolsCaptureArgs, HostToolsCatalogArgs,
        HostToolsOperateArgs, InspectNodeArgs, OpenDesktopArgs, OpenTerminalArgs,
        OperationStateArgs, PublicCredentialSlot, PublicDesktopMouseButton, PublicToolCall,
        QuickCommandsDescribeArgs, QuickCommandsListArgs, QuickCommandsRemoveArgs,
        QuickCommandsRunArgs, QuickCommandsSaveArgs, ReadArtifactArgs, ReadDesktopClipboardArgs,
        ReadTerminalArgs, RecordingsControlArgs, RecordingsExportArgs, RecordingsSearchArgs,
        RecordingsStatusArgs, ReleaseNodeArgs, RemovePublicConnectionArgs, RequestAccessArgs,
        ResizeDesktopArgs, ResizeTerminalArgs, RevertArgs, RevokeAccessArgs,
        SavePublicConnectionArgs, StageArtifactArgs, StartCommandArgs, StartTransferArgs,
        StoreCredentialArgs, SubmitTerminalArgs, SyncApplyPlanArgs, SyncPublishPreviewArgs,
        SyncPullPreviewArgs, SyncRestoreArgs, SyncStatusArgs, TerminalHandleArgs, ToolEnvelope,
        ToolOutcome, TransferHandleArgs, WorkspaceApplyEditsArgs, WorkspaceCloseArgs,
        WorkspaceFileEdits, WorkspaceMountArgs, WorkspaceReadArgs, WorkspaceSearchArgs,
        WorkspaceTextEdit, WorkspaceTreeArgs, WriteDesktopClipboardArgs,
    },
    handles::{ApprovalRef, ClientRef, ConnectionRef, NodeRef, TerminalRef, WorkspaceRef},
};

const TOOL_LIST_CACHE_TTL_MS: u64 = 1_000;
const COMMAND_TEXT_LIMIT_BYTES: usize = 64 * 1024;
const WORKING_DIRECTORY_LIMIT_BYTES: usize = 16 * 1024;
const ARTIFACT_STAGE_LIMIT_BYTES: usize = 16 * 1024 * 1024;
const QUICK_COMMAND_NAME_LIMIT_BYTES: usize = 160;
const QUICK_COMMAND_BODY_LIMIT_BYTES: usize = 4 * 1024;
const QUICK_COMMAND_DESCRIPTION_LIMIT_BYTES: usize = 1024;
const QUICK_COMMAND_ARGUMENT_LIMIT: usize = 32;
const QUICK_COMMAND_ARGUMENT_NAME_LIMIT_BYTES: usize = 64;
const QUICK_COMMAND_ARGUMENT_VALUE_LIMIT_BYTES: usize = 8 * 1024;
const QUICK_COMMAND_ARGUMENT_TOTAL_LIMIT_BYTES: usize = 64 * 1024;
const QUICK_COMMAND_PARAMETER_DEFAULT_LIMIT_BYTES: usize = 1024;
const QUICK_COMMAND_PARAMETER_CHOICE_LIMIT: usize = 64;
const QUICK_COMMAND_HOST_PATTERN_LIMIT: usize = 32;
const QUICK_COMMAND_HOST_PATTERN_LIMIT_BYTES: usize = 256;
const ADDON_ID_LIMIT_BYTES: usize = 255;
const FORWARD_ENDPOINT_LIMIT_BYTES: usize = 255;
const FORWARD_DESCRIPTION_LIMIT_BYTES: usize = 512;
const FORWARD_REVISION_LIMIT_BYTES: usize = 80;
const REMOTE_PATH_LIMIT_BYTES: usize = 16 * 1024;
const FILE_LIST_LIMIT_MAXIMUM: u32 = 500;
const FILE_READ_LIMIT_MAXIMUM: u32 = 4 * 1024 * 1024;
const WORKSPACE_EDIT_FILE_LIMIT: usize = 16;
const WORKSPACE_EDIT_COUNT_LIMIT: usize = 512;
const WORKSPACE_EDIT_REPLACEMENT_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const WORKSPACE_SEARCH_PATTERN_LIMIT_BYTES: usize = 8 * 1024;
const WORKSPACE_SEARCH_RESULT_LIMIT: u32 = 500;
const TERMINAL_INPUT_LIMIT_BYTES: usize = 256 * 1024;
const TERMINAL_QUERY_LIMIT_BYTES: usize = 4 * 1024;
const TERMINAL_LINE_LIMIT_MAXIMUM: u32 = 1_000;
const TERMINAL_MATCH_LIMIT_MAXIMUM: u32 = 500;
const TERMINAL_DIMENSION_MAXIMUM: u16 = 1_000;
const TERMINAL_TITLE_LIMIT_BYTES: usize = 256;
const RECORDING_TITLE_LIMIT_BYTES: usize = 256;
const RECORDING_QUERY_LIMIT_BYTES: usize = 4 * 1024;
const RECORDING_SEARCH_LIMIT_MAXIMUM: u32 = 50;
const DESKTOP_MIN_WIDTH: u32 = 200;
const DESKTOP_MIN_HEIGHT: u32 = 120;
const DESKTOP_MAX_DIMENSION: u32 = 8_192;
const DESKTOP_KEY_CODE_LIMIT_BYTES: usize = 128;
const DESKTOP_KEY_TEXT_LIMIT_BYTES: usize = 4 * 1024;
const DESKTOP_TEXT_INPUT_LIMIT_BYTES: usize = 256 * 1024;
const DESKTOP_CLIPBOARD_TEXT_LIMIT_BYTES: usize = 1024 * 1024;
const DESKTOP_WHEEL_DELTA_LIMIT: f32 = 10_000.0;
const CREDENTIAL_SECRET_LIMIT_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub struct PublicMcpService {
    state: Arc<PublicMcpState>,
}

pub struct PublicMcpState {
    pub clients: Arc<ClientRegistry>,
    pub approvals: Arc<ApprovalStore>,
    pub audit: Arc<AuditStore>,
    pub artifacts: Arc<ArtifactStore>,
    pub broker: Arc<DomainBroker>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct CommitActionArgs {
    approval_ref: ApprovalRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "this type exists only to generate the public tool schema"
)]
struct StartCommandSchema {
    node_ref: NodeRef,
    command: String,
    #[serde(default)]
    working_directory: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct StartCommandMetadata {
    node_ref: NodeRef,
    #[serde(default)]
    working_directory: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "this type exists only to generate the public tool schema"
)]
struct SubmitTerminalSchema {
    terminal_ref: TerminalRef,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    bytes_base64: Option<String>,
    #[serde(default)]
    append_enter: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WorkspaceTextEditSchema {
    start_byte: u32,
    end_byte: u32,
    replacement: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WorkspaceFileEditsSchema {
    path: String,
    expected_revision: String,
    edits: Vec<WorkspaceTextEditSchema>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WorkspaceApplyEditsSchema {
    workspace_ref: WorkspaceRef,
    files: Vec<WorkspaceFileEditsSchema>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitTerminalMetadata {
    terminal_ref: TerminalRef,
    #[serde(default)]
    append_enter: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "this type exists only to generate the public tool schema"
)]
#[serde(deny_unknown_fields)]
struct StoreCredentialSchema {
    connection_ref: ConnectionRef,
    slot: PublicCredentialSlot,
    new_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreCredentialMetadata {
    connection_ref: ConnectionRef,
    slot: PublicCredentialSlot,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "this type exists only to generate the public tool schema"
)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
enum DesktopInputEventSchema {
    MouseMove {
        x: u32,
        y: u32,
    },
    MouseButton {
        x: u32,
        y: u32,
        button: PublicDesktopMouseButton,
        state: DesktopButtonState,
    },
    Wheel {
        x: u32,
        y: u32,
        delta_x: f32,
        delta_y: f32,
    },
    Key {
        code: String,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        shift: bool,
        #[serde(default)]
        meta: bool,
        state: DesktopButtonState,
    },
    Text {
        text: String,
    },
    ReleaseAll,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "this type exists only to generate the public tool schema"
)]
#[serde(deny_unknown_fields)]
struct DesktopInputSchema {
    desktop_ref: crate::DesktopRef,
    graphics_epoch: u64,
    event: DesktopInputEventSchema,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "this type exists only to generate the public tool schema"
)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
enum DesktopClipboardPayloadSchema {
    Text {
        text: String,
    },
    Image {
        artifact_ref: crate::ArtifactRef,
        format: DesktopClipboardImageFormat,
    },
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "this type exists only to generate the public tool schema"
)]
#[serde(deny_unknown_fields)]
struct WriteDesktopClipboardSchema {
    desktop_ref: crate::DesktopRef,
    payload: DesktopClipboardPayloadSchema,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "this type exists only to generate the public tool schema"
)]
struct StageArtifactSchema {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    bytes_base64: Option<String>,
    #[serde(default)]
    media_type: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StageArtifactMetadata {
    #[serde(default)]
    media_type: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "this type exists only to generate the public tool schema"
)]
struct QuickCommandsSaveSchema {
    #[serde(default)]
    quickcommand_ref: Option<crate::QuickCommandRef>,
    name: String,
    command: String,
    category: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    host_pattern: Option<String>,
    #[serde(default)]
    host_patterns: Option<Vec<String>>,
    #[serde(default)]
    parameters: Option<Vec<crate::PublicQuickCommandParameter>>,
    #[serde(default)]
    protocols: Option<Vec<crate::PublicQuickCommandTargetProtocol>>,
    #[serde(default)]
    confirmation: Option<crate::PublicQuickCommandConfirmationPolicy>,
    expected_revision: u64,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuickCommandsSaveMetadata {
    #[serde(default)]
    quickcommand_ref: Option<crate::QuickCommandRef>,
    name: String,
    category: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    host_pattern: Option<String>,
    #[serde(default)]
    host_patterns: Option<Vec<String>>,
    #[serde(default)]
    parameters: Option<Vec<crate::PublicQuickCommandParameter>>,
    #[serde(default)]
    protocols: Option<Vec<crate::PublicQuickCommandTargetProtocol>>,
    #[serde(default)]
    confirmation: Option<crate::PublicQuickCommandConfirmationPolicy>,
    expected_revision: u64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "this type exists only to generate the public tool schema"
)]
struct QuickCommandsRunSchema {
    quickcommand_ref: crate::QuickCommandRef,
    node_ref: NodeRef,
    expected_revision: u64,
    #[serde(default)]
    arguments: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuickCommandsRunMetadata {
    quickcommand_ref: crate::QuickCommandRef,
    node_ref: NodeRef,
    expected_revision: u64,
}

#[derive(Debug, Clone, Serialize)]
struct CatalogEntry {
    name: String,
    tool_group: ToolGroup,
    additional_tool_groups: &'static [ToolGroup],
    requires_approval: bool,
}

impl PublicMcpService {
    pub fn new(state: Arc<PublicMcpState>) -> Self {
        Self { state }
    }

    fn resolve_client(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<ClientProjection, McpError> {
        let authorization = context
            .extensions
            .get::<Parts>()
            .and_then(|parts| parts.headers.get(AUTHORIZATION))
            .and_then(|value| value.to_str().ok())
            .ok_or_else(unauthorized_error)?;
        self.state
            .clients
            .authenticate_bearer(authorization)
            .ok_or_else(unauthorized_error)
    }

    fn visible_tools(&self, client: &ClientProjection) -> Vec<Tool> {
        tool_definitions()
            .into_iter()
            .filter(|definition| definition.is_visible_to(client))
            .map(|definition| definition.tool)
            .collect()
    }

    async fn execute_call(
        &self,
        client: &ClientProjection,
        call: PublicToolCall,
    ) -> CallToolResult {
        if !client.tool_groups.contains(&call.required_group())
            || call
                .additional_required_groups()
                .iter()
                .any(|group| !client.tool_groups.contains(group))
        {
            return tool_error(
                "tool_group_disabled",
                "This tool group is disabled for the client",
            );
        }

        // Domain-prepared tools stage their frozen payload after workspace-side validation.
        if call.requires_approval()
            && !call.requires_domain_preparation()
            && (client.approval_mode == ClientApprovalMode::Standard
                || call.requires_explicit_app_approval())
        {
            let tool_name = call.tool_name();
            let target = call.target_summary();
            let approval = match self.state.approvals.stage(client.client_ref.clone(), call) {
                Ok(approval) => approval,
                Err(error) => return tool_error("approval_unavailable", error.to_string()),
            };
            self.state.audit.record_fields(
                client.client_ref.clone(),
                tool_name,
                &target,
                AuditAuthorization::AppApproval,
                ToolOutcome::Accepted,
            );
            self.state.broker.notify_state_changed();
            return CallToolResult::structured(json!({
                "outcome": "approval_required",
                "approval": approval,
            }));
        }

        let authorization =
            if call.requires_approval() && client.approval_mode == ClientApprovalMode::Unattended {
                AuditAuthorization::Unattended
            } else {
                AuditAuthorization::NotRequired
            };
        self.execute_approved_call(
            client.client_ref.clone(),
            client.approval_mode,
            call,
            authorization,
        )
        .await
    }

    async fn execute_approved_call(
        &self,
        client_ref: ClientRef,
        expected_approval_mode: ClientApprovalMode,
        call: PublicToolCall,
        authorization: AuditAuthorization,
    ) -> CallToolResult {
        let tool_name = call.tool_name().to_owned();
        let target = call.target_summary();
        let response = self
            .state
            .broker
            .execute(
                &self.state.clients,
                expected_approval_mode,
                client_ref.clone(),
                call,
            )
            .await;
        match response {
            Ok(envelope) => {
                self.state.audit.record_fields(
                    client_ref,
                    tool_name,
                    &target,
                    authorization,
                    envelope.outcome.clone(),
                );
                envelope_result(envelope)
            }
            Err(error) => {
                self.state.audit.record_fields(
                    client_ref,
                    tool_name,
                    &target,
                    authorization,
                    ToolOutcome::Failed,
                );
                let error_code = match error {
                    BrokerError::AuthorizationChanged => "authorization_changed",
                    _ => "workspace_unavailable",
                };
                tool_error(error_code, error.to_string())
            }
        }
    }

    async fn commit_action(
        &self,
        client: &ClientProjection,
        arguments: JsonObject,
    ) -> CallToolResult {
        let args = match parse_arguments::<CommitActionArgs>(arguments) {
            Ok(args) => args,
            Err(error) => return *error,
        };
        let call = match self
            .state
            .approvals
            .take_approved(&client.client_ref, &args.approval_ref)
        {
            Ok(call) => call,
            Err(error) => return tool_error("approval_unavailable", error.to_string()),
        };
        if !client.tool_groups.contains(&call.required_group())
            || call
                .additional_required_groups()
                .iter()
                .any(|group| !client.tool_groups.contains(group))
        {
            return tool_error(
                "tool_group_disabled",
                "The required tool group was disabled before commit",
            );
        }
        self.execute_approved_call(
            client.client_ref.clone(),
            client.approval_mode,
            call,
            AuditAuthorization::AppApproval,
        )
        .await
    }
}

impl ServerHandler for PublicMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
            .with_server_info(
                Implementation::new("oxideterm-public-mcp", env!("CARGO_PKG_VERSION"))
                    .with_title("OxideTerm Public MCP")
                    .with_description("Authorized automation for the active OxideTerm workspace"),
            )
            .with_instructions(
                "Use only opaque public references. Mutating tools may require approval in OxideTerm before mcp_commit_action succeeds.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let client = self.resolve_client(&context)?;
        Ok(ListToolsResult::with_all_items(self.visible_tools(&client))
            .with_ttl_ms(TOOL_LIST_CACHE_TTL_MS)
            .with_cache_scope(CacheScope::Private))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tool_definitions()
            .into_iter()
            .find(|definition| definition.tool.name == name)
            .map(|definition| definition.tool)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let client = self.resolve_client(&context)?;
        let arguments = request.arguments.unwrap_or_default();
        let result = match request.name.as_ref() {
            "mcp_overview" => {
                let approval_policy = match client.approval_mode {
                    ClientApprovalMode::Standard => "in_app_approval",
                    ClientApprovalMode::Unattended => "unattended_for_enabled_groups",
                };
                CallToolResult::structured(json!({
                    "server": "OxideTerm Public MCP",
                    "protocol": ProtocolVersion::V_2026_07_28.to_string(),
                    "approval_policy": approval_policy,
                    "enabled_tool_groups": client.tool_groups,
                    "available_tool_groups": ToolGroup::selectable(),
                    "security": "Bearer authentication, per-client tool groups, app-lock enforcement, secret hard boundaries, and audit remain active in every mode",
                }))
            }
            "mcp_catalog" => {
                let tools = tool_definitions()
                    .into_iter()
                    .filter(|definition| definition.is_visible_to(&client))
                    .map(|definition| CatalogEntry {
                        name: definition.tool.name.into_owned(),
                        tool_group: definition.group,
                        additional_tool_groups: definition.additional_groups,
                        requires_approval: definition.requires_approval
                            && (client.approval_mode == ClientApprovalMode::Standard
                                || definition.requires_explicit_app_approval),
                    })
                    .collect::<Vec<_>>();
                let tool_groups = ToolGroup::selectable()
                    .iter()
                    .map(|group| {
                        json!({
                            "group": group,
                            "enabled": client.tool_groups.contains(group),
                        })
                    })
                    .collect::<Vec<_>>();
                CallToolResult::structured(json!({
                    "tools": tools,
                    "tool_groups": tool_groups,
                }))
            }
            "mcp_access_state" => {
                let access_requests = self
                    .state
                    .approvals
                    .list()
                    .into_iter()
                    .filter(|approval| {
                        approval.client_ref == client.client_ref
                            && approval.tool_name == "mcp_request_access"
                    })
                    .collect::<Vec<_>>();
                CallToolResult::structured(json!({
                    "client": client,
                    "selectable_groups": ToolGroup::selectable(),
                    "access_requests": access_requests,
                }))
            }
            "mcp_request_access" => match parse_arguments::<RequestAccessArgs>(arguments) {
                Ok(mut args) if access_groups_are_valid(&args.groups) => {
                    let requested_groups = args.groups.into_iter().collect::<BTreeSet<_>>();
                    args.groups = requested_groups
                        .into_iter()
                        .filter(|group| !client.tool_groups.contains(group))
                        .collect();
                    if args.groups.is_empty() {
                        CallToolResult::structured(json!({
                            "outcome": "already_granted",
                            "client": client,
                        }))
                    } else {
                        self.execute_call(&client, PublicToolCall::RequestAccess(args))
                            .await
                    }
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "Select one or more non-basic Public MCP tool groups",
                ),
                Err(error) => *error,
            },
            "mcp_revoke_access" => match parse_arguments::<RevokeAccessArgs>(arguments) {
                Ok(args) if access_groups_are_valid(&args.groups) => {
                    let groups = args
                        .groups
                        .into_iter()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    self.execute_call(
                        &client,
                        PublicToolCall::RevokeAccess(RevokeAccessArgs { groups }),
                    )
                    .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "Select one or more non-basic Public MCP tool groups",
                ),
                Err(error) => *error,
            },
            "mcp_operation" => match parse_arguments::<OperationStateArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::OperationState(args))
                        .await
                }
                Err(error) => *error,
            },
            "mcp_cancel_operation" => match parse_arguments::<CancelOperationArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::CancelOperation(args))
                        .await
                }
                Err(error) => *error,
            },
            "mcp_revert" => match parse_arguments::<RevertArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::Revert(args))
                        .await
                }
                Err(error) => *error,
            },
            "mcp_commit_action" => self.commit_action(&client, arguments).await,
            "connections_browse" => match parse_arguments::<BrowseConnectionsArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::BrowseConnections(args))
                        .await
                }
                Err(error) => *error,
            },
            "connections_describe" => match parse_arguments::<DescribeConnectionArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::DescribeConnection(args))
                        .await
                }
                Err(error) => *error,
            },
            "connections_save" => match parse_arguments::<SavePublicConnectionArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::SaveConnection(Box::new(args)))
                        .await
                }
                Err(error) => *error,
            },
            "connections_remove" => {
                match parse_arguments::<RemovePublicConnectionArgs>(arguments) {
                    Ok(args) => {
                        self.execute_call(&client, PublicToolCall::RemoveConnection(args))
                            .await
                    }
                    Err(error) => *error,
                }
            }
            "credentials_status" => match parse_arguments::<CredentialStatusArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::CredentialStatus(args))
                        .await
                }
                Err(error) => *error,
            },
            "credentials_store" => match parse_store_credential(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::StoreCredential(args))
                        .await
                }
                Err(error) => *error,
            },
            "credentials_forget" => match parse_arguments::<ForgetCredentialArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::ForgetCredential(args))
                        .await
                }
                Err(error) => *error,
            },
            "sync_status" => match parse_arguments::<SyncStatusArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::SyncStatus(args))
                        .await
                }
                Err(error) => *error,
            },
            "sync_pull_preview" => match parse_arguments::<SyncPullPreviewArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::SyncPullPreview(args))
                        .await
                }
                Err(error) => *error,
            },
            "sync_publish_preview" => match parse_arguments::<SyncPublishPreviewArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::SyncPublishPreview(args))
                        .await
                }
                Err(error) => *error,
            },
            "sync_apply_plan" => match parse_arguments::<SyncApplyPlanArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::SyncApplyPlan(args))
                        .await
                }
                Err(error) => *error,
            },
            "sync_restore" => match parse_arguments::<SyncRestoreArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::SyncRestore(args))
                        .await
                }
                Err(error) => *error,
            },
            "nodes_connect" => match parse_arguments::<ConnectNodeArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::ConnectNode(args))
                        .await
                }
                Err(error) => *error,
            },
            "nodes_inspect" => match parse_arguments::<InspectNodeArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::InspectNode(args))
                        .await
                }
                Err(error) => *error,
            },
            "nodes_release" => match parse_arguments::<ReleaseNodeArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::ReleaseNode(args))
                        .await
                }
                Err(error) => *error,
            },
            "nodes_disconnect" => match parse_arguments::<DisconnectNodeArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::DisconnectNode(args))
                        .await
                }
                Err(error) => *error,
            },
            "terminals_open" => match parse_arguments::<OpenTerminalArgs>(arguments) {
                Ok(args)
                    if terminal_dimensions_are_valid(args.cols, args.rows)
                        && args.title.as_deref().is_none_or(terminal_title_is_valid) =>
                {
                    self.execute_call(&client, PublicToolCall::OpenTerminal(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "Terminal dimensions must be between 2 and 1000 cells",
                ),
                Err(error) => *error,
            },
            "terminals_state" => match parse_arguments::<TerminalHandleArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::TerminalState(args))
                        .await
                }
                Err(error) => *error,
            },
            "terminals_read" => match parse_arguments::<ReadTerminalArgs>(arguments) {
                Ok(args)
                    if args.line_limit > 0 && args.line_limit <= TERMINAL_LINE_LIMIT_MAXIMUM =>
                {
                    self.execute_call(&client, PublicToolCall::ReadTerminal(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The terminal line limit must be between 1 and 1000",
                ),
                Err(error) => *error,
            },
            "terminals_find" => {
                match parse_arguments::<crate::calls::FindTerminalArgs>(arguments) {
                    Ok(args)
                        if !args.query.trim().is_empty()
                            && args.query.len() <= TERMINAL_QUERY_LIMIT_BYTES
                            && args.limit > 0
                            && args.limit <= TERMINAL_MATCH_LIMIT_MAXIMUM =>
                    {
                        self.execute_call(&client, PublicToolCall::FindTerminal(args))
                            .await
                    }
                    Ok(_) => tool_error(
                        "invalid_arguments",
                        "The terminal query or match limit is invalid",
                    ),
                    Err(error) => *error,
                }
            }
            "terminals_submit" => match parse_terminal_submit(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::SubmitTerminal(args))
                        .await
                }
                Err(error) => *error,
            },
            "terminals_resize" => match parse_arguments::<ResizeTerminalArgs>(arguments) {
                Ok(args) if terminal_dimensions_are_valid(args.cols, args.rows) => {
                    self.execute_call(&client, PublicToolCall::ResizeTerminal(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "Terminal dimensions must be between 2 and 1000 cells",
                ),
                Err(error) => *error,
            },
            "terminals_control" => {
                match parse_arguments::<crate::calls::ControlTerminalArgs>(arguments) {
                    Ok(args) => {
                        self.execute_call(&client, PublicToolCall::ControlTerminal(args))
                            .await
                    }
                    Err(error) => *error,
                }
            }
            "terminals_close" => match parse_arguments::<TerminalHandleArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::CloseTerminal(args))
                        .await
                }
                Err(error) => *error,
            },
            "recordings_control" => match parse_arguments::<RecordingsControlArgs>(arguments) {
                Ok(args) if recording_control_is_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::RecordingsControl(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "Terminal recording input capture is unavailable or the title is invalid",
                ),
                Err(error) => *error,
            },
            "recordings_status" => match parse_arguments::<RecordingsStatusArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::RecordingsStatus(args))
                        .await
                }
                Err(error) => *error,
            },
            "recordings_search" => match parse_arguments::<RecordingsSearchArgs>(arguments) {
                Ok(args)
                    if !args.query.trim().is_empty()
                        && args.query.len() <= RECORDING_QUERY_LIMIT_BYTES
                        && args.limit > 0
                        && args.limit <= RECORDING_SEARCH_LIMIT_MAXIMUM =>
                {
                    self.execute_call(&client, PublicToolCall::RecordingsSearch(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The recording query or result limit is invalid",
                ),
                Err(error) => *error,
            },
            "recordings_export" => match parse_arguments::<RecordingsExportArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::RecordingsExport(args))
                        .await
                }
                Err(error) => *error,
            },
            "desktops_open" => match parse_arguments::<OpenDesktopArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::OpenDesktop(args))
                        .await
                }
                Err(error) => *error,
            },
            "desktops_state" => match parse_arguments::<DesktopHandleArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::DesktopState(args))
                        .await
                }
                Err(error) => *error,
            },
            "desktops_frame" => match parse_arguments::<DesktopFrameArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::DesktopFrame(args))
                        .await
                }
                Err(error) => *error,
            },
            "desktops_input" => match parse_arguments::<DesktopInputArgs>(arguments) {
                Ok(args) if desktop_input_is_valid(&args.event) => {
                    self.execute_call(&client, PublicToolCall::DesktopInput(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The remote desktop input event exceeds the supported bounds",
                ),
                Err(error) => *error,
            },
            "desktops_resize" => match parse_arguments::<ResizeDesktopArgs>(arguments) {
                Ok(args) if desktop_dimensions_are_valid(args.width, args.height) => {
                    self.execute_call(&client, PublicToolCall::ResizeDesktop(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The remote desktop dimensions are outside the supported range",
                ),
                Err(error) => *error,
            },
            "desktops_clipboard_read" => {
                match parse_arguments::<ReadDesktopClipboardArgs>(arguments) {
                    Ok(args) => {
                        self.execute_call(&client, PublicToolCall::ReadDesktopClipboard(args))
                            .await
                    }
                    Err(error) => *error,
                }
            }
            "desktops_clipboard_write" => {
                match parse_arguments::<WriteDesktopClipboardArgs>(arguments) {
                    Ok(args) if desktop_clipboard_payload_is_valid(&args.payload) => {
                        self.execute_call(&client, PublicToolCall::WriteDesktopClipboard(args))
                            .await
                    }
                    Ok(_) => tool_error(
                        "invalid_arguments",
                        "The remote desktop clipboard payload exceeds the supported bounds",
                    ),
                    Err(error) => *error,
                }
            }
            "desktops_reconnect" => match parse_arguments::<DesktopHandleArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::ReconnectDesktop(args))
                        .await
                }
                Err(error) => *error,
            },
            "desktops_close" => match parse_arguments::<DesktopHandleArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::CloseDesktop(args))
                        .await
                }
                Err(error) => *error,
            },
            "commands_start" => match parse_start_command(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::StartCommand(args))
                        .await
                }
                Err(error) => *error,
            },
            "commands_state" => match parse_arguments::<CommandStateArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::CommandState(args))
                        .await
                }
                Err(error) => *error,
            },
            "commands_output" => match parse_arguments::<CommandOutputArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::CommandOutput(args))
                        .await
                }
                Err(error) => *error,
            },
            "commands_cancel" => match parse_arguments::<CancelCommandArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::CancelCommand(args))
                        .await
                }
                Err(error) => *error,
            },
            "artifacts_stage" => match parse_stage_artifact(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::StageArtifact(args))
                        .await
                }
                Err(error) => *error,
            },
            "artifacts_read" => match parse_arguments::<ReadArtifactArgs>(arguments) {
                Ok(args) if args.length > 0 && args.length <= 256 * 1024 => {
                    self.execute_call(&client, PublicToolCall::ReadArtifact(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The artifact read length must be between 1 and 262144 bytes",
                ),
                Err(error) => *error,
            },
            "transfers_start" => match parse_arguments::<StartTransferArgs>(arguments) {
                Ok(args) if remote_path_is_valid(args.remote_path()) => {
                    self.execute_call(&client, PublicToolCall::TransferStart(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The remote transfer path exceeds the supported bounds",
                ),
                Err(error) => *error,
            },
            "transfers_status" => match parse_arguments::<TransferHandleArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::TransferStatus(args))
                        .await
                }
                Err(error) => *error,
            },
            "transfers_cancel" => match parse_arguments::<TransferHandleArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::TransferCancel(args))
                        .await
                }
                Err(error) => *error,
            },
            "workspaces_mount" => match parse_arguments::<WorkspaceMountArgs>(arguments) {
                Ok(args) if args.root.as_deref().is_none_or(remote_path_is_valid) => {
                    self.execute_call(&client, PublicToolCall::WorkspaceMount(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The workspace root is invalid"),
                Err(error) => *error,
            },
            "workspaces_tree" => match parse_arguments::<WorkspaceTreeArgs>(arguments) {
                Ok(args) if workspace_tree_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::WorkspaceTree(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The workspace tree request is invalid"),
                Err(error) => *error,
            },
            "workspaces_read" => match parse_arguments::<WorkspaceReadArgs>(arguments) {
                Ok(args) if remote_path_is_valid(&args.path) => {
                    self.execute_call(&client, PublicToolCall::WorkspaceRead(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The workspace path is invalid"),
                Err(error) => *error,
            },
            "workspaces_apply_edits" => match parse_workspace_apply_edits(arguments) {
                Ok(args) if workspace_apply_edits_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::WorkspaceApplyEdits(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The structured workspace edit exceeds the supported bounds",
                ),
                Err(error) => *error,
            },
            "workspaces_search" => match parse_arguments::<WorkspaceSearchArgs>(arguments) {
                Ok(args) if workspace_search_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::WorkspaceSearch(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The workspace search request is invalid",
                ),
                Err(error) => *error,
            },
            "workspaces_close" => match parse_arguments::<WorkspaceCloseArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::WorkspaceClose(args))
                        .await
                }
                Err(error) => *error,
            },
            "mcp_audit_search" => match parse_arguments::<AuditSearchArgs>(arguments) {
                Ok(args) if args.limit > 0 && args.limit <= 200 => {
                    self.execute_call(&client, PublicToolCall::AuditSearch(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The audit result limit must be between 1 and 200",
                ),
                Err(error) => *error,
            },
            "hosttools_catalog" => match parse_arguments::<HostToolsCatalogArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::HostToolsCatalog(args))
                        .await
                }
                Err(error) => *error,
            },
            "hosttools_capture" => match parse_arguments::<HostToolsCaptureArgs>(arguments) {
                Ok(args) if args.limit > 0 && args.limit <= 500 => {
                    self.execute_call(&client, PublicToolCall::HostToolsCapture(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The Host Tools row limit must be between 1 and 500",
                ),
                Err(error) => *error,
            },
            "hosttools_operate" => match parse_arguments::<HostToolsOperateArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::HostToolsOperate(Box::new(args)))
                        .await
                }
                Err(error) => *error,
            },
            "quickcommands_list" => match parse_arguments::<QuickCommandsListArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::QuickCommandsList(args))
                        .await
                }
                Err(error) => *error,
            },
            "quickcommands_describe" => {
                match parse_arguments::<QuickCommandsDescribeArgs>(arguments) {
                    Ok(args) => {
                        self.execute_call(&client, PublicToolCall::QuickCommandsDescribe(args))
                            .await
                    }
                    Err(error) => *error,
                }
            }
            "quickcommands_save" => match parse_quick_commands_save(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::QuickCommandsSave(Box::new(args)))
                        .await
                }
                Err(error) => *error,
            },
            "quickcommands_remove" => match parse_arguments::<QuickCommandsRemoveArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::QuickCommandsRemove(args))
                        .await
                }
                Err(error) => *error,
            },
            "quickcommands_run" => match parse_quick_commands_run(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::QuickCommandsRun(args))
                        .await
                }
                Err(error) => *error,
            },
            "addons_list" => match parse_arguments::<AddonsListArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::AddonsList(args))
                        .await
                }
                Err(error) => *error,
            },
            "addons_install" => match parse_arguments::<AddonsInstallArgs>(arguments) {
                Ok(args) if managed_addon_install_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::AddonsInstall(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The expected identity and SHA-256 checksum must be valid",
                ),
                Err(error) => *error,
            },
            "addons_set_enabled" => match parse_arguments::<AddonsSetEnabledArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::AddonsSetEnabled(args))
                        .await
                }
                Err(error) => *error,
            },
            "addons_remove" => match parse_arguments::<AddonsRemoveArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::AddonsRemove(args))
                        .await
                }
                Err(error) => *error,
            },
            "forwards_list" => match parse_arguments::<ForwardsListArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::ForwardsList(args))
                        .await
                }
                Err(error) => *error,
            },
            "forwards_open" => match parse_arguments::<ForwardsOpenArgs>(arguments) {
                Ok(args) if forwards_open_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::ForwardsOpen(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The forward bind and target definition is invalid",
                ),
                Err(error) => *error,
            },
            "forwards_change" => match parse_arguments::<ForwardsChangeArgs>(arguments) {
                Ok(args) if forward_patch_is_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::ForwardsChange(args))
                        .await
                }
                Ok(_) => tool_error(
                    "invalid_arguments",
                    "The forward patch and expected revision are required",
                ),
                Err(error) => *error,
            },
            "forwards_stop" => match parse_arguments::<ForwardHandleArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::ForwardsStop(args))
                        .await
                }
                Err(error) => *error,
            },
            "forwards_restart" => match parse_arguments::<ForwardHandleArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::ForwardsRestart(args))
                        .await
                }
                Err(error) => *error,
            },
            "forwards_remove" => match parse_arguments::<ForwardsRemoveArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::ForwardsRemove(args))
                        .await
                }
                Err(error) => *error,
            },
            "forwards_metrics" => match parse_arguments::<ForwardHandleArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::ForwardsMetrics(args))
                        .await
                }
                Err(error) => *error,
            },
            "forwards_discover_ports" => {
                match parse_arguments::<ForwardsDiscoverPortsArgs>(arguments) {
                    Ok(args) => {
                        self.execute_call(&client, PublicToolCall::ForwardsDiscoverPorts(args))
                            .await
                    }
                    Err(error) => *error,
                }
            }
            "files_open" => match parse_arguments::<FilesOpenArgs>(arguments) {
                Ok(args) if files_open_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::FilesOpen(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The SFTP root path is invalid"),
                Err(error) => *error,
            },
            "files_close" => match parse_arguments::<FilesCloseArgs>(arguments) {
                Ok(args) => {
                    self.execute_call(&client, PublicToolCall::FilesClose(args))
                        .await
                }
                Err(error) => *error,
            },
            "files_list" => match parse_arguments::<FilesListArgs>(arguments) {
                Ok(args) if files_list_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::FilesList(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The file listing request is invalid"),
                Err(error) => *error,
            },
            "files_stat" => match parse_arguments::<FilesStatArgs>(arguments) {
                Ok(args) if remote_path_is_valid(&args.path) => {
                    self.execute_call(&client, PublicToolCall::FilesStat(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The remote path is invalid"),
                Err(error) => *error,
            },
            "files_read" => match parse_arguments::<FilesReadArgs>(arguments) {
                Ok(args) if files_read_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::FilesRead(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The file read request is invalid"),
                Err(error) => *error,
            },
            "files_compare" => match parse_arguments::<FilesCompareArgs>(arguments) {
                Ok(args) if remote_path_is_valid(&args.path) => {
                    self.execute_call(&client, PublicToolCall::FilesCompare(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The remote path is invalid"),
                Err(error) => *error,
            },
            "files_write" => match parse_arguments::<FilesWriteArgs>(arguments) {
                Ok(args) if files_write_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::FilesWrite(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The file write request is invalid"),
                Err(error) => *error,
            },
            "files_move" => match parse_arguments::<FilesMoveArgs>(arguments) {
                Ok(args) if files_move_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::FilesMove(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The file move request is invalid"),
                Err(error) => *error,
            },
            "files_remove" => match parse_arguments::<FilesRemoveArgs>(arguments) {
                Ok(args) if files_remove_args_are_valid(&args) => {
                    self.execute_call(&client, PublicToolCall::FilesRemove(args))
                        .await
                }
                Ok(_) => tool_error("invalid_arguments", "The file removal request is invalid"),
                Err(error) => *error,
            },
            _ => tool_error("unknown_tool", "The requested tool is not implemented"),
        };
        Ok(result.into())
    }
}

struct ToolDefinition {
    tool: Tool,
    group: ToolGroup,
    additional_groups: &'static [ToolGroup],
    requires_approval: bool,
    requires_explicit_app_approval: bool,
}

impl ToolDefinition {
    fn with_additional_groups(mut self, additional_groups: &'static [ToolGroup]) -> Self {
        self.additional_groups = additional_groups;
        self
    }

    fn is_visible_to(&self, client: &ClientProjection) -> bool {
        client.tool_groups.contains(&self.group)
            && self
                .additional_groups
                .iter()
                .all(|group| client.tool_groups.contains(group))
    }
}

fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        define_tool::<EmptyArgs>(
            "mcp_overview",
            "Describe the OxideTerm public MCP endpoint and its authorization model.",
            ToolGroup::Basic,
            true,
            false,
        ),
        define_tool::<EmptyArgs>(
            "mcp_catalog",
            "List the tool groups visible to the current authorized client.",
            ToolGroup::Basic,
            true,
            false,
        ),
        define_tool::<EmptyArgs>(
            "mcp_access_state",
            "Show the current client's enabled tool groups without returning its credential.",
            ToolGroup::Basic,
            true,
            false,
        ),
        define_explicit_approval_tool::<RequestAccessArgs>(
            "mcp_request_access",
            "Request additional tool groups for this client through an in-app approval.",
            ToolGroup::Basic,
        ),
        define_tool::<RevokeAccessArgs>(
            "mcp_revoke_access",
            "Immediately disable selected tool groups for this client and release their capabilities.",
            ToolGroup::Basic,
            false,
            false,
        ),
        define_tool::<OperationStateArgs>(
            "mcp_operation",
            "Read redacted state and progress for a client-owned background operation.",
            ToolGroup::Basic,
            true,
            false,
        ),
        define_tool::<CancelOperationArgs>(
            "mcp_cancel_operation",
            "Request cancellation of a client-owned background operation without claiming rollback.",
            ToolGroup::Basic,
            false,
            false,
        ),
        define_tool::<RevertArgs>(
            "mcp_revert",
            "Apply the exact inverse retained for a client-owned Cloud Sync undo handle.",
            ToolGroup::Basic,
            false,
            true,
        ),
        define_tool::<CommitActionArgs>(
            "mcp_commit_action",
            "Commit an action that the user already approved in OxideTerm.",
            ToolGroup::Basic,
            false,
            false,
        ),
        define_tool::<BrowseConnectionsArgs>(
            "connections_browse",
            "Browse saved connection projections without secret values.",
            ToolGroup::ConnectionDirectory,
            true,
            false,
        ),
        define_tool::<DescribeConnectionArgs>(
            "connections_describe",
            "Read one saved connection projection without secret values.",
            ToolGroup::ConnectionRead,
            true,
            false,
        ),
        define_tool::<SavePublicConnectionArgs>(
            "connections_save",
            "Create or update a typed saved connection profile without secret values.",
            ToolGroup::ConnectionManage,
            false,
            true,
        ),
        define_tool::<RemovePublicConnectionArgs>(
            "connections_remove",
            "Remove a saved connection with explicit protected-credential handling.",
            ToolGroup::ConnectionManage,
            false,
            true,
        ),
        define_tool::<CredentialStatusArgs>(
            "credentials_status",
            "Report configured protected credential slots without returning values or storage references.",
            ToolGroup::CredentialManage,
            true,
            false,
        ),
        define_tool::<StoreCredentialSchema>(
            "credentials_store",
            "Store a new credential directly in OxideTerm's protected backend without exposing existing values.",
            ToolGroup::CredentialManage,
            false,
            true,
        ),
        define_tool::<ForgetCredentialArgs>(
            "credentials_forget",
            "Forget one protected credential slot without returning its previous value.",
            ToolGroup::CredentialManage,
            false,
            true,
        ),
        define_tool::<SyncStatusArgs>(
            "sync_status",
            "Read Cloud Sync state and configured capability without locations, tokens, or protected references.",
            ToolGroup::CloudSync,
            true,
            false,
        ),
        define_tool::<SyncPullPreviewArgs>(
            "sync_pull_preview",
            "Download and freeze a bounded Cloud Sync pull plan without applying it.",
            ToolGroup::CloudSync,
            true,
            false,
        ),
        define_tool::<SyncPublishPreviewArgs>(
            "sync_publish_preview",
            "Freeze a bounded Cloud Sync publish plan and check the current remote revision.",
            ToolGroup::CloudSync,
            true,
            false,
        ),
        define_tool::<SyncApplyPlanArgs>(
            "sync_apply_plan",
            "Apply one frozen pull or publish plan after checking local and remote revisions.",
            ToolGroup::CloudSync,
            false,
            true,
        ),
        define_tool::<SyncRestoreArgs>(
            "sync_restore",
            "Restore an exact local checkpoint returned by a prior Cloud Sync apply.",
            ToolGroup::CloudSync,
            false,
            true,
        ),
        define_tool::<ConnectNodeArgs>(
            "nodes_connect",
            "Connect or acquire a physical SSH node through OxideTerm's NodeRouter.",
            ToolGroup::NodeSession,
            false,
            true,
        ),
        define_tool::<InspectNodeArgs>(
            "nodes_inspect",
            "Inspect the public state of an acquired node.",
            ToolGroup::NodeSession,
            true,
            false,
        ),
        define_tool::<ReleaseNodeArgs>(
            "nodes_release",
            "Release this MCP client's node consumer without disconnecting the physical node.",
            ToolGroup::NodeSession,
            false,
            false,
        ),
        define_tool::<DisconnectNodeArgs>(
            "nodes_disconnect",
            "Explicitly disconnect the physical node after user approval.",
            ToolGroup::NodeSession,
            false,
            true,
        ),
        define_tool::<OpenTerminalArgs>(
            "terminals_open",
            "Open a real visible SSH, local, Mosh, Telnet, or serial terminal session.",
            ToolGroup::TerminalSession,
            false,
            true,
        ),
        define_tool::<TerminalHandleArgs>(
            "terminals_state",
            "Read terminal lifecycle, dimensions, transport, and capabilities without content.",
            ToolGroup::TerminalObserve,
            true,
            false,
        ),
        define_tool::<ReadTerminalArgs>(
            "terminals_read",
            "Read a bounded visible terminal snapshot with a generation cursor.",
            ToolGroup::TerminalObserve,
            true,
            false,
        ),
        define_tool::<crate::calls::FindTerminalArgs>(
            "terminals_find",
            "Search the real terminal scrollback and return bounded match coordinates.",
            ToolGroup::TerminalObserve,
            true,
            false,
        ),
        define_tool::<SubmitTerminalSchema>(
            "terminals_submit",
            "Submit exact text or bytes to a live terminal without claiming command completion.",
            ToolGroup::TerminalInput,
            false,
            true,
        ),
        define_tool::<ResizeTerminalArgs>(
            "terminals_resize",
            "Resize the live terminal grid using its current cell metrics.",
            ToolGroup::TerminalSession,
            false,
            false,
        ),
        define_tool::<crate::calls::ControlTerminalArgs>(
            "terminals_control",
            "Apply one typed control supported by the terminal's actual transport.",
            ToolGroup::TerminalInput,
            false,
            true,
        ),
        define_tool::<TerminalHandleArgs>(
            "terminals_close",
            "Close this client-owned terminal without disconnecting a shared physical SSH node.",
            ToolGroup::TerminalSession,
            false,
            false,
        ),
        define_tool::<RecordingsControlArgs>(
            "recordings_control",
            "Start, pause, resume, or stop an output-only recording on a client-owned terminal.",
            ToolGroup::RecordingControl,
            false,
            true,
        ),
        define_tool::<RecordingsStatusArgs>(
            "recordings_status",
            "Read recording state and bounded metadata without returning terminal content.",
            ToolGroup::RecordingControl,
            true,
            false,
        ),
        define_tool::<RecordingsSearchArgs>(
            "recordings_search",
            "Search bounded snippets in one stopped client-owned terminal recording.",
            ToolGroup::RecordingContent,
            true,
            false,
        ),
        define_tool::<RecordingsExportArgs>(
            "recordings_export",
            "Export one stopped terminal recording to a client-scoped temporary artifact.",
            ToolGroup::RecordingContent,
            false,
            true,
        )
        .with_additional_groups(&[ToolGroup::ArtifactTransfer]),
        define_tool::<OpenDesktopArgs>(
            "desktops_open",
            "Open a real saved RDP or VNC profile in a visible OxideTerm tab.",
            ToolGroup::DesktopSession,
            false,
            true,
        ),
        define_tool::<DesktopHandleArgs>(
            "desktops_state",
            "Read the session, security, framebuffer, input, and clipboard capability state.",
            ToolGroup::DesktopObserve,
            true,
            false,
        ),
        define_tool::<DesktopFrameArgs>(
            "desktops_frame",
            "Encode the latest bounded framebuffer as a client-scoped PNG artifact.",
            ToolGroup::DesktopObserve,
            true,
            false,
        )
        .with_additional_groups(&[ToolGroup::ArtifactTransfer]),
        define_tool::<DesktopInputSchema>(
            "desktops_input",
            "Send one strict mouse, wheel, key, text, or release-all event for the current framebuffer epoch.",
            ToolGroup::DesktopInput,
            false,
            true,
        ),
        define_tool::<ResizeDesktopArgs>(
            "desktops_resize",
            "Request a bounded remote framebuffer resize when the provider supports it.",
            ToolGroup::DesktopInput,
            false,
            false,
        ),
        define_tool::<ReadDesktopClipboardArgs>(
            "desktops_clipboard_read",
            "Read the latest remote clipboard value; image content also requires artifact transfer.",
            ToolGroup::DesktopClipboard,
            true,
            false,
        ),
        define_tool::<WriteDesktopClipboardSchema>(
            "desktops_clipboard_write",
            "Write exact text or a bounded image artifact; images also require artifact transfer.",
            ToolGroup::DesktopClipboard,
            false,
            true,
        ),
        define_tool::<DesktopHandleArgs>(
            "desktops_reconnect",
            "Reconnect the existing client-owned remote desktop session using its retained profile.",
            ToolGroup::DesktopSession,
            false,
            true,
        ),
        define_tool::<DesktopHandleArgs>(
            "desktops_close",
            "Release all remote inputs and close the client-owned desktop helper and tab.",
            ToolGroup::DesktopSession,
            false,
            false,
        ),
        define_tool::<StartCommandSchema>(
            "commands_start",
            "Start a command on an acquired SSH node and return a command handle.",
            ToolGroup::CommandExecute,
            false,
            true,
        ),
        define_tool::<CommandStateArgs>(
            "commands_state",
            "Read the state and exit status of a command handle.",
            ToolGroup::CommandObserve,
            true,
            false,
        ),
        define_tool::<CommandOutputArgs>(
            "commands_output",
            "Read a bounded output range from a command handle.",
            ToolGroup::CommandObserve,
            true,
            false,
        ),
        define_tool::<CancelCommandArgs>(
            "commands_cancel",
            "Cancel a running command owned by this client.",
            ToolGroup::CommandExecute,
            false,
            false,
        ),
        define_tool::<StageArtifactSchema>(
            "artifacts_stage",
            "Stage bounded content in OxideTerm's client-scoped temporary artifact store.",
            ToolGroup::ArtifactTransfer,
            false,
            false,
        ),
        define_tool::<ReadArtifactArgs>(
            "artifacts_read",
            "Read a bounded range from a temporary artifact owned by this client.",
            ToolGroup::ArtifactTransfer,
            true,
            false,
        ),
        define_tool::<StartTransferArgs>(
            "transfers_start",
            "Start one bounded background SFTP upload or download between an authorized remote path and client-owned artifact storage.",
            ToolGroup::ArtifactTransfer,
            false,
            true,
        ),
        define_tool::<TransferHandleArgs>(
            "transfers_status",
            "Read bounded progress and the completed artifact for one client-owned transfer.",
            ToolGroup::ArtifactTransfer,
            true,
            false,
        ),
        define_tool::<TransferHandleArgs>(
            "transfers_cancel",
            "Cancel one client-owned background transfer without disconnecting its SSH node.",
            ToolGroup::ArtifactTransfer,
            false,
            false,
        ),
        define_tool::<WorkspaceMountArgs>(
            "workspaces_mount",
            "Mount a client-scoped remote IDE workspace beneath an authorized SFTP root.",
            ToolGroup::WorkspaceRead,
            false,
            false,
        ),
        define_tool::<WorkspaceTreeArgs>(
            "workspaces_tree",
            "List one bounded page from a mounted remote IDE workspace tree.",
            ToolGroup::WorkspaceRead,
            true,
            false,
        ),
        define_tool::<WorkspaceReadArgs>(
            "workspaces_read",
            "Read one bounded editable text file and its conflict-detection revision.",
            ToolGroup::WorkspaceRead,
            true,
            false,
        ),
        define_tool::<WorkspaceApplyEditsSchema>(
            "workspaces_apply_edits",
            "Apply bounded byte-range text edits after checking every observed file revision.",
            ToolGroup::WorkspaceEdit,
            false,
            true,
        ),
        define_tool::<WorkspaceSearchArgs>(
            "workspaces_search",
            "Search a mounted workspace through the node agent or bounded remote fallback.",
            ToolGroup::WorkspaceRead,
            true,
            false,
        ),
        define_tool::<WorkspaceCloseArgs>(
            "workspaces_close",
            "Release one IDE workspace consumer without disconnecting the physical SSH node.",
            ToolGroup::WorkspaceRead,
            false,
            false,
        ),
        define_tool::<AuditSearchArgs>(
            "mcp_audit_search",
            "Search this client's own redacted Public MCP audit records.",
            ToolGroup::AuditRead,
            true,
            false,
        ),
        define_tool::<HostToolsCatalogArgs>(
            "hosttools_catalog",
            "List the fixed typed Host Tools resources available for an acquired SSH node.",
            ToolGroup::HostToolsObserve,
            true,
            false,
        ),
        define_tool::<HostToolsCaptureArgs>(
            "hosttools_capture",
            "Capture one bounded typed Host Tools snapshot without accepting shell text.",
            ToolGroup::HostToolsObserve,
            true,
            false,
        ),
        define_tool::<HostToolsOperateArgs>(
            "hosttools_operate",
            "Run one fixed typed Host Tools action without accepting shell or plugin calls.",
            ToolGroup::HostToolsOperate,
            false,
            true,
        ),
        define_tool::<QuickCommandsListArgs>(
            "quickcommands_list",
            "List saved Quick Command metadata without returning command bodies.",
            ToolGroup::QuickCommandRead,
            true,
            false,
        ),
        define_tool::<QuickCommandsDescribeArgs>(
            "quickcommands_describe",
            "Read one saved Quick Command body under its separate content grant.",
            ToolGroup::QuickCommandContentRead,
            true,
            false,
        ),
        define_tool::<QuickCommandsSaveSchema>(
            "quickcommands_save",
            "Create or update one saved Quick Command at an expected store revision.",
            ToolGroup::QuickCommandManage,
            false,
            true,
        ),
        define_tool::<QuickCommandsRemoveArgs>(
            "quickcommands_remove",
            "Remove one saved Quick Command at an expected store revision.",
            ToolGroup::QuickCommandManage,
            false,
            true,
        ),
        define_tool::<QuickCommandsRunSchema>(
            "quickcommands_run",
            "Expand and execute one saved Quick Command on an acquired SSH node.",
            ToolGroup::QuickCommandExecute,
            false,
            true,
        ),
        define_tool::<AddonsListArgs>(
            "addons_list",
            "List installed addon metadata without exposing local paths or plugin call surfaces.",
            ToolGroup::AddonRead,
            true,
            false,
        ),
        define_tool::<AddonsInstallArgs>(
            "addons_install",
            "Install a checksum-verified addon package from a client-owned temporary artifact.",
            ToolGroup::AddonManage,
            false,
            true,
        )
        .with_additional_groups(&[ToolGroup::ArtifactTransfer]),
        define_tool::<AddonsSetEnabledArgs>(
            "addons_set_enabled",
            "Enable or disable an installed addon through OxideTerm's managed lifecycle.",
            ToolGroup::AddonManage,
            false,
            true,
        ),
        define_tool::<AddonsRemoveArgs>(
            "addons_remove",
            "Remove an installed addon while explicitly choosing whether to retain its settings.",
            ToolGroup::AddonManage,
            false,
            true,
        ),
        define_tool::<ForwardsListArgs>(
            "forwards_list",
            "List bounded port-forward projections without exposing internal rule identities.",
            ToolGroup::ForwardRead,
            true,
            false,
        ),
        define_tool::<ForwardsOpenArgs>(
            "forwards_open",
            "Open one typed local, remote, or dynamic forward on an acquired SSH node.",
            ToolGroup::ForwardManage,
            false,
            true,
        ),
        define_tool::<ForwardsChangeArgs>(
            "forwards_change",
            "Change one owned or explicitly listed forward at an expected revision.",
            ToolGroup::ForwardManage,
            false,
            true,
        ),
        define_tool::<ForwardHandleArgs>(
            "forwards_stop",
            "Stop one forward without releasing the MCP node or other forward consumers.",
            ToolGroup::ForwardManage,
            false,
            true,
        ),
        define_tool::<ForwardHandleArgs>(
            "forwards_restart",
            "Restart one stopped forward using its existing typed definition.",
            ToolGroup::ForwardManage,
            false,
            true,
        ),
        define_tool::<ForwardsRemoveArgs>(
            "forwards_remove",
            "Remove one runtime forward and optionally its saved definition.",
            ToolGroup::ForwardManage,
            false,
            true,
        ),
        define_tool::<ForwardHandleArgs>(
            "forwards_metrics",
            "Read connection and byte counters for one forward.",
            ToolGroup::ForwardRead,
            true,
            false,
        ),
        define_tool::<ForwardsDiscoverPortsArgs>(
            "forwards_discover_ports",
            "Run one bounded typed remote listening-port scan without starting a profiler.",
            ToolGroup::ForwardRead,
            true,
            false,
        ),
        define_tool::<FilesOpenArgs>(
            "files_open",
            "Open a client-scoped SFTP capability rooted at a canonical remote directory.",
            ToolGroup::FileRead,
            false,
            false,
        ),
        define_tool::<FilesCloseArgs>(
            "files_close",
            "Release one SFTP capability without disconnecting its shared SSH node.",
            ToolGroup::FileRead,
            false,
            false,
        ),
        define_tool::<FilesListArgs>(
            "files_list",
            "List one bounded page of entries beneath an authorized SFTP root.",
            ToolGroup::FileRead,
            true,
            false,
        ),
        define_tool::<FilesStatArgs>(
            "files_stat",
            "Read public metadata and a revision for one authorized remote path.",
            ToolGroup::FileRead,
            true,
            false,
        ),
        define_tool::<FilesReadArgs>(
            "files_read",
            "Read one bounded remote file range into a client-owned temporary artifact.",
            ToolGroup::FileRead,
            true,
            false,
        )
        .with_additional_groups(&[ToolGroup::ArtifactTransfer]),
        define_tool::<FilesCompareArgs>(
            "files_compare",
            "Compare one bounded remote file with a client-owned artifact without changing it.",
            ToolGroup::FileRead,
            true,
            false,
        )
        .with_additional_groups(&[ToolGroup::ArtifactTransfer]),
        define_tool::<FilesWriteArgs>(
            "files_write",
            "Write one client-owned artifact to an authorized remote path.",
            ToolGroup::FileWrite,
            false,
            true,
        )
        .with_additional_groups(&[ToolGroup::ArtifactTransfer]),
        define_tool::<FilesMoveArgs>(
            "files_move",
            "Move one authorized remote path within the same SFTP root.",
            ToolGroup::FileWrite,
            false,
            true,
        ),
        define_tool::<FilesRemoveArgs>(
            "files_remove",
            "Remove one authorized remote path with explicit recursive intent.",
            ToolGroup::FileWrite,
            false,
            true,
        ),
    ]
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
struct EmptyArgs {}

fn define_tool<T: JsonSchema>(
    name: &'static str,
    description: &'static str,
    group: ToolGroup,
    read_only: bool,
    requires_approval: bool,
) -> ToolDefinition {
    let annotations = ToolAnnotations::new()
        .read_only(read_only)
        .destructive(requires_approval)
        .open_world(!read_only);
    ToolDefinition {
        tool: Tool::new(name, description, schema_object::<T>()).with_annotations(annotations),
        group,
        additional_groups: &[],
        requires_approval,
        requires_explicit_app_approval: false,
    }
}

fn define_explicit_approval_tool<T: JsonSchema>(
    name: &'static str,
    description: &'static str,
    group: ToolGroup,
) -> ToolDefinition {
    let mut definition = define_tool::<T>(name, description, group, false, true);
    definition.requires_explicit_app_approval = true;
    definition
}

fn schema_object<T: JsonSchema>() -> JsonObject {
    let mut schema = serde_json::to_value(schema_for!(T))
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    // MCP tool arguments cross the protocol as JSON objects, even when the schema uses oneOf.
    schema.insert("type".to_owned(), Value::String("object".to_owned()));
    schema
}

fn parse_arguments<T: DeserializeOwned>(arguments: JsonObject) -> Result<T, Box<CallToolResult>> {
    serde_json::from_value(Value::Object(arguments)).map_err(|error| {
        Box::new(tool_error(
            "invalid_arguments",
            format!("The tool arguments are invalid: {error}"),
        ))
    })
}

fn access_groups_are_valid(groups: &[ToolGroup]) -> bool {
    !groups.is_empty()
        && groups.len() <= ToolGroup::selectable().len()
        && groups
            .iter()
            .all(|group| ToolGroup::selectable().contains(group))
}

fn parse_start_command(mut arguments: JsonObject) -> Result<StartCommandArgs, Box<CallToolResult>> {
    let command = match arguments.remove("command") {
        Some(Value::String(command))
            if !command.trim().is_empty() && command.len() <= COMMAND_TEXT_LIMIT_BYTES =>
        {
            Zeroizing::new(command)
        }
        _ => {
            return Err(Box::new(tool_error(
                "invalid_arguments",
                "The command must be a non-empty string within the supported size limit",
            )));
        }
    };
    let metadata = parse_arguments::<StartCommandMetadata>(arguments)?;
    if metadata
        .working_directory
        .as_ref()
        .is_some_and(|directory| directory.len() > WORKING_DIRECTORY_LIMIT_BYTES)
    {
        return Err(Box::new(tool_error(
            "invalid_arguments",
            "The working directory exceeds the supported size limit",
        )));
    }
    Ok(StartCommandArgs {
        node_ref: metadata.node_ref,
        command,
        working_directory: metadata.working_directory.map(Zeroizing::new),
    })
}

fn parse_workspace_apply_edits(
    arguments: JsonObject,
) -> Result<WorkspaceApplyEditsArgs, Box<CallToolResult>> {
    let schema = parse_arguments::<WorkspaceApplyEditsSchema>(arguments)?;
    Ok(WorkspaceApplyEditsArgs {
        workspace_ref: schema.workspace_ref,
        files: schema
            .files
            .into_iter()
            .map(|file| WorkspaceFileEdits {
                path: file.path,
                expected_revision: file.expected_revision,
                edits: file
                    .edits
                    .into_iter()
                    .map(|edit| WorkspaceTextEdit {
                        start_byte: edit.start_byte,
                        end_byte: edit.end_byte,
                        replacement: Zeroizing::new(edit.replacement),
                    })
                    .collect(),
            })
            .collect(),
    })
}

fn parse_store_credential(
    mut arguments: JsonObject,
) -> Result<StoreCredentialArgs, Box<CallToolResult>> {
    // Move the incoming secret into a zeroizing owner before parsing non-secret metadata.
    let new_secret = match arguments.remove("new_secret") {
        Some(Value::String(secret))
            if !secret.is_empty() && secret.len() <= CREDENTIAL_SECRET_LIMIT_BYTES =>
        {
            Zeroizing::new(secret)
        }
        _ => {
            return Err(Box::new(tool_error(
                "invalid_arguments",
                "The new credential must be non-empty and within the supported size limit",
            )));
        }
    };
    let metadata = parse_arguments::<StoreCredentialMetadata>(arguments)?;
    Ok(StoreCredentialArgs {
        connection_ref: metadata.connection_ref,
        slot: metadata.slot,
        new_secret,
    })
}

fn parse_terminal_submit(
    mut arguments: JsonObject,
) -> Result<SubmitTerminalArgs, Box<CallToolResult>> {
    let text = arguments.remove("text");
    let bytes_base64 = arguments.remove("bytes_base64");
    let (input, is_text) = match (text, bytes_base64) {
        (Some(Value::String(text)), None) if text.len() <= TERMINAL_INPUT_LIMIT_BYTES => {
            (Zeroizing::new(text.into_bytes()), true)
        }
        (None, Some(Value::String(encoded))) => {
            let encoded = Zeroizing::new(encoded);
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| {
                    Box::new(tool_error(
                        "invalid_arguments",
                        "bytes_base64 must contain valid base64",
                    ))
                })?;
            if decoded.len() > TERMINAL_INPUT_LIMIT_BYTES {
                return Err(Box::new(tool_error(
                    "input_too_large",
                    "Terminal input exceeds the 262144-byte limit",
                )));
            }
            (Zeroizing::new(decoded), false)
        }
        _ => {
            return Err(Box::new(tool_error(
                "invalid_arguments",
                "Provide exactly one of text or bytes_base64 within the supported limit",
            )));
        }
    };
    let metadata = parse_arguments::<SubmitTerminalMetadata>(arguments)?;
    if input.is_empty() && !metadata.append_enter {
        return Err(Box::new(tool_error(
            "invalid_arguments",
            "Terminal input cannot be empty unless append_enter is true",
        )));
    }
    Ok(SubmitTerminalArgs {
        terminal_ref: metadata.terminal_ref,
        input,
        append_enter: metadata.append_enter,
        is_text,
    })
}

fn terminal_dimensions_are_valid(cols: u16, rows: u16) -> bool {
    (2..=TERMINAL_DIMENSION_MAXIMUM).contains(&cols)
        && (2..=TERMINAL_DIMENSION_MAXIMUM).contains(&rows)
}

fn terminal_title_is_valid(title: &str) -> bool {
    !title.trim().is_empty()
        && title.len() <= TERMINAL_TITLE_LIMIT_BYTES
        && !title.chars().any(char::is_control)
}

fn recording_control_is_valid(args: &RecordingsControlArgs) -> bool {
    match args {
        RecordingsControlArgs::Start {
            title,
            capture_input,
            ..
        } => {
            !capture_input
                && title.as_deref().is_none_or(|title| {
                    !title.trim().is_empty()
                        && title.len() <= RECORDING_TITLE_LIMIT_BYTES
                        && !title.chars().any(char::is_control)
                })
        }
        RecordingsControlArgs::Pause { .. }
        | RecordingsControlArgs::Resume { .. }
        | RecordingsControlArgs::Stop { .. } => true,
    }
}

fn desktop_dimensions_are_valid(width: u32, height: u32) -> bool {
    (DESKTOP_MIN_WIDTH..=DESKTOP_MAX_DIMENSION).contains(&width)
        && (DESKTOP_MIN_HEIGHT..=DESKTOP_MAX_DIMENSION).contains(&height)
}

fn desktop_input_is_valid(event: &DesktopInputEvent) -> bool {
    match event {
        DesktopInputEvent::MouseMove { .. } | DesktopInputEvent::MouseButton { .. } => true,
        DesktopInputEvent::Wheel {
            delta_x, delta_y, ..
        } => {
            delta_x.is_finite()
                && delta_y.is_finite()
                && delta_x.abs() <= DESKTOP_WHEEL_DELTA_LIMIT
                && delta_y.abs() <= DESKTOP_WHEEL_DELTA_LIMIT
                && (delta_x.abs() > f32::EPSILON || delta_y.abs() > f32::EPSILON)
        }
        DesktopInputEvent::Key { code, text, .. } => {
            !code.trim().is_empty()
                && code.len() <= DESKTOP_KEY_CODE_LIMIT_BYTES
                && !code.chars().any(char::is_control)
                && text
                    .as_deref()
                    .is_none_or(|text| text.len() <= DESKTOP_KEY_TEXT_LIMIT_BYTES)
        }
        DesktopInputEvent::Text { text } => {
            !text.is_empty() && text.len() <= DESKTOP_TEXT_INPUT_LIMIT_BYTES
        }
        DesktopInputEvent::ReleaseAll => true,
    }
}

fn desktop_clipboard_payload_is_valid(payload: &DesktopClipboardPayload) -> bool {
    match payload {
        DesktopClipboardPayload::Text { text } => {
            !text.is_empty() && text.len() <= DESKTOP_CLIPBOARD_TEXT_LIMIT_BYTES
        }
        DesktopClipboardPayload::Image { .. } => true,
    }
}

fn parse_stage_artifact(
    mut arguments: JsonObject,
) -> Result<StageArtifactArgs, Box<CallToolResult>> {
    let content = arguments.remove("content");
    let bytes_base64 = arguments.remove("bytes_base64");
    let (bytes, default_media_type) = match (content, bytes_base64) {
        (Some(Value::String(content)), None) if content.len() <= ARTIFACT_STAGE_LIMIT_BYTES => (
            Zeroizing::new(content.into_bytes()),
            "text/plain; charset=utf-8",
        ),
        (None, Some(Value::String(encoded))) => {
            let encoded = Zeroizing::new(encoded);
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| {
                    Box::new(tool_error(
                        "invalid_arguments",
                        "bytes_base64 must contain valid standard Base64",
                    ))
                })?;
            if decoded.len() > ARTIFACT_STAGE_LIMIT_BYTES {
                return Err(Box::new(tool_error(
                    "invalid_arguments",
                    "The decoded artifact exceeds the supported staging limit",
                )));
            }
            (Zeroizing::new(decoded), "application/octet-stream")
        }
        _ => {
            return Err(Box::new(tool_error(
                "invalid_arguments",
                "Provide exactly one bounded content or bytes_base64 value",
            )));
        }
    };
    let metadata = parse_arguments::<StageArtifactMetadata>(arguments)?;
    Ok(StageArtifactArgs {
        bytes,
        media_type: metadata
            .media_type
            .unwrap_or_else(|| default_media_type.to_owned()),
        name: metadata.name,
    })
}

fn parse_quick_commands_save(
    mut arguments: JsonObject,
) -> Result<QuickCommandsSaveArgs, Box<CallToolResult>> {
    let command = match arguments.remove("command") {
        Some(Value::String(command))
            if !command.trim().is_empty() && command.len() <= QUICK_COMMAND_BODY_LIMIT_BYTES =>
        {
            Zeroizing::new(command)
        }
        _ => {
            return Err(Box::new(tool_error(
                "invalid_arguments",
                "The Quick Command body must be non-empty and at most 4096 bytes",
            )));
        }
    };
    let metadata = parse_arguments::<QuickCommandsSaveMetadata>(arguments)?;
    if metadata.name.trim().is_empty() || metadata.name.len() > QUICK_COMMAND_NAME_LIMIT_BYTES {
        return Err(Box::new(tool_error(
            "invalid_arguments",
            "The Quick Command name must be non-empty and at most 160 bytes",
        )));
    }
    if metadata.host_pattern.is_some() && metadata.host_patterns.is_some() {
        return Err(Box::new(tool_error(
            "invalid_arguments",
            "Provide host_pattern or host_patterns, not both",
        )));
    }
    if !quick_commands_save_metadata_is_bounded(&metadata) {
        return Err(Box::new(tool_error(
            "invalid_arguments",
            "The Quick Command definition exceeds its supported limits",
        )));
    }
    Ok(QuickCommandsSaveArgs {
        quickcommand_ref: metadata.quickcommand_ref,
        name: metadata.name,
        command,
        category: metadata.category,
        description: metadata.description,
        host_pattern: metadata.host_pattern,
        host_patterns: metadata.host_patterns,
        parameters: metadata.parameters,
        protocols: metadata.protocols,
        confirmation: metadata.confirmation,
        expected_revision: metadata.expected_revision,
    })
}

fn parse_quick_commands_run(
    mut arguments: JsonObject,
) -> Result<QuickCommandsRunArgs, Box<CallToolResult>> {
    let parameter_values = match arguments.remove("arguments") {
        None | Some(Value::Null) => BTreeMap::new(),
        Some(Value::Object(values)) => {
            if values.len() > QUICK_COMMAND_ARGUMENT_LIMIT {
                return Err(Box::new(tool_error(
                    "invalid_arguments",
                    "Too many Quick Command arguments were provided",
                )));
            }
            let mut total_bytes = 0usize;
            values
                .into_iter()
                .map(|(name, value)| match value {
                    Value::String(value)
                        if name.len() <= QUICK_COMMAND_ARGUMENT_NAME_LIMIT_BYTES
                            && value.len() <= QUICK_COMMAND_ARGUMENT_VALUE_LIMIT_BYTES
                            && total_bytes.saturating_add(value.len())
                                <= QUICK_COMMAND_ARGUMENT_TOTAL_LIMIT_BYTES =>
                    {
                        total_bytes = total_bytes.saturating_add(value.len());
                        Ok((name, Zeroizing::new(value)))
                    }
                    Value::String(_) => Err(Box::new(tool_error(
                        "invalid_arguments",
                        "Quick Command arguments exceed their supported limits",
                    ))),
                    _ => Err(Box::new(tool_error(
                        "invalid_arguments",
                        "Quick Command argument values must be strings",
                    ))),
                })
                .collect::<Result<_, _>>()?
        }
        Some(_) => {
            return Err(Box::new(tool_error(
                "invalid_arguments",
                "Quick Command arguments must be an object",
            )));
        }
    };
    let metadata = parse_arguments::<QuickCommandsRunMetadata>(arguments)?;
    Ok(QuickCommandsRunArgs {
        quickcommand_ref: metadata.quickcommand_ref,
        node_ref: metadata.node_ref,
        expected_revision: metadata.expected_revision,
        arguments: parameter_values,
    })
}

fn quick_commands_save_metadata_is_bounded(metadata: &QuickCommandsSaveMetadata) -> bool {
    if metadata.category.trim().is_empty()
        || metadata.category.len() > 128
        || metadata
            .description
            .as_ref()
            .is_some_and(|value| value.len() > QUICK_COMMAND_DESCRIPTION_LIMIT_BYTES)
        || metadata
            .protocols
            .as_ref()
            .is_some_and(|protocols| protocols.len() > 6)
    {
        return false;
    }
    let host_patterns = metadata.host_patterns.as_deref().unwrap_or_default();
    if host_patterns.len() > QUICK_COMMAND_HOST_PATTERN_LIMIT
        || host_patterns
            .iter()
            .any(|pattern| pattern.len() > QUICK_COMMAND_HOST_PATTERN_LIMIT_BYTES)
        || metadata
            .host_pattern
            .as_ref()
            .is_some_and(|pattern| pattern.len() > QUICK_COMMAND_HOST_PATTERN_LIMIT_BYTES)
    {
        return false;
    }
    let parameters = metadata.parameters.as_deref().unwrap_or_default();
    if parameters.len() > QUICK_COMMAND_ARGUMENT_LIMIT {
        return false;
    }
    let mut total_bytes = metadata.description.as_ref().map_or(0, String::len)
        + metadata.host_pattern.as_ref().map_or(0, String::len)
        + host_patterns.iter().map(String::len).sum::<usize>();
    for parameter in parameters {
        if parameter.name.len() > QUICK_COMMAND_ARGUMENT_NAME_LIMIT_BYTES
            || parameter.label.len() > QUICK_COMMAND_NAME_LIMIT_BYTES
            || parameter
                .default_value
                .as_ref()
                .is_some_and(|value| value.len() > QUICK_COMMAND_PARAMETER_DEFAULT_LIMIT_BYTES)
            || parameter.choices.len() > QUICK_COMMAND_PARAMETER_CHOICE_LIMIT
            || parameter
                .choices
                .iter()
                .any(|choice| choice.len() > QUICK_COMMAND_PARAMETER_DEFAULT_LIMIT_BYTES)
        {
            return false;
        }
        if matches!(
            parameter.kind,
            crate::PublicQuickCommandParameterKind::Secret
        ) && (parameter.default_value.is_some() || !parameter.choices.is_empty())
        {
            return false;
        }
        total_bytes = total_bytes
            .saturating_add(parameter.name.len())
            .saturating_add(parameter.label.len())
            .saturating_add(parameter.default_value.as_ref().map_or(0, String::len))
            .saturating_add(parameter.choices.iter().map(String::len).sum::<usize>());
    }
    total_bytes <= QUICK_COMMAND_ARGUMENT_TOTAL_LIMIT_BYTES
}

fn managed_addon_install_args_are_valid(args: &AddonsInstallArgs) -> bool {
    let expected_identity = args.expected_identity.trim();
    let checksum = args
        .checksum
        .strip_prefix("sha256:")
        .unwrap_or(&args.checksum);
    !expected_identity.is_empty()
        && expected_identity.len() <= ADDON_ID_LIMIT_BYTES
        && !expected_identity.chars().any(char::is_control)
        && checksum.len() == 64
        && checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn forwards_open_args_are_valid(args: &ForwardsOpenArgs) -> bool {
    if !forward_text_is_valid(&args.bind_address, FORWARD_ENDPOINT_LIMIT_BYTES)
        || args
            .description
            .as_deref()
            .is_some_and(|description| !forward_description_is_valid(description))
    {
        return false;
    }
    match args.kind {
        ForwardKind::Local | ForwardKind::Remote => {
            args.target_host
                .as_deref()
                .is_some_and(|host| forward_text_is_valid(host, FORWARD_ENDPOINT_LIMIT_BYTES))
                && args.target_port.is_some_and(|port| port > 0)
        }
        ForwardKind::Dynamic => {
            args.target_host.as_deref().is_none_or(str::is_empty)
                && args.target_port.is_none_or(|port| port == 0)
        }
    }
}

fn forward_patch_is_valid(args: &ForwardsChangeArgs) -> bool {
    let patch = &args.patch;
    forward_text_is_valid(&args.expected_revision, FORWARD_REVISION_LIMIT_BYTES)
        && (patch.kind.is_some()
            || patch.bind_address.is_some()
            || patch.bind_port.is_some()
            || patch.target_host.is_some()
            || patch.target_port.is_some()
            || patch.description.is_some())
        && patch
            .bind_address
            .as_deref()
            .is_none_or(|address| forward_text_is_valid(address, FORWARD_ENDPOINT_LIMIT_BYTES))
        && patch
            .target_host
            .as_deref()
            .is_none_or(|host| forward_text_is_valid(host, FORWARD_ENDPOINT_LIMIT_BYTES))
        && patch
            .description
            .as_deref()
            .is_none_or(forward_description_is_valid)
}

fn forward_text_is_valid(value: &str, maximum_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}

fn forward_description_is_valid(value: &str) -> bool {
    value.len() <= FORWARD_DESCRIPTION_LIMIT_BYTES && !value.chars().any(char::is_control)
}

fn files_open_args_are_valid(args: &FilesOpenArgs) -> bool {
    args.root.as_deref().is_none_or(remote_path_is_valid)
}

fn files_list_args_are_valid(args: &FilesListArgs) -> bool {
    args.path.as_deref().is_none_or(remote_path_is_valid)
        && args
            .limit
            .is_none_or(|limit| limit > 0 && limit <= FILE_LIST_LIMIT_MAXIMUM)
        && args.pattern.as_deref().is_none_or(|pattern| {
            pattern.len() <= FORWARD_ENDPOINT_LIMIT_BYTES && !pattern.chars().any(char::is_control)
        })
}

fn files_read_args_are_valid(args: &FilesReadArgs) -> bool {
    remote_path_is_valid(&args.path)
        && args
            .maximum_bytes
            .is_none_or(|limit| limit > 0 && limit <= FILE_READ_LIMIT_MAXIMUM)
}

fn files_write_args_are_valid(args: &FilesWriteArgs) -> bool {
    remote_path_is_valid(&args.path)
        && optional_revision_is_valid(args.expected_revision.as_deref())
}

fn files_move_args_are_valid(args: &FilesMoveArgs) -> bool {
    remote_path_is_valid(&args.source_path)
        && remote_path_is_valid(&args.destination_path)
        && args.source_path != args.destination_path
        && optional_revision_is_valid(args.expected_revision.as_deref())
}

fn files_remove_args_are_valid(args: &FilesRemoveArgs) -> bool {
    remote_path_is_valid(&args.path)
        && optional_revision_is_valid(args.expected_revision.as_deref())
}

fn workspace_tree_args_are_valid(args: &WorkspaceTreeArgs) -> bool {
    args.path.as_deref().is_none_or(remote_path_is_valid)
        && args
            .limit
            .is_none_or(|limit| limit > 0 && limit <= FILE_LIST_LIMIT_MAXIMUM)
}

fn workspace_apply_edits_args_are_valid(args: &WorkspaceApplyEditsArgs) -> bool {
    !args.files.is_empty()
        && args.files.len() <= WORKSPACE_EDIT_FILE_LIMIT
        && args.files.iter().all(|file| {
            remote_path_is_valid(&file.path)
                && forward_text_is_valid(&file.expected_revision, FORWARD_REVISION_LIMIT_BYTES)
                && !file.edits.is_empty()
                && file.edits.len() <= WORKSPACE_EDIT_COUNT_LIMIT
                && file.edits.iter().all(|edit| {
                    edit.start_byte <= edit.end_byte
                        && edit.replacement.len() <= WORKSPACE_EDIT_REPLACEMENT_LIMIT_BYTES
                })
        })
}

fn workspace_search_args_are_valid(args: &WorkspaceSearchArgs) -> bool {
    !args.pattern.is_empty()
        && args.pattern.len() <= WORKSPACE_SEARCH_PATTERN_LIMIT_BYTES
        && !args.pattern.chars().any(char::is_control)
        && args.root.as_deref().is_none_or(remote_path_is_valid)
        && args
            .maximum_results
            .is_none_or(|limit| limit > 0 && limit <= WORKSPACE_SEARCH_RESULT_LIMIT)
}

fn optional_revision_is_valid(revision: Option<&str>) -> bool {
    revision.is_none_or(|revision| forward_text_is_valid(revision, FORWARD_REVISION_LIMIT_BYTES))
}

fn remote_path_is_valid(path: &str) -> bool {
    !path.trim().is_empty()
        && path.len() <= REMOTE_PATH_LIMIT_BYTES
        && !path.chars().any(char::is_control)
}

fn envelope_result(envelope: ToolEnvelope) -> CallToolResult {
    match envelope.outcome {
        ToolOutcome::Failed => CallToolResult::structured_error(json!({
            "outcome": envelope.outcome,
            "error": envelope.error,
        })),
        ToolOutcome::Accepted => CallToolResult::structured(
            envelope
                .data
                .unwrap_or_else(|| json!({ "outcome": "accepted" })),
        ),
        ToolOutcome::Completed => CallToolResult::structured(json!({
            "outcome": envelope.outcome,
            "data": envelope.data,
        })),
    }
}

fn tool_error(code: &'static str, message: impl Into<String>) -> CallToolResult {
    CallToolResult::structured_error(json!({
        "error_code": code,
        "message": message.into(),
    }))
}

fn unauthorized_error() -> McpError {
    McpError::invalid_request("Unauthorized MCP client", None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DomainMessage;

    #[test]
    fn public_tool_input_schemas_have_object_roots() {
        // MCP tool arguments must expose an object at the schema root, including tagged enums.
        for definition in tool_definitions() {
            assert_eq!(
                definition
                    .tool
                    .input_schema
                    .get("type")
                    .and_then(Value::as_str),
                Some("object"),
                "tool {} must expose an object input schema",
                definition.tool.name
            );
        }

        for schema in [
            schema_object::<RecordingsControlArgs>(),
            schema_object::<StartTransferArgs>(),
        ] {
            assert!(
                schema
                    .get("oneOf")
                    .and_then(Value::as_array)
                    .is_some_and(|variants| !variants.is_empty()),
                "tagged enum alternatives must be preserved"
            );
        }
    }

    #[test]
    fn quick_command_run_parses_and_redacts_parameter_values() {
        let secret_value = "sensitive-token";
        let quickcommand_ref = crate::QuickCommandRef::new();
        let node_ref = NodeRef::new();
        let arguments = json!({
            "quickcommand_ref": quickcommand_ref,
            "node_ref": node_ref,
            "expected_revision": 7,
            "arguments": { "token": secret_value }
        })
        .as_object()
        .cloned()
        .unwrap();

        let parsed = parse_quick_commands_run(arguments).unwrap();

        assert_eq!(parsed.arguments["token"].as_str(), secret_value);
        assert!(!format!("{parsed:?}").contains(secret_value));
    }

    #[test]
    fn quick_command_run_rejects_oversized_parameter_values_before_approval() {
        let arguments = json!({
            "quickcommand_ref": crate::QuickCommandRef::new(),
            "node_ref": NodeRef::new(),
            "expected_revision": 7,
            "arguments": {
                "value": "x".repeat(QUICK_COMMAND_ARGUMENT_VALUE_LIMIT_BYTES + 1)
            }
        })
        .as_object()
        .cloned()
        .unwrap();

        assert!(parse_quick_commands_run(arguments).is_err());
    }

    #[test]
    fn quick_command_save_roundtrips_advanced_fields_without_debugging_defaults() {
        let secret_default = "sensitive-default";
        let arguments = json!({
            "name": "Deploy",
            "command": "deploy {{param.service}}",
            "category": "custom",
            "expected_revision": 7,
            "host_patterns": ["*.prod", "bastion.*"],
            "protocols": ["ssh", "mosh"],
            "confirmation": "always",
            "parameters": [{
                "name": "service",
                "label": "Service",
                "kind": "choice",
                "default_value": secret_default,
                "choices": [secret_default, "worker"],
                "required": true
            }]
        })
        .as_object()
        .cloned()
        .unwrap();

        let parsed = parse_quick_commands_save(arguments).unwrap();

        assert_eq!(parsed.host_patterns.as_ref().map(Vec::len), Some(2));
        assert_eq!(parsed.protocols.as_ref().map(Vec::len), Some(2));
        assert_eq!(parsed.parameters.as_ref().map(Vec::len), Some(1));
        assert!(!format!("{parsed:?}").contains(secret_default));
    }

    #[test]
    fn quick_command_save_rejects_persisted_defaults_for_secret_parameters() {
        let arguments = json!({
            "name": "Login",
            "command": "login {{param.password}}",
            "category": "custom",
            "expected_revision": 7,
            "parameters": [{
                "name": "password",
                "label": "Password",
                "kind": "secret",
                "default_value": "must-not-persist",
                "required": true
            }]
        })
        .as_object()
        .cloned()
        .unwrap();

        assert!(parse_quick_commands_save(arguments).is_err());
    }

    #[tokio::test]
    async fn standard_quick_command_run_reaches_domain_preparation_before_approval() {
        let clients = Arc::new(ClientRegistry::default());
        let mut tool_groups = ToolGroup::selectable()
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        tool_groups.insert(ToolGroup::Basic);
        let registered = clients
            .register(
                "test client".to_string(),
                ClientApprovalMode::Standard,
                tool_groups,
            )
            .unwrap();
        let client = clients.get(&registered.projection.client_ref).unwrap();
        let (broker, mut receiver) = DomainBroker::channel(1);
        let state = Arc::new(PublicMcpState {
            clients,
            approvals: Arc::default(),
            audit: Arc::new(AuditStore::new(16)),
            artifacts: Arc::default(),
            broker,
        });
        let service = PublicMcpService::new(state.clone());
        let call = PublicToolCall::QuickCommandsRun(QuickCommandsRunArgs {
            quickcommand_ref: crate::QuickCommandRef::new(),
            node_ref: NodeRef::new(),
            expected_revision: 1,
            arguments: BTreeMap::new(),
        });

        let execution = tokio::spawn(async move { service.execute_call(&client, call).await });
        let Some(DomainMessage::Request(request)) = receiver.recv().await else {
            panic!("Quick Command run must reach workspace preparation");
        };
        assert!(matches!(&request.call, PublicToolCall::QuickCommandsRun(_)));
        assert!(state.approvals.list().is_empty());
        request.finish(ToolEnvelope::accepted(json!({ "outcome": "approval_required" })).unwrap());
        let _ = execution.await.unwrap();
    }
}
