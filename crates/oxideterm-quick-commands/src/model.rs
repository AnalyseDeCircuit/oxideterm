// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

pub const QUICK_COMMANDS_SCHEMA_VERSION: u32 = 2;

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
    pub sort_order: i64,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickCommandParameterKind {
    #[default]
    Text,
    Choice,
    Secret,
}

#[derive(Clone, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommandParameter {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub kind: QuickCommandParameterKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickCommandTargetProtocol {
    Local,
    Ssh,
    Mosh,
    Telnet,
    Serial,
    Tmux,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommandAvailability {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<QuickCommandTargetProtocol>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_patterns: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickCommandConfirmationPolicy {
    #[default]
    Inherit,
    Always,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommand {
    pub id: String,
    pub name: String,
    #[serde(rename = "body")]
    pub command: String,
    #[serde(rename = "collectionId")]
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<QuickCommandParameter>,
    #[serde(default)]
    pub availability: QuickCommandAvailability,
    #[serde(default)]
    pub confirmation: QuickCommandConfirmationPolicy,
    pub sort_order: i64,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommandsSnapshot {
    pub version: u32,
    #[serde(rename = "collections")]
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
