use oxideterm_ai::terminal_observation::{
    AiTerminalCommandWait, ai_terminal_command_output_ready, ai_terminal_input_payload,
    ai_terminal_input_wait_reason, ai_terminal_observation_data, ai_terminal_tui_state,
    ai_terminal_wait_match as match_terminal_wait, annotate_ai_run_command_execution_result,
    looks_waiting_for_input, terminal_delta_output,
};

pub(in crate::workspace) use oxideterm_ai::tool_result_protocol::{
    ai_policy_risk_label, annotate_executed_ai_tool_result_policy, executed_summary,
    pre_execution_rejected_ai_tool_result, rejected_ai_tool_result, unavailable_ai_tool_result,
};
pub(in crate::workspace) fn ai_terminal_command_record_json(
    record: &oxideterm_gpui_terminal::TerminalAiCommandRecord,
    prompt_returned: bool,
) -> serde_json::Value {
    let status = match record.status {
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Open => {
            if prompt_returned {
                "shell_ready"
            } else {
                "running"
            }
        }
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed => "completed",
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Stale => "stale",
    };
    serde_json::json!({
        "commandId": record.command_id,
        "command": oxideterm_ai::sanitize_for_ai(&record.command),
        "status": status,
        "completionConfirmed": record.status == oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed,
        "startedAt": record.started_at,
        "finishedAt": record.finished_at,
        "exitCode": record.exit_code,
    })
}

pub(in crate::workspace) fn ai_terminal_wait_match(
    args: &serde_json::Value,
    initial_buffer: &str,
    current_buffer: &str,
    initial_alternate_screen: bool,
    current_alternate_screen: bool,
    command_records: &[oxideterm_gpui_terminal::TerminalAiCommandRecord],
) -> Option<serde_json::Value> {
    match_terminal_wait(
        args,
        initial_buffer,
        current_buffer,
        initial_alternate_screen,
        current_alternate_screen,
        |command_id| {
            command_records
                .iter()
                .find(|record| {
                    record.command_id == command_id
                        && record.status
                            == oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed
                })
                .map(|record| ai_terminal_command_record_json(record, false))
        },
    )
}

pub(in crate::workspace) fn ai_terminal_screen_snapshot_json(
    snapshot: &oxideterm_terminal::TerminalSnapshot,
    is_alternate_buffer: bool,
) -> serde_json::Value {
    // Keep the payload shape close to Tauri's readScreen result while avoiding
    // renderer-only fields that are not useful to an AI tool.
    serde_json::json!({
        "lines": snapshot
            .lines
            .iter()
            .map(|row| row.text().trim_end().to_string())
            .collect::<Vec<_>>(),
        "cursorX": snapshot.cursor_col + 1,
        "cursorY": snapshot.cursor_row + 1,
        "rows": snapshot.rows,
        "cols": snapshot.cols,
        "isAlternateBuffer": is_alternate_buffer,
        "scrollbackLines": snapshot.scrollback_lines,
        "displayOffset": snapshot.display_offset,
    })
}

pub(in crate::workspace) fn settings_with_json_patch(
    settings: &PersistedSettings,
    section: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<PersistedSettings, String> {
    let mut root = serde_json::to_value(settings).map_err(|error| error.to_string())?;
    let Some(section_value) = root.get_mut(section) else {
        return Err(format!("No settings section named {section}."));
    };
    let Some(section_object) = section_value.as_object_mut() else {
        return Err(format!("Settings section {section} cannot be updated."));
    };
    section_object.insert(key.to_string(), value);
    serde_json::from_value(root).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_wait_matches_only_the_requested_completed_command() {
        let records = vec![oxideterm_gpui_terminal::TerminalAiCommandRecord {
            command_id: "command-1".to_string(),
            command: "printf done".to_string(),
            source: oxideterm_terminal::TerminalCommandMarkDetectionSource::Ai,
            status: oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed,
            started_at: 10,
            finished_at: Some(20),
            exit_code: Some(0),
            start_line: 1,
            end_line: Some(2),
        }];

        let matched = ai_terminal_wait_match(
            &serde_json::json!({
                "condition": "command_completed",
                "command_id": "command-1"
            }),
            "",
            "done",
            false,
            false,
            &records,
        );

        assert_eq!(
            matched
                .as_ref()
                .and_then(|value| value.pointer("/command/exitCode")),
            Some(&serde_json::json!(0))
        );
        assert!(
            ai_terminal_wait_match(
                &serde_json::json!({
                    "condition": "command_completed",
                    "command_id": "command-2"
                }),
                "",
                "done",
                false,
                false,
                &records,
            )
            .is_none()
        );
    }
}
