// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/// User-facing labels copied from Tauri `src/locales/*/ide.json`.
///
/// The GPUI app can replace these with its i18n loader. English defaults keep
/// the crate testable without depending on the app-level locale crate.
#[derive(Clone, Debug)]
pub struct IdeLabels {
    pub open_folder: String,
    pub select_folder: String,
    pub select_folder_desc: String,
    pub go: String,
    pub go_to_parent: String,
    pub no_subfolders: String,
    pub selected_path: String,
    pub loading_project: String,
    pub open_failed: String,
    pub retry: String,
    pub disconnected_overlay: String,
    pub no_project: String,
    pub no_open_files: String,
    pub click_to_open: String,
    pub loading_file: String,
    pub save_failed: String,
    pub unsaved_changes: String,
    pub unsaved_changes_folder: String,
    pub unsaved_changes_desc: String,
    pub save: String,
    pub discard: String,
    pub cancel: String,
    pub sftp_mode: String,
}

impl Default for IdeLabels {
    fn default() -> Self {
        Self {
            open_folder: "Open Folder".into(),
            select_folder: "Select Folder".into(),
            select_folder_desc: "Select a remote folder to open".into(),
            go: "Go".into(),
            go_to_parent: "Go to parent directory".into(),
            no_subfolders: "No subfolders".into(),
            selected_path: "Selected path".into(),
            loading_project: "Loading project...".into(),
            open_failed: "Failed to open project".into(),
            retry: "Retry".into(),
            disconnected_overlay: "Connection lost. Reconnecting...".into(),
            no_project: "No project open".into(),
            no_open_files: "No open files".into(),
            click_to_open: "Click a file in the tree to open it".into(),
            loading_file: "Loading file...".into(),
            save_failed: "Save failed".into(),
            unsaved_changes: "Unsaved Changes".into(),
            unsaved_changes_folder:
                "You have unsaved changes. Switching folders will discard them. Continue?".into(),
            unsaved_changes_desc: "Do you want to save the changes to {{fileName}}?".into(),
            save: "Save".into(),
            discard: "Discard".into(),
            cancel: "Cancel".into(),
            sftp_mode: "SFTP".into(),
        }
    }
}
