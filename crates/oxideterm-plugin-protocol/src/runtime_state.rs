// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

use crate::message::PluginRuntimeLogLevel;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginRuntimeLifecycleState {
    Inactive,
    Activating,
    Active,
    Deactivating,
    Error,
    AutoDisabled,
    Killed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRuntimeHealth {
    pub state: PluginRuntimeLifecycleState,
    pub healthy: bool,
    pub error_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRuntimeLogEntry {
    pub level: PluginRuntimeLogLevel,
    pub message: String,
}
