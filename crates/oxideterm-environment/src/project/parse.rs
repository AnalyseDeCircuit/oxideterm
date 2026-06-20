// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeSet;

use serde_json::Value;

use super::model::{
    ProjectFacet, ProjectFacetKind, ProjectManifestEntry, ProjectProbeOutcome, ProjectSnapshot,
    ProjectTask, ProjectTaskGroup,
};

const PACKAGE_JSON: &str = "package.json";
const CARGO_TOML: &str = "Cargo.toml";
const PYPROJECT_TOML: &str = "pyproject.toml";
const REQUIREMENTS_TXT: &str = "requirements.txt";
const GO_MOD: &str = "go.mod";
const MAKEFILE: &str = "Makefile";
const MAKEFILE_LOWER: &str = "makefile";
const JUSTFILE: &str = "Justfile";
const JUSTFILE_LOWER: &str = "justfile";
const TASKFILE_YAML: &str = "Taskfile.yml";
const TASKFILE_YAML_ALT: &str = "Taskfile.yaml";
const DOCKER_COMPOSE_YML: &str = "docker-compose.yml";
const DOCKER_COMPOSE_YAML: &str = "docker-compose.yaml";
const COMPOSE_YML: &str = "compose.yml";
const COMPOSE_YAML: &str = "compose.yaml";

/// Interpret collected manifest files as the active terminal's project context.
pub fn interpret_project_manifest_entries(
    entries: Vec<ProjectManifestEntry>,
) -> ProjectProbeOutcome {
    let manifest_entries = entries
        .into_iter()
        .filter(|entry| project_manifest_file_names().contains(&entry.file_name()))
        .collect::<Vec<_>>();
    if manifest_entries.is_empty() {
        return ProjectProbeOutcome::NoProject;
    }

    let Some(root_path) = nearest_project_root(&manifest_entries) else {
        return ProjectProbeOutcome::NoProject;
    };
    // A nested project should own its own task surface; parent lockfiles and
    // manifests are intentionally ignored once the nearest root is known.
    let root_entries = manifest_entries
        .iter()
        .filter(|entry| entry.parent_path() == root_path)
        .collect::<Vec<_>>();
    let mut facets = Vec::new();

    if let Some(facet) = parse_cargo_facet(&root_path, &root_entries) {
        facets.push(facet);
    }
    if let Some(facet) = parse_node_facet(&root_path, &root_entries) {
        facets.push(facet);
    }
    if let Some(facet) = parse_python_facet(&root_path, &root_entries) {
        facets.push(facet);
    }
    if let Some(facet) = parse_go_facet(&root_path, &root_entries) {
        facets.push(facet);
    }
    if let Some(facet) = parse_make_facet(&root_path, &root_entries) {
        facets.push(facet);
    }
    if let Some(facet) = parse_just_facet(&root_path, &root_entries) {
        facets.push(facet);
    }
    if let Some(facet) = parse_taskfile_facet(&root_path, &root_entries) {
        facets.push(facet);
    }
    if let Some(facet) = parse_docker_compose_facet(&root_path, &root_entries) {
        facets.push(facet);
    }

    ProjectSnapshot::new(root_path, facets)
        .map(ProjectProbeOutcome::Ready)
        .unwrap_or(ProjectProbeOutcome::NoProject)
}

pub fn project_manifest_file_names() -> &'static [&'static str] {
    &[
        CARGO_TOML,
        PACKAGE_JSON,
        PYPROJECT_TOML,
        REQUIREMENTS_TXT,
        GO_MOD,
        MAKEFILE,
        MAKEFILE_LOWER,
        JUSTFILE,
        JUSTFILE_LOWER,
        TASKFILE_YAML,
        TASKFILE_YAML_ALT,
        DOCKER_COMPOSE_YML,
        DOCKER_COMPOSE_YAML,
        COMPOSE_YML,
        COMPOSE_YAML,
        "pnpm-lock.yaml",
        "yarn.lock",
        "bun.lock",
        "package-lock.json",
    ]
}

fn nearest_project_root(entries: &[ProjectManifestEntry]) -> Option<String> {
    entries
        .iter()
        .filter(|entry| project_root_manifest_names().contains(&entry.file_name()))
        .map(ProjectManifestEntry::parent_path)
        .max_by_key(|path| project_path_depth(path))
        .map(str::to_string)
}

fn project_root_manifest_names() -> &'static [&'static str] {
    &[
        CARGO_TOML,
        PACKAGE_JSON,
        PYPROJECT_TOML,
        REQUIREMENTS_TXT,
        GO_MOD,
        MAKEFILE,
        MAKEFILE_LOWER,
        JUSTFILE,
        JUSTFILE_LOWER,
        TASKFILE_YAML,
        TASKFILE_YAML_ALT,
        DOCKER_COMPOSE_YML,
        DOCKER_COMPOSE_YAML,
        COMPOSE_YML,
        COMPOSE_YAML,
    ]
}

fn project_path_depth(path: &str) -> usize {
    path.split('/').filter(|part| !part.is_empty()).count()
}

fn find_entry<'a>(
    entries: &'a [&'a ProjectManifestEntry],
    names: &[&str],
) -> Option<&'a ProjectManifestEntry> {
    entries
        .iter()
        .copied()
        .find(|entry| names.contains(&entry.file_name()))
}

fn parse_cargo_facet(root_path: &str, entries: &[&ProjectManifestEntry]) -> Option<ProjectFacet> {
    let entry = find_entry(entries, &[CARGO_TOML])?;
    let mut tasks = vec![
        task(
            ProjectFacetKind::Cargo,
            ProjectTaskGroup::Build,
            "cargo-check",
            "check",
            "cargo check",
        ),
        task(
            ProjectFacetKind::Cargo,
            ProjectTaskGroup::Test,
            "cargo-test",
            "test",
            "cargo test",
        ),
        task(
            ProjectFacetKind::Cargo,
            ProjectTaskGroup::Build,
            "cargo-build",
            "build",
            "cargo build",
        ),
        task(
            ProjectFacetKind::Cargo,
            ProjectTaskGroup::Run,
            "cargo-run",
            "run",
            "cargo run",
        ),
    ];
    if entry.content().contains("[workspace]") {
        tasks.push(task(
            ProjectFacetKind::Cargo,
            ProjectTaskGroup::Test,
            "cargo-test-workspace",
            "test workspace",
            "cargo test --workspace",
        ));
    }
    ProjectFacet::new(
        ProjectFacetKind::Cargo,
        root_path,
        entry.path(),
        tasks.into_iter().flatten().collect(),
    )
}

fn parse_node_facet(root_path: &str, entries: &[&ProjectManifestEntry]) -> Option<ProjectFacet> {
    let entry = find_entry(entries, &[PACKAGE_JSON])?;
    let package = serde_json::from_str::<Value>(entry.content()).ok()?;
    let scripts = package.get("scripts")?.as_object()?;
    let runner = node_script_runner(entries);
    let mut tasks = Vec::new();
    for script_name in scripts.keys() {
        let group = node_script_group(script_name);
        let command = format!("{runner} run {}", shell_quote(script_name));
        if let Some(task) = ProjectTask::new(
            ProjectFacetKind::Node,
            group,
            format!("node:{script_name}"),
            script_name,
            command,
        ) {
            tasks.push(task);
        }
    }
    ProjectFacet::new(ProjectFacetKind::Node, root_path, entry.path(), tasks)
}

fn node_script_runner(entries: &[&ProjectManifestEntry]) -> &'static str {
    if find_entry(entries, &["pnpm-lock.yaml"]).is_some() {
        "pnpm"
    } else if find_entry(entries, &["yarn.lock"]).is_some() {
        "yarn"
    } else if find_entry(entries, &["bun.lock"]).is_some() {
        "bun"
    } else {
        "npm"
    }
}

fn node_script_group(script_name: &str) -> ProjectTaskGroup {
    let normalized = script_name.to_ascii_lowercase();
    if normalized.contains("test") || normalized.contains("spec") {
        ProjectTaskGroup::Test
    } else if normalized.contains("build") || normalized.contains("compile") {
        ProjectTaskGroup::Build
    } else if normalized == "start" || normalized.contains("dev") || normalized.contains("serve") {
        ProjectTaskGroup::Develop
    } else {
        ProjectTaskGroup::Custom
    }
}

fn parse_python_facet(root_path: &str, entries: &[&ProjectManifestEntry]) -> Option<ProjectFacet> {
    let entry = find_entry(entries, &[PYPROJECT_TOML, REQUIREMENTS_TXT])?;
    let combined = entries
        .iter()
        .filter(|entry| matches!(entry.file_name(), PYPROJECT_TOML | REQUIREMENTS_TXT))
        .map(|entry| entry.content())
        .collect::<Vec<_>>()
        .join("\n");
    let mut tasks = Vec::new();
    if combined_contains_word(&combined, "pytest") {
        tasks.push(task(
            ProjectFacetKind::Python,
            ProjectTaskGroup::Test,
            "python-pytest",
            "pytest",
            "python -m pytest",
        ));
    }
    if combined.contains("[tool.ruff") || combined_contains_word(&combined, "ruff") {
        tasks.push(task(
            ProjectFacetKind::Python,
            ProjectTaskGroup::Custom,
            "python-ruff-check",
            "ruff check",
            "python -m ruff check .",
        ));
    }
    ProjectFacet::new(
        ProjectFacetKind::Python,
        root_path,
        entry.path(),
        tasks.into_iter().flatten().collect(),
    )
}

fn parse_go_facet(root_path: &str, entries: &[&ProjectManifestEntry]) -> Option<ProjectFacet> {
    let entry = find_entry(entries, &[GO_MOD])?;
    let tasks = vec![
        task(
            ProjectFacetKind::Go,
            ProjectTaskGroup::Test,
            "go-test",
            "test",
            "go test ./...",
        ),
        task(
            ProjectFacetKind::Go,
            ProjectTaskGroup::Build,
            "go-build",
            "build",
            "go build ./...",
        ),
        task(
            ProjectFacetKind::Go,
            ProjectTaskGroup::Run,
            "go-run",
            "run",
            "go run .",
        ),
    ];
    ProjectFacet::new(
        ProjectFacetKind::Go,
        root_path,
        entry.path(),
        tasks.into_iter().flatten().collect(),
    )
}

fn parse_make_facet(root_path: &str, entries: &[&ProjectManifestEntry]) -> Option<ProjectFacet> {
    let entry = find_entry(entries, &[MAKEFILE, MAKEFILE_LOWER])?;
    let targets = parse_colon_targets(entry.content());
    let tasks = targets
        .into_iter()
        .filter_map(|target| {
            ProjectTask::new(
                ProjectFacetKind::Make,
                target_group(&target),
                format!("make:{target}"),
                target.clone(),
                format!("make {}", shell_quote(&target)),
            )
        })
        .collect();
    ProjectFacet::new(ProjectFacetKind::Make, root_path, entry.path(), tasks)
}

fn parse_just_facet(root_path: &str, entries: &[&ProjectManifestEntry]) -> Option<ProjectFacet> {
    let entry = find_entry(entries, &[JUSTFILE, JUSTFILE_LOWER])?;
    let targets = parse_colon_targets(entry.content());
    let tasks = targets
        .into_iter()
        .filter_map(|target| {
            ProjectTask::new(
                ProjectFacetKind::Just,
                target_group(&target),
                format!("just:{target}"),
                target.clone(),
                format!("just {}", shell_quote(&target)),
            )
        })
        .collect();
    ProjectFacet::new(ProjectFacetKind::Just, root_path, entry.path(), tasks)
}

fn parse_taskfile_facet(
    root_path: &str,
    entries: &[&ProjectManifestEntry],
) -> Option<ProjectFacet> {
    let entry = find_entry(entries, &[TASKFILE_YAML, TASKFILE_YAML_ALT])?;
    let tasks = parse_taskfile_tasks(entry.content())
        .into_iter()
        .filter_map(|target| {
            ProjectTask::new(
                ProjectFacetKind::Taskfile,
                target_group(&target),
                format!("task:{target}"),
                target.clone(),
                format!("task {}", shell_quote(&target)),
            )
        })
        .collect();
    ProjectFacet::new(ProjectFacetKind::Taskfile, root_path, entry.path(), tasks)
}

fn parse_docker_compose_facet(
    root_path: &str,
    entries: &[&ProjectManifestEntry],
) -> Option<ProjectFacet> {
    let entry = find_entry(
        entries,
        &[
            DOCKER_COMPOSE_YML,
            DOCKER_COMPOSE_YAML,
            COMPOSE_YML,
            COMPOSE_YAML,
        ],
    )?;
    let tasks = vec![
        task(
            ProjectFacetKind::DockerCompose,
            ProjectTaskGroup::Docker,
            "compose-up",
            "compose up",
            "docker compose up",
        ),
        task(
            ProjectFacetKind::DockerCompose,
            ProjectTaskGroup::Docker,
            "compose-ps",
            "compose ps",
            "docker compose ps",
        ),
        task(
            ProjectFacetKind::DockerCompose,
            ProjectTaskGroup::Docker,
            "compose-logs",
            "compose logs",
            "docker compose logs -f",
        ),
        task(
            ProjectFacetKind::DockerCompose,
            ProjectTaskGroup::Docker,
            "compose-build",
            "compose build",
            "docker compose build",
        ),
        task(
            ProjectFacetKind::DockerCompose,
            ProjectTaskGroup::Docker,
            "compose-down",
            "compose down",
            "docker compose down",
        ),
    ];
    ProjectFacet::new(
        ProjectFacetKind::DockerCompose,
        root_path,
        entry.path(),
        tasks.into_iter().flatten().collect(),
    )
}

fn task(
    source: ProjectFacetKind,
    group: ProjectTaskGroup,
    id: &str,
    label: &str,
    command: &str,
) -> Option<ProjectTask> {
    ProjectTask::new(source, group, id, label, command)
}

fn parse_colon_targets(content: &str) -> Vec<String> {
    let mut targets = BTreeSet::new();
    for line in content.lines() {
        let trimmed = line.trim_end();
        // Make and just files allow many declarations that are not runnable
        // targets. Keep this parser conservative until a dedicated parser is
        // worth the extra dependency and compatibility surface.
        if trimmed.is_empty() || trimmed.starts_with(['#', '\t']) || trimmed.contains('=') {
            continue;
        }
        let Some((head, _)) = trimmed.split_once(':') else {
            continue;
        };
        if head.trim_start().starts_with('.') || head.contains('%') {
            continue;
        }
        for target in head.split_whitespace() {
            if is_runnable_target_name(target) {
                targets.insert(target.to_string());
            }
        }
    }
    targets.into_iter().take(32).collect()
}

fn parse_taskfile_tasks(content: &str) -> Vec<String> {
    let mut in_tasks = false;
    let mut tasks = BTreeSet::new();
    for line in content.lines() {
        if line.trim() == "tasks:" {
            in_tasks = true;
            continue;
        }
        if !in_tasks {
            continue;
        }
        if !line.starts_with("  ") || line.starts_with("    ") {
            continue;
        }
        let trimmed = line.trim();
        let Some(name) = trimmed.strip_suffix(':') else {
            continue;
        };
        let name = name.trim_matches(['"', '\'']);
        if is_runnable_target_name(name) {
            tasks.insert(name.to_string());
        }
    }
    tasks.into_iter().take(32).collect()
}

fn is_runnable_target_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with(['.', '_', '-'])
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.'))
}

fn target_group(target: &str) -> ProjectTaskGroup {
    let normalized = target.to_ascii_lowercase();
    if normalized.contains("test") || normalized.contains("spec") {
        ProjectTaskGroup::Test
    } else if normalized.contains("build") || normalized.contains("compile") {
        ProjectTaskGroup::Build
    } else if normalized.contains("run")
        || normalized.contains("start")
        || normalized.contains("dev")
        || normalized.contains("serve")
    {
        ProjectTaskGroup::Develop
    } else {
        ProjectTaskGroup::Custom
    }
}

fn combined_contains_word(content: &str, needle: &str) -> bool {
    content
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
        .any(|word| word.eq_ignore_ascii_case(needle))
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(path: &str, content: &str) -> ProjectManifestEntry {
        ProjectManifestEntry::new(path, content).unwrap()
    }

    #[test]
    fn node_scripts_use_nearest_root_and_package_manager_lockfile() {
        let outcome = interpret_project_manifest_entries(vec![
            manifest("/repo/Cargo.toml", "[workspace]\n"),
            manifest(
                "/repo/app/package.json",
                r#"{"scripts":{"dev":"vite","test":"vitest","build":"vite build"}}"#,
            ),
            manifest("/repo/app/pnpm-lock.yaml", ""),
        ]);

        let ProjectProbeOutcome::Ready(snapshot) = outcome else {
            panic!("expected project");
        };
        assert_eq!(snapshot.root_path(), "/repo/app");
        assert_eq!(snapshot.display_label(), "Node");
        let commands = snapshot
            .tasks()
            .into_iter()
            .map(|task| task.command().to_string())
            .collect::<Vec<_>>();
        assert!(commands.contains(&"pnpm run 'dev'".to_string()));
        assert!(commands.contains(&"pnpm run 'test'".to_string()));
    }

    #[test]
    fn cargo_and_compose_facets_merge_at_same_root() {
        let outcome = interpret_project_manifest_entries(vec![
            manifest("/repo/Cargo.toml", "[workspace]\n"),
            manifest("/repo/docker-compose.yml", "services: {}\n"),
        ]);

        let ProjectProbeOutcome::Ready(snapshot) = outcome else {
            panic!("expected project");
        };
        assert_eq!(snapshot.root_path(), "/repo");
        assert_eq!(snapshot.facets().len(), 2);
        assert!(
            snapshot
                .tasks()
                .iter()
                .any(|task| task.command() == "cargo test --workspace")
        );
        assert!(
            snapshot
                .tasks()
                .iter()
                .any(|task| task.command() == "docker compose up")
        );
    }

    #[test]
    fn make_just_and_taskfile_targets_are_filtered() {
        assert_eq!(
            parse_colon_targets("build:\n.PHONY: build\nfoo bar:\nVAR = nope\n"),
            vec!["bar", "build", "foo"]
        );
        assert_eq!(
            parse_taskfile_tasks("version: '3'\ntasks:\n  test:\n    cmds: []\n  \"dev\":\n"),
            vec!["dev", "test"]
        );
    }

    #[test]
    fn python_only_adds_tasks_when_tools_are_declared() {
        let outcome = interpret_project_manifest_entries(vec![manifest(
            "/repo/pyproject.toml",
            "[tool.pytest.ini_options]\n[tool.ruff]\n",
        )]);

        let ProjectProbeOutcome::Ready(snapshot) = outcome else {
            panic!("expected project");
        };
        let commands = snapshot
            .tasks()
            .into_iter()
            .map(|task| task.command().to_string())
            .collect::<Vec<_>>();
        assert_eq!(commands, vec!["python -m pytest", "python -m ruff check ."]);
    }
}
