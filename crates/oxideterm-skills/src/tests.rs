// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{SkillDiagnosticKind, SkillDiscoveryOptions, SkillRegistry, SkillScope};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "oxideterm-skills-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_skill(root: &Path, name: &str, description: &str, body: &str) -> PathBuf {
    let directory = root.join(name);
    fs::create_dir_all(&directory).expect("create skill directory");
    let path = directory.join("SKILL.md");
    fs::write(
        &path,
        format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n"),
    )
    .expect("write skill");
    path
}

#[test]
fn discovers_workspace_skill_and_loads_body_on_demand() {
    let directory = TestDirectory::new("workspace");
    write_skill(
        &directory.path().join(".agents/skills"),
        "release-review",
        "Review a release before publishing",
        "Run the release checklist.",
    );
    let registry = SkillRegistry::discover(&SkillDiscoveryOptions {
        workspace_root: Some(directory.path().to_path_buf()),
        ..SkillDiscoveryOptions::default()
    });

    let catalog = registry.catalog();
    let discovered = catalog
        .iter()
        .find(|skill| skill.id == "release-review")
        .expect("workspace skill");
    assert_eq!(discovered.scope, SkillScope::Workspace);
    assert_eq!(
        registry.load("release-review").unwrap(),
        "Run the release checklist."
    );
}

#[test]
fn discovers_skill_from_the_settings_data_directory() {
    let directory = TestDirectory::new("application-data");
    let data_dir = directory.path().join("portable-store");
    write_skill(
        &data_dir.join("skills"),
        "portable-workflow",
        "Portable workflow",
        "Keep these instructions with the portable installation.",
    );
    let registry = SkillRegistry::discover(&SkillDiscoveryOptions {
        settings_path: Some(data_dir.join("settings.json")),
        ..SkillDiscoveryOptions::default()
    });

    let discovered = registry
        .catalog()
        .into_iter()
        .find(|skill| skill.id == "portable-workflow")
        .expect("portable data skill");
    assert_eq!(discovered.scope, SkillScope::User);
    assert_eq!(
        registry.load("portable-workflow").unwrap(),
        "Keep these instructions with the portable installation."
    );
}

#[test]
fn workspace_standard_skill_wins_compatible_duplicate() {
    let directory = TestDirectory::new("precedence");
    write_skill(
        &directory.path().join(".agents/skills"),
        "review",
        "Standard review",
        "standard",
    );
    write_skill(
        &directory.path().join(".claude/skills"),
        "review",
        "Compatible review",
        "compatible",
    );
    let registry = SkillRegistry::discover(&SkillDiscoveryOptions {
        workspace_root: Some(directory.path().to_path_buf()),
        ..SkillDiscoveryOptions::default()
    });

    assert_eq!(registry.load("review").unwrap(), "standard");
    assert!(
        registry
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind == SkillDiagnosticKind::Shadowed)
    );
}

#[test]
fn disabled_skill_is_hidden_and_cannot_be_loaded() {
    let directory = TestDirectory::new("disabled");
    let path = write_skill(
        &directory.path().join(".agents/skills"),
        "manual-only",
        "Manual workflow",
        "instructions",
    );
    let canonical_path = fs::canonicalize(path).unwrap();
    let registry = SkillRegistry::discover(&SkillDiscoveryOptions {
        workspace_root: Some(directory.path().to_path_buf()),
        disabled_paths: HashSet::from([canonical_path]),
        ..SkillDiscoveryOptions::default()
    });

    assert!(
        registry
            .catalog()
            .iter()
            .all(|skill| skill.id != "manual-only")
    );
    assert!(registry.load("manual-only").is_err());
}

#[test]
fn resource_reader_rejects_paths_outside_skill_root() {
    let directory = TestDirectory::new("resource-boundary");
    let skill_path = write_skill(
        &directory.path().join(".agents/skills"),
        "bounded",
        "Read bounded resources",
        "Read references/details.md.",
    );
    let skill_root = skill_path.parent().unwrap();
    fs::create_dir_all(skill_root.join("references")).unwrap();
    fs::write(skill_root.join("references/details.md"), "safe").unwrap();
    fs::write(directory.path().join("outside.md"), "unsafe").unwrap();
    let registry = SkillRegistry::discover(&SkillDiscoveryOptions {
        workspace_root: Some(directory.path().to_path_buf()),
        ..SkillDiscoveryOptions::default()
    });

    assert_eq!(
        registry
            .read_resource("bounded", Path::new("references/details.md"))
            .unwrap(),
        "safe"
    );
    assert!(
        registry
            .read_resource("bounded", Path::new("../../../outside.md"))
            .is_err()
    );
}

#[test]
fn invalid_name_is_reported_without_poisoning_other_skills() {
    let directory = TestDirectory::new("invalid");
    write_skill(
        &directory.path().join(".agents/skills"),
        "Bad_Name",
        "Invalid name",
        "ignored",
    );
    write_skill(
        &directory.path().join(".agents/skills"),
        "valid-name",
        "Valid workflow",
        "loaded",
    );
    let registry = SkillRegistry::discover(&SkillDiscoveryOptions {
        workspace_root: Some(directory.path().to_path_buf()),
        ..SkillDiscoveryOptions::default()
    });

    assert!(
        registry
            .catalog()
            .iter()
            .any(|skill| skill.id == "valid-name")
    );
    assert!(
        registry
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind == SkillDiagnosticKind::Invalid)
    );
}

#[test]
fn empty_instructions_are_rejected() {
    let directory = TestDirectory::new("empty-body");
    write_skill(
        &directory.path().join(".agents/skills"),
        "empty-skill",
        "Invalid empty workflow",
        "",
    );
    let registry = SkillRegistry::discover(&SkillDiscoveryOptions {
        workspace_root: Some(directory.path().to_path_buf()),
        ..SkillDiscoveryOptions::default()
    });

    assert!(
        registry
            .catalog()
            .iter()
            .all(|skill| skill.id != "empty-skill")
    );
    assert!(
        registry
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind == SkillDiagnosticKind::Invalid)
    );
}

#[cfg(unix)]
#[test]
fn skill_symlink_cannot_escape_discovery_root() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new("symlink-boundary");
    let outside_root = directory.path().join("outside");
    write_skill(
        &outside_root,
        "escaped",
        "Must not load through a symlink",
        "unsafe",
    );
    let discovery_root = directory.path().join(".agents/skills");
    fs::create_dir_all(&discovery_root).unwrap();
    symlink(outside_root.join("escaped"), discovery_root.join("escaped")).unwrap();
    let registry = SkillRegistry::discover(&SkillDiscoveryOptions {
        workspace_root: Some(directory.path().to_path_buf()),
        ..SkillDiscoveryOptions::default()
    });

    assert!(registry.catalog().iter().all(|skill| skill.id != "escaped"));
    assert!(
        registry
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message.contains("escapes"))
    );
}
