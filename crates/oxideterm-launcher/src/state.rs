// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{LauncherAppEntry, LauncherLoadResponse, WslDistro, filter_apps, filter_wsl_distros};

#[derive(Clone, Debug)]
pub struct LauncherRuntimeState {
    pub enabled: bool,
    pub apps: Vec<LauncherAppEntry>,
    pub wsl_distros: Vec<WslDistro>,
    pub icon_dir: Option<String>,
    pub search_query: String,
    pub loading: bool,
    pub error: Option<String>,
    pub show_disable_confirm: bool,
    generation: u64,
}

impl LauncherRuntimeState {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            apps: Vec::new(),
            wsl_distros: Vec::new(),
            icon_dir: None,
            search_query: String::new(),
            loading: false,
            error: None,
            show_disable_confirm: false,
            generation: 0,
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
        self.error = None;
        self.show_disable_confirm = false;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
        self.apps.clear();
        self.wsl_distros.clear();
        self.icon_dir = None;
        self.search_query.clear();
        self.error = None;
        self.loading = false;
        self.show_disable_confirm = false;
        self.generation = self.generation.saturating_add(1);
    }

    pub fn clear_for_refresh(&mut self) {
        self.apps.clear();
        self.wsl_distros.clear();
        self.icon_dir = None;
        self.error = None;
    }

    pub fn begin_load(&mut self, force: bool, requires_opt_in: bool) -> Option<u64> {
        if (requires_opt_in && !self.enabled) || self.loading {
            return None;
        }
        if !force && (!self.apps.is_empty() || !self.wsl_distros.is_empty() || self.error.is_some())
        {
            return None;
        }
        self.loading = true;
        self.error = None;
        self.generation = self.generation.saturating_add(1);
        Some(self.generation)
    }

    pub fn apply_load_result(
        &mut self,
        generation: u64,
        result: Result<LauncherLoadResponse, String>,
        requires_opt_in: bool,
    ) -> bool {
        if generation != self.generation {
            return false;
        }
        self.loading = false;
        if requires_opt_in && !self.enabled {
            return true;
        }
        match result {
            Ok(response) => {
                self.apps = response.apps;
                self.icon_dir = response.icon_dir;
                self.wsl_distros = response.wsl_distros;
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error);
            }
        }
        true
    }

    pub fn filtered_apps(&self) -> Vec<LauncherAppEntry> {
        filter_apps(&self.apps, &self.search_query)
    }

    pub fn filtered_wsl_distros(&self) -> Vec<WslDistro> {
        filter_wsl_distros(&self.wsl_distros, &self.search_query)
    }

    pub fn mark_launch_error(&mut self, error: String) {
        self.error = Some(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_scan_result_does_not_overwrite_newer_state() {
        let mut state = LauncherRuntimeState::new(true);
        let old_generation = state.begin_load(true, true).unwrap();
        state.disable();
        state.enable();
        let new_generation = state.begin_load(true, true).unwrap();

        let response = LauncherLoadResponse {
            apps: vec![LauncherAppEntry {
                name: "Old".to_string(),
                path: "/Applications/Old.app".to_string(),
                bundle_id: None,
                icon_path: None,
            }],
            icon_dir: None,
            wsl_distros: Vec::new(),
        };
        assert!(!state.apply_load_result(old_generation, Ok(response), true));
        assert!(state.apps.is_empty());
        assert!(state.loading);

        let response = LauncherLoadResponse {
            apps: vec![LauncherAppEntry {
                name: "New".to_string(),
                path: "/Applications/New.app".to_string(),
                bundle_id: None,
                icon_path: None,
            }],
            icon_dir: None,
            wsl_distros: Vec::new(),
        };
        assert!(state.apply_load_result(new_generation, Ok(response), true));
        assert_eq!(state.apps[0].name, "New");
        assert!(!state.loading);
    }

    #[test]
    fn windows_load_does_not_require_launcher_opt_in() {
        let mut state = LauncherRuntimeState::new(false);
        let generation = state.begin_load(true, false).unwrap();
        assert!(state.apply_load_result(
            generation,
            Ok(LauncherLoadResponse {
                apps: Vec::new(),
                icon_dir: None,
                wsl_distros: vec![WslDistro {
                    name: "Ubuntu".to_string(),
                    is_default: true,
                    is_running: false,
                }],
            }),
            false,
        ));
        assert_eq!(state.filtered_wsl_distros()[0].name, "Ubuntu");
        assert!(!state.loading);
    }
}
