use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditCategory {
    Connection,
    Reconnect,
    Node,
    Command,
    File,
    Forward,
    Host,
    Configuration,
    Security,
    Automation,
    System,
    Audit,
}

impl AuditCategory {
    pub const ALL: [Self; 12] = [
        Self::Connection,
        Self::Reconnect,
        Self::Node,
        Self::Command,
        Self::File,
        Self::Forward,
        Self::Host,
        Self::Configuration,
        Self::Security,
        Self::Automation,
        Self::System,
        Self::Audit,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Connection => "connection",
            Self::Reconnect => "reconnect",
            Self::Node => "node",
            Self::Command => "command",
            Self::File => "file",
            Self::Forward => "forward",
            Self::Host => "host",
            Self::Configuration => "configuration",
            Self::Security => "security",
            Self::Automation => "automation",
            Self::System => "system",
            Self::Audit => "audit",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSeverity {
    Info,
    Warning,
    Error,
}

impl AuditSeverity {
    pub fn key(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warn",
            Self::Error => "error",
        }
    }
}

// Human-readable values never enter database indexes or diagnostics.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct AuditDetails {
    pub title: Zeroizing<String>,
    pub detail: Option<Zeroizing<String>>,
    pub source: Zeroizing<String>,
    pub actor: Zeroizing<String>,
    pub device: Zeroizing<String>,
    pub target: Option<Zeroizing<String>>,
    pub node_id: Option<Zeroizing<String>>,
    pub connection_id: Option<Zeroizing<String>>,
    #[serde(default)]
    pub remote_account: Option<Zeroizing<String>>,
    pub operation: Option<OperationDetails>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: String,
    pub occurred_at_ms: i64,
    pub category: AuditCategory,
    pub severity: AuditSeverity,
    pub details: AuditDetails,
}

impl AuditRecord {
    pub(crate) fn queued_bytes(&self) -> usize {
        let details = &self.details;
        let mut bytes = std::mem::size_of::<Self>() + self.id.capacity();
        bytes += [
            &details.title,
            &details.source,
            &details.actor,
            &details.device,
        ]
        .into_iter()
        .map(|value| value.capacity())
        .sum::<usize>();
        bytes += [
            &details.detail,
            &details.target,
            &details.node_id,
            &details.connection_id,
            &details.remote_account,
        ]
        .into_iter()
        .filter_map(Option::as_ref)
        .map(|value| value.capacity())
        .sum::<usize>();
        if let Some(operation) = &details.operation {
            bytes += [&operation.id, &operation.instance_id, &operation.action]
                .into_iter()
                .map(String::capacity)
                .sum::<usize>();
            bytes += [
                &operation.parent_id,
                &operation.session_id,
                &operation.consumer_id,
                &operation.transport_id,
                &operation.protocol,
                &operation.authorization_ref,
                &operation.recovered_by,
            ]
            .into_iter()
            .filter_map(Option::as_ref)
            .map(String::capacity)
            .sum::<usize>();
            bytes += operation
                .agent_id
                .as_ref()
                .map_or(0, |value| value.capacity());
        }
        bytes
    }

    pub fn new(category: AuditCategory, severity: AuditSeverity, details: AuditDetails) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            occurred_at_ms: now_ms(),
            category,
            severity,
            details,
        }
    }
}

#[derive(Clone, Default)]
pub struct AuditQuery {
    pub latest_only: bool,
    /// Exclusive snapshot fence, retained when pagination advances.
    pub sequence_ceiling: Option<i64>,
    pub category: Option<AuditCategory>,
    pub severity: Option<AuditSeverity>,
    pub before_sequence: Option<i64>,
    pub after_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub search: Option<Zeroizing<String>>,
    pub operation_id: Option<String>,
    pub session_id: Option<String>,
    pub source: Option<AuditSource>,
    pub outcome: Option<AuditOutcome>,
    pub protocol: Option<String>,
    pub connection_id: Option<Zeroizing<String>>,
    pub remote_account: Option<Zeroizing<String>>,
    pub local_account: Option<Zeroizing<String>>,
    pub parent_id: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Default)]
pub struct AuditSessionQuery {
    pub before: Option<(i64, String)>,
    pub after_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub limit: usize,
}

pub struct AuditSessionSummary {
    pub session_id: String,
    pub started_at_ms: i64,
    pub last_event_at_ms: i64,
    pub protocol: Option<String>,
    pub target: Option<Zeroizing<String>>,
    pub local_account: Zeroizing<String>,
    pub remote_account: Option<Zeroizing<String>>,
    pub transport_count: u64,
    pub command_count: u64,
    pub file_count: u64,
    pub operation_count: u64,
    pub capture: AuditCapture,
}

pub struct AuditSessionPage {
    pub sessions: Vec<AuditSessionSummary>,
    pub next_cursor: Option<(i64, String)>,
}

pub struct AuditPage {
    pub records: Vec<AuditRecord>,
    pub next_cursor: Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AuditError {
    #[error("audit policy is outside the supported range")]
    InvalidPolicy,
    #[error("audit storage is unavailable")]
    Storage,
    #[error("audit key is unavailable")]
    KeyUnavailable,
    #[error("audit data could not be authenticated")]
    Integrity,
    #[error("audit storage format is unsupported")]
    UnsupportedVersion,
    #[error("audit queue is full")]
    QueueFull,
    #[error("audit service is closed")]
    Closed,
    #[error("audit record exceeds the size limit")]
    TooLarge,
    #[error("audit export exceeded the read snapshot limit")]
    ExportLimit,
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSource {
    Application,
    User,
    CommandBar,
    QuickCommand,
    Broadcast,
    Ai,
    Mcp,
    Plugin,
    Cli,
    System,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Unchanged,
    Started,
    Sent,
    Succeeded,
    Failed,
    Partial,
    CancelRequested,
    Cancelled,
    Interrupted,
    Unknown,
    Denied,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEvidence {
    Request,
    Dispatch,
    Protocol,
    ExitCode,
    ShellIntegration,
    InputInference,
    Lifecycle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAuthorization {
    Unknown,
    NotRequired,
    Pending,
    Approved,
    Denied,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OperationDetails {
    pub id: String,
    pub parent_id: Option<String>,
    pub instance_id: String,
    pub session_id: Option<String>,
    pub consumer_id: Option<String>,
    pub transport_id: Option<String>,
    pub protocol: Option<String>,
    pub agent_id: Option<Zeroizing<String>>,
    pub source: AuditSource,
    pub action: String,
    pub outcome: AuditOutcome,
    pub evidence: AuditEvidence,
    pub authorization: AuditAuthorization,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_ref: Option<String>,
    pub duration_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub bytes: Option<u64>,
    pub phase: Option<AuditPhase>,
    pub capture: Option<AuditCapture>,
    pub recovered_by: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditPolicy {
    pub enabled: bool,
    pub retention_days: u32,
    pub max_bytes: u64,
    pub record_output: bool,
    pub output_retention_days: u32,
    pub output_max_bytes: u64,
}

impl AuditPolicy {
    pub const MIN_RETENTION_DAYS: u32 = 1;
    pub const MAX_RETENTION_DAYS: u32 = 3650;
    pub const MIN_BYTES: u64 = 1024 * 1024;
    pub const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
}

impl Default for AuditPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: 90,
            max_bytes: 512 * 1024 * 1024,
            record_output: false,
            output_retention_days: 7,
            output_max_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

impl AuditSource {
    pub const ALL: [Self; 10] = [
        Self::Application,
        Self::User,
        Self::CommandBar,
        Self::QuickCommand,
        Self::Broadcast,
        Self::Ai,
        Self::Mcp,
        Self::Plugin,
        Self::Cli,
        Self::System,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::User => "user",
            Self::CommandBar => "command_bar",
            Self::QuickCommand => "quick_command",
            Self::Broadcast => "broadcast",
            Self::Ai => "ai",
            Self::Mcp => "mcp",
            Self::Plugin => "plugin",
            Self::Cli => "cli",
            Self::System => "system",
        }
    }
}

impl AuditOutcome {
    pub const ALL: [Self; 11] = [
        Self::Unchanged,
        Self::Started,
        Self::Sent,
        Self::Succeeded,
        Self::Failed,
        Self::Partial,
        Self::CancelRequested,
        Self::Cancelled,
        Self::Interrupted,
        Self::Unknown,
        Self::Denied,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Started => "started",
            Self::Sent => "sent",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Partial => "partial",
            Self::CancelRequested => "cancel_requested",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
            Self::Unknown => "unknown",
            Self::Denied => "denied",
        }
    }
}

impl AuditEvidence {
    pub const ALL: [Self; 7] = [
        Self::Request,
        Self::Dispatch,
        Self::Protocol,
        Self::ExitCode,
        Self::ShellIntegration,
        Self::InputInference,
        Self::Lifecycle,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Dispatch => "dispatch",
            Self::Protocol => "protocol",
            Self::ExitCode => "exit_code",
            Self::ShellIntegration => "shell_integration",
            Self::InputInference => "input_inference",
            Self::Lifecycle => "lifecycle",
        }
    }
}

impl AuditAuthorization {
    pub const ALL: [Self; 7] = [
        Self::Unknown,
        Self::NotRequired,
        Self::Pending,
        Self::Approved,
        Self::Denied,
        Self::Cancelled,
        Self::TimedOut,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::NotRequired => "not_required",
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditPhase {
    Observation,
    Start,
    Result,
    Authorization,
    Progress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditCapture {
    Complete,
    Partial,
    Interrupted,
    Disabled,
}

impl AuditCapture {
    pub fn key(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Interrupted => "interrupted",
            Self::Disabled => "disabled",
        }
    }
}

/// FIFO capture state travels with each queued event, including disabled starts.
pub(crate) struct PendingRecord {
    pub record: AuditRecord,
    pub suppressed: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub start: bool,
}
