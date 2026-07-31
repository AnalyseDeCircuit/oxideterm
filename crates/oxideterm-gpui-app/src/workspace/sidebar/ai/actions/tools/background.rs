// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

impl WorkspaceApp {
    pub(in crate::workspace) fn execute_ai_create_background_task(
        &self,
        conversation_id: &str,
        arguments: &serde_json::Value,
        cx: &App,
    ) -> Result<serde_json::Value, String> {
        let safe_arguments =
            oxideterm_ai::sanitize_json_for_ai(&arguments["arguments"]);
        let mode = ai_background_task_mode(arguments)?;
        let spec = oxideterm_ai_tasks::BackgroundTaskSpec {
            owner: oxideterm_ai_tasks::BackgroundTaskOwner {
                conversation_id: conversation_id.to_string(),
            },
            title: arguments
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tool_name: arguments["tool_name"]
                .as_str()
                .ok_or_else(|| "A background task tool is required.".to_string())?
                .to_string(),
            // Long-lived tasks retain only the redacted projection accepted at the AI boundary.
            arguments_json: zeroize::Zeroizing::new(
                serde_json::to_string(&safe_arguments)
                    .map_err(|_| "The background task arguments are invalid.".to_string())?,
            ),
            mode,
        };
        let task_id = self
            .ai_background_tasks
            .read(cx)
            .create(spec)
            .map_err(|error| error.to_string())?;
        Ok(serde_json::json!({
            "accepted": true,
            "taskId": task_id,
        }))
    }

    pub(in crate::workspace) fn execute_ai_list_background_tasks(
        &self,
        conversation_id: &str,
        cx: &App,
    ) -> serde_json::Value {
        serde_json::json!({
            "tasks": self.ai_background_tasks
                .read(cx)
                .snapshots_for_owner(conversation_id),
        })
    }

    pub(in crate::workspace) fn execute_ai_get_background_task(
        &self,
        conversation_id: &str,
        arguments: &serde_json::Value,
        cx: &App,
    ) -> Result<serde_json::Value, String> {
        let task_id = ai_background_task_id(arguments)?;
        self.ai_background_tasks
            .read(cx)
            .snapshot_for_owner(conversation_id, &task_id)
            .map(|task| serde_json::json!({ "task": task }))
            .ok_or_else(|| "The background task does not exist in this conversation.".to_string())
    }

    pub(in crate::workspace) fn execute_ai_cancel_background_task(
        &self,
        conversation_id: &str,
        arguments: &serde_json::Value,
        cx: &App,
    ) -> Result<serde_json::Value, String> {
        let task_id = ai_background_task_id(arguments)?;
        self.ai_background_tasks
            .read(cx)
            .cancel_for_owner(conversation_id, &task_id)
            .then(|| serde_json::json!({ "cancelled": true, "taskId": task_id }))
            .ok_or_else(|| {
                "The background task is not running in this conversation.".to_string()
            })
    }

    pub(in crate::workspace) fn handle_ai_background_task_event(
        &mut self,
        _event: ai_background_tasks::AiBackgroundTaskEvent,
        cx: &mut Context<Self>,
    ) {
        let events = self.ai_background_tasks.read(cx).take_events();
        let requests = self
            .ai_background_tasks
            .read(cx)
            .take_execution_requests();
        for request in requests {
            let result = self.execute_ai_background_read(request.execution, cx);
            let _ = request.response.send(result);
        }
        for event in events {
            let oxideterm_ai_tasks::BackgroundTaskEvent::Changed(snapshot) = event else {
                continue;
            };
            let (severity, title_key) = match snapshot.state {
                oxideterm_ai_tasks::BackgroundTaskState::Completed => (
                    WorkspaceNotificationSeverity::Info,
                    "ai.background_tasks.completed",
                ),
                oxideterm_ai_tasks::BackgroundTaskState::Failed => (
                    WorkspaceNotificationSeverity::Error,
                    "ai.background_tasks.failed",
                ),
                _ => continue,
            };
            let body = self
                .i18n
                .t("ai.background_tasks.notification_body")
                .replace("{{count}}", &snapshot.run_count.to_string());
            self.push_notification_entry(
                WorkspaceNotificationKind::Agent,
                severity,
                format!("{} · {}", self.i18n.t(title_key), snapshot.title),
                Some(body),
                WorkspaceNotificationScope::Global,
                Some(format!("ai-background-task:{}", snapshot.id.as_str())),
            );
        }
        cx.notify();
    }

    pub(in crate::workspace) fn cancel_ai_background_task_from_ui(
        &mut self,
        conversation_id: &str,
        task_id: &oxideterm_ai_tasks::BackgroundTaskId,
        cx: &mut Context<Self>,
    ) {
        self.ai_background_tasks.update(cx, |tasks, _cx| {
            tasks.cancel_for_owner(conversation_id, task_id);
        });
        cx.notify();
    }

    fn execute_ai_background_read(
        &mut self,
        execution: oxideterm_ai_tasks::BackgroundTaskExecution,
        cx: &mut Context<Self>,
    ) -> Result<oxideterm_ai_tasks::BackgroundTaskExecutionResult, String> {
        let arguments = serde_json::from_str::<serde_json::Value>(
            execution.arguments_json.as_str(),
        )
        .map_err(|_| "background_task_arguments_invalid".to_string())?;
        let arguments = oxideterm_ai::canonicalize_orchestrator_tool_arguments(
            &execution.tool_name,
            arguments,
        )
        .map_err(|_| "background_task_arguments_invalid".to_string())?;
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let result = match execution.tool_name.as_str() {
            "list_targets" => snapshot.list_targets(&arguments),
            "get_state" => {
                self.execute_ai_get_state(&ToolSessionId::new(), &arguments, cx)
            }
            "read_resource" => self.execute_ai_read_stable_resource(&arguments, cx),
            "inspect_host_tools" => ai_application_action_result(
                &snapshot,
                self.execute_ai_inspect_host_tools(
                    &ToolSessionId::new(),
                    &arguments,
                    cx,
                ),
                "Host Tools inspection completed.",
                "read",
            ),
            "list_forwards" => ai_application_action_result(
                &snapshot,
                self.execute_ai_list_forwards(
                    &ToolSessionId::new(),
                    &arguments,
                    cx,
                ),
                "Forwarding rules listed.",
                "read",
            ),
            "list_plugins" => {
                let data = self.execute_ai_list_plugins(cx);
                snapshot.ok(
                    "Installed plugins listed.",
                    serde_json::to_string_pretty(&data).unwrap_or_default(),
                    data,
                    "read",
                )
            }
            _ => return Err("background_task_tool_not_allowed".to_string()),
        };
        if !result.ok {
            return Err(result
                .error_code
                .unwrap_or_else(|| "background_task_execution_failed".to_string()));
        }
        let condition_value = oxideterm_ai::sanitize_json_for_ai(&result.data);
        let fingerprint = oxideterm_ai_tasks::fingerprint_json(&condition_value);
        Ok(oxideterm_ai_tasks::BackgroundTaskExecutionResult::sanitized(
            result.summary,
            fingerprint,
            Some(condition_value),
        ))
    }
}

fn ai_background_task_id(
    arguments: &serde_json::Value,
) -> Result<oxideterm_ai_tasks::BackgroundTaskId, String> {
    arguments
        .get("task_id")
        .cloned()
        .and_then(|value| {
            serde_json::from_value::<oxideterm_ai_tasks::BackgroundTaskId>(value).ok()
        })
        .ok_or_else(|| "A valid background task identifier is required.".to_string())
}

fn ai_background_task_mode(
    arguments: &serde_json::Value,
) -> Result<oxideterm_ai_tasks::BackgroundTaskMode, String> {
    let mode = arguments
        .get("mode")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "A background task mode is required.".to_string())?;
    if mode == "one_shot" {
        return Ok(oxideterm_ai_tasks::BackgroundTaskMode::OneShot);
    }
    let interval_seconds = arguments
        .get("interval_seconds")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "A recurring task interval is required.".to_string())?;
    let max_runs = arguments
        .get("max_runs")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| "A recurring task run limit is required.".to_string())?;
    if mode == "interval" {
        return Ok(oxideterm_ai_tasks::BackgroundTaskMode::Interval {
            interval_seconds,
            max_runs,
        });
    }
    let condition = match arguments
        .get("condition")
        .and_then(serde_json::Value::as_str)
    {
        Some("result_changed") => oxideterm_ai_tasks::BackgroundTaskCondition::ResultChanged,
        Some("result_contains") => oxideterm_ai_tasks::BackgroundTaskCondition::ResultContains {
            text: arguments
                .get("condition_text")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "A result condition value is required.".to_string())?
                .to_string(),
        },
        Some("result_field_equals") => {
            oxideterm_ai_tasks::BackgroundTaskCondition::ResultFieldEquals {
                pointer: arguments
                    .get("condition_pointer")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "A result condition pointer is required.".to_string())?
                    .to_string(),
                expected: oxideterm_ai::sanitize_json_for_ai(
                    arguments
                        .get("condition_value")
                        .ok_or_else(|| "A result condition value is required.".to_string())?,
                ),
            }
        }
        Some("execution_fails") => oxideterm_ai_tasks::BackgroundTaskCondition::ExecutionFails,
        Some("execution_recovers") => {
            oxideterm_ai_tasks::BackgroundTaskCondition::ExecutionRecovers
        }
        _ => return Err("The background task condition is unsupported.".to_string()),
    };
    Ok(oxideterm_ai_tasks::BackgroundTaskMode::Condition {
        interval_seconds,
        max_runs,
        condition,
    })
}
