// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

const MAX_TASK_SUMMARY_CHARS: usize = 4_096;
const MAX_TASK_TITLE_CHARS: usize = 160;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BackgroundTaskId(String);

impl BackgroundTaskId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for BackgroundTaskId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTaskOwner {
    pub conversation_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskCondition {
    ResultChanged,
    ResultContains {
        text: String,
    },
    ResultFieldEquals {
        pointer: String,
        expected: serde_json::Value,
    },
    ExecutionFails,
    ExecutionRecovers,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackgroundTaskMode {
    OneShot,
    Interval {
        interval_seconds: u64,
        max_runs: u32,
    },
    Condition {
        interval_seconds: u64,
        max_runs: u32,
        condition: BackgroundTaskCondition,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskState {
    Queued,
    Running,
    Waiting,
    Completed,
    Failed,
    Cancelled,
}

/// Holds execution arguments in a zeroizing buffer and is intentionally not serializable.
pub struct BackgroundTaskSpec {
    pub owner: BackgroundTaskOwner,
    pub title: String,
    pub tool_name: String,
    pub arguments_json: Zeroizing<String>,
    pub mode: BackgroundTaskMode,
}

impl fmt::Debug for BackgroundTaskSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BackgroundTaskSpec")
            .field("owner", &self.owner)
            .field("title", &self.title)
            .field("tool_name", &self.tool_name)
            .field("arguments_json", &"<redacted>")
            .field("mode", &self.mode)
            .finish()
    }
}

impl BackgroundTaskSpec {
    pub fn normalized_title(&self) -> String {
        let title = self.title.trim();
        let title = if title.is_empty() {
            self.tool_name.as_str()
        } else {
            title
        };
        title.chars().take(MAX_TASK_TITLE_CHARS).collect()
    }
}

/// Represents one executor handoff. The argument copy is cleared after the call completes.
pub struct BackgroundTaskExecution {
    pub task_id: BackgroundTaskId,
    pub tool_name: String,
    pub arguments_json: Zeroizing<String>,
    pub run_number: u32,
}

impl fmt::Debug for BackgroundTaskExecution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BackgroundTaskExecution")
            .field("task_id", &self.task_id)
            .field("tool_name", &self.tool_name)
            .field("arguments_json", &"<redacted>")
            .field("run_number", &self.run_number)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackgroundTaskExecutionResult {
    pub summary: String,
    pub fingerprint: String,
    pub condition_value: Option<serde_json::Value>,
}

impl BackgroundTaskExecutionResult {
    pub fn sanitized(
        summary: impl Into<String>,
        fingerprint: impl Into<String>,
        condition_value: Option<serde_json::Value>,
    ) -> Self {
        let mut summary = summary.into();
        if summary.chars().count() > MAX_TASK_SUMMARY_CHARS {
            summary = summary.chars().take(MAX_TASK_SUMMARY_CHARS).collect();
        }
        Self {
            summary,
            fingerprint: fingerprint.into(),
            condition_value,
        }
    }
}

impl Drop for BackgroundTaskExecutionResult {
    fn drop(&mut self) {
        // Summaries are sanitized before construction, but clearing them still limits lifetime.
        self.summary.zeroize();
        self.fingerprint.zeroize();
        if let Some(value) = self.condition_value.as_mut() {
            zeroize_json(value);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTaskSnapshot {
    pub id: BackgroundTaskId,
    pub owner: BackgroundTaskOwner,
    pub title: String,
    pub tool_name: String,
    pub mode: BackgroundTaskMode,
    pub state: BackgroundTaskState,
    pub run_count: u32,
    pub last_summary: Option<String>,
    pub last_error_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackgroundTaskEvent {
    Changed(BackgroundTaskSnapshot),
    Removed(BackgroundTaskId),
}

fn zeroize_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(value) => value.zeroize(),
        serde_json::Value::Array(values) => values.iter_mut().for_each(zeroize_json),
        serde_json::Value::Object(values) => values.values_mut().for_each(zeroize_json),
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}
