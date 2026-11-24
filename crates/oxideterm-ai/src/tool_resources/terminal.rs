use std::time::{Duration, Instant};

use oxideterm_workspace::TerminalSessionId;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{
    AiActionResultLite, AiOrchestratorRuntimeSnapshot, AiOrchestratorTarget,
    AiTerminalCommandStatus, ai_runtime_validation_recovery_message,
};
use crate::{
    terminal_observation::{
        ai_terminal_command_output_ready, ai_terminal_input_wait_reason,
        ai_terminal_observation_data, ai_terminal_tui_state, ai_terminal_wait_match,
        terminal_delta_output,
    },
    tool_result_protocol::{rejected_ai_tool_result, trim_tail_chars},
};

pub struct AiTerminalCommand<'a> {
    pub id: &'a str,
    pub command: &'a str,
    pub status: AiTerminalCommandStatus,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub exit_code: Option<i32>,
}

impl AiTerminalCommand<'_> {
    pub fn json(&self) -> Value {
        let status = match self.status {
            AiTerminalCommandStatus::Running => "running",
            AiTerminalCommandStatus::ShellReady => "shell_ready",
            AiTerminalCommandStatus::Completed => "completed",
            AiTerminalCommandStatus::Stale => "stale",
        };
        json!({
            "commandId": self.id,
            "command": crate::sanitize_for_ai(self.command),
            "status": status,
            "completionConfirmed": self.status == AiTerminalCommandStatus::Completed,
            "startedAt": self.started_at,
            "finishedAt": self.finished_at,
            "exitCode": self.exit_code,
        })
    }
}

impl AiOrchestratorRuntimeSnapshot {
    pub fn terminal_target(&self, session: TerminalSessionId) -> Option<AiOrchestratorTarget> {
        let id = session.0.to_string();
        self.targets
            .iter()
            .find(|target| {
                target.kind == "terminal-session" && target.refs.get("sessionId") == Some(&id)
            })
            .cloned()
    }

    pub fn terminal_observed<'a>(
        &self,
        target: AiOrchestratorTarget,
        args: &Value,
        records: impl IntoIterator<Item = AiTerminalCommand<'a>>,
        waiting_for_secret: bool,
    ) -> AiActionResultLite {
        let buffer = target.terminal_buffer.as_deref().unwrap_or_default();
        let max_chars = args
            .get("max_chars")
            .and_then(Value::as_u64)
            .unwrap_or(4000) as usize;
        let output = trim_tail_chars(buffer, max_chars);
        let records = records
            .into_iter()
            .take(5)
            .map(|record| record.json())
            .collect::<Vec<_>>();
        let screen = target
            .terminal_screen
            .clone()
            .unwrap_or_else(|| json!({ "lines": [] }));
        let input_wait_reason = ai_terminal_input_wait_reason(buffer, waiting_for_secret);
        let terminal_observation = records.first().map(|record| {
            let state = record.get("status").and_then(Value::as_str).unwrap_or("running");
            json!({
                "commandId": record["commandId"], "state": state,
                "completionConfirmed": record["completionConfirmed"], "exitCode": record["exitCode"],
                "canRunCommand": matches!(state, "completed" | "shell_ready") && input_wait_reason.is_none(),
            })
        });
        self.ok("Terminal observed.", output.clone(), json!({
            "buffer": output, "screen": screen,
            "waitingForInput": input_wait_reason.is_some(), "inputWaitReason": input_wait_reason,
            "tuiState": ai_terminal_tui_state(target.terminal_screen.as_ref(), buffer),
            "recentCommands": records, "terminalObservation": terminal_observation,
        }), "read").with_target(target)
    }

    pub fn terminal_command_status<'a>(
        &self,
        target: Option<AiOrchestratorTarget>,
        args: &Value,
        commands: impl IntoIterator<Item = AiTerminalCommand<'a>>,
    ) -> AiActionResultLite {
        let command_id = args.get("command_id").and_then(Value::as_str);
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(5) as usize;
        let records = commands
            .into_iter()
            .filter(|record| command_id.is_none_or(|id| record.id == id))
            .take(if command_id.is_some() { 1 } else { limit })
            .map(|record| record.json())
            .collect::<Vec<_>>();
        if command_id.is_some() && records.is_empty() {
            return self
                .fail(
                    "Terminal command is not tracked.",
                    "terminal_command_not_found",
                    "The command mark is unavailable or has expired from the terminal ledger.",
                    "read",
                )
                .with_optional_target(target);
        }
        self.ok(
            "Terminal command status read.",
            serde_json::to_string_pretty(&records).unwrap_or_default(),
            json!({ "commands": records }),
            "read",
        )
        .with_optional_target(target)
    }
}

#[derive(Clone, Copy)]
pub enum AiTerminalWaitFailure {
    DirectionChanged,
    OwnerClosed,
    OutcomeUnknown,
    TimedOut,
}

pub enum AiTerminalWaitProgress {
    Pending,
    Observed(AiActionResultLite),
    Failed(AiTerminalWaitFailure),
}

/// The host owns wakeups and revalidates the session handle before each sample.
/// This state controls the wait condition, timeout and model-visible result.
pub struct AiTerminalOutputWait<'a> {
    args: &'a Value,
    initial_buffer: Zeroizing<String>,
    initial_alternate_screen: bool,
    last: Zeroizing<String>,
    changed_at: Instant,
    deadline: Instant,
    timeout_secs: u64,
    command_wait: bool,
    max_chars: usize,
    connection_lost: bool,
}

pub struct AiTerminalWaitSample<'a> {
    pub buffer: &'a str,
    pub alternate_screen: bool,
    pub waiting_for_secret: bool,
    pub prompt_returned: bool,
}

impl<'a> AiTerminalOutputWait<'a> {
    pub fn new(
        args: &'a Value,
        initial_buffer: String,
        initial_alternate_screen: bool,
        now: Instant,
    ) -> Self {
        let timeout_secs = args
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .unwrap_or(30)
            .clamp(1, 30);
        Self {
            args,
            last: Zeroizing::new(initial_buffer.clone()),
            initial_buffer: Zeroizing::new(initial_buffer),
            initial_alternate_screen,
            changed_at: now,
            deadline: now + Duration::from_secs(timeout_secs),
            timeout_secs,
            command_wait: args.get("condition").and_then(Value::as_str)
                == Some("command_completed"),
            max_chars: args
                .get("max_chars")
                .and_then(Value::as_u64)
                .unwrap_or(4_000) as usize,
            connection_lost: false,
        }
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn connection_lost(&self) -> bool {
        self.connection_lost
    }
    pub fn requested_command(&self) -> Option<&str> {
        self.args.get("command_id").and_then(Value::as_str)
    }

    pub fn recovering(&mut self, now: Instant) -> bool {
        if now >= self.deadline {
            return false;
        }
        self.connection_lost = true;
        true
    }

    pub fn observe(
        &mut self,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        sample: AiTerminalWaitSample<'_>,
        records: &[AiTerminalCommand<'_>],
        now: Instant,
    ) -> AiTerminalWaitProgress {
        let requested = self.requested_command();
        let tracked = records.iter().find(|record| Some(record.id) == requested);
        if self.command_wait
            && ((self.connection_lost && tracked.is_none())
                || tracked.is_some_and(|record| record.status == AiTerminalCommandStatus::Stale))
        {
            return AiTerminalWaitProgress::Failed(AiTerminalWaitFailure::OutcomeUnknown);
        }
        self.connection_lost = false;
        if sample.buffer != *self.last {
            self.last = Zeroizing::new(sample.buffer.to_owned());
            self.changed_at = now;
        }
        let matched = ai_terminal_wait_match(
            self.args,
            &self.initial_buffer,
            sample.buffer,
            self.initial_alternate_screen,
            sample.alternate_screen,
            |id| {
                records
                    .iter()
                    .find(|record| {
                        record.id == id && record.status == AiTerminalCommandStatus::Completed
                    })
                    .map(|record| record.json())
            },
        )
        .or_else(|| {
            (self.args.get("condition").and_then(Value::as_str) == Some("prompt")
                && sample.prompt_returned)
                .then(|| json!({"condition":"prompt", "reason":"shell_prompt"}))
        });
        let expired = now >= self.deadline;
        let yield_observation = self.command_wait
            && ai_terminal_command_output_ready(
                false,
                sample.prompt_returned,
                &self.initial_buffer,
                sample.buffer,
                now.saturating_duration_since(self.changed_at),
                sample.waiting_for_secret,
                false,
            );
        if expired && !self.command_wait && matched.is_none() {
            return AiTerminalWaitProgress::Failed(AiTerminalWaitFailure::TimedOut);
        }
        if matched.is_none() && !yield_observation && !expired {
            return AiTerminalWaitProgress::Pending;
        }
        let output = trim_tail_chars(sample.buffer, self.max_chars);
        let input_reason = ai_terminal_input_wait_reason(sample.buffer, sample.waiting_for_secret);
        let command_id = self
            .requested_command()
            .or_else(|| records.last().map(|record| record.id));
        let record = records.iter().find(|record| Some(record.id) == command_id);
        let completed =
            record.is_some_and(|record| record.status == AiTerminalCommandStatus::Completed);
        let mut data = ai_terminal_observation_data(
            command_id,
            completed,
            sample.prompt_returned,
            input_reason,
            record.and_then(|record| record.exit_code),
            expired,
        );
        data["conditionSatisfied"] = json!(matched.is_some());
        data["matched"] = json!(matched);
        data["terminalObservation"]["conditionSatisfied"] = data["conditionSatisfied"].clone();
        data["buffer"] = json!(output);
        data["tuiState"] = json!(if sample.alternate_screen {
            "alternate_screen"
        } else if input_reason.is_some() {
            "prompt"
        } else {
            "shell"
        });
        AiTerminalWaitProgress::Observed(snapshot.ok(
            if matched.is_some() {
                "Terminal wait condition satisfied."
            } else {
                "Terminal observation returned; command completion is not confirmed."
            },
            output,
            data,
            "read",
        ))
    }

    pub fn failure(
        &self,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        failure: AiTerminalWaitFailure,
    ) -> AiActionResultLite {
        let code = match failure {
            AiTerminalWaitFailure::DirectionChanged => "agent_direction_changed",
            AiTerminalWaitFailure::OwnerClosed => "runtime_owner_closed",
            AiTerminalWaitFailure::OutcomeUnknown => "command_outcome_unknown",
            AiTerminalWaitFailure::TimedOut => "terminal_wait_timeout",
        };
        snapshot.fail(
            "Terminal wait timed out.",
            code,
            format!(
                "The requested terminal condition was not observed within {} seconds.",
                self.timeout_secs
            ),
            "read",
        )
    }
}

pub enum AiTerminalRunEnd {
    OutcomeUnknown,
    DirectionChanged,
    TakenOver,
    OwnerClosed,
    Unconfirmed,
}

impl AiOrchestratorRuntimeSnapshot {
    pub fn terminal_run_observed(
        &self,
        command_id: Option<&str>,
        before: &str,
        current: &str,
        completed: bool,
        prompt_returned: bool,
        waiting_for_secret: bool,
        exit_code: Option<i32>,
        expired: bool,
        target: Option<AiOrchestratorTarget>,
    ) -> AiActionResultLite {
        self.ok(
            "Terminal command output captured.",
            terminal_delta_output(before, current),
            ai_terminal_observation_data(
                command_id,
                completed,
                prompt_returned,
                ai_terminal_input_wait_reason(current, waiting_for_secret),
                exit_code,
                expired,
            ),
            "interactive",
        )
        .with_optional_target(target)
    }

    pub fn terminal_run_finished(
        &self,
        tool_call_id: String,
        tool_name: String,
        duration_ms: u128,
        end: AiTerminalRunEnd,
        before: &str,
        last: &str,
        command_id: Option<String>,
        last_waiting_for_secret: bool,
        target: Option<AiOrchestratorTarget>,
        post_user_approval: bool,
    ) -> crate::AiExecutedToolResult {
        let output = terminal_delta_output(before, last);
        let output_empty = output.trim().is_empty();
        if matches!(end, AiTerminalRunEnd::OutcomeUnknown) {
            return self.to_executed_tool_result(
                tool_call_id,
                tool_name,
                self.fail(
                    "Command outcome is unknown after a connection change.",
                    "command_outcome_unknown",
                    "Inspect the current system state before repeating this command.",
                    "interactive",
                ),
                duration_ms,
            );
        }
        if matches!(end, AiTerminalRunEnd::DirectionChanged) {
            return self.to_executed_tool_result(tool_call_id, tool_name,
                        self.ok("Command observation yielded to new instructions.", output,
                            serde_json::json!({"executionState":"running", "commandId":command_id, "outcomeUnknown":true}), "interactive"),
                        duration_ms);
        }
        if matches!(end, AiTerminalRunEnd::TakenOver) {
            return rejected_ai_tool_result(
                tool_call_id,
                tool_name,
                "terminal_taken_over",
                "The terminal was taken over or disconnected. Output attribution is no longer reliable; the remote command's termination is not confirmed.",
            );
        }
        if matches!(end, AiTerminalRunEnd::OwnerClosed) {
            return self.to_executed_tool_result(
                tool_call_id,
                tool_name,
                self.fail(
                    "Runtime terminal changed while waiting for output.",
                    if post_user_approval {
                        "runtime_state_changed_after_approval"
                    } else {
                        "runtime_owner_closed"
                    },
                    ai_runtime_validation_recovery_message(post_user_approval),
                    "interactive",
                )
                .with_optional_target(target),
                duration_ms,
            );
        }
        self.to_executed_tool_result(
            tool_call_id,
            tool_name,
            AiActionResultLite {
                images: Vec::new(),
                ok: !output_empty,
                summary: "Terminal command did not produce completed output.".to_string(),
                output: if output_empty {
                    "No new output captured.".to_string()
                } else {
                    output
                },
                data: serde_json::json!({
                    "executionState": if output_empty { "timeout" } else { "output_captured" },
                    "visibleInTerminal": true,
                    "waitingForInput": last_waiting_for_secret,
                    "inputWaitReason": last_waiting_for_secret.then_some("credential_prompt"),
                    "commandId": command_id,
                }),
                error_code: output_empty.then(|| "terminal_command_wait_timeout".to_string()),
                error_message: output_empty.then(|| {
                    "No new output was captured before the command wait timed out.".to_string()
                }),
                risk: "interactive",
                target,
                targets: Vec::new(),
                next_actions: Vec::new(),
                observations: Vec::new(),
                verified: None,
                state_version: None,
            },
            duration_ms,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_wait_matches_only_the_requested_completed_command() {
        let snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
        let records = [AiTerminalCommand {
            id: "command-1",
            command: "printf done",
            status: AiTerminalCommandStatus::Completed,
            started_at: 10,
            finished_at: Some(20),
            exit_code: Some(0),
        }];
        for (requested, matches) in [("command-1", true), ("command-2", false)] {
            let args = json!({ "condition": "command_completed", "command_id": requested });
            let now = Instant::now();
            let mut wait = AiTerminalOutputWait::new(&args, "".into(), false, now);
            let progress = wait.observe(
                &snapshot,
                AiTerminalWaitSample {
                    buffer: "done",
                    alternate_screen: false,
                    waiting_for_secret: false,
                    prompt_returned: false,
                },
                &records,
                now,
            );
            if matches {
                let AiTerminalWaitProgress::Observed(result) = progress else {
                    panic!("expected completed command");
                };
                assert_eq!(
                    result.data.pointer("/matched/command/exitCode"),
                    Some(&json!(0))
                );
                assert_eq!(
                    result.data.pointer("/terminalObservation/commandId"),
                    Some(&json!(requested))
                );
            } else {
                assert!(matches!(progress, AiTerminalWaitProgress::Pending));
            }
        }
    }
}

pub struct AiTerminalCommandPlan<'a> {
    pub text: zeroize::Zeroizing<String>,
    pub cwd: Option<&'a str>,
    pub timeout: Duration,
    pub await_output: bool,
}

impl AiTerminalCommandPlan<'_> {
    pub fn with_terminal_cwd(&self) -> zeroize::Zeroizing<String> {
        zeroize::Zeroizing::new(super::ai_command_with_cwd(&self.text, self.cwd))
    }
}

impl AiOrchestratorRuntimeSnapshot {
    pub fn plan_terminal_command<'a>(
        &self,
        args: &'a Value,
    ) -> Result<AiTerminalCommandPlan<'a>, AiActionResultLite> {
        let text = args
            .get("command")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| {
                self.fail(
                    "Command is required.",
                    "missing_command",
                    "run_command requires a command.",
                    super::ai_run_command_preflight_risk(),
                )
            })?;
        Ok(AiTerminalCommandPlan {
            text: zeroize::Zeroizing::new(text.to_owned()),
            cwd: args.get("cwd").and_then(Value::as_str),
            timeout: Duration::from_secs(
                args.get("timeout_secs")
                    .and_then(Value::as_u64)
                    .unwrap_or(1800)
                    .clamp(1, 1800),
            ),
            await_output: args.get("await_output").and_then(Value::as_bool) != Some(false),
        })
    }

    pub fn terminal_command_dispatched(
        &self,
        id: Option<String>,
        target: Option<AiOrchestratorTarget>,
    ) -> AiActionResultLite {
        self.ok(
            "Command sent to terminal.",
            "Command sent to the visible terminal.",
            serde_json::json!({"executionState":"sent","visibleInTerminal":true,"commandId":id}),
            "interactive",
        )
        .with_optional_target(target)
    }

    pub fn terminal_owner_closed(&self, risk: &'static str) -> AiActionResultLite {
        self.fail(
            "Runtime terminal is unavailable.",
            "runtime_owner_closed",
            "The terminal pane is no longer registered.",
            risk,
        )
    }

    pub fn prepare_terminal_input(
        &self,
        args: &Value,
    ) -> Result<zeroize::Zeroizing<Vec<u8>>, AiActionResultLite> {
        // Input may contain passwords even when it is not marked as a secret.
        let payload =
            zeroize::Zeroizing::new(crate::terminal_observation::ai_terminal_input_payload(args));
        if payload.is_empty() {
            return Err(self.fail(
                "No terminal input specified.",
                "missing_terminal_input",
                "Provide text or request Enter with append_enter.",
                "interactive",
            ));
        }
        Ok(payload)
    }

    pub fn terminal_input_sent(&self) -> AiActionResultLite {
        self.ok(
            "Terminal input sent.",
            "Input sent.",
            Value::Null,
            "interactive",
        )
    }
}
