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

pub fn sanitize_chat_state_for_persistence(state: &mut AiChatState) {
    for conversation in &mut state.conversations {
        conversation.title = sanitize_for_ai(&conversation.title);
        if let Some(metadata) = conversation.session_metadata.as_mut() {
            *metadata = sanitize_json_for_ai(metadata);
        }
        for message in &mut conversation.messages {
            sanitize_chat_message_for_persistence(message);
        }
    }
}

fn sanitize_chat_message_for_persistence(message: &mut AiChatMessage) {
    // Persisted conversation projections can replay into prompts and
    // diagnostics, including nested branch and compaction snapshots.
    message.content = sanitize_for_ai(&message.content);
    if let Some(context) = message.context.as_mut() {
        *context = sanitize_for_ai(context);
    }
    if let Some(thinking) = message.thinking_content.as_mut() {
        *thinking = sanitize_for_ai(thinking);
    }
    for tool_call in &mut message.tool_calls {
        *tool_call = sanitize_json_for_ai(tool_call);
    }
    for value in [
        &mut message.turn,
        &mut message.transcript_ref,
        &mut message.summary_ref,
    ] {
        if let Some(value) = value.as_mut() {
            *value = sanitize_json_for_ai(value);
        }
    }
    for suggestion in &mut message.suggestions {
        suggestion.text = sanitize_for_ai(&suggestion.text);
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
