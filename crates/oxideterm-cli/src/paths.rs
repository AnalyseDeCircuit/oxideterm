// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use serde::Serialize;

const CONNECTIONS_FILE_NAME: &str = "connections.json";
const FORWARDS_FILE_NAME: &str = "forwards.json";
const BACKUPS_DIR_NAME: &str = "backups";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliPaths {
    pub settings_dir: String,
    pub settings: String,
    pub connections: String,
    pub forwards: String,
    pub cloud_sync: String,
    pub backups_dir: String,
}

pub fn cli_paths() -> CliPaths {
    let settings = oxideterm_settings::default_settings_path();
    let settings_dir = settings
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let connections = settings_dir.join(CONNECTIONS_FILE_NAME);
    let forwards = settings_dir.join(FORWARDS_FILE_NAME);
    let cloud_sync = oxideterm_cloud_sync::state::default_cloud_sync_state_path(&settings);
    let backups_dir = settings_dir.join(BACKUPS_DIR_NAME);

    // Keep path calculation centralized so every CLI command reads the same files as the app.
    CliPaths {
        settings_dir: settings_dir.display().to_string(),
        settings: settings.display().to_string(),
        connections: connections.display().to_string(),
        forwards: forwards.display().to_string(),
        cloud_sync: cloud_sync.display().to_string(),
        backups_dir: backups_dir.display().to_string(),
    }
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
