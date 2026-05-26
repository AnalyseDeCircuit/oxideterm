// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

pub const QUICK_COMMANDS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickCommandIcon {
    Terminal,
    Server,
    Folder,
    Docker,
    Zap,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommandCategory {
    pub id: String,
    pub name: String,
    pub icon: QuickCommandIcon,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommand {
    pub id: String,
    pub name: String,
    pub command: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_pattern: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommandsSnapshot {
    pub version: u32,
    pub categories: Vec<QuickCommandCategory>,
    pub commands: Vec<QuickCommand>,
    pub updated_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickCommandImportStrategy {
    Rename,
    Skip,
    Replace,
    Merge,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QuickCommandImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}
