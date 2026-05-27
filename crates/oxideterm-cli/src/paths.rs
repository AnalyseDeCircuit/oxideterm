// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    path::{Path, PathBuf},
    sync::{OnceLock, RwLock},
};

use serde::Serialize;

const CONNECTIONS_FILE_NAME: &str = "connections.json";
const FORWARDS_FILE_NAME: &str = "forwards.json";
const BACKUPS_DIR_NAME: &str = "backups";
const SETTINGS_FILE_NAME: &str = "settings.json";
const CONFIG_DIR_ENV: &str = "OXIDETERM_CONFIG_DIR";

static CLI_PATH_CONTEXT: OnceLock<RwLock<CliPathContext>> = OnceLock::new();

#[derive(Clone, Debug, Default)]
pub struct CliPathContext {
    pub config_dir: Option<PathBuf>,
    pub profile: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliPaths {
    pub config_dir: String,
    pub profile: Option<String>,
    pub settings_dir: String,
    pub settings: String,
    pub connections: String,
    pub forwards: String,
    pub cloud_sync: String,
    pub backups_dir: String,
}

pub fn set_cli_path_context(config_dir: Option<PathBuf>, profile: Option<String>) {
    let context = CliPathContext {
        config_dir,
        profile: profile.filter(|profile| !profile.trim().is_empty()),
    };
    let lock = CLI_PATH_CONTEXT.get_or_init(|| RwLock::new(CliPathContext::default()));
    if let Ok(mut current) = lock.write() {
        *current = context;
    }
}

pub fn cli_paths() -> CliPaths {
    let context = active_context();
    let settings_dir = settings_dir_from_context(&context);
    let settings = settings_dir.join(SETTINGS_FILE_NAME);
    let connections = settings_dir.join(CONNECTIONS_FILE_NAME);
    let forwards = settings_dir.join(FORWARDS_FILE_NAME);
    let cloud_sync = oxideterm_cloud_sync::state::default_cloud_sync_state_path(&settings);
    let backups_dir = settings_dir.join(BACKUPS_DIR_NAME);

    // Keep path calculation centralized so every CLI command reads the same files as the app.
    CliPaths {
        config_dir: config_dir_display(&context, &settings_dir),
        profile: context.profile,
        settings_dir: settings_dir.display().to_string(),
        settings: settings.display().to_string(),
        connections: connections.display().to_string(),
        forwards: forwards.display().to_string(),
        cloud_sync: cloud_sync.display().to_string(),
        backups_dir: backups_dir.display().to_string(),
    }
}

pub fn default_settings_path() -> PathBuf {
    PathBuf::from(cli_paths().settings)
}

pub fn default_connections_path() -> PathBuf {
    PathBuf::from(cli_paths().connections)
}

pub fn default_cloud_sync_path() -> PathBuf {
    PathBuf::from(cli_paths().cloud_sync)
}

pub fn default_forwards_path() -> PathBuf {
    PathBuf::from(cli_paths().forwards)
}

pub fn default_backups_dir() -> PathBuf {
    PathBuf::from(cli_paths().backups_dir)
}

fn active_context() -> CliPathContext {
    let mut context = CLI_PATH_CONTEXT
        .get()
        .and_then(|lock| lock.read().ok().map(|context| context.clone()))
        .unwrap_or_default();
    if context.config_dir.is_none() {
        context.config_dir = std::env::var_os(CONFIG_DIR_ENV).map(PathBuf::from);
    }
    context
}

fn settings_dir_from_context(context: &CliPathContext) -> PathBuf {
    let base_dir = context
        .config_dir
        .clone()
        .unwrap_or_else(default_app_settings_dir);
    match context.profile.as_deref() {
        Some(profile) => base_dir.join("profiles").join(profile),
        None => base_dir,
    }
}

fn default_app_settings_dir() -> PathBuf {
    oxideterm_settings::default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn config_dir_display(context: &CliPathContext, settings_dir: &Path) -> String {
    context
        .config_dir
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| settings_dir.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_paths_use_expected_file_names() {
        let paths = cli_paths();

        assert!(paths.settings.ends_with("settings.json"));
        assert!(paths.connections.ends_with(CONNECTIONS_FILE_NAME));
        assert!(paths.forwards.ends_with(FORWARDS_FILE_NAME));
        assert!(paths.cloud_sync.ends_with("cloud_sync.json"));
        assert!(paths.backups_dir.ends_with(BACKUPS_DIR_NAME));
    }
}
