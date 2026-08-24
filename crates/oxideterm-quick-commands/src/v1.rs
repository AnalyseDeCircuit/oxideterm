// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Published schema version one. These DTOs remain private and read-only.

use serde::Deserialize;

use crate::QuickCommandIcon;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuickCommandsSnapshotV1 {
    pub version: u32,
    pub categories: Vec<QuickCommandCategoryV1>,
    pub commands: Vec<QuickCommandV1>,
    pub updated_at: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuickCommandCategoryV1 {
    pub id: String,
    pub name: String,
    pub icon: QuickCommandIcon,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuickCommandV1 {
    pub id: String,
    pub name: String,
    pub command: String,
    pub category: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub host_pattern: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}
