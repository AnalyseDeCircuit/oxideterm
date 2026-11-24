// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::Context;
use oxideterm_connections::oxide_file::{
    ExportPreflightResult, ImportConflictStrategy, ImportPreview, OxideMetadata,
};
use oxideterm_forwarding::PersistedForward;
use oxideterm_settings::{
    ALL_OXIDE_SETTINGS_SECTIONS as OXIDE_APP_SETTINGS_SECTIONS, DEFAULT_OXIDE_SETTINGS_SECTIONS,
};
use std::{collections::HashSet, path::PathBuf, sync::Arc};
use zeroize::Zeroizing;

#[derive(Clone, Debug, Default)]
pub struct OxideImportResultView {
    pub imported: usize,
    pub skipped: usize,
    pub merged: usize,
    pub replaced: usize,
    pub renamed: usize,
    pub renames: Vec<(String, String)>,
    pub errors: Vec<String>,
    pub imported_forwards: usize,
    pub skipped_forwards: usize,
    pub imported_app_settings: bool,
    pub skipped_app_settings: bool,
    pub imported_quick_commands: usize,
    pub skipped_quick_commands: bool,
    pub imported_serial_profiles: usize,
    pub skipped_serial_profiles: usize,
    pub imported_telnet_profiles: usize,
    pub skipped_telnet_profiles: usize,
    pub imported_mosh_profiles: usize,
    pub skipped_mosh_profiles: usize,
    pub quick_commands_errors: Vec<String>,
    pub imported_plugin_settings: usize,
    pub skipped_plugin_settings: bool,
    pub imported_portable_secrets: usize,
    pub skipped_portable_secrets: usize,
}

#[derive(Clone, Debug)]
pub struct OxideTransferProgress {
    pub stage: String,
    pub current: usize,
    pub total: usize,
}

impl OxideTransferProgress {
    pub fn new(stage: impl Into<String>, current: usize, total: usize) -> Self {
        Self {
            stage: stage.into(),
            current,
            total,
        }
    }

    pub fn percent(&self) -> usize {
        (self.current.min(self.total) * 100)
            .checked_div(self.total)
            .unwrap_or(0)
            .min(100)
    }
}

impl SessionManagerState {
    pub fn clear_ssh_config_hosts(&mut self, cx: &mut Context<Self>) {
        self.ssh_config_load_generation = self.ssh_config_load_generation.wrapping_add(1);
        self.ssh_config_load_task = None;
        if !self.ssh_config_hosts.is_empty() {
            self.ssh_config_hosts.clear();
            cx.notify();
        }
    }

    pub fn refresh_ssh_config_hosts(
        &mut self,
        enabled: bool,
        runtime: Arc<tokio::runtime::Runtime>,
        store: &oxideterm_connections::ConnectionStore,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut Context<Self>,
    ) {
        if !enabled {
            self.clear_ssh_config_hosts(cx);
            return;
        }
        let existing_names = store
            .connections()
            .iter()
            .map(|saved| saved.name.clone())
            .collect();
        self.begin_ssh_config_host_load(
            runtime,
            existing_names,
            i18n.t("settings_view.connections.ssh_config.load_failed"),
            cx,
        );
    }

    pub fn import_ssh_config_host(
        &mut self,
        store: &mut oxideterm_connections::ConnectionStore,
        alias: &str,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let (imported, status) = match oxideterm_connections::import_ssh_config_alias(store, alias)
        {
            Ok(true) => {
                self.remove_ssh_config_host_alias(alias, cx);
                (
                    true,
                    i18n.t("settings_view.errors.import_success")
                        .replace("{{name}}", alias),
                )
            }
            Ok(false) => (
                false,
                i18n.t("settings_view.connections.ssh_config.batch_import_skipped")
                    .replace("{{count}}", "1"),
            ),
            Err(error) => (
                false,
                i18n.t("settings_view.errors.import_failed")
                    .replace("{{error}}", &error.to_string()),
            ),
        };
        self.set_status(Some(status), cx);
        imported
    }

    fn begin_ssh_config_host_load(
        &mut self,
        runtime: Arc<tokio::runtime::Runtime>,
        existing_names: HashSet<String>,
        load_failed_template: String,
        cx: &mut Context<Self>,
    ) {
        self.ssh_config_load_generation = self.ssh_config_load_generation.wrapping_add(1);
        let generation = self.ssh_config_load_generation;
        // Entity lifetime owns discovery; closing its tab only hides the view.
        self.ssh_config_load_task = Some(cx.spawn(async move |entity, cx| {
            let result = runtime
                .spawn_blocking(move || {
                    oxideterm_connections::list_ssh_config_hosts(&existing_names)
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = entity.update(cx, |session_manager, cx| {
                if generation != session_manager.ssh_config_load_generation {
                    return;
                }
                match result {
                    Ok(hosts) => {
                        session_manager.ssh_config_hosts = hosts;
                    }
                    Err(error) => {
                        session_manager.ssh_config_hosts.clear();
                        session_manager.status =
                            Some(load_failed_template.replace("{{error}}", &error));
                    }
                }
                cx.notify();
            });
        }));
    }

    pub fn remove_ssh_config_host_alias(&mut self, alias: &str, cx: &mut Context<Self>) {
        let previous_count = self.ssh_config_hosts.len();
        self.ssh_config_hosts.retain(|host| host.alias != alias);
        if self.ssh_config_hosts.len() != previous_count {
            cx.notify();
        }
    }
}

pub struct OxideImportDialogState {
    pub presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub file_path: Option<PathBuf>,
    pub file_data: Option<Arc<[u8]>>,
    pub metadata_summary: Option<String>,
    pub metadata: Option<OxideMetadata>,
    pub password: Zeroizing<String>,
    pub conflict_strategy: ImportConflictStrategy,
    pub preview: Option<Arc<ImportPreview>>,
    pub selected_names: HashSet<String>,
    pub import_app_settings: bool,
    pub selected_app_settings_sections: HashSet<String>,
    pub expanded_app_settings_sections: HashSet<String>,
    pub import_quick_commands: bool,
    pub import_serial_profiles: bool,
    pub import_telnet_profiles: bool,
    pub import_mosh_profiles: bool,
    pub import_plugin_settings: bool,
    pub selected_plugin_ids: HashSet<String>,
    pub import_forwards: bool,
    pub import_portable_secrets: bool,
    pub restore_managed_keys: bool,
    pub restore_managed_key_passphrases: bool,
    pub busy: bool,
    pub operation_generation: u64,
    pub progress_stage: Option<OxideTransferProgress>,
    pub focused_footer_action: Option<OxideDialogFooterAction>,
    pub error: Option<String>,
    pub result_summary: Option<String>,
    pub result: Option<Arc<OxideImportResultView>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OxideDialogFooterAction {
    Cancel,
    Secondary,
    Primary,
}

impl Default for OxideImportDialogState {
    fn default() -> Self {
        Self {
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            file_path: None,
            file_data: None,
            metadata_summary: None,
            metadata: None,
            password: Zeroizing::new(String::new()),
            conflict_strategy: ImportConflictStrategy::Rename,
            preview: None,
            selected_names: HashSet::new(),
            import_app_settings: true,
            selected_app_settings_sections: OXIDE_APP_SETTINGS_SECTIONS
                .iter()
                .map(|section| (*section).to_string())
                .collect(),
            expanded_app_settings_sections: HashSet::new(),
            import_quick_commands: true,
            import_serial_profiles: true,
            import_telnet_profiles: true,
            import_mosh_profiles: true,
            import_plugin_settings: true,
            selected_plugin_ids: HashSet::new(),
            import_forwards: true,
            import_portable_secrets: false,
            restore_managed_keys: true,
            restore_managed_key_passphrases: false,
            busy: false,
            operation_generation: 0,
            progress_stage: None,
            focused_footer_action: Some(OxideDialogFooterAction::Secondary),
            error: None,
            result_summary: None,
            result: None,
        }
    }
}

impl std::fmt::Debug for OxideImportDialogState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OxideImportDialogState")
            .field("file_path", &self.file_path)
            .field("file_data", &self.file_data.as_ref().map(|data| data.len()))
            .field("metadata_summary", &self.metadata_summary)
            .field("metadata", &self.metadata)
            .field("password", &"[redacted secret]")
            .field("conflict_strategy", &self.conflict_strategy)
            .field("preview", &self.preview)
            .field("selected_names", &self.selected_names)
            .field("import_app_settings", &self.import_app_settings)
            .field(
                "selected_app_settings_sections",
                &self.selected_app_settings_sections,
            )
            .field(
                "expanded_app_settings_sections",
                &self.expanded_app_settings_sections,
            )
            .field("import_quick_commands", &self.import_quick_commands)
            .field("import_serial_profiles", &self.import_serial_profiles)
            .field("import_telnet_profiles", &self.import_telnet_profiles)
            .field("import_mosh_profiles", &self.import_mosh_profiles)
            .field("import_plugin_settings", &self.import_plugin_settings)
            .field("selected_plugin_ids", &self.selected_plugin_ids)
            .field("import_forwards", &self.import_forwards)
            .field("import_portable_secrets", &self.import_portable_secrets)
            .field("restore_managed_keys", &self.restore_managed_keys)
            .field(
                "restore_managed_key_passphrases",
                &self.restore_managed_key_passphrases,
            )
            .field("busy", &self.busy)
            .field("operation_generation", &self.operation_generation)
            .field("progress_stage", &self.progress_stage)
            .field("focused_footer_action", &self.focused_footer_action)
            .field("error", &self.error)
            .field("result_summary", &self.result_summary)
            .field("result", &self.result)
            .finish()
    }
}

pub struct OxideExportDialogState {
    pub presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub selected_ids: HashSet<String>,
    pub connection_rows: Arc<[OxideExportConnectionRow]>,
    pub forward_group_rows: Arc<[OxideExportForwardGroupRow]>,
    pub available_forwards: Vec<PersistedForward>,
    pub selected_forward_ids: HashSet<String>,
    pub include_app_settings: bool,
    pub selected_app_settings_sections: HashSet<String>,
    pub include_local_terminal_env_vars: bool,
    pub include_quick_commands: bool,
    pub include_serial_profiles: bool,
    pub include_telnet_profiles: bool,
    pub include_mosh_profiles: bool,
    pub include_remote_desktop_profiles: bool,
    pub include_ble_profiles: bool,
    pub include_plugin_settings: bool,
    pub plugin_groups: HashMap<String, usize>,
    pub selected_plugin_ids: HashSet<String>,
    pub include_forwards: bool,
    pub include_portable_secrets: bool,
    pub embed_keys: bool,
    pub include_passwords: bool,
    pub include_key_passphrases: bool,
    pub include_managed_keys: bool,
    pub include_managed_key_passphrases: bool,
    pub password: Zeroizing<String>,
    pub confirm_password: Zeroizing<String>,
    pub description: String,
    pub busy: bool,
    pub operation_generation: u64,
    pub progress_stage: Option<OxideTransferProgress>,
    pub focused_footer_action: Option<OxideDialogFooterAction>,
    pub last_export_timestamp: Option<i64>,
    pub preflight: Option<ExportPreflightResult>,
    pub error: Option<String>,
    pub result_summary: Option<String>,
}

impl Default for OxideExportDialogState {
    fn default() -> Self {
        Self {
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            selected_ids: HashSet::new(),
            connection_rows: Arc::from([]),
            forward_group_rows: Arc::from([]),
            available_forwards: Vec::new(),
            selected_forward_ids: HashSet::new(),
            include_app_settings: true,
            selected_app_settings_sections: DEFAULT_OXIDE_SETTINGS_SECTIONS
                .iter()
                .map(|section| (*section).to_string())
                .collect(),
            include_local_terminal_env_vars: false,
            include_quick_commands: true,
            include_serial_profiles: true,
            include_telnet_profiles: true,
            include_mosh_profiles: true,
            include_remote_desktop_profiles: true,
            include_ble_profiles: true,
            include_plugin_settings: true,
            plugin_groups: HashMap::new(),
            selected_plugin_ids: HashSet::new(),
            include_forwards: true,
            include_portable_secrets: false,
            embed_keys: false,
            include_passwords: false,
            include_key_passphrases: true,
            include_managed_keys: true,
            include_managed_key_passphrases: false,
            password: Zeroizing::new(String::new()),
            confirm_password: Zeroizing::new(String::new()),
            description: String::new(),
            busy: false,
            operation_generation: 0,
            progress_stage: None,
            focused_footer_action: Some(OxideDialogFooterAction::Cancel),
            last_export_timestamp: None,
            preflight: None,
            error: None,
            result_summary: None,
        }
    }
}

impl std::fmt::Debug for OxideExportDialogState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OxideExportDialogState")
            .field("selected_ids", &self.selected_ids)
            .field("available_forwards", &self.available_forwards)
            .field("selected_forward_ids", &self.selected_forward_ids)
            .field("include_app_settings", &self.include_app_settings)
            .field(
                "selected_app_settings_sections",
                &self.selected_app_settings_sections,
            )
            .field(
                "include_local_terminal_env_vars",
                &self.include_local_terminal_env_vars,
            )
            .field("include_quick_commands", &self.include_quick_commands)
            .field("include_serial_profiles", &self.include_serial_profiles)
            .field("include_telnet_profiles", &self.include_telnet_profiles)
            .field("include_mosh_profiles", &self.include_mosh_profiles)
            .field(
                "include_remote_desktop_profiles",
                &self.include_remote_desktop_profiles,
            )
            .field("include_ble_profiles", &self.include_ble_profiles)
            .field("include_plugin_settings", &self.include_plugin_settings)
            .field("plugin_groups", &self.plugin_groups)
            .field("selected_plugin_ids", &self.selected_plugin_ids)
            .field("include_forwards", &self.include_forwards)
            .field("include_portable_secrets", &self.include_portable_secrets)
            .field("embed_keys", &self.embed_keys)
            .field("include_passwords", &self.include_passwords)
            .field("include_key_passphrases", &self.include_key_passphrases)
            .field("include_managed_keys", &self.include_managed_keys)
            .field(
                "include_managed_key_passphrases",
                &self.include_managed_key_passphrases,
            )
            .field("password", &"[redacted secret]")
            .field("confirm_password", &"[redacted secret]")
            .field("description", &self.description)
            .field("busy", &self.busy)
            .field("operation_generation", &self.operation_generation)
            .field("progress_stage", &self.progress_stage)
            .field("focused_footer_action", &self.focused_footer_action)
            .field("last_export_timestamp", &self.last_export_timestamp)
            .field("preflight", &self.preflight)
            .field("error", &self.error)
            .field("result_summary", &self.result_summary)
            .finish()
    }
}
