// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Quick Commands JSON persistence.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

const QUICK_COMMANDS_FILENAME: &str = "quick-commands.json";
const QUICK_COMMANDS_SCHEMA_VERSION: u32 = 1;
const MAX_QUICK_COMMANDS_FILE_BYTES: u64 = 512 * 1024;
const MAX_CATEGORIES: usize = 100;
const MAX_COMMANDS: usize = 1000;
const MAX_ID_LEN: usize = 128;
const MAX_NAME_LEN: usize = 160;
const MAX_COMMAND_LEN: usize = 4096;
const MAX_DESCRIPTION_LEN: usize = 1024;
const MAX_HOST_PATTERN_LEN: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommandsSnapshot {
    pub version: u32,
    pub categories: Vec<QuickCommandCategory>,
    pub commands: Vec<QuickCommand>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommandCategory {
    pub id: String,
    pub name: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

fn quick_commands_path() -> Result<PathBuf, String> {
    crate::config::storage::config_dir()
        .map(|dir| dir.join(QUICK_COMMANDS_FILENAME))
        .map_err(|err| err.to_string())
}

fn sanitize_snapshot(snapshot: QuickCommandsSnapshot) -> Result<QuickCommandsSnapshot, String> {
    if snapshot.version != QUICK_COMMANDS_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported Quick Commands schema version {}",
            snapshot.version
        ));
    }
    if snapshot.categories.len() > MAX_CATEGORIES {
        return Err(format!(
            "Quick Commands category count exceeds limit {}",
            MAX_CATEGORIES
        ));
    }
    if snapshot.commands.len() > MAX_COMMANDS {
        return Err(format!(
            "Quick Commands command count exceeds limit {}",
            MAX_COMMANDS
        ));
    }

    let categories = snapshot
        .categories
        .into_iter()
        .map(sanitize_category)
        .collect::<Result<Vec<_>, _>>()?;
    let commands = snapshot
        .commands
        .into_iter()
        .map(sanitize_command)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(QuickCommandsSnapshot {
        version: QUICK_COMMANDS_SCHEMA_VERSION,
        categories,
        commands,
        updated_at: snapshot.updated_at,
    })
}

fn sanitize_category(category: QuickCommandCategory) -> Result<QuickCommandCategory, String> {
    Ok(QuickCommandCategory {
        id: bounded_required(category.id, "category.id", MAX_ID_LEN)?,
        name: bounded_required(category.name, "category.name", MAX_NAME_LEN)?,
        icon: sanitize_icon(category.icon),
    })
}

fn sanitize_command(command: QuickCommand) -> Result<QuickCommand, String> {
    Ok(QuickCommand {
        id: bounded_required(command.id, "command.id", MAX_ID_LEN)?,
        name: bounded_required(command.name, "command.name", MAX_NAME_LEN)?,
        command: bounded_required(command.command, "command.command", MAX_COMMAND_LEN)?,
        category: bounded_required(command.category, "command.category", MAX_ID_LEN)?,
        description: bounded_optional(command.description, "command.description", MAX_DESCRIPTION_LEN)?,
        host_pattern: bounded_optional(command.host_pattern, "command.hostPattern", MAX_HOST_PATTERN_LEN)?,
        created_at: command.created_at,
        updated_at: command.updated_at,
    })
}

fn bounded_required(value: String, field: &str, max_len: usize) -> Result<String, String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(format!("Quick Commands field {} cannot be empty", field));
    }
    if trimmed.len() > max_len {
        return Err(format!(
            "Quick Commands field {} exceeds limit {}",
            field, max_len
        ));
    }
    Ok(trimmed)
}

fn bounded_optional(value: Option<String>, field: &str, max_len: usize) -> Result<Option<String>, String> {
    match value.map(|item| item.trim().to_string()) {
        Some(item) if item.is_empty() => Ok(None),
        Some(item) if item.len() > max_len => Err(format!(
            "Quick Commands field {} exceeds limit {}",
            field, max_len
        )),
        Some(item) => Ok(Some(item)),
        None => Ok(None),
    }
}

fn sanitize_icon(icon: String) -> String {
    match icon.as_str() {
        "terminal" | "server" | "folder" | "docker" | "zap" => icon,
        _ => "terminal".to_string(),
    }
}

#[tauri::command]
pub async fn load_quick_commands() -> Result<Option<QuickCommandsSnapshot>, String> {
    let path = quick_commands_path()?;
    match fs::metadata(&path).await {
        Ok(metadata) => {
            if metadata.len() > MAX_QUICK_COMMANDS_FILE_BYTES {
                return Err("Quick Commands file exceeds size limit".to_string());
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("Failed to stat Quick Commands file: {}", err)),
    }

    let contents = fs::read_to_string(&path)
        .await
        .map_err(|err| format!("Failed to read Quick Commands file: {}", err))?;
    if contents.trim().is_empty() {
        return Ok(None);
    }

    let parsed: QuickCommandsSnapshot = serde_json::from_str(&contents)
        .map_err(|err| format!("Failed to parse Quick Commands file: {}", err))?;
    sanitize_snapshot(parsed).map(Some)
}

#[tauri::command]
pub async fn save_quick_commands(snapshot: QuickCommandsSnapshot) -> Result<(), String> {
    let snapshot = sanitize_snapshot(snapshot)?;
    let path = quick_commands_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|err| format!("Failed to create Quick Commands directory: {}", err))?;
    }

    let json = serde_json::to_vec_pretty(&snapshot)
        .map_err(|err| format!("Failed to serialize Quick Commands: {}", err))?;
    if json.len() as u64 > MAX_QUICK_COMMANDS_FILE_BYTES {
        return Err("Quick Commands snapshot exceeds size limit".to_string());
    }

    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, json)
        .await
        .map_err(|err| format!("Failed to write Quick Commands temp file: {}", err))?;
    fs::rename(&temp_path, &path)
        .await
        .map_err(|err| format!("Failed to replace Quick Commands file: {}", err))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_snapshot() -> QuickCommandsSnapshot {
        QuickCommandsSnapshot {
            version: 1,
            categories: vec![QuickCommandCategory {
                id: "system".to_string(),
                name: "System".to_string(),
                icon: "server".to_string(),
            }],
            commands: vec![QuickCommand {
                id: "qc-pwd".to_string(),
                name: "pwd".to_string(),
                command: "pwd".to_string(),
                category: "system".to_string(),
                description: None,
                host_pattern: None,
                created_at: 0,
                updated_at: 0,
            }],
            updated_at: 1,
        }
    }

    #[test]
    fn sanitize_rejects_unsupported_version() {
        let mut snapshot = valid_snapshot();
        snapshot.version = 2;
        assert!(sanitize_snapshot(snapshot).is_err());
    }

    #[test]
    fn sanitize_bounds_commands() {
        let mut snapshot = valid_snapshot();
        snapshot.commands = (0..=MAX_COMMANDS)
            .map(|index| QuickCommand {
                id: format!("qc-{}", index),
                name: "pwd".to_string(),
                command: "pwd".to_string(),
                category: "system".to_string(),
                description: None,
                host_pattern: None,
                created_at: 0,
                updated_at: 0,
            })
            .collect();
        assert!(sanitize_snapshot(snapshot).is_err());
    }

    #[test]
    fn sanitize_defaults_unknown_icon() {
        let mut snapshot = valid_snapshot();
        snapshot.categories[0].icon = "unknown".to_string();
        let sanitized = sanitize_snapshot(snapshot).unwrap();
        assert_eq!(sanitized.categories[0].icon, "terminal");
    }
}
