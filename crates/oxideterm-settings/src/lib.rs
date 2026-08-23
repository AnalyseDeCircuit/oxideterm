// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod background_images;
mod model;
mod normalize;
mod oxide_snapshot;
mod session_log_template;
mod store;

pub use background_images::{
    background_images_directory, clear_background_images, ensure_bundled_background_image,
    import_background_images, is_managed_background_image, is_supported_background_image,
    list_background_images, remove_background_image,
};
pub use model::*;
pub use normalize::{SanitizedSettings, sanitize_settings_value};
pub use oxide_snapshot::{
    ALL_OXIDE_SETTINGS_SECTIONS, DEFAULT_OXIDE_SETTINGS_SECTIONS, OXIDE_SETTINGS_FORMAT,
    OXIDE_SETTINGS_VERSION, export_oxide_settings_snapshot_json, merge_oxide_settings_snapshot,
};
pub use oxideterm_portable_runtime as portable_runtime;
pub use session_log_template::*;
pub use store::{
    DataDirectoryCheck, DataDirectoryInfo, SETTINGS_FILENAME, SettingsLoadResult,
    SettingsSaveResult, SettingsStore, SettingsStoreCheckpoint, check_data_directory,
    data_directory_info, default_settings_path, reset_data_directory, save_settings_to_path,
    set_data_directory,
};
