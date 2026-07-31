// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, path::PathBuf};

use crate::{SkillOrigin, SkillScope};

#[derive(Clone, Debug, Default)]
pub struct SkillDiscoveryOptions {
    /// Stable root selected for the conversation, not the shell's changing CWD.
    pub workspace_root: Option<PathBuf>,
    pub settings_path: Option<PathBuf>,
    pub plugin_roots: Vec<PathBuf>,
    pub disabled_paths: HashSet<PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct SkillRoot {
    pub path: PathBuf,
    pub scope: SkillScope,
    pub origin: SkillOrigin,
    pub priority: u16,
}

pub(crate) fn discovery_roots(options: &SkillDiscoveryOptions) -> Vec<SkillRoot> {
    let mut roots = Vec::new();
    if let Some(workspace) = options.workspace_root.as_deref() {
        roots.extend([
            root(
                workspace.join(".agents/skills"),
                SkillScope::Workspace,
                SkillOrigin::AgentStandard,
                600,
            ),
            root(
                workspace.join(".github/skills"),
                SkillScope::Workspace,
                SkillOrigin::CopilotCompatible,
                550,
            ),
            root(
                workspace.join(".claude/skills"),
                SkillScope::Workspace,
                SkillOrigin::ClaudeCompatible,
                540,
            ),
            root(
                workspace.join(".opencode/skills"),
                SkillScope::Workspace,
                SkillOrigin::OpenCodeCompatible,
                530,
            ),
        ]);
    }
    if let Some(settings_path) = options.settings_path.as_deref() {
        roots.push(root(
            settings_path
                .parent()
                .unwrap_or(settings_path)
                .join("skills"),
            SkillScope::User,
            SkillOrigin::OxideTerm,
            500,
        ));
    }
    if let Some(home) = dirs::home_dir() {
        roots.extend([
            root(
                home.join(".agents/skills"),
                SkillScope::User,
                SkillOrigin::AgentStandard,
                450,
            ),
            root(
                home.join(".claude/skills"),
                SkillScope::User,
                SkillOrigin::ClaudeCompatible,
                420,
            ),
            root(
                home.join(".copilot/skills"),
                SkillScope::User,
                SkillOrigin::CopilotCompatible,
                410,
            ),
            root(
                home.join(".config/opencode/skills"),
                SkillScope::User,
                SkillOrigin::OpenCodeCompatible,
                400,
            ),
        ]);
    }
    for plugin_root in &options.plugin_roots {
        roots.push(root(
            plugin_root.join("skills"),
            SkillScope::Plugin,
            SkillOrigin::Plugin,
            300,
        ));
    }
    roots
}

fn root(path: PathBuf, scope: SkillScope, origin: SkillOrigin, priority: u16) -> SkillRoot {
    SkillRoot {
        path,
        scope,
        origin,
        priority,
    }
}
