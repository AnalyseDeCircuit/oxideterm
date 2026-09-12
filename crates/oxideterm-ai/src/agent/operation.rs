use crate::{AiExecutedToolResult, AiToolCall};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recovery {
    Transient,
    NeedsUser,
    InvalidCall,
    OutcomeUnknown,
    Cancelled,
    Permanent,
}

pub fn result_recovery(result: &AiExecutedToolResult) -> Option<Recovery> {
    if result.success {
        return None;
    }
    let code = result
        .envelope
        .pointer("/error/code")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if code == "mcp_resource_read_failed" {
        if let Some(category) = result
            .envelope
            .pointer("/data/recovery")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
        {
            return Some(category);
        }
    }
    Some(match code {
        "network_timeout" | "connection_reset" | "service_unavailable" | "rate_limited" => {
            Recovery::Transient
        }
        "permission_denied"
        | "authentication_required"
        | "credential_interaction_required"
        | "user_rejected"
        | "resource_execution_unresolved" => Recovery::NeedsUser,
        "invalid_tool_arguments"
        | "missing_command"
        | "missing_path"
        | "missing_mcp_resource_args"
        | "unknown_tool"
        | "tool_unavailable"
        | "tool_not_available" => Recovery::InvalidCall,
        "operation_cancelled"
        | "agent_direction_changed"
        | "dependency_failed"
        | "agent_wait_paused" => Recovery::Cancelled,
        "agent_scope_denied" | "runtime_capability_unavailable" => Recovery::Permanent,
        _ => Recovery::OutcomeUnknown,
    })
}

pub fn annotate_recovery(result: &mut AiExecutedToolResult) {
    if let Some(recovery) = result_recovery(result)
        && let Some(envelope) = result.envelope.as_object_mut()
    {
        envelope.insert("recovery".into(), serde_json::to_value(recovery).unwrap());
    }
}

/// Only application-owned read implementations can advertise replay safety.
pub struct ToolExecutionDescription {
    pub resources: Vec<String>,
    pub read_only: bool,
    pub retry_safe: bool,
}

impl ToolExecutionDescription {
    pub fn for_call(call: &AiToolCall) -> Self {
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments).unwrap_or_default();
        let handle = args
            .get("handle_id")
            .and_then(serde_json::Value::as_str)
            .filter(|value| crate::RuntimeHandleId::parse((*value).to_owned()).is_ok());
        let server = args.get("server_id").and_then(serde_json::Value::as_str);
        let resource = match call.name.as_str() {
            "observe_terminal" | "read_resource" => handle,
            "read_mcp_resource" | "list_mcp_resources" => server,
            _ => None,
        };
        let read_only = resource.is_some()
            && crate::orchestrator_risk_for_tool(&call.name, Some(&args))
                == crate::AiActionRisk::Read;
        Self {
            resources: resource.into_iter().map(str::to_owned).collect(),
            read_only,
            retry_safe: read_only,
        }
    }

    pub fn retry_delay(
        &self,
        result: &AiExecutedToolResult,
        attempt: usize,
    ) -> Option<std::time::Duration> {
        if !self.retry_safe || attempt >= 2 || result_recovery(result) != Some(Recovery::Transient)
        {
            return None;
        }
        let seconds = result
            .envelope
            .pointer("/meta/retryAfterSeconds")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1 << attempt);
        (seconds <= 30).then(|| std::time::Duration::from_secs(seconds))
    }
}

pub struct ToolDependencies {
    dependencies: HashMap<String, Vec<String>>,
    outcomes: HashMap<String, bool>,
}

impl ToolDependencies {
    pub fn new(calls: &[AiToolCall]) -> Self {
        let descriptions: Vec<_> = calls
            .iter()
            .map(ToolExecutionDescription::for_call)
            .collect();
        let dependencies = calls
            .iter()
            .enumerate()
            .map(|(index, call)| {
                let edges = (0..index)
                    .filter(|&previous| {
                        // Mutations and unknown effects are ordering barriers, even across different handles.
                        !descriptions[index].read_only || !descriptions[previous].read_only
                    })
                    .map(|previous| calls[previous].id.clone())
                    .collect();
                (call.id.clone(), edges)
            })
            .collect();
        Self {
            dependencies,
            outcomes: HashMap::new(),
        }
    }

    pub fn failed_dependency(&self, call: &AiToolCall) -> bool {
        self.dependencies
            .get(&call.id)
            .is_some_and(|edges| edges.iter().any(|id| self.outcomes.get(id) == Some(&false)))
    }

    pub fn record(&mut self, call: &AiToolCall, success: bool) {
        self.outcomes.insert(call.id.clone(), success);
    }
}
