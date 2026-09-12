use std::{
    collections::{BTreeSet, HashSet},
    fmt,
};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

macro_rules! agent_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::new_v4())
            }
            pub fn parse(value: &str) -> Result<Self, uuid::Error> {
                uuid::Uuid::parse_str(value).map(Self)
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
agent_id!(AgentGroupId);
agent_id!(AgentId);
agent_id!(AgentRunId);

/// Sanitization happens before text crosses a mailbox or durable-history boundary.
#[derive(Clone, Default)]
pub struct AgentText(Zeroizing<String>);

impl AgentText {
    pub fn new(text: &str) -> Self {
        Self(Zeroizing::new(crate::sanitize_for_persistence(text)))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
impl fmt::Debug for AgentText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AgentText([redacted])")
    }
}
impl Serialize for AgentText {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for AgentText {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = Zeroizing::new(String::deserialize(deserializer)?);
        Ok(Self::new(&text))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentModel {
    pub provider_id: String,
    pub model: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentConversationOptions {
    #[serde(default)]
    pub enabled: bool,
    pub default_model: Option<AgentModel>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRecord {
    pub parent_usage: AgentUsage,
    pub created_at_ms: i64,
    pub snapshot: AgentSnapshot,
    pub parent_message_id: String,
    pub target_labels: Vec<AgentText>,
    pub messages: Vec<crate::AiChatMessage>,
    pub communication: Vec<AgentMessage>,
    pub revision: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AgentScope {
    /// Host-owned identities survive pane selection, but cannot enter model or durable data.
    pub targets: HashSet<crate::RuntimeOwnerKey>,
    pub tools: BTreeSet<String>,
}
impl AgentScope {
    pub fn contains(&self, child: &Self) -> bool {
        child.targets.is_subset(&self.targets) && child.tools.is_subset(&self.tools)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Queued,
    Running,
    AwaitingApproval,
    AwaitingParent,
    AwaitingResource,
    AwaitingCondition,
    AwaitingUser,
    AwaitingConnection,
    Replanning,
    Stopping,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}
impl AgentState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct AgentRunRef {
    pub group_id: AgentGroupId,
    pub agent_id: AgentId,
    pub run_id: AgentRunId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentResult {
    pub summary: AgentText,
    pub evidence: Vec<AgentText>,
    pub actions: Vec<AgentText>,
    pub unfinished: Vec<AgentText>,
    pub error_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentMessageKind {
    UserSupplement,
    Assignment,
    FollowUp,
    Question,
    Answer,
    Result,
    Failure,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentUpdate {
    pub sequence: u64,
    pub source: AgentRunRef,
    pub state: AgentState,
    pub message: Option<AgentMessage>,
    pub result: Option<AgentResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentMessage {
    pub sequence: u64,
    pub from: AgentRunRef,
    pub to: AgentRunRef,
    pub kind: AgentMessageKind,
    pub text: AgentText,
    pub consumed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentSnapshot {
    #[serde(default)]
    pub resources: Vec<super::OwnedResource>,
    pub usage: AgentUsage,
    pub run: AgentRunRef,
    pub parent_id: Option<AgentId>,
    pub conversation_id: String,
    pub title: AgentText,
    pub task: AgentText,
    pub model: AgentModel,
    #[serde(skip)]
    pub scope: AgentScope,
    #[serde(deserialize_with = "restore_agent_state")]
    pub state: AgentState,
    pub progress: AgentText,
    pub result: Option<AgentResult>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentUsage {
    pub requests: usize,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl AgentUsage {
    pub fn total(usages: impl IntoIterator<Item = Self>) -> Self {
        let mut total = Self {
            requests: 0,
            input_tokens: Some(0),
            output_tokens: Some(0),
        };
        for usage in usages {
            if usage.requests == 0 {
                continue;
            }
            total.requests += usage.requests;
            total.input_tokens = total
                .input_tokens
                .zip(usage.input_tokens)
                .map(|(a, b)| a.saturating_add(b));
            total.output_tokens = total
                .output_tokens
                .zip(usage.output_tokens)
                .map(|(a, b)| a.saturating_add(b));
        }
        if total.requests == 0 {
            Self::default()
        } else {
            total
        }
    }
}

fn restore_agent_state<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<AgentState, D::Error> {
    let state = AgentState::deserialize(deserializer)?;
    // Durable history is descriptive, never authority to replay an unfinished operation.
    Ok(if state.is_terminal() {
        state
    } else {
        AgentState::Interrupted
    })
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum AgentError {
    #[error("agent run is no longer current")]
    StaleRun,
    #[error("task direction changed before dispatch")]
    DirectionChanged,
    #[error("operation is restricted to the parent agent")]
    ParentOnly,
    #[error("target or tool is outside the delegated scope")]
    ScopeDenied,
    #[error("agent mailbox is full")]
    MailboxFull,
    #[error("agent has unprocessed messages")]
    PendingMessages,
    #[error("agent group has finished")]
    GroupFinished,
    #[error("agent is not ready for this operation")]
    InvalidState,
    #[error("agent group tool-round budget is exhausted")]
    BudgetExhausted,
    #[error("agent operation was cancelled")]
    Cancelled,
    #[error("resource execution status is unresolved")]
    ResourceUnresolved,
}
