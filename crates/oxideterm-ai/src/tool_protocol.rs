#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiOrchestratorObligationMode {
    Auto,
    Required,
}

#[derive(Clone, Debug)]
pub struct AiOrchestratorObligation {
    pub mode: AiOrchestratorObligationMode,
    pub reason: String,
    pub candidate_tools: Vec<String>,
}

impl AiOrchestratorObligation {
    pub fn auto() -> Self {
        Self {
            mode: AiOrchestratorObligationMode::Auto,
            reason: "No mandatory app action detected.".to_string(),
            candidate_tools: Vec::new(),
        }
    }
}

pub fn ai_classify_orchestrator_obligation(text: &str) -> AiOrchestratorObligation {
    let lower = text.to_lowercase();
    let discovery = [
        "有哪些",
        "哪些",
        "列出",
        "看看",
        "查看",
        "show",
        "list",
        "available",
        "host",
        "target",
        "connection",
        "主机",
        "连接",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    // Mirror Tauri's discovery exclusion so direct action requests such as
    // "connect to host" enter the broader action-tool candidate set.
    let direct_target_action = text.contains("连接到")
        || text.contains("connect to")
        || text.contains("run on")
        || ai_contains_in_order(text, "在", "运行")
        || ai_contains_in_order(text, "执行", "在");
    if discovery && !direct_target_action {
        return AiOrchestratorObligation {
            mode: AiOrchestratorObligationMode::Required,
            reason: "The user is asking for real available app targets; call list_targets before answering.".to_string(),
            candidate_tools: vec!["list_targets".to_string()],
        };
    }

    let action = [
        "连接", "打开", "执行", "运行", "修改", "设置", "上传", "下载", "读取", "写入", "搜索",
        "connect", "open", "run", "execute", "modify", "set", "upload", "download", "read",
        "write", "search",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    if action {
        return AiOrchestratorObligation {
            mode: AiOrchestratorObligationMode::Required,
            reason: "The request asks OxideTerm to inspect, connect, execute, open, or modify real app state.".to_string(),
            candidate_tools: vec![
                "list_targets".to_string(),
                "select_target".to_string(),
                "connect_target".to_string(),
                "run_command".to_string(),
                "open_app_surface".to_string(),
                "read_resource".to_string(),
                "write_resource".to_string(),
            ],
        };
    }

    AiOrchestratorObligation::auto()
}

fn ai_contains_in_order(text: &str, first: &str, second: &str) -> bool {
    text.find(first)
        .and_then(|first_index| text[first_index + first.len()..].find(second))
        .is_some()
}

pub fn ai_orchestrator_obligation_prompt(obligation: &AiOrchestratorObligation) -> Option<String> {
    if obligation.mode != AiOrchestratorObligationMode::Required {
        return None;
    }
    Some(
        [
            "## Required Tool Call".to_string(),
            obligation.reason.clone(),
            format!(
                "Call one of these tools before the final answer: {}.",
                obligation
                    .candidate_tools
                    .iter()
                    .map(|tool| format!("`{tool}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "If a tool returns disambiguation or multiple targets, ask the user to choose instead of guessing.".to_string(),
        ]
        .join("\n"),
    )
}

pub fn ai_required_tool_retry_prompt(obligation: &AiOrchestratorObligation) -> String {
    let candidates = if obligation.candidate_tools.is_empty() {
        "the relevant available tool".to_string()
    } else {
        obligation
            .candidate_tools
            .iter()
            .take(8)
            .map(|tool| format!("`{tool}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    [
        "The previous assistant response did not call a structured tool, but this user request requires real app/tool state.".to_string(),
        format!("Reason: {}.", obligation.reason),
        format!("Call one of these tools before giving a final answer: {candidates}."),
        "Do not claim that anything was opened, connected, executed, read, modified, checked, verified, or diagnosed until a tool result proves it.".to_string(),
    ]
    .join("\n")
}

pub fn ai_should_retry_required_tool_round(
    obligation: &AiOrchestratorObligation,
    assistant_text: &str,
) -> bool {
    if obligation.mode != AiOrchestratorObligationMode::Required
        || obligation.candidate_tools.is_empty()
    {
        return false;
    }
    let trimmed = assistant_text.trim();
    if trimmed.is_empty() {
        return true;
    }
    if ai_text_contains_tauri_action_claim(trimmed) {
        return true;
    }
    let looks_like_clarification = trimmed.ends_with('?')
        || trimmed.ends_with('？')
        || ["请", "需要你", "你可以", "是否", "哪一个", "哪个", "确认"]
            .iter()
            .any(|needle| trimmed.contains(needle));
    !looks_like_clarification
}

pub fn ai_text_contains_tauri_action_claim(text: &str) -> bool {
    static ACTION_CLAIM_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        // Keep this in sync with Tauri's ACTION_CLAIM_RE in aiChatStore.ts.
        regex::Regex::new(r"(?i)\b(?:opened|connected|executed|ran|read|modified|changed|checked|verified|diagnosed|found|failed|succeeded)\b|(?:已经|已|我来|我已|现在).*(?:打开|连接|执行|运行|读取|修改|检查|诊断|确认|发现)|(?:结果是|连接失败|执行完成|修改完成)")
            .expect("valid AI action-claim regex")
    });
    ACTION_CLAIM_RE.is_match(text)
}

pub fn ai_user_explicitly_requested_json(text: &str) -> bool {
    static JSON_REQUEST_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        // Mirror Tauri's JSON_REQUEST_RE so hard-deny suppression only
        // applies when the user explicitly asks for JSON-like output.
        regex::Regex::new(
            r"(?i)\b(json|jsonl|json schema|jsonschema|payload|response format|object literal|schema)\b",
        )
        .expect("valid AI JSON-request regex")
    });
    JSON_REQUEST_RE.is_match(text)
}

pub fn ai_should_trigger_hard_deny(assistant_text: &str, user_requested_json: bool) -> bool {
    if user_requested_json {
        return false;
    }
    let trimmed = ai_strip_code_fence(assistant_text);
    if trimmed.is_empty() {
        return false;
    }
    let looks_json = (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'));
    if !looks_json {
        return false;
    }
    let lower = trimmed.to_lowercase();
    let field_count = [
        "\"name\"",
        "\"arguments\"",
        "\"stdout\"",
        "\"stderr\"",
        "\"exit_code\"",
        "\"exit-code\"",
        "\"status\"",
        "\"tool_call_id\"",
        "\"toolname\"",
        "\"toolcallid\"",
    ]
    .iter()
    .filter(|needle| lower.contains(*needle))
    .count();
    let looks_like_tool_request = lower.contains("\"name\"") && lower.contains("\"arguments\"");
    let looks_like_tool_result = (lower.contains("\"stdout\"") || lower.contains("\"stderr\""))
        && (lower.contains("\"exit_code\"")
            || lower.contains("\"exit-code\"")
            || lower.contains("\"status\""));
    looks_like_tool_request || looks_like_tool_result || field_count >= 3
}

fn ai_strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    for prefix in ["```json", "```javascript", "```js", "```text", "```"] {
        if let Some(rest) = trimmed.strip_prefix(prefix)
            && let Some(inner) = rest.strip_suffix("```")
        {
            return inner.trim().to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orchestrator_obligation_keeps_direct_connect_out_of_discovery_only_mode() {
        let obligation = ai_classify_orchestrator_obligation("连接到 prod.example.com");

        assert_eq!(obligation.mode, AiOrchestratorObligationMode::Required);
        assert!(
            obligation
                .candidate_tools
                .contains(&"connect_target".to_string())
        );
        assert!(
            obligation
                .candidate_tools
                .contains(&"run_command".to_string())
        );
    }

    #[test]
    fn orchestrator_discovery_exclusion_keeps_tauri_case_sensitivity() {
        let obligation = ai_classify_orchestrator_obligation("Connect to host prod");

        assert_eq!(obligation.mode, AiOrchestratorObligationMode::Required);
        assert_eq!(obligation.candidate_tools, vec!["list_targets".to_string()]);
    }

    #[test]
    fn required_tool_retry_action_claim_uses_tauri_boundaries() {
        let obligation = AiOrchestratorObligation {
            mode: AiOrchestratorObligationMode::Required,
            reason: "test".to_string(),
            candidate_tools: vec!["list_targets".to_string()],
        };

        assert!(!ai_text_contains_tauri_action_claim("ready?"));
        assert!(!ai_should_retry_required_tool_round(&obligation, "ready?"));
        assert!(!ai_text_contains_tauri_action_claim("已知条件是哪一个？"));
        assert!(!ai_should_retry_required_tool_round(
            &obligation,
            "已知条件是哪一个？"
        ));
        assert!(ai_text_contains_tauri_action_claim("已经连接到目标"));
        assert!(ai_should_retry_required_tool_round(
            &obligation,
            "已经连接到目标"
        ));
    }

    #[test]
    fn json_request_detection_uses_tauri_word_boundaries() {
        assert!(ai_user_explicitly_requested_json("Return JSON schema."));
        assert!(ai_user_explicitly_requested_json("Use an object literal"));
        assert!(!ai_user_explicitly_requested_json(
            "Please jsonify this later"
        ));
        assert!(!ai_user_explicitly_requested_json("This is schematic only"));
    }

    #[test]
    fn pseudo_tool_json_hard_deny_respects_json_requests() {
        let pseudo = r#"{"name":"run_command","arguments":{"command":"pwd"}}"#;

        assert!(ai_should_trigger_hard_deny(pseudo, false));
        assert!(!ai_should_trigger_hard_deny(pseudo, true));
        assert!(!ai_should_trigger_hard_deny("正常回答", false));
    }
}
