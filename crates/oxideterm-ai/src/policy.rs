use std::{collections::HashMap, sync::OnceLock};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiPolicyDecisionKind {
    Allow,
    RequireApproval,
    Deny,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiActionRisk {
    Read,
    Write,
    Execute,
    Interactive,
    Destructive,
    Credential,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiPolicySafetyMode {
    Default,
    Bypass,
}

impl Default for AiPolicySafetyMode {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPolicyDecision {
    pub decision: AiPolicyDecisionKind,
    pub risk: AiActionRisk,
    pub reason_code: String,
    pub reason_text_key: String,
    pub matched_policy_key: String,
    pub approval_mode: AiPolicySafetyMode,
    #[serde(default)]
    pub profile_id: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AiToolUsePolicy {
    pub enabled: bool,
    pub auto_approve_tools: HashMap<String, bool>,
    pub disabled_tools: Vec<String>,
    pub max_rounds: Option<i64>,
    pub max_calls_per_round: Option<i64>,
}

pub const ORCHESTRATOR_TOOL_NAMES: &[&str] = &[
    "list_targets",
    "select_target",
    "connect_target",
    "run_command",
    "observe_terminal",
    "send_terminal_input",
    "read_resource",
    "write_resource",
    "transfer_resource",
    "open_app_surface",
    "get_state",
    "remember_preference",
    "recall_preferences",
];

pub fn resolve_ai_policy_decision(
    tool_name: &str,
    args: Option<&Value>,
    tool_use: &AiToolUsePolicy,
    safety_mode: AiPolicySafetyMode,
    profile_id: Option<String>,
) -> AiPolicyDecision {
    let risk = if crate::is_mcp_tool_name(tool_name)
        || matches!(tool_name, "list_mcp_resources" | "read_mcp_resource")
    {
        AiActionRisk::Read
    } else if is_orchestrator_tool_name(tool_name) {
        orchestrator_risk_for_tool(tool_name, args)
    } else {
        AiActionRisk::Write
    };
    let matched_policy_key = if crate::is_mcp_tool_name(tool_name)
        || matches!(tool_name, "list_mcp_resources" | "read_mcp_resource")
    {
        tool_name.to_string()
    } else if is_orchestrator_tool_name(tool_name) {
        orchestrator_approval_key_for_tool(tool_name, args)
    } else {
        tool_name.to_string()
    };

    let disabled = tool_use
        .disabled_tools
        .iter()
        .any(|tool| tool == tool_name || tool == &matched_policy_key);
    if disabled {
        return policy_decision(
            AiPolicyDecisionKind::Deny,
            risk,
            "tool_disabled",
            "ai.tool_use.policy_reason_tool_disabled",
            matched_policy_key,
            safety_mode,
            profile_id,
        );
    }

    if risk == AiActionRisk::Read {
        return policy_decision(
            AiPolicyDecisionKind::Allow,
            risk,
            "read_only_auto_allowed",
            "ai.tool_use.policy_reason_read_only",
            matched_policy_key,
            safety_mode,
            profile_id,
        );
    }

    if risk == AiActionRisk::Credential {
        return policy_decision(
            AiPolicyDecisionKind::RequireApproval,
            risk,
            "credential_requires_user",
            "ai.tool_use.policy_reason_credential",
            matched_policy_key,
            safety_mode,
            profile_id,
        );
    }

    if risk == AiActionRisk::Destructive {
        if safety_mode == AiPolicySafetyMode::Bypass {
            return policy_decision(
                AiPolicyDecisionKind::Allow,
                risk,
                "bypass_destructive_allowed",
                "ai.tool_use.policy_reason_bypass",
                matched_policy_key,
                safety_mode,
                profile_id,
            );
        }
        return policy_decision(
            AiPolicyDecisionKind::RequireApproval,
            risk,
            "destructive_requires_approval",
            "ai.tool_use.policy_reason_destructive",
            matched_policy_key,
            safety_mode,
            profile_id,
        );
    }

    if tool_use
        .auto_approve_tools
        .get(&matched_policy_key)
        .copied()
        .unwrap_or(false)
    {
        return policy_decision(
            AiPolicyDecisionKind::Allow,
            risk,
            "auto_approved",
            "ai.tool_use.policy_reason_auto_approved",
            matched_policy_key,
            safety_mode,
            profile_id,
        );
    }

    policy_decision(
        AiPolicyDecisionKind::RequireApproval,
        risk,
        "policy_requires_approval",
        "ai.tool_use.policy_reason_requires_approval",
        matched_policy_key,
        safety_mode,
        profile_id,
    )
}

pub fn is_orchestrator_tool_name(name: &str) -> bool {
    ORCHESTRATOR_TOOL_NAMES.contains(&name)
}

pub fn orchestrator_risk_for_tool(name: &str, args: Option<&Value>) -> AiActionRisk {
    if name == "run_command" {
        return if has_denied_commands(name, args) {
            AiActionRisk::Destructive
        } else {
            AiActionRisk::Execute
        };
    }

    match name {
        "send_terminal_input" => AiActionRisk::Interactive,
        "write_resource" | "transfer_resource" => AiActionRisk::Write,
        "connect_target" | "open_app_surface" | "remember_preference" => AiActionRisk::Write,
        _ => AiActionRisk::Read,
    }
}

pub fn orchestrator_approval_key_for_tool(name: &str, args: Option<&Value>) -> String {
    if name == "write_resource" {
        let resource = args
            .and_then(|args| args.get("resource"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        return match resource {
            "settings" | "file" => format!("write_resource:{resource}"),
            "" => "write_resource:unsupported".to_string(),
            other => format!("write_resource:{other}"),
        };
    }

    name.to_string()
}

pub fn has_denied_commands(tool_name: &str, args: Option<&Value>) -> bool {
    !denied_commands(tool_name, args).is_empty()
}

pub fn denied_commands(tool_name: &str, args: Option<&Value>) -> Vec<String> {
    let Some(args) = args else {
        return Vec::new();
    };
    if matches!(tool_name, "terminal_exec" | "local_exec" | "run_command")
        && let Some(command) = args.get("command").and_then(Value::as_str)
    {
        return is_command_denied(command)
            .then(|| command.to_string())
            .into_iter()
            .collect();
    }
    if tool_name == "batch_exec"
        && let Some(commands) = args.get("commands").and_then(Value::as_array)
    {
        return commands
            .iter()
            .filter_map(Value::as_str)
            .filter(|command| is_command_denied(command))
            .map(str::to_string)
            .collect();
    }
    Vec::new()
}

pub fn is_command_denied(command: &str) -> bool {
    command_deny_list()
        .iter()
        .any(|pattern| pattern.is_match(command))
}

fn policy_decision(
    decision: AiPolicyDecisionKind,
    risk: AiActionRisk,
    reason_code: &str,
    reason_text_key: &str,
    matched_policy_key: String,
    approval_mode: AiPolicySafetyMode,
    profile_id: Option<String>,
) -> AiPolicyDecision {
    AiPolicyDecision {
        decision,
        risk,
        reason_code: reason_code.to_string(),
        reason_text_key: reason_text_key.to_string(),
        matched_policy_key,
        approval_mode,
        profile_id,
    }
}

fn command_deny_list() -> &'static [Regex] {
    static COMMAND_DENY_LIST: OnceLock<Vec<Regex>> = OnceLock::new();
    COMMAND_DENY_LIST
        .get_or_init(|| {
            // Source: Tauri `lib/ai/tools/toolDefinitions.ts` COMMAND_DENY_LIST.
            [
                r"\brm\s+.*\s+/(\s|$|\*)",
                r"\brm\s+(-[a-zA-Z]*)*\s*--no-preserve-root",
                r"\brm\s+-[^\n]*[rf][^\n]*\s+",
                r"\bmkfs\b",
                r"\bdd\s+if=",
                r"\bfdisk\b",
                r"\bchmod\s+777\s+/",
                r"\bchmod\s+-[^\n]*R[^\n]*\s+",
                r"\bchown\s+-R\s+.*\s+/",
                r"\bchown\s+-[^\n]*R[^\n]*\s+",
                r"\bgit\s+clean\s+-[^\n]*[fd][^\n]*[dx]?[^\n]*",
                r"\bsudo\b",
                r"\bdoas\b",
                r"\bpkexec\b",
                r"\brunuser\b",
                r"\brun0\b",
                r"\bsu\s+-?c\b",
                r"\bsu\s+-\s*$",
                r"\bsudo\s+-i\b",
                r"\bshutdown\b",
                r"\breboot\b",
                r"\bhalt\b",
                r"\bpoweroff\b",
                r"\bsystemctl\s+(disable|mask)\b",
                r"\bsystemctl\s+(?:restart|stop|kill|reload|try-restart|isolate)\b",
                r"\bservice\s+\S+\s+(?:restart|stop|reload)\b",
                r"\bdocker\s+(?:rm|rmi)\b",
                r"\bdocker\s+(?:container|image|volume|network)\s+rm\b",
                r"\bdocker\s+(?:system|container|image|volume|network)\s+prune\b",
                r"\bdocker\s+compose\s+down\b[^\n]*\s-v\b",
                r"\bkubectl\s+delete\b",
                r"\bkubectl\s+drain\b",
                r"\bkubectl\s+scale\b[^\n]*--replicas\s*=\s*0\b",
                r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;?\s*:",
                r"\biptables\s+-F\b",
                r"\b(?:curl|wget)\b[^\n]*\|\s*(?:sh|bash|zsh)\b",
                r"\b(?:curl|wget)\b[^\n]*-[oO]\s*[^\s]+.*;\s*(?:sh|bash|zsh)\b",
                r"\bbase64\b[^\n]*\|\s*(?:sh|bash|zsh)\b",
                r"\bprintf\b[^\n]*\|\s*(?:sh|bash|zsh)\b",
                r"\becho\b[^\n]*\|\s*(?:sh|bash|zsh)\b",
                r"\$\([^)]*\)\s*\|\s*(?:sh|bash|zsh)\b",
                r"`[^`]*`\s*\|\s*(?:sh|bash|zsh)\b",
                r">>?\s*~?/?\.ssh/authorized_keys",
                r">>?\s*~?/?\.ssh/config",
                r"\bcrontab\b",
                r"/etc/cron",
                r"\bunset\s+HISTFILE\b",
                r"\bhistory\s+-c\b",
                r"\bHISTSIZE=0\b",
                r"\bnc\s+.*-[elp]",
                r"(?i)\bsocat\b.*TCP-LISTEN",
                r"/dev/tcp/",
                r"\beval\b",
                r"(?:^|[;&|]\s*)exec\s",
                r"\bsource\s",
            ]
            .into_iter()
            .map(|pattern| Regex::new(pattern).expect("valid Tauri AI command deny pattern"))
            .collect()
        })
        .as_slice()
}
