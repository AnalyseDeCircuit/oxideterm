use oxideterm_ai::terminal_observation::{
    ai_terminal_input_payload,
    terminal_delta_output,
};

pub(in crate::workspace) use oxideterm_ai::tool_result_protocol::{
    rejected_ai_tool_result,
};
use oxideterm_ai::tool_resources::{AiTerminalCommand, AiTerminalCommandStatus};

fn ai_terminal_command_capture(record: &oxideterm_gpui_terminal::TerminalAiCommandRecord, prompt_returned: bool) -> AiTerminalCommand<'_> {
    use oxideterm_gpui_terminal::TerminalCommandFactStatus;
    let status = match record.status {
        TerminalCommandFactStatus::Open if prompt_returned => AiTerminalCommandStatus::ShellReady,
        TerminalCommandFactStatus::Open => AiTerminalCommandStatus::Running,
        TerminalCommandFactStatus::Closed => AiTerminalCommandStatus::Completed,
        TerminalCommandFactStatus::Stale => AiTerminalCommandStatus::Stale,
    };
    AiTerminalCommand { id: &record.command_id, command: &record.command, status,
        started_at: record.started_at, finished_at: record.finished_at, exit_code: record.exit_code }
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
