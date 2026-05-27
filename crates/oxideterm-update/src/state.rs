// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeUpdateStage {
    Downloading,
    Verifying,
    Ready,
    Error,
    Cancelled,
}

impl NativeUpdateStage {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Ready | Self::Error | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TauriUpdaterEvent {
    Started,
    Resumed,
    Progress,
    Retrying,
    Verifying,
    Ready,
    Error,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumableUpdateStatus {
    pub task_id: String,
    pub version: String,
    pub attempt: u32,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub resumable: bool,
    pub stage: NativeUpdateStage,
    pub status: NativeUpdateStage,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub timestamp: i64,
    pub retry_delay_ms: Option<u64>,
    pub last_http_status: Option<u16>,
    pub can_resume_after_restart: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedUpdateState {
    pub status: ResumableUpdateStatus,
    pub download_url: String,
    pub signature: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

pub fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
