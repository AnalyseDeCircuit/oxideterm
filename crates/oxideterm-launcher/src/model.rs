// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LauncherAppEntry {
    pub name: String,
    pub path: String,
    pub bundle_id: Option<String>,
    pub icon_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LauncherListResponse {
    pub apps: Vec<LauncherAppEntry>,
    pub icon_dir: Option<String>,
}

pub use oxideterm_wsl_graphics::WslDistro;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LauncherLoadResponse {
    pub apps: Vec<LauncherAppEntry>,
    pub icon_dir: Option<String>,
    pub wsl_distros: Vec<WslDistro>,
}

impl From<LauncherListResponse> for LauncherLoadResponse {
    fn from(response: LauncherListResponse) -> Self {
        Self {
            apps: response.apps,
            icon_dir: response.icon_dir,
            wsl_distros: Vec::new(),
        }
    }
}
