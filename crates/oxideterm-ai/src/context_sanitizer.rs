use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::{AiChatMessage, AiChatState};

const REDACTED: &str = "[REDACTED]";

static PRIVATE_KEY_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"-----BEGIN\s+(?:RSA\s+|EC\s+|DSA\s+|OPENSSH\s+)?PRIVATE\s+KEY-----[\s\S]*?-----END\s+(?:RSA\s+|EC\s+|DSA\s+|OPENSSH\s+)?PRIVATE\s+KEY-----",
    )
    .unwrap()
});
static EXPORT_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(export\s+\w*(?:SECRET|TOKEN|PASSWORD|PASSWD|KEY|CREDENTIAL|AUTH)[A-Z_]*\s*=\s*).+",
    )
    .unwrap()
});
static KEY_VALUE_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b(\w*(?:SECRET|_KEY|TOKEN|PASSWORD|PASSWD|CREDENTIAL|AUTH_TOKEN|API_KEY|APIKEY|ACCESS_KEY|PRIVATE_KEY)\s*[=:]\s*)(?:"[^"\n]{8,}"|'[^'\n]{8,}'|[^\s'";\n,)}{]{8,})"#,
    )
    .unwrap()
});
static JSON_DOUBLE_QUOTED_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)("[^"]*(?:secret|_key|token|password|passwd|credential|auth_token|api_key|apikey|access_key|private_key)"\s*:\s*")[^"\n]{8,}(")"#,
    )
    .unwrap()
});
static JSON_SINGLE_QUOTED_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)('[^']*(?:secret|_key|token|password|passwd|credential|auth_token|api_key|apikey|access_key|private_key)'\s*:\s*')[^'\n]{8,}(')"#,
    )
    .unwrap()
});
static AUTH_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b((?:Authorization|Proxy-Authorization)\s*:\s*(?:Bearer|Basic|Token|Digest)\s+)\S+",
    )
    .unwrap()
});
static AWS_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap());
static VENDOR_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-proj-[A-Za-z0-9]{20,}|sk-ant-[A-Za-z0-9]{20,}|sk_(?:live|test)_[A-Za-z0-9]{10,}|pk_(?:live|test)_[A-Za-z0-9]{10,}|rk_(?:live|test)_[A-Za-z0-9]{10,}|xox[bpoas]-[A-Za-z0-9\-]{10,})\b",
    )
    .unwrap()
});
static LONG_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Za-z0-9+/]{40,}={0,2}\b").unwrap());
static CONNECTION_PASSWORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)((?:postgres|mysql|mongodb|redis|amqp|mssql|sqlite|mariadb|cockroachdb)://[^:\s]+:)([^@\s]+)(@)")
        .unwrap()
});
static RUNTIME_HANDLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\brt_[0-9a-fA-F]{32}\b").unwrap());
static RUNTIME_REGISTRY_EPOCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bepoch_[0-9a-fA-F]{32}\b").unwrap());
static RUNTIME_SNAPSHOT_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bsnap_[0-9a-fA-F]{32}\b").unwrap());

pub fn sanitize_for_ai(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut result = text.to_string();
    result = PRIVATE_KEY_BLOCK
        .replace_all(
            &result,
            format!("-----BEGIN PRIVATE KEY-----\n{REDACTED}\n-----END PRIVATE KEY-----"),
        )
        .into_owned();
    result = EXPORT_SECRET
        .replace_all(&result, format!("${{1}}{REDACTED}"))
        .into_owned();
    result = KEY_VALUE_SECRET
        .replace_all(&result, |captures: &regex::Captures<'_>| {
            let full_match = captures
                .get(0)
                .map(|value| value.as_str())
                .unwrap_or_default();
            let prefix = captures
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or_default();
            let value = full_match.strip_prefix(prefix).unwrap_or_default();
            if is_tauri_type_annotation_value(prefix, value) {
                full_match.to_string()
            } else {
                format!("{prefix}{REDACTED}")
            }
        })
        .into_owned();
    result = JSON_DOUBLE_QUOTED_SECRET
        .replace_all(&result, format!("${{1}}{REDACTED}${{2}}"))
        .into_owned();
    result = JSON_SINGLE_QUOTED_SECRET
        .replace_all(&result, format!("${{1}}{REDACTED}${{2}}"))
        .into_owned();
    result = AUTH_HEADER
        .replace_all(&result, format!("${{1}}{REDACTED}"))
        .into_owned();
    result = AWS_KEY.replace_all(&result, REDACTED).into_owned();
    result = VENDOR_TOKEN.replace_all(&result, REDACTED).into_owned();
    result = LONG_TOKEN
        .replace_all(&result, |captures: &regex::Captures<'_>| {
            let token = captures
                .get(0)
                .map(|value| value.as_str())
                .unwrap_or_default();
            if token.chars().any(char::is_lowercase)
                && token.chars().any(char::is_uppercase)
                && token.chars().any(|ch| ch.is_ascii_digit())
            {
                REDACTED.to_string()
            } else {
                token.to_string()
            }
        })
        .into_owned();
    CONNECTION_PASSWORD
        .replace_all(&result, format!("${{1}}{REDACTED}${{3}}"))
        .into_owned()
}

/// Long-term memory accepts only content that survives secret screening and
/// does not explicitly describe a one-off instruction.
pub fn preference_is_safe_to_persist(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || sanitize_for_ai(trimmed) != trimmed {
        return false;
    }
    let normalized = trimmed.to_ascii_lowercase();
    const EPHEMERAL_MARKERS: &[&str] = &[
        "for this task",
        "for this request",
        "only this time",
        "one-time",
        "temporary instruction",
        "current task",
        "本次任务",
        "这次任务",
        "仅这一次",
        "临时指令",
    ];
    !EPHEMERAL_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker))
}

pub fn sanitize_json_for_ai(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let sanitized = if value.is_string() && is_sensitive_json_key(key) {
                        Value::String(REDACTED.to_string())
                    } else if let Some(value) = value.as_str()
                        && is_embedded_json_key(key)
                    {
                        Value::String(sanitize_json_text_for_ai(value))
                    } else {
                        sanitize_json_for_ai(value)
                    };
                    (key.clone(), sanitized)
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(sanitize_json_for_ai).collect()),
        Value::String(value) => Value::String(sanitize_for_ai(value)),
        other => other.clone(),
    }
}

pub fn sanitize_json_text_for_ai(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .map(|value| sanitize_json_for_ai(&value).to_string())
        .unwrap_or_else(|_| sanitize_for_ai(text))
}

/// Removes short-lived capability tokens at durable-history and diagnostic boundaries.
/// Provider messages deliberately use `sanitize_for_ai` instead because a current
/// tool session may need its live handle again during the same turn.
pub fn sanitize_for_persistence(text: &str) -> String {
    let without_handles = RUNTIME_HANDLE
        .replace_all(&sanitize_for_ai(text), "[REDACTED_RUNTIME_HANDLE]")
        .into_owned();
    let without_epochs = RUNTIME_REGISTRY_EPOCH
        .replace_all(&without_handles, "[REDACTED_RUNTIME_EPOCH]")
        .into_owned();
    RUNTIME_SNAPSHOT_ID
        .replace_all(&without_epochs, "[REDACTED_RUNTIME_SNAPSHOT]")
        .into_owned()
}

pub fn sanitize_json_for_persistence(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let sanitized = if value.is_string() && is_sensitive_json_key(key) {
                        Value::String(REDACTED.to_string())
                    } else if let Some(value) = value.as_str()
                        && is_embedded_json_key(key)
                    {
                        Value::String(sanitize_json_text_for_persistence(value))
                    } else {
                        sanitize_json_for_persistence(value)
                    };
                    (key.clone(), sanitized)
                })
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.iter().map(sanitize_json_for_persistence).collect())
        }
        Value::String(value) => Value::String(sanitize_for_persistence(value)),
        other => other.clone(),
    }
}

/// Builds a durable, non-actionable projection of tool protocol data.
///
/// Runtime authority and application-owner coordinates are removed structurally instead
/// of being retained as redacted placeholders that could later be mistaken for input.
pub fn sanitize_tool_protocol_json_for_persistence(value: &Value) -> Value {
    sanitize_tool_protocol_value_for_persistence(value, None)
}

/// Removes secret-capable execution payloads from a persisted tool argument object.
pub fn sanitize_tool_arguments_json_for_persistence(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .filter_map(|(key, value)| {
                    if is_runtime_authority_json_key(key)
                        || is_secret_capable_tool_argument_key(key)
                    {
                        return None;
                    }
                    if value.is_string() && is_sensitive_json_key(key) {
                        return Some((key.clone(), Value::String(REDACTED.to_string())));
                    }
                    Some((
                        key.clone(),
                        sanitize_tool_arguments_json_for_persistence(value),
                    ))
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(sanitize_tool_arguments_json_for_persistence)
                .collect(),
        ),
        Value::String(value) => Value::String(sanitize_for_persistence(value)),
        other => other.clone(),
    }
}

pub fn sanitize_tool_arguments_text_for_persistence(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .map(|value| sanitize_tool_arguments_json_for_persistence(&value).to_string())
        .unwrap_or_else(|_| sanitize_for_persistence(text))
}

/// Produces the durable tool-result view without retaining file contents or
/// terminal observations that were intended only for the active model turn.
pub fn sanitize_tool_result_json_for_persistence(tool_name: &str, value: &Value) -> Value {
    sanitize_tool_result_value_for_persistence(tool_name, value)
}

fn sanitize_tool_protocol_value_for_persistence(
    value: &Value,
    inherited_tool_name: Option<&str>,
) -> Value {
    match value {
        Value::Object(object) => {
            let tool_name = object
                .get("name")
                .or_else(|| object.get("toolName"))
                .and_then(Value::as_str)
                .or(inherited_tool_name);
            Value::Object(
                object
                    .iter()
                    .filter_map(|(key, value)| {
                        if is_runtime_authority_json_key(key) {
                            return None;
                        }
                        let sanitized = if value.is_string() && is_sensitive_json_key(key) {
                            Value::String(REDACTED.to_string())
                        } else if is_embedded_json_key(key) {
                            value
                                .as_str()
                                .map(sanitize_tool_arguments_text_for_persistence)
                                .map(Value::String)
                                .unwrap_or_else(|| {
                                    sanitize_tool_arguments_json_for_persistence(value)
                                })
                        } else if normalized_json_key(key) == "result" {
                            sanitize_tool_result_value_for_persistence(
                                tool_name.unwrap_or_default(),
                                value,
                            )
                        } else {
                            sanitize_tool_protocol_value_for_persistence(value, tool_name)
                        };
                        Some((key.clone(), sanitized))
                    })
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| {
                    sanitize_tool_protocol_value_for_persistence(value, inherited_tool_name)
                })
                .collect(),
        ),
        Value::String(value) => Value::String(sanitize_for_persistence(value)),
        other => other.clone(),
    }
}

fn sanitize_tool_result_value_for_persistence(tool_name: &str, value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .filter_map(|(key, value)| {
                    let normalized = normalized_json_key(key);
                    if is_runtime_authority_json_key(key)
                        || is_secret_capable_tool_result_key(&normalized)
                        || (normalized == "output"
                            && matches!(tool_name, "read_resource" | "observe_terminal"))
                    {
                        return None;
                    }
                    if value.is_string() && is_sensitive_json_key(key) {
                        return Some((key.clone(), Value::String(REDACTED.to_string())));
                    }
                    Some((
                        key.clone(),
                        sanitize_tool_result_value_for_persistence(tool_name, value),
                    ))
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| sanitize_tool_result_value_for_persistence(tool_name, value))
                .collect(),
        ),
        Value::String(value) => Value::String(sanitize_for_persistence(value)),
        other => other.clone(),
    }
}

pub fn sanitize_json_text_for_persistence(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .map(|value| sanitize_json_for_persistence(&value).to_string())
        .unwrap_or_else(|_| sanitize_for_persistence(text))
}

pub fn sanitize_chat_state_for_persistence(state: &mut AiChatState) {
    for conversation in &mut state.conversations {
        conversation.title = sanitize_for_persistence(&conversation.title);
        if let Some(metadata) = conversation.session_metadata.as_mut() {
            *metadata = sanitize_tool_protocol_json_for_persistence(metadata);
        }
        for message in &mut conversation.messages {
            sanitize_chat_message_for_persistence(message);
        }
    }
}

fn sanitize_chat_message_for_persistence(message: &mut AiChatMessage) {
    // Persisted conversation projections can replay into prompts and
    // diagnostics, including nested branch and compaction snapshots.
    message.content = sanitize_for_persistence(&message.content);
    if let Some(context) = message.context.as_mut() {
        *context = sanitize_for_persistence(context);
    }
    if let Some(thinking) = message.thinking_content.as_mut() {
        *thinking = sanitize_for_persistence(thinking);
    }
    for tool_call in &mut message.tool_calls {
        *tool_call = sanitize_tool_protocol_json_for_persistence(tool_call);
        if let Some(object) = tool_call.as_object_mut() {
            object.insert("historical".to_string(), Value::Bool(true));
            object.insert("actionable".to_string(), Value::Bool(false));
        }
    }
    for value in [
        &mut message.turn,
        &mut message.transcript_ref,
        &mut message.summary_ref,
    ] {
        if let Some(value) = value.as_mut() {
            *value = sanitize_tool_protocol_json_for_persistence(value);
        }
    }
    for suggestion in &mut message.suggestions {
        suggestion.text = sanitize_for_persistence(&suggestion.text);
    }
    if let Some(metadata) = message.metadata.as_mut()
        && let Some(original_messages) = metadata.original_messages.as_mut()
    {
        for original in original_messages {
            sanitize_chat_message_for_persistence(original);
        }
    }
    if let Some(branches) = message.branches.as_mut() {
        for tail in branches.tails.values_mut() {
            for branch_message in tail {
                sanitize_chat_message_for_persistence(branch_message);
            }
        }
    }
}

fn is_sensitive_json_key(key: &str) -> bool {
    // JSON payloads lose the surrounding key when string values are sanitized
    // independently, so credential-bearing keys need an explicit boundary.
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    normalized.contains("apikey")
        || normalized.contains("accesskey")
        || normalized.contains("privatekey")
        || normalized.contains("secretkey")
        || normalized.contains("signingkey")
        || normalized.contains("encryptionkey")
        || normalized.contains("password")
        || normalized.contains("passwd")
        || normalized.contains("passphrase")
        || normalized.contains("secret")
        || normalized == "token"
        || normalized.ends_with("authtoken")
        || normalized.ends_with("accesstoken")
        || normalized.ends_with("refreshtoken")
        || normalized.contains("credential")
        || normalized == "authorization"
        || normalized == "proxyauthorization"
}

fn is_embedded_json_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    matches!(normalized.as_str(), "arguments" | "argumentstext")
}

fn is_runtime_authority_json_key(key: &str) -> bool {
    let normalized = normalized_json_key(key);
    matches!(
        normalized.as_str(),
        "handleid"
            | "targetid"
            | "nodeid"
            | "sessionid"
            | "connectionid"
            | "tabid"
            | "paneid"
            | "runtimeepoch"
            | "ownerkey"
            | "ownergeneration"
            | "toolsessionid"
    )
}

fn is_secret_capable_tool_argument_key(key: &str) -> bool {
    matches!(
        normalized_json_key(key).as_str(),
        "command"
            | "text"
            | "content"
            | "value"
            | "preference"
            | "stdin"
            | "input"
            | "body"
            | "headers"
            | "environment"
            | "env"
            | "expectedhash"
    )
}

fn is_secret_capable_tool_result_key(normalized_key: &str) -> bool {
    matches!(
        normalized_key,
        "content"
            | "filecontent"
            | "command"
            | "terminalinput"
            | "rawterminalbuffer"
            | "hash"
            | "contenthash"
            | "expectedhash"
    )
}

fn normalized_json_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_tauri_type_annotation_value(prefix: &str, value: &str) -> bool {
    if !prefix.trim_end().ends_with(':') {
        return false;
    }
    let normalized = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches(',')
        .trim();
    matches!(
        normalized,
        "string"
            | "number"
            | "boolean"
            | "any"
            | "unknown"
            | "never"
            | "void"
            | "null"
            | "undefined"
            | "Buffer"
            | "Uint8Array"
    )
}

pub fn sanitize_api_messages_for_provider(messages: Vec<AiChatMessage>) -> Vec<AiChatMessage> {
    messages
        .into_iter()
        .map(|mut message| {
            if !message.content.is_empty() {
                message.content = sanitize_for_ai(&message.content);
            }
            message
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        preference_is_safe_to_persist, sanitize_for_persistence, sanitize_json_for_persistence,
        sanitize_tool_arguments_json_for_persistence, sanitize_tool_protocol_json_for_persistence,
        sanitize_tool_result_json_for_persistence,
    };

    #[test]
    fn persistence_sanitizer_removes_runtime_handle_tokens() {
        let handle = "rt_4e22e673067e46e28b9f902d7b21af4c";
        let epoch = "epoch_7e22e673067e46e28b9f902d7b21af4c";
        let snapshot = "snap_8e22e673067e46e28b9f902d7b21af4c";
        let text = sanitize_for_persistence(&format!("handle_id={handle} {epoch} {snapshot}"));
        let value = sanitize_json_for_persistence(&serde_json::json!({
            "authority": { "handle_id": handle },
        }));

        assert!(!text.contains(handle));
        assert!(!text.contains(epoch));
        assert!(!text.contains(snapshot));
        assert!(!value.to_string().contains(handle));
    }

    #[test]
    fn persisted_tool_projection_removes_runtime_authority_fields() {
        let handle = "rt_4e22e673067e46e28b9f902d7b21af4c";
        let projected = sanitize_tool_protocol_json_for_persistence(&serde_json::json!({
            "arguments": serde_json::json!({
                "handle_id": handle,
                "resource_ref": {
                    "kind": "saved_connection",
                    "id": "4cb736c8-78ef-4aec-9570-f68895be0167"
                }
            }).to_string(),
            "result": {
                "handleId": handle,
                "nodeId": "node-7",
                "sessionId": "42",
                "runtimeEpoch": "epoch-1",
                "summary": "Completed"
            }
        }));
        let encoded = projected.to_string();

        assert!(!encoded.contains(handle));
        assert!(!encoded.contains("handle_id"));
        assert!(!encoded.contains("handleId"));
        assert!(!encoded.contains("nodeId"));
        assert!(!encoded.contains("sessionId"));
        assert!(!encoded.contains("runtimeEpoch"));
        assert!(encoded.contains("saved_connection"));
        assert!(encoded.contains("Completed"));
    }

    #[test]
    fn persisted_tool_projection_drops_current_turn_execution_payloads() {
        let arguments = sanitize_tool_arguments_json_for_persistence(&serde_json::json!({
            "handle_id": "rt_4e22e673067e46e28b9f902d7b21af4c",
            "resource": "file",
            "path": "/srv/report.txt",
            "command": "deploy --token unknown-shape",
            "text": "terminal input",
            "content": "private file contents",
            "value": "private setting value",
            "expected_hash": "sha256:private-precondition",
        }));
        let result = sanitize_tool_result_json_for_persistence(
            "read_resource",
            &serde_json::json!({
                "summary": "Read file.",
                "output": "private file contents",
                "contentHash": "sha256:private-result",
                "data": {
                    "path": "/srv/report.txt",
                    "content": "private file contents",
                },
            }),
        );
        let retained = format!("{arguments}{result}");

        assert!(retained.contains("/srv/report.txt"));
        assert!(retained.contains("Read file."));
        for forbidden in [
            "handle_id",
            "deploy --token",
            "terminal input",
            "private file contents",
            "private setting value",
            "private-precondition",
            "private-result",
        ] {
            assert!(
                !retained.contains(forbidden),
                "durable tool projection retained {forbidden}"
            );
        }
    }

    #[test]
    fn long_term_preference_rejects_secrets_and_one_off_instructions() {
        assert!(preference_is_safe_to_persist(
            "Prefer concise answers and explain SSH failures."
        ));
        assert!(!preference_is_safe_to_persist("API_KEY=actualsecret123456"));
        assert!(!preference_is_safe_to_persist(
            "For this task, skip validation."
        ));
    }
}
