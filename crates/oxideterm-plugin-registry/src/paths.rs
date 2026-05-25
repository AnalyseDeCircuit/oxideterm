// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Filesystem path helpers for native plugin registry data.

use super::*;

pub fn native_plugins_dir(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(PLUGINS_DIR_NAME)
}

pub fn native_plugin_config_path(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(PLUGIN_CONFIG_FILENAME)
}

pub(crate) fn native_plugins_dir_from_config_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or(config_path)
        .join(PLUGINS_DIR_NAME)
}

pub(crate) fn settings_path_from_native_plugin_config_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or(config_path)
        .join("settings.json")
}
