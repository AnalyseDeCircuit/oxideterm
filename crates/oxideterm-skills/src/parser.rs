// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::SkillRegistryError;

const MAX_SKILL_FILE_BYTES: usize = 512 * 1024;
const MAX_SKILL_NAME_CHARS: usize = 64;
const MAX_SKILL_DESCRIPTION_CHARS: usize = 1024;
const MAX_SKILL_COMPATIBILITY_CHARS: usize = 500;

#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    compatibility: Option<String>,
}

pub(crate) struct ParsedSkill {
    pub name: String,
    pub description: String,
    pub compatibility: Option<String>,
    pub body: String,
    pub content_hash: String,
}

pub(crate) fn parse_skill_file(path: &Path) -> Result<ParsedSkill, SkillRegistryError> {
    let bytes = std::fs::read(path).map_err(|source| SkillRegistryError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if bytes.len() > MAX_SKILL_FILE_BYTES {
        return Err(SkillRegistryError::Invalid {
            path: path.to_path_buf(),
            message: format!("SKILL.md exceeds {MAX_SKILL_FILE_BYTES} bytes"),
        });
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| SkillRegistryError::Invalid {
        path: path.to_path_buf(),
        message: "SKILL.md is not valid UTF-8".to_string(),
    })?;
    let (frontmatter, body) =
        split_frontmatter(text).ok_or_else(|| SkillRegistryError::Invalid {
            path: path.to_path_buf(),
            message: "SKILL.md must start with YAML frontmatter".to_string(),
        })?;
    let metadata: SkillFrontmatter =
        serde_yaml::from_str(frontmatter).map_err(|error| SkillRegistryError::Invalid {
            path: path.to_path_buf(),
            message: format!("invalid YAML frontmatter: {error}"),
        })?;
    validate_metadata(path, &metadata)?;
    if body.trim().is_empty() {
        return Err(SkillRegistryError::Invalid {
            path: path.to_path_buf(),
            message: "SKILL.md must contain instructions after the frontmatter".to_string(),
        });
    }
    let directory_name = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if directory_name != metadata.name {
        return Err(SkillRegistryError::Invalid {
            path: path.to_path_buf(),
            message: "skill name must match its parent directory".to_string(),
        });
    }
    let content_hash = format!("{:x}", Sha256::digest(&bytes));
    Ok(ParsedSkill {
        name: metadata.name,
        description: metadata.description.trim().to_string(),
        compatibility: metadata
            .compatibility
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        body: body.trim().to_string(),
        content_hash,
    })
}

fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let remainder = text
        .strip_prefix("---\r\n")
        .or_else(|| text.strip_prefix("---\n"))?;
    let delimiter = remainder
        .find("\n---\n")
        .map(|index| (index, 5))
        .or_else(|| remainder.find("\r\n---\r\n").map(|index| (index, 9)))?;
    Some((
        &remainder[..delimiter.0],
        &remainder[delimiter.0 + delimiter.1..],
    ))
}

fn validate_metadata(path: &Path, metadata: &SkillFrontmatter) -> Result<(), SkillRegistryError> {
    let valid_name = !metadata.name.is_empty()
        && metadata.name.chars().count() <= MAX_SKILL_NAME_CHARS
        && !metadata.name.starts_with('-')
        && !metadata.name.ends_with('-')
        && !metadata.name.contains("--")
        && metadata
            .name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid_name {
        return Err(SkillRegistryError::Invalid {
            path: path.to_path_buf(),
            message: "skill name must use lowercase letters, digits, and single hyphens"
                .to_string(),
        });
    }
    let description_length = metadata.description.trim().chars().count();
    if description_length == 0 || description_length > MAX_SKILL_DESCRIPTION_CHARS {
        return Err(SkillRegistryError::Invalid {
            path: path.to_path_buf(),
            message: format!(
                "skill description must contain 1-{MAX_SKILL_DESCRIPTION_CHARS} characters"
            ),
        });
    }
    if metadata
        .compatibility
        .as_deref()
        .is_some_and(|value| value.chars().count() > MAX_SKILL_COMPATIBILITY_CHARS)
    {
        return Err(SkillRegistryError::Invalid {
            path: path.to_path_buf(),
            message: format!(
                "skill compatibility must not exceed {MAX_SKILL_COMPATIBILITY_CHARS} characters"
            ),
        });
    }
    Ok(())
}
