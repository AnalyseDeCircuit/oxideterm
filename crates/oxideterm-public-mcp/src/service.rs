use std::sync::Arc;

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
    broker::DomainBroker,
    calls::{
        AuditSearchArgs, BrowseConnectionsArgs, CancelCommandArgs, CommandOutputArgs,
        CommandStateArgs, ConnectNodeArgs, DescribeConnectionArgs, DisconnectNodeArgs,
        HostToolsCaptureArgs, HostToolsCatalogArgs, HostToolsOperateArgs, InspectNodeArgs,
        PublicToolCall, ReadArtifactArgs, ReleaseNodeArgs, StageArtifactArgs, StartCommandArgs,
        ToolEnvelope, ToolOutcome,
    },
    handles::{ApprovalRef, ClientRef, NodeRef},
};

const TOOL_LIST_CACHE_TTL_MS: u64 = 1_000;
const COMMAND_TEXT_LIMIT_BYTES: usize = 64 * 1024;
const WORKING_DIRECTORY_LIMIT_BYTES: usize = 16 * 1024;
const ARTIFACT_STAGE_LIMIT_BYTES: usize = 512 * 1024;

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

#[derive(Debug, Clone, Serialize)]
struct CatalogEntry {
    name: String,
    tool_group: ToolGroup,
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
            .filter(|definition| client.tool_groups.contains(&definition.group))
            .map(|definition| definition.tool)
            .collect()
    }

    async fn execute_call(
        &self,
        client: &ClientProjection,
        call: PublicToolCall,
    ) -> CallToolResult {
        if !client.tool_groups.contains(&call.required_group()) {
            return tool_error(
                "tool_group_disabled",
                "This tool group is disabled for the client",
            );
        }

        if call.requires_approval() && client.approval_mode == ClientApprovalMode::Standard {
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

        let authorization = if call.requires_approval() {
            AuditAuthorization::Unattended
        } else {
            AuditAuthorization::NotRequired
        };
        self.execute_approved_call(client.client_ref.clone(), call, authorization)
            .await
    }

    async fn execute_approved_call(
        &self,
        client_ref: ClientRef,
        call: PublicToolCall,
        authorization: AuditAuthorization,
    ) -> CallToolResult {
        let tool_name = call.tool_name().to_owned();
        let target = call.target_summary();
        let response = self.state.broker.execute(client_ref.clone(), call).await;
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
                tool_error("workspace_unavailable", error.to_string())
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
        if !client.tool_groups.contains(&call.required_group()) {
            return tool_error(
                "tool_group_disabled",
                "The required tool group was disabled before commit",
            );
        }
        self.execute_approved_call(
            client.client_ref.clone(),
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
                    "security": "Bearer authentication, per-client tool groups, app-lock enforcement, secret hard boundaries, and audit remain active in every mode",
                }))
            }
            "mcp_catalog" => {
                let catalog = tool_definitions()
                    .into_iter()
                    .filter(|definition| client.tool_groups.contains(&definition.group))
                    .map(|definition| CatalogEntry {
                        name: definition.tool.name.into_owned(),
                        tool_group: definition.group,
                        requires_approval: definition.requires_approval
                            && client.approval_mode == ClientApprovalMode::Standard,
                    })
                    .collect::<Vec<_>>();
                CallToolResult::structured(json!({ "tools": catalog }))
            }
            "mcp_access_state" => CallToolResult::structured(json!({ "client": client })),
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
                    self.execute_call(&client, PublicToolCall::HostToolsOperate(args))
                        .await
                }
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
    requires_approval: bool,
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
        requires_approval,
    }
}

fn schema_object<T: JsonSchema>() -> JsonObject {
    serde_json::to_value(schema_for!(T))
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn parse_arguments<T: DeserializeOwned>(arguments: JsonObject) -> Result<T, Box<CallToolResult>> {
    serde_json::from_value(Value::Object(arguments)).map_err(|error| {
        Box::new(tool_error(
            "invalid_arguments",
            format!("The tool arguments are invalid: {error}"),
        ))
    })
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

fn envelope_result(envelope: ToolEnvelope) -> CallToolResult {
    match envelope.outcome {
        ToolOutcome::Failed => CallToolResult::structured_error(json!({
            "outcome": envelope.outcome,
            "error": envelope.error,
        })),
        _ => CallToolResult::structured(json!({
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
