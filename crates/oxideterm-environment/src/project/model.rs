// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::hash::{Hash, Hasher};

/// Ownership scope for a terminal project probe.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ProjectProbeScope {
    Local,
    SshNode(String),
}

impl ProjectProbeScope {
    pub fn ssh_node(node_id: impl Into<String>) -> Self {
        Self::SshNode(node_id.into())
    }
}

/// Stable cache key for cwd-scoped project discovery.
#[derive(Clone, Debug, Eq)]
pub struct ProjectProbeKey {
    scope: ProjectProbeScope,
    cwd: String,
}

impl ProjectProbeKey {
    pub fn new(scope: ProjectProbeScope, cwd: impl Into<String>) -> Option<Self> {
        let cwd = cwd.into();
        let cwd = cwd.trim();
        if cwd.is_empty() || cwd.chars().any(char::is_control) {
            return None;
        }
        Some(Self {
            scope,
            cwd: cwd.to_string(),
        })
    }

    pub fn scope(&self) -> &ProjectProbeScope {
        &self.scope
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }
}

impl PartialEq for ProjectProbeKey {
    fn eq(&self, other: &Self) -> bool {
        self.scope == other.scope && self.cwd == other.cwd
    }
}

impl Hash for ProjectProbeKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.scope.hash(state);
        self.cwd.hash(state);
    }
}

/// A small manifest file collected during project probing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectManifestEntry {
    path: String,
    content: String,
}

impl ProjectManifestEntry {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Option<Self> {
        let path = path.into();
        let path = path.trim();
        if path.is_empty() || path.chars().any(char::is_control) {
            return None;
        }
        Some(Self {
            path: path.to_string(),
            content: content.into(),
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(self.path.as_str())
    }

    pub fn parent_path(&self) -> &str {
        self.path
            .rsplit_once('/')
            .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
            .unwrap_or(".")
    }
}

/// Project families shown in the terminal project surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProjectFacetKind {
    Cargo,
    Node,
    Python,
    Go,
    Make,
    Just,
    Taskfile,
    DockerCompose,
}

impl ProjectFacetKind {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Cargo => "Cargo",
            Self::Node => "Node",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::Make => "Make",
            Self::Just => "Just",
            Self::Taskfile => "Taskfile",
            Self::DockerCompose => "Docker Compose",
        }
    }
}

/// High-level task group for compact UI sections.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProjectTaskGroup {
    Develop,
    Test,
    Build,
    Run,
    Docker,
    Custom,
}

/// A user-runnable project task. Execution is intentionally visible-terminal-only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectTask {
    id: String,
    label: String,
    command: String,
    group: ProjectTaskGroup,
    source: ProjectFacetKind,
}

impl ProjectTask {
    pub fn new(
        source: ProjectFacetKind,
        group: ProjectTaskGroup,
        id: impl Into<String>,
        label: impl Into<String>,
        command: impl Into<String>,
    ) -> Option<Self> {
        let id = normalize_project_label(id)?;
        let label = normalize_project_label(label)?;
        let command = command.into();
        let command = command.trim();
        if command.is_empty() || command.chars().any(char::is_control) {
            return None;
        }
        Some(Self {
            id,
            label,
            command: command.to_string(),
            group,
            source,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn group(&self) -> ProjectTaskGroup {
        self.group
    }

    pub fn source(&self) -> ProjectFacetKind {
        self.source
    }
}

/// One detected project facet at a root path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectFacet {
    kind: ProjectFacetKind,
    root_path: String,
    manifest_path: String,
    tasks: Vec<ProjectTask>,
}

impl ProjectFacet {
    pub fn new(
        kind: ProjectFacetKind,
        root_path: impl Into<String>,
        manifest_path: impl Into<String>,
        tasks: Vec<ProjectTask>,
    ) -> Option<Self> {
        let root_path = normalize_project_path(root_path)?;
        let manifest_path = normalize_project_path(manifest_path)?;
        Some(Self {
            kind,
            root_path,
            manifest_path,
            tasks,
        })
    }

    pub fn kind(&self) -> ProjectFacetKind {
        self.kind
    }

    pub fn root_path(&self) -> &str {
        &self.root_path
    }

    pub fn manifest_path(&self) -> &str {
        &self.manifest_path
    }

    pub fn tasks(&self) -> &[ProjectTask] {
        &self.tasks
    }
}

/// The terminal's current project context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSnapshot {
    root_path: String,
    facets: Vec<ProjectFacet>,
}

impl ProjectSnapshot {
    pub fn new(root_path: impl Into<String>, mut facets: Vec<ProjectFacet>) -> Option<Self> {
        let root_path = normalize_project_path(root_path)?;
        if facets.is_empty() {
            return None;
        }
        facets.sort_by_key(|facet| (facet.root_path().to_string(), facet.kind()));
        Some(Self { root_path, facets })
    }

    pub fn root_path(&self) -> &str {
        &self.root_path
    }

    pub fn facets(&self) -> &[ProjectFacet] {
        &self.facets
    }

    pub fn tasks(&self) -> Vec<ProjectTask> {
        let mut tasks = self
            .facets
            .iter()
            .flat_map(|facet| facet.tasks().iter().cloned())
            .collect::<Vec<_>>();
        tasks.sort_by_key(|task| (task.group(), task.source(), task.label().to_string()));
        tasks
    }

    pub fn display_label(&self) -> String {
        let mut names = self
            .facets
            .iter()
            .map(|facet| facet.kind().display_name())
            .collect::<Vec<_>>();
        names.dedup();
        match names.as_slice() {
            [] => "Project".to_string(),
            [single] => (*single).to_string(),
            [first, second] => format!("{first} + {second}"),
            [first, second, ..] => format!("{first} + {second} + {}", names.len() - 2),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectProbeError {
    message: String,
}

impl ProjectProbeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectProbeOutcome {
    Ready(ProjectSnapshot),
    NoProject,
    CwdMissing,
    Error(ProjectProbeError),
}

fn normalize_project_path(path: impl Into<String>) -> Option<String> {
    let path = path.into();
    let path = path.trim();
    if path.is_empty() || path.chars().any(char::is_control) {
        return None;
    }
    Some(trim_trailing_project_separators(path))
}

fn trim_trailing_project_separators(path: &str) -> String {
    let mut end = path.len();
    while end > 1 {
        let candidate = &path[..end];
        if candidate == "~" || candidate.ends_with(":\\") || candidate.ends_with(":/") {
            break;
        }
        let Some(last) = candidate.chars().next_back() else {
            break;
        };
        if last != '/' && last != '\\' {
            break;
        }
        end -= last.len_utf8();
    }
    path[..end].to_string()
}

fn normalize_project_label(label: impl Into<String>) -> Option<String> {
    let label = label.into();
    let label = label.trim();
    if label.is_empty() || label.chars().any(char::is_control) {
        return None;
    }
    Some(label.to_string())
}
