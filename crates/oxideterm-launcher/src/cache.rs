// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, path::PathBuf};

use oxideterm_settings::default_settings_path;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const CACHE_MAX_AGE_SECS: u64 = 7 * 86_400; // Tauri icon cache freshness.

pub fn icon_cache_dir() -> PathBuf {
    default_settings_path()
        .parent()
        .map(|parent| parent.join("launcher_icons"))
        .unwrap_or_else(|| PathBuf::from("launcher_icons"))
}

pub fn clear_icon_cache() -> Result<(), String> {
    let icon_cache_dir = icon_cache_dir();
    if icon_cache_dir.exists() {
        fs::remove_dir_all(&icon_cache_dir)
            .map_err(|error| format!("Failed to clear icon cache: {error}"))?;
    }
    Ok(())
}
