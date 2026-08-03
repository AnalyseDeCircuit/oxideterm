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
