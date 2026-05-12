// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{LauncherListResponse, LauncherLoadResponse};

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

pub fn load_entries() -> Result<LauncherLoadResponse, String> {
    #[cfg(target_os = "windows")]
    {
        Ok(LauncherLoadResponse {
            apps: Vec::new(),
            icon_dir: None,
            wsl_distros: oxideterm_wsl_graphics::wsl::list_distros()
                .map_err(|error| error.to_string())?,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        list_apps().map(Into::into)
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

pub fn launch_wsl(distro: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        oxideterm_wsl_graphics::wsl::launch_distro(distro).map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = distro;
        Err("WSL is only available on Windows".to_string())
    }
}
