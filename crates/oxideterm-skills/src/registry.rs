// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    SkillCatalogEntry, SkillDiagnostic, SkillDiagnosticKind, SkillDiscoveryOptions, SkillRecord,
    discovery::discovery_roots, parser::parse_skill_file,
};

const MAX_RESOURCE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum SkillRegistryError {
    #[error("failed to read {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("invalid skill at {path}: {message}")]
    Invalid { path: PathBuf, message: String },
    #[error("skill not found: {0}")]
    NotFound(String),
    #[error("skill resource escapes its root")]
    ResourceOutsideRoot,
    #[error("skill resource exceeds {MAX_RESOURCE_BYTES} bytes")]
    ResourceTooLarge,
    #[error("skill resource is not valid UTF-8")]
    ResourceNotUtf8,
}

#[derive(Clone, Debug, Default)]
pub struct SkillRegistry {
    skills: BTreeMap<String, SkillRecord>,
    diagnostics: Vec<SkillDiagnostic>,
}

impl SkillRegistry {
    pub fn discover(options: &SkillDiscoveryOptions) -> Self {
        let mut registry = Self::default();
        for root in discovery_roots(options) {
            let Ok(canonical_discovery_root) = std::fs::canonicalize(&root.path) else {
                continue;
            };
            let Ok(entries) = std::fs::read_dir(&root.path) else {
                continue;
            };
            for entry in entries.flatten() {
                let skill_path = entry.path().join("SKILL.md");
                if !skill_path.is_file() {
                    continue;
                }
                let canonical_path = match std::fs::canonicalize(&skill_path) {
                    Ok(path) => path,
                    Err(error) => {
                        registry.diagnostics.push(SkillDiagnostic {
                            path: skill_path,
                            kind: SkillDiagnosticKind::Unreadable,
                            message: error.to_string(),
                        });
                        continue;
                    }
                };
                if !canonical_path.starts_with(&canonical_discovery_root) {
                    registry.diagnostics.push(SkillDiagnostic {
                        path: skill_path,
                        kind: SkillDiagnosticKind::Invalid,
                        message: "skill symlink escapes its discovery root".to_string(),
                    });
                    continue;
                }
                match parse_skill_file(&canonical_path) {
                    Ok(parsed) => {
                        let canonical_root = canonical_path
                            .parent()
                            .expect("SKILL.md always has a parent")
                            .to_path_buf();
                        let enabled = !options.disabled_paths.contains(&canonical_path)
                            && !options.disabled_paths.contains(&canonical_root);
                        let record = SkillRecord {
                            id: parsed.name.clone(),
                            name: parsed.name,
                            description: parsed.description,
                            compatibility: parsed.compatibility,
                            scope: root.scope,
                            origin: root.origin,
                            skill_path: canonical_path,
                            root: canonical_root,
                            content_hash: parsed.content_hash,
                            enabled,
                            priority: root.priority,
                        };
                        registry.insert(record);
                    }
                    Err(error) => registry.diagnostics.push(SkillDiagnostic {
                        path: canonical_path,
                        kind: SkillDiagnosticKind::Invalid,
                        message: error.to_string(),
                    }),
                }
            }
        }
        registry
    }

    pub fn catalog(&self) -> Vec<SkillCatalogEntry> {
        self.skills
            .values()
            .filter(|skill| skill.enabled)
            .map(SkillCatalogEntry::from)
            .collect()
    }

    pub fn records(&self) -> impl Iterator<Item = &SkillRecord> {
        self.skills.values()
    }

    pub fn enabled_record(&self, id: &str) -> Option<&SkillRecord> {
        self.skills.get(id).filter(|skill| skill.enabled)
    }

    pub fn diagnostics(&self) -> &[SkillDiagnostic] {
        &self.diagnostics
    }

    pub fn load(&self, id: &str) -> Result<String, SkillRegistryError> {
        let skill = self
            .skills
            .get(id)
            .filter(|skill| skill.enabled)
            .ok_or_else(|| SkillRegistryError::NotFound(id.to_string()))?;
        Ok(parse_skill_file(&skill.skill_path)?.body)
    }

    pub fn read_resource(
        &self,
        id: &str,
        relative_path: &Path,
    ) -> Result<String, SkillRegistryError> {
        let skill = self
            .skills
            .get(id)
            .filter(|skill| skill.enabled)
            .ok_or_else(|| SkillRegistryError::NotFound(id.to_string()))?;
        let candidate =
            std::fs::canonicalize(skill.root.join(relative_path)).map_err(|source| {
                SkillRegistryError::Io {
                    path: skill.root.join(relative_path),
                    source,
                }
            })?;
        if !candidate.starts_with(&skill.root) || candidate == skill.skill_path {
            return Err(SkillRegistryError::ResourceOutsideRoot);
        }
        let bytes = std::fs::read(&candidate).map_err(|source| SkillRegistryError::Io {
            path: candidate,
            source,
        })?;
        if bytes.len() > MAX_RESOURCE_BYTES {
            return Err(SkillRegistryError::ResourceTooLarge);
        }
        String::from_utf8(bytes).map_err(|_| SkillRegistryError::ResourceNotUtf8)
    }

    fn insert(&mut self, record: SkillRecord) {
        if let Some(existing) = self.skills.get(&record.id)
            && existing.priority >= record.priority
        {
            self.diagnostics.push(SkillDiagnostic {
                path: record.skill_path,
                kind: SkillDiagnosticKind::Shadowed,
                message: format!("shadowed by {}", existing.skill_path.display()),
            });
            return;
        }
        if let Some(shadowed) = self.skills.insert(record.id.clone(), record) {
            self.diagnostics.push(SkillDiagnostic {
                path: shadowed.skill_path,
                kind: SkillDiagnosticKind::Shadowed,
                message: "shadowed by a higher-priority skill".to_string(),
            });
        }
    }
}
