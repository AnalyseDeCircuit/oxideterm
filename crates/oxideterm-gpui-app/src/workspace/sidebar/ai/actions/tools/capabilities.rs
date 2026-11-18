use oxideterm_ai::tool_resources::{
    AiActionResultLite, AiStableResourceOperation, ai_live_resource_operation,
    ai_run_command_preflight_risk,
    ai_stable_resource_operation,
};

#[cfg(windows)]
pub(in crate::workspace) const AI_LOCAL_COMMAND_CREATE_NO_WINDOW: u32 = 0x08000000;

pub(in crate::workspace) async fn run_local_ai_command(
    command: &str,
    cwd: Option<&str>,
    dangerous_command_approved: bool,
    resource: Option<oxideterm_ai::agent::AgentResourceRecord>,
) -> AiActionResultLite {
    if oxideterm_ai::has_denied_commands(
        "run_command",
        Some(&serde_json::json!({ "command": command })),
    ) && !dangerous_command_approved
    {
        return AiActionResultLite {
            images: Vec::new(),
            ok: false,
            summary: "Local command failed.".to_string(),
            output: "Command denied for security reasons".to_string(),
            data: serde_json::json!({"executionState": "not_started"}),
            error_code: Some("local_command_error".to_string()),
            error_message: Some("Command denied for security reasons".to_string()),
            risk: "execute",
            target: None,
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        };
    }
    let mut process = tokio::process::Command::new(if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "sh"
    });
    // Dropping a cancelled tool future must also terminate its local child.
    process.kill_on_drop(true);
    configure_ai_local_command_process(&mut process);
    if cfg!(target_os = "windows") {
        process.arg("/C").arg(command);
    } else {
        process.arg("-c").arg(command);
    }
    if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
        let path = std::path::Path::new(cwd);
        if !path.exists() {
            return AiActionResultLite {
                images: Vec::new(),
                ok: false,
                summary: "Local command failed.".to_string(),
                output: format!("Working directory does not exist: {cwd}"),
                data: serde_json::json!({"executionState": "not_started"}),
                error_code: Some("local_command_error".to_string()),
                error_message: Some("Working directory does not exist.".to_string()),
                risk: "execute",
                target: None,
                targets: Vec::new(),
                next_actions: Vec::new(),
                observations: Vec::new(),
                verified: None,
                state_version: None,
            };
        }
        process.current_dir(path);
    }
    let evidence_record = resource.clone();
    let mut dispatched = false;
    let result = match async {
        let process = oxideterm_ai::agent::AgentProcess::spawn(&mut process)?.track(resource);
        dispatched = true;
        process.output().await
    }
    .await
    {
        Ok(output) => {
            let stdout_bytes = zeroize::Zeroizing::new(output.stdout);
            let stderr_bytes = zeroize::Zeroizing::new(output.stderr);
            let stdout = truncate_ai_local_exec_output(&String::from_utf8_lossy(&stdout_bytes));
            let stderr = truncate_ai_local_exec_output(&String::from_utf8_lossy(&stderr_bytes));
            let exit_code = output.status.code();
            let has_output = !stdout.trim().is_empty() || !stderr.trim().is_empty();
            let ok = output.status.success() || (exit_code.is_none() && has_output);
            let body = [
                stdout,
                (!stderr.trim().is_empty())
                    .then(|| format!("[stderr]\n{stderr}"))
                    .unwrap_or_default(),
                format!(
                    "[exit_code: {}]",
                    exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
            AiActionResultLite {
                images: Vec::new(),
                ok,
                summary: if output.status.success() {
                    "Local command completed.".to_string()
                } else if exit_code.is_none() && has_output {
                    "Local command output captured; exit code was not reported.".to_string()
                } else {
                    format!(
                        "Local command exited with {}.",
                        exit_code
                            .map(|code| code.to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    )
                },
                output: body,
                data: serde_json::json!({
                    "exitCode": exit_code,
                    "timedOut": false,
                    "executionState": "completed",
                    "visibleInTerminal": false,
                }),
                error_code: (!ok).then(|| "local_command_failed".to_string()),
                error_message: (!ok).then(|| "The local command failed.".to_string()),
                risk: "execute",
                target: None,
                targets: Vec::new(),
                next_actions: Vec::new(),
                observations: (exit_code.is_none() && has_output)
                    .then(|| "The local command produced output, but the backend did not report an exit code.".to_string())
                    .into_iter()
                    .collect(),
                verified: None,
                state_version: None,
            }
        }
        Err(error) => AiActionResultLite {
            images: Vec::new(),
            ok: false,
            summary: "Local command failed.".to_string(),
            output: error.to_string(),
            data: serde_json::json!({"executionState": if dispatched { "unknown" } else { "not_started" }}),
            error_code: Some("local_command_error".to_string()),
            error_message: Some("The local command could not be started.".to_string()),
            risk: "execute",
            target: None,
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        },
    };
    if let Some(record) = evidence_record {
        record.outcome(&result.output);
    }
    result
}

pub(in crate::workspace) fn configure_ai_local_command_process(
    process: &mut tokio::process::Command,
) {
    #[cfg(windows)]
    {
        // AI local commands capture stdout/stderr in the app. Hide the bridge
        // shell so cmd.exe, pwsh.exe, and child console programs do not flash.
        process.creation_flags(AI_LOCAL_COMMAND_CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = process;
    }
}

pub(in crate::workspace) fn truncate_ai_local_exec_output(value: &str) -> String {
    const MAX_BYTES: usize = 64 * 1024;
    if value.len() <= MAX_BYTES {
        return value.to_string();
    }
    // Tauri truncates local command output at a valid UTF-8 boundary before
    // the AI tool envelope applies its model-facing preview limits.
    let mut end = MAX_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...(truncated)", &value[..end])
}

pub(in crate::workspace) fn ai_shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(in crate::workspace) fn ai_command_with_cwd(command: &str, cwd: Option<&str>) -> String {
    match cwd.filter(|value| !value.trim().is_empty()) {
        Some("~") => format!("cd ~ && {command}"),
        Some(cwd) => {
            let target = cwd
                .strip_prefix("~/")
                .filter(|rest| !rest.is_empty())
                .map(|rest| format!("~/{}", ai_shell_single_quote(rest)))
                .unwrap_or_else(|| ai_shell_single_quote(cwd));
            format!("cd {target} && {command}")
        }
        None => command.to_string(),
    }
}

fn ai_transfers_state(manager: &SftpTransferManager) -> serde_json::Value {
    use oxideterm_ai::tool_resources::{AiTransferCapture, AiTransferState};
    let transfers = manager.list_background_transfers(None);
    let facts = transfers.iter().map(|transfer| AiTransferCapture {
        id: &transfer.id,
        direction: match transfer.direction {
            BackgroundTransferDirection::Upload => "upload",
            BackgroundTransferDirection::Download => "download",
        },
        state: match transfer.state {
            BackgroundTransferState::Pending => AiTransferState::Pending,
            BackgroundTransferState::Active => AiTransferState::Active,
            BackgroundTransferState::Paused => AiTransferState::Paused,
            BackgroundTransferState::Completed => AiTransferState::Completed,
            BackgroundTransferState::Cancelled => AiTransferState::Cancelled,
            BackgroundTransferState::Error => AiTransferState::Error,
        },
        size: transfer.size, transferred: transfer.transferred, has_error: transfer.error.is_some(),
        start_time: transfer.start_time, end_time: transfer.end_time,
    }).collect::<Vec<_>>();
    oxideterm_ai::tool_resources::ai_transfers_state(&facts)
}
