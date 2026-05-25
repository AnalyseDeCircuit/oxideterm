// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::PluginError;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginResponse {
    pub request_id: String,
    pub result: PluginResponseResult,
}

impl PluginResponse {
    pub fn ok(request_id: impl Into<String>, value: Value) -> Self {
        Self {
            request_id: request_id.into(),
            result: PluginResponseResult::Ok { value },
        }
    }

    pub fn error(request_id: impl Into<String>, error: PluginError) -> Self {
        Self {
            request_id: request_id.into(),
            result: PluginResponseResult::Error { error },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PluginResponseResult {
    Ok { value: Value },
    Error { error: PluginError },
}
