//! Terminal observations in the model protocol; live session access stays with the host.

use std::time::Duration;

use crate::AiExecutedToolResult;

pub fn annotate_ai_run_command_execution_result(
    result: &mut AiExecutedToolResult,
    args: &serde_json::Value,
) {
    let Some(envelope) = result.envelope.as_object_mut() else {
        return;
    };
    let command = args
        .get("command")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let cwd = args
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let target = envelope
        .get("targets")
        .and_then(serde_json::Value::as_array)
        .and_then(|targets| targets.first())
        .cloned();
    let target_kind = target
        .as_ref()
        .and_then(|target| target.get("kind"))
        .and_then(serde_json::Value::as_str);
    let data = envelope.get("data");
    let command_id = data.and_then(|value| value.get("commandId")).cloned();
    let exit_code = data.and_then(|value| value.get("exitCode")).cloned();
    let timed_out = data
        .and_then(|value| value.get("timedOut"))
        .and_then(serde_json::Value::as_bool);
    let execution_state = data
        .and_then(|value| value.get("executionState"))
        .and_then(serde_json::Value::as_str);
    let visible_in_terminal = data
        .and_then(|value| value.get("visibleInTerminal"))
        .and_then(serde_json::Value::as_bool);
    let truncated = envelope
        .get("meta")
        .and_then(|meta| meta.get("truncated"))
        .and_then(serde_json::Value::as_bool);
    let stderr_summary = envelope
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(serde_json::Value::as_str)
        .and_then(ai_execution_stderr_summary);

    let mut execution = serde_json::Map::new();
    execution.insert(
        "kind".to_string(),
        serde_json::json!(if target_kind == Some("terminal-session") {
            "terminal"
        } else {
            "command"
        }),
    );
    if let Some(command) = command {
        execution.insert("command".to_string(), serde_json::json!(command));
    }
    if let Some(cwd) = cwd {
        execution.insert("cwd".to_string(), serde_json::json!(cwd));
    }
    if let Some(target) = target {
        let mut execution_target = serde_json::Map::new();
        if let Some(id) = target.get("id") {
            execution_target.insert("id".to_string(), id.clone());
        }
        if let Some(kind) = target.get("kind") {
            execution_target.insert("kind".to_string(), kind.clone());
        }
        if let Some(label) = target.get("label") {
            execution_target.insert("label".to_string(), label.clone());
        }
        if !execution_target.is_empty() {
            execution.insert(
                "target".to_string(),
                serde_json::Value::Object(execution_target),
            );
        }
    }
    if let Some(command_id) = command_id {
        execution.insert("commandId".to_string(), command_id);
    }
    if let Some(exit_code) = exit_code {
        execution.insert("exitCode".to_string(), exit_code);
    }
    if let Some(timed_out) = timed_out {
        execution.insert("timedOut".to_string(), serde_json::json!(timed_out));
    }
    if let Some(execution_state) = execution_state {
        execution.insert("state".to_string(), serde_json::json!(execution_state));
    }
    if let Some(visible_in_terminal) = visible_in_terminal {
        execution.insert(
            "visibleInTerminal".to_string(),
            serde_json::json!(visible_in_terminal),
        );
    }
    if let Some(truncated) = truncated {
        execution.insert("truncated".to_string(), serde_json::json!(truncated));
    }
    if let Some(stderr_summary) = stderr_summary {
        execution.insert(
            "stderrSummary".to_string(),
            serde_json::json!(stderr_summary),
        );
    }
    envelope.insert(
        "execution".to_string(),
        serde_json::Value::Object(execution),
    );
}

fn ai_execution_stderr_summary(message: &str) -> Option<String> {
    let summary = message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    if summary.is_empty() {
        None
    } else {
        Some(truncate_ai_execution_stderr_summary(&summary, 600))
    }
}

fn truncate_ai_execution_stderr_summary(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let head = value.chars().take(max_chars).collect::<String>();
    format!("{head}...[truncated]")
}

pub fn ai_terminal_input_payload(args: &serde_json::Value) -> Vec<u8> {
    if let Some(key) = args.get("key").and_then(serde_json::Value::as_str) {
        return ai_terminal_key_bytes(key).map_or_else(Vec::new, <[u8]>::to_vec);
    }
    let mut payload = args
        .get("text")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .as_bytes()
        .to_vec();
    if args
        .get("append_enter")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        payload.push(b'\r');
    }
    payload
}

fn ai_terminal_key_bytes(key: &str) -> Option<&'static [u8]> {
    // These are terminal protocol bytes, not platform key events, so the same
    // payload remains valid for local, SSH, Telnet, and serial sessions.
    match key {
        "ctrl_c" => Some(b"\x03"),
        "ctrl_d" => Some(b"\x04"),
        "ctrl_z" => Some(b"\x1a"),
        "escape" => Some(b"\x1b"),
        "enter" => Some(b"\r"),
        "tab" => Some(b"\t"),
        "backspace" => Some(b"\x7f"),
        "up" => Some(b"\x1b[A"),
        "down" => Some(b"\x1b[B"),
        "right" => Some(b"\x1b[C"),
        "left" => Some(b"\x1b[D"),
        "home" => Some(b"\x1b[H"),
        "end" => Some(b"\x1b[F"),
        "page_up" => Some(b"\x1b[5~"),
        "page_down" => Some(b"\x1b[6~"),
        "delete" => Some(b"\x1b[3~"),
        _ => None,
    }
}

pub fn ai_terminal_tui_state(screen: Option<&serde_json::Value>, buffer: &str) -> &'static str {
    if screen
        .and_then(|screen| screen.get("isAlternateBuffer"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return "alternate_screen";
    }
    if looks_waiting_for_input(buffer) {
        return "prompt";
    }
    "shell"
}

pub fn terminal_delta_output(before: &str, after: &str) -> String {
    if let Some(delta) = after.strip_prefix(before) {
        let delta = delta.trim();
        if !delta.is_empty() {
            return delta.to_string();
        }
    }
    after
        .chars()
        .rev()
        .take(1000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

pub fn looks_waiting_for_input(value: &str) -> bool {
    let tail = value
        .chars()
        .rev()
        .take(1000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>()
        .to_ascii_lowercase();
    let prompt_line = tail
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    // A mention in command output is not an input request. Keep this heuristic
    // separate from the terminal owner's authoritative credential prompt state.
    let prompt_suffix = prompt_line.ends_with([':', '：', '?', '？']);
    prompt_suffix
        && [
            "password",
            "passphrase",
            "verification code",
            "验证码",
            "口令",
            "密码",
        ]
        .iter()
        .any(|needle| prompt_line.contains(needle))
}

pub fn ai_terminal_input_wait_reason(
    buffer: &str,
    waiting_for_secret: bool,
) -> Option<&'static str> {
    if waiting_for_secret {
        Some("credential_prompt")
    } else if looks_waiting_for_input(buffer) {
        Some("possible_credential_prompt")
    } else {
        let line = buffer
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default();
        let line = zeroize::Zeroizing::new(line.trim().to_ascii_lowercase());
        let confirmation = [
            "[y/n]",
            "[y/n]:",
            "(y/n)",
            "(y/n)?",
            "(yes/no)?",
            "[yes/no]:",
            "([y]/n)?",
            "(y/[n])?",
            "[y/n]?",
        ]
        .iter()
        .any(|suffix| line.ends_with(suffix));
        let enter = line.contains("press enter")
            || line.contains("press return")
            || line.contains("按回车")
            || line.contains("按 enter");
        (confirmation || enter).then_some("interactive_prompt")
    }
}

pub struct AiTerminalCommandWait {
    deadline: std::time::Instant,
}

impl AiTerminalCommandWait {
    pub fn with_timeout(now: std::time::Instant, timeout: Duration) -> Self {
        Self {
            deadline: now + timeout.min(Duration::from_secs(30)),
        }
    }

    pub fn deadline(&self) -> std::time::Instant {
        self.deadline
    }

    pub fn expired(&self, now: std::time::Instant) -> bool {
        now >= self.deadline
    }
}

pub fn ai_terminal_command_output_ready(
    command_completed: bool,
    prompt_returned: bool,
    before: &str,
    current: &str,
    quiet_for: Duration,
    waiting_for_secret: bool,
    recovering: bool,
) -> bool {
    if recovering {
        return false;
    }
    // Historical integration capability cannot describe a nested shell. Quiet output
    // yields an observation regardless of that capability, without proving an exit.
    command_completed
        || prompt_returned
        || ((current != before || waiting_for_secret) && quiet_for >= Duration::from_millis(400))
}

pub fn ai_terminal_observation_data(
    command_id: Option<&str>,
    completed: bool,
    prompt_returned: bool,
    input_reason: Option<&str>,
    exit_code: Option<i32>,
    expired: bool,
) -> serde_json::Value {
    let state = if completed {
        "completed"
    } else if prompt_returned {
        "shell_ready"
    } else if input_reason.is_some() {
        "waiting_input"
    } else {
        "output_captured"
    };
    serde_json::json!({
        "executionState": state,
        "visibleInTerminal": true,
        "commandId": command_id,
        "exitCode": if completed { exit_code } else { None },
        "waitingForInput": input_reason.is_some(),
        "inputWaitReason": input_reason,
        "terminalObservation": {
            "state": state,
            "commandId": command_id,
            "completionConfirmed": completed,
            "canRunCommand": (completed || prompt_returned) && input_reason.is_none(),
            "exitCode": if completed { exit_code } else { None },
            "waitExpired": expired,
        },
    })
}

pub fn ai_terminal_wait_match(
    args: &serde_json::Value,
    initial_buffer: &str,
    current_buffer: &str,
    initial_alternate_screen: bool,
    current_alternate_screen: bool,
    completed_command: impl FnOnce(&str) -> Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    let condition = args.get("condition").and_then(serde_json::Value::as_str)?;
    match condition {
        "changed" if current_buffer != initial_buffer => {
            Some(serde_json::json!({ "condition": condition }))
        }
        "contains" => {
            let expected = args.get("text").and_then(serde_json::Value::as_str)?;
            let case_sensitive = args
                .get("case_sensitive")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            let matched = if case_sensitive {
                current_buffer.contains(expected)
            } else {
                current_buffer
                    .to_lowercase()
                    .contains(&expected.to_lowercase())
            };
            matched.then(|| serde_json::json!({ "condition": condition }))
        }
        "prompt" if ai_terminal_input_wait_reason(current_buffer, false).is_some() => {
            Some(serde_json::json!({ "condition": condition }))
        }
        "tui_entered" if !initial_alternate_screen && current_alternate_screen => {
            Some(serde_json::json!({ "condition": condition }))
        }
        "tui_exited" if initial_alternate_screen && !current_alternate_screen => {
            Some(serde_json::json!({ "condition": condition }))
        }
        "command_completed" => {
            let command_id = args.get("command_id").and_then(serde_json::Value::as_str)?;
            completed_command(command_id)
                .map(|record| serde_json::json!({ "condition": condition, "command": record }))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_control_keys_map_to_protocol_bytes() {
        assert_eq!(
            ai_terminal_input_payload(&serde_json::json!({ "key": "ctrl_c" })),
            b"\x03"
        );
        assert_eq!(
            ai_terminal_input_payload(&serde_json::json!({ "key": "page_down" })),
            b"\x1b[6~"
        );
    }

    #[test]
    fn output_capture_respects_shell_events_activity_and_wait_states() {
        let quiet = Duration::from_millis(500);
        for (completed, prompt_returned, output, elapsed, secret, reconnecting, expected) in [
            (
                false,
                false,
                "command\nresult\n$ ",
                quiet,
                false,
                false,
                true,
            ),
            (
                false,
                true,
                "command\nresult\n$ ",
                quiet,
                false,
                false,
                true,
            ),
            (
                true,
                true,
                "command\nresult\n$ ",
                Duration::ZERO,
                false,
                false,
                true,
            ),
            (
                false,
                false,
                "command\nresult",
                Duration::from_millis(100),
                false,
                false,
                false,
            ),
            (false, false, "before", quiet, false, false, false),
            (false, false, "command\nPassword:", quiet, true, false, true),
            (false, false, "command\nresult", quiet, false, true, false),
        ] {
            assert_eq!(
                ai_terminal_command_output_ready(
                    completed,
                    prompt_returned,
                    "before",
                    output,
                    elapsed,
                    secret,
                    reconnecting
                ),
                expected
            );
        }
    }
}
