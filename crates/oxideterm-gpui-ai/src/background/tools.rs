use super::AiBackgroundTaskEntity;
use oxideterm_ai::tool_resources::{
    AiActionResultLite, AiApplicationTool, AiOrchestratorRuntimeSnapshot,
};
use oxideterm_ai_tasks::{BackgroundTaskExecution, BackgroundTaskExecutionResult};
use serde_json::Value;

pub enum AiBackgroundRead {
    State,
    StableResource,
    HostTools,
    Forwards,
    Plugins,
}

pub struct AiBackgroundReadPlan {
    tool_name: String,
    args: Value,
}

impl AiBackgroundReadPlan {
    pub fn from_execution(execution: BackgroundTaskExecution) -> Result<Self, String> {
        let args = serde_json::from_str::<Value>(execution.arguments_json.as_str())
            .map_err(|_| "background_task_arguments_invalid".to_string())?;
        let args =
            oxideterm_ai::canonicalize_orchestrator_tool_arguments(&execution.tool_name, args)
                .map_err(|_| "background_task_arguments_invalid".to_string())?;
        Ok(Self {
            tool_name: execution.tool_name,
            args,
        })
    }

    pub fn execute(
        self,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        read: impl FnOnce(
            AiBackgroundRead,
            &Value,
            &AiOrchestratorRuntimeSnapshot,
        ) -> AiActionResultLite,
    ) -> Result<BackgroundTaskExecutionResult, String> {
        let args = &self.args;
        let result = match self.tool_name.as_str() {
            "list_targets" => snapshot.list_targets(args),
            "get_state" => read(AiBackgroundRead::State, args, snapshot),
            "read_resource" => read(AiBackgroundRead::StableResource, args, snapshot),
            "inspect_host_tools" => read(AiBackgroundRead::HostTools, args, snapshot),
            "list_forwards" => read(AiBackgroundRead::Forwards, args, snapshot),
            "list_plugins" => read(AiBackgroundRead::Plugins, args, snapshot),
            _ => return Err("background_task_tool_not_allowed".to_string()),
        };
        if !result.ok {
            return Err(result
                .error_code
                .unwrap_or_else(|| "background_task_execution_failed".to_string()));
        }
        let data = oxideterm_ai::sanitize_json_for_ai(&result.data);
        let fingerprint = oxideterm_ai_tasks::fingerprint_json(&data);
        Ok(BackgroundTaskExecutionResult::sanitized(
            result.summary,
            fingerprint,
            Some(data),
        ))
    }
}

impl AiBackgroundRead {
    pub fn result(
        self,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        result: Result<Value, String>,
    ) -> AiActionResultLite {
        match self {
            Self::HostTools => AiApplicationTool::InspectHostTools.result(snapshot, result),
            Self::Forwards => AiApplicationTool::ListForwards.result(snapshot, result),
            Self::Plugins => AiApplicationTool::ListPlugins.result(snapshot, result),
            _ => unreachable!("structured reads return their own result"),
        }
    }
}

impl AiBackgroundTaskEntity {
    pub fn execute_ai_create_background_task(
        &self,
        conversation_id: &str,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let safe_arguments = oxideterm_ai::sanitize_json_for_ai(&arguments["arguments"]);
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
        let task_id = self.create(spec).map_err(|error| error.to_string())?;
        Ok(serde_json::json!({
            "accepted": true,
            "taskId": task_id,
        }))
    }

    pub fn execute_ai_list_background_tasks(&self, conversation_id: &str) -> serde_json::Value {
        serde_json::json!({
            "tasks": self
                .snapshots_for_owner(conversation_id),
        })
    }

    pub fn execute_ai_get_background_task(
        &self,
        conversation_id: &str,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let task_id = ai_background_task_id(arguments)?;
        self.snapshot_for_owner(conversation_id, &task_id)
            .map(|task| serde_json::json!({ "task": task }))
            .ok_or_else(|| "The background task does not exist in this conversation.".to_string())
    }

    pub fn execute_ai_cancel_background_task(
        &self,
        conversation_id: &str,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let task_id = ai_background_task_id(arguments)?;
        self.cancel_for_owner(conversation_id, &task_id)
            .then(|| serde_json::json!({ "cancelled": true, "taskId": task_id }))
            .ok_or_else(|| "The background task is not running in this conversation.".to_string())
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

pub struct AiBackgroundNotice {
    pub failed: bool,
    pub title: String,
    pub body: String,
    pub key: String,
}

impl super::AiBackgroundTaskEntity {
    pub fn drain_tool_events(
        &self,
        i18n: &oxideterm_i18n::I18n,
    ) -> (
        Vec<AiBackgroundNotice>,
        Vec<super::AiBackgroundExecutionRequest>,
    ) {
        use oxideterm_ai_tasks::{BackgroundTaskEvent, BackgroundTaskState};
        let events = self.take_events();
        let requests = self.take_execution_requests();
        let notices = events
            .into_iter()
            .filter_map(|event| {
                let BackgroundTaskEvent::Changed(snapshot) = event else {
                    return None;
                };
                let (failed, key) = match snapshot.state {
                    BackgroundTaskState::Completed => (false, "ai.background_tasks.completed"),
                    BackgroundTaskState::Failed => (true, "ai.background_tasks.failed"),
                    _ => return None,
                };
                Some(AiBackgroundNotice {
                    failed,
                    title: format!("{} · {}", i18n.t(key), snapshot.title),
                    body: i18n
                        .t("ai.background_tasks.notification_body")
                        .replace("{{count}}", &snapshot.run_count.to_string()),
                    key: format!("ai-background-task:{}", snapshot.id.as_str()),
                })
            })
            .collect();
        (notices, requests)
    }
}
