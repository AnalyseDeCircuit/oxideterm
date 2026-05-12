// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::LauncherListResponse;

#[cfg(target_os = "macos")]
mod macos;

pub fn list_apps() -> Result<LauncherListResponse, String> {
    #[cfg(target_os = "macos")]
    {
        macos::list_apps()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(LauncherListResponse {
            apps: Vec::new(),
            icon_dir: None,
        })
    }
}

pub fn launch_app(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::launch_app(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err("Not supported on this platform".to_string())
    }
}
