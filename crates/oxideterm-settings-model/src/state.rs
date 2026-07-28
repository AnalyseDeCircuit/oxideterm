// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Persistent page-model state for settings surfaces.
//!
//! This is intentionally free of GPUI handles, anchors, list state, focus
//! handles, and rendered element caches. The app owns those view concerns while
//! this model owns settings-page business state and drafts.

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CliCompanionStatus {
    pub bundled: bool,
    pub installed: bool,
    pub install_path: Option<String>,
    pub legacy_installed: bool,
    pub legacy_install_path: Option<String>,
    pub bundle_path: Option<String>,
    pub app_version: String,
    pub matches_bundled: Option<bool>,
    pub needs_reinstall: bool,
}

#[derive(Clone, Debug)]
pub struct SettingsPageModel {
    pub settings_reset_confirm_open: bool,
    pub legal_notice_open: bool,
}

impl Default for SettingsPageModel {
    fn default() -> Self {
        Self {
            settings_reset_confirm_open: false,
            legal_notice_open: false,
        }
    }
}

impl SettingsPageModel {
    /// Opens or closes the settings reset confirmation without exposing the flag layout.
    pub fn set_settings_reset_confirm_open(&mut self, is_open: bool) {
        self.settings_reset_confirm_open = is_open;
    }
}
