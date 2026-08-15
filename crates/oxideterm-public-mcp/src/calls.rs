use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{
    auth::ToolGroup,
    handles::{ArtifactRef, AuditRef, CommandRef, ConnectionRef, NodeRef},
};

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct BrowseConnectionsArgs {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub connection_types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DescribeConnectionArgs {
    pub connection_ref: ConnectionRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConnectNodeArgs {
    pub connection_ref: ConnectionRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct InspectNodeArgs {
    pub node_ref: NodeRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReleaseNodeArgs {
    pub node_ref: NodeRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DisconnectNodeArgs {
    pub node_ref: NodeRef,
}

pub struct StartCommandArgs {
    pub node_ref: NodeRef,
    pub command: Zeroizing<String>,
    pub working_directory: Option<Zeroizing<String>>,
}

impl fmt::Debug for StartCommandArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartCommandArgs")
            .field("node_ref", &self.node_ref)
            .field("command", &"[REDACTED]")
            .field(
                "working_directory",
                &self.working_directory.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommandStateArgs {
    pub command_ref: CommandRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommandOutputArgs {
    pub command_ref: CommandRef,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_output_limit")]
    pub limit: u32,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CancelCommandArgs {
    pub command_ref: CommandRef,
}

pub struct StageArtifactArgs {
    pub bytes: Zeroizing<Vec<u8>>,
    pub media_type: String,
    pub name: Option<String>,
}

impl fmt::Debug for StageArtifactArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StageArtifactArgs")
            .field(
                "bytes",
                &format_args!("[REDACTED; {} bytes]", self.bytes.len()),
            )
            .field("media_type", &self.media_type)
            .field("name", &self.name)
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadArtifactArgs {
    pub artifact_ref: ArtifactRef,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_artifact_read_length")]
    pub length: u32,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct AuditSearchArgs {
    #[serde(default)]
    pub after_ms: Option<u128>,
    #[serde(default)]
    pub before_ms: Option<u128>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub target_ref: Option<String>,
    #[serde(default)]
    pub cursor: Option<AuditRef>,
    #[serde(default = "default_audit_limit")]
    pub limit: u32,
}

fn default_output_limit() -> u32 {
    64 * 1024
}

fn default_artifact_read_length() -> u32 {
    256 * 1024
}

fn default_audit_limit() -> u32 {
    50
}

pub enum PublicToolCall {
    BrowseConnections(BrowseConnectionsArgs),
    DescribeConnection(DescribeConnectionArgs),
    ConnectNode(ConnectNodeArgs),
    InspectNode(InspectNodeArgs),
    ReleaseNode(ReleaseNodeArgs),
    DisconnectNode(DisconnectNodeArgs),
    StartCommand(StartCommandArgs),
    CommandState(CommandStateArgs),
    CommandOutput(CommandOutputArgs),
    CancelCommand(CancelCommandArgs),
    StageArtifact(StageArtifactArgs),
    ReadArtifact(ReadArtifactArgs),
    AuditSearch(AuditSearchArgs),
}

impl PublicToolCall {
    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::BrowseConnections(_) => "connections_browse",
            Self::DescribeConnection(_) => "connections_describe",
            Self::ConnectNode(_) => "nodes_connect",
            Self::InspectNode(_) => "nodes_inspect",
            Self::ReleaseNode(_) => "nodes_release",
            Self::DisconnectNode(_) => "nodes_disconnect",
            Self::StartCommand(_) => "commands_start",
            Self::CommandState(_) => "commands_state",
            Self::CommandOutput(_) => "commands_output",
            Self::CancelCommand(_) => "commands_cancel",
            Self::StageArtifact(_) => "artifacts_stage",
            Self::ReadArtifact(_) => "artifacts_read",
            Self::AuditSearch(_) => "mcp_audit_search",
        }
    }

    pub fn required_group(&self) -> ToolGroup {
        match self {
            Self::BrowseConnections(_) => ToolGroup::ConnectionDirectory,
            Self::DescribeConnection(_) => ToolGroup::ConnectionRead,
            Self::ConnectNode(_)
            | Self::InspectNode(_)
            | Self::ReleaseNode(_)
            | Self::DisconnectNode(_) => ToolGroup::NodeSession,
            Self::StartCommand(_) | Self::CancelCommand(_) => ToolGroup::CommandExecute,
            Self::CommandState(_) | Self::CommandOutput(_) => ToolGroup::CommandObserve,
            Self::StageArtifact(_) | Self::ReadArtifact(_) => ToolGroup::ArtifactTransfer,
            Self::AuditSearch(_) => ToolGroup::AuditRead,
        }
    }

    pub fn requires_approval(&self) -> bool {
        matches!(
            self,
            Self::ConnectNode(_) | Self::DisconnectNode(_) | Self::StartCommand(_)
        )
    }

    pub fn target_summary(&self) -> String {
        match self {
            Self::BrowseConnections(_) => "connection directory".to_owned(),
            Self::DescribeConnection(args) => args.connection_ref.to_string(),
            Self::ConnectNode(args) => args.connection_ref.to_string(),
            Self::InspectNode(args) => args.node_ref.to_string(),
            Self::ReleaseNode(args) => args.node_ref.to_string(),
            Self::DisconnectNode(args) => args.node_ref.to_string(),
            Self::StartCommand(args) => args.node_ref.to_string(),
            Self::CommandState(args) => args.command_ref.to_string(),
            Self::CommandOutput(args) => args.command_ref.to_string(),
            Self::CancelCommand(args) => args.command_ref.to_string(),
            Self::StageArtifact(_) => "artifact staging".to_owned(),
            Self::ReadArtifact(args) => args.artifact_ref.to_string(),
            Self::AuditSearch(_) => "audit log".to_owned(),
        }
    }
}

impl fmt::Debug for PublicToolCall {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublicToolCall")
            .field("tool", &self.tool_name())
            .field("target", &self.target_summary())
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    Completed,
    Accepted,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolEnvelope {
    pub outcome: ToolOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolEnvelope {
    pub fn completed(data: impl Serialize) -> Result<Self, serde_json::Error> {
        Ok(Self {
            outcome: ToolOutcome::Completed,
            data: Some(serde_json::to_value(data)?),
            error: None,
        })
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            outcome: ToolOutcome::Failed,
            data: None,
            error: Some(error.into()),
        }
    }
}
