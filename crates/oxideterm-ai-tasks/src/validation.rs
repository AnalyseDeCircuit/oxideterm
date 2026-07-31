// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde_json::Value;
use thiserror::Error;

use crate::{BackgroundTaskMode, BackgroundTaskSpec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackgroundTaskLimits {
    pub minimum_interval_seconds: u64,
    pub maximum_interval_seconds: u64,
    pub maximum_runs: u32,
    pub maximum_argument_chars: usize,
}

impl Default for BackgroundTaskLimits {
    fn default() -> Self {
        Self {
            minimum_interval_seconds: 5,
            maximum_interval_seconds: 86_400,
            maximum_runs: 10_000,
            maximum_argument_chars: 128 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BackgroundTaskValidationError {
    #[error("invalid background task")]
    Invalid,
    #[error("background task interval is outside the allowed range")]
    InvalidInterval,
    #[error("background task run limit is outside the allowed range")]
    InvalidRunLimit,
    #[error("background task arguments are too large")]
    ArgumentsTooLarge,
    #[error("background tasks cannot retain live capability handles")]
    LiveHandleNotAllowed,
    #[error("this conversation already owns the maximum number of active background tasks")]
    TooManyActiveTasks,
    #[error("the background task history has reached its bounded capacity")]
    TaskCapacityReached,
}

pub fn validate_background_task_spec(
    spec: &BackgroundTaskSpec,
    limits: BackgroundTaskLimits,
) -> Result<(), BackgroundTaskValidationError> {
    if spec.owner.conversation_id.trim().is_empty()
        || spec.tool_name.trim().is_empty()
        || spec.arguments_json.chars().count() > limits.maximum_argument_chars
    {
        return if spec.arguments_json.chars().count() > limits.maximum_argument_chars {
            Err(BackgroundTaskValidationError::ArgumentsTooLarge)
        } else {
            Err(BackgroundTaskValidationError::Invalid)
        };
    }

    let arguments: Value = serde_json::from_str(spec.arguments_json.as_str())
        .map_err(|_| BackgroundTaskValidationError::Invalid)?;
    if contains_live_handle(&arguments) {
        return Err(BackgroundTaskValidationError::LiveHandleNotAllowed);
    }

    match spec.mode {
        BackgroundTaskMode::OneShot => Ok(()),
        BackgroundTaskMode::Interval {
            interval_seconds,
            max_runs,
        }
        | BackgroundTaskMode::Condition {
            interval_seconds,
            max_runs,
            ..
        } => {
            if !(limits.minimum_interval_seconds..=limits.maximum_interval_seconds)
                .contains(&interval_seconds)
            {
                return Err(BackgroundTaskValidationError::InvalidInterval);
            }
            if max_runs == 0 || max_runs > limits.maximum_runs {
                return Err(BackgroundTaskValidationError::InvalidRunLimit);
            }
            Ok(())
        }
    }
}

fn contains_live_handle(value: &Value) -> bool {
    match value {
        Value::Object(values) => values.iter().any(|(key, value)| {
            matches!(
                key.as_str(),
                "handle_id" | "handleId" | "session_id" | "sessionId"
            ) || contains_live_handle(value)
        }),
        Value::Array(values) => values.iter().any(contains_live_handle),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::*;
    use crate::{BackgroundTaskCondition, BackgroundTaskOwner};

    fn spec(arguments: &str, mode: BackgroundTaskMode) -> BackgroundTaskSpec {
        BackgroundTaskSpec {
            owner: BackgroundTaskOwner {
                conversation_id: "conversation".to_string(),
            },
            title: "Monitor".to_string(),
            tool_name: "inspect_host_tools".to_string(),
            arguments_json: Zeroizing::new(arguments.to_string()),
            mode,
        }
    }

    #[test]
    fn recurring_tasks_reject_live_handles() {
        let task = spec(
            r#"{"handle_id":"turn-scoped"}"#,
            BackgroundTaskMode::Interval {
                interval_seconds: 30,
                max_runs: 10,
            },
        );

        assert_eq!(
            validate_background_task_spec(&task, BackgroundTaskLimits::default()),
            Err(BackgroundTaskValidationError::LiveHandleNotAllowed)
        );
    }

    #[test]
    fn condition_tasks_accept_stable_resource_references() {
        let task = spec(
            r#"{"resource_ref":{"kind":"saved_connection","id":"stable"}}"#,
            BackgroundTaskMode::Condition {
                interval_seconds: 30,
                max_runs: 10,
                condition: BackgroundTaskCondition::ResultChanged,
            },
        );

        assert_eq!(
            validate_background_task_spec(&task, BackgroundTaskLimits::default()),
            Ok(())
        );
    }

    #[test]
    fn busy_polling_is_rejected() {
        let task = spec(
            "{}",
            BackgroundTaskMode::Interval {
                interval_seconds: 1,
                max_runs: 10,
            },
        );

        assert_eq!(
            validate_background_task_spec(&task, BackgroundTaskLimits::default()),
            Err(BackgroundTaskValidationError::InvalidInterval)
        );
    }
}
