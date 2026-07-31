// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    Workspace,
    User,
    Plugin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillOrigin {
    AgentStandard,
    OxideTerm,
    ClaudeCompatible,
    CopilotCompatible,
    OpenCodeCompatible,
    Plugin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub compatibility: Option<String>,
    pub scope: SkillScope,
    pub origin: SkillOrigin,
    pub skill_path: PathBuf,
    pub root: PathBuf,
    pub content_hash: String,
    pub enabled: bool,
    pub priority: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SkillCatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub compatibility: Option<String>,
    pub scope: SkillScope,
    pub origin: SkillOrigin,
    pub content_hash: String,
}

impl From<&SkillRecord> for SkillCatalogEntry {
    fn from(record: &SkillRecord) -> Self {
        Self {
            id: record.id.clone(),
            name: record.name.clone(),
            description: record.description.clone(),
            compatibility: record.compatibility.clone(),
            scope: record.scope,
            origin: record.origin,
            content_hash: record.content_hash.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillDiagnosticKind {
    Invalid,
    Shadowed,
    Unreadable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SkillDiagnostic {
    pub path: PathBuf,
    pub kind: SkillDiagnosticKind,
    pub message: String,
}
