// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod migration;
mod model;
mod normalize;
mod store;

pub use migration::{
    AGENT_ROLES_KEY, APP_LANG_KEY, CUSTOM_THEMES_KEY, KEYBINDINGS_KEY, LAUNCHER_ENABLED_KEY,
    LEGACY_FOCUSED_NODE_KEY, LEGACY_SETTINGS_KEY, LEGACY_TREE_EXPANDED_KEY, LEGACY_UI_STATE_KEY,
    NEW_CONNECTION_SAVE_KEY, SETTINGS_STORAGE_KEY, legacy_local_storage_value,
};
pub use model::*;
pub use normalize::{SanitizedSettings, sanitize_settings_value};
pub use store::{
    SETTINGS_FILENAME, SettingsLoadResult, SettingsSaveResult, SettingsStore, default_settings_path,
};
