// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    cell::RefCell,
    collections::{HashSet, VecDeque},
    fs,
    ops::DerefMut,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use super::{
    OxideTransferProgress, SessionManagerState, SessionManagerWorkspaceEvent,
    oxide_export_connection_count,
};
use chrono::{Local, Utc};
use gpui::{App, Context, Entity, Timer};
use oxideterm_audit::{AuditContext, AuditEvidence, AuditOutcome};
use oxideterm_connections::{
    ConnectionStore,
    oxide_file::{
        ImportPreview, ImportResultEnvelope, OxideFile, OxideFileError, OxideImportOptions,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_quick_commands::QuickCommandImportStrategy;
use oxideterm_workspace::delivery;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct OxideClientStateImportOptions {
    pub oxide_options: OxideImportOptions,
    pub import_quick_commands: bool,
    pub quick_command_strategy: QuickCommandImportStrategy,
    pub import_plugin_settings: bool,
    pub selected_plugin_ids: Option<HashSet<String>>,
    pub import_app_settings: bool,
    pub selected_app_settings_sections: Option<HashSet<String>>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct OxideClientStateImportResult {
    pub envelope: ImportResultEnvelope,
    pub imported_app_settings: bool,
    pub skipped_app_settings: bool,
    pub imported_quick_commands: usize,
    pub skipped_quick_commands: bool,
    pub quick_commands_errors: Vec<String>,
    pub imported_plugin_settings: usize,
    pub skipped_plugin_settings: bool,
}

impl OxideClientStateImportResult {
    pub fn audit_counts(&self, plugin_settings_failed: bool, app_settings_requested: bool) -> (usize, usize) {
        let applied = self.envelope.imported
            + self.envelope.merged
            + self.envelope.replaced
            + self.envelope.renamed
            + self.envelope.imported_forwards
            + self.envelope.imported_serial_profiles
            + self.envelope.imported_telnet_profiles
            + self.envelope.imported_mosh_profiles
            + self.envelope.imported_standalone_sftp_profiles
            + self.envelope.imported_remote_desktop_profiles
            + self.envelope.restored_managed_keys
            + self.envelope.restored_profile_credentials
            + self.envelope.cleared_profile_credentials
            + self.imported_quick_commands
            + self.imported_plugin_settings
            + usize::from(self.imported_app_settings)
            + self.envelope.imported_portable_secrets;
        let errors = self.envelope.errors.len()
            + self.quick_commands_errors.len()
            + usize::from(plugin_settings_failed)
            + usize::from(app_settings_requested && self.skipped_app_settings);
        (applied, errors)
    }
}

pub struct OxideCoreImportResult {
    pub store: ConnectionStore,
    pub envelope: ImportResultEnvelope,
}

pub enum OxideWorkerDelivery {
    PreviewProgress {
        generation: u64,
        progress: OxideTransferProgress,
    },
    PreviewDone {
        generation: u64,
        result: Result<ImportPreview, String>,
        password: zeroize::Zeroizing<String>,
    },
    ImportProgress {
        generation: u64,
        progress: OxideTransferProgress,
    },
    ImportDone {
        generation: u64,
        result: Result<OxideCoreImportResult, String>,
        options: OxideClientStateImportOptions,
        password: zeroize::Zeroizing<String>,
        audit: oxideterm_audit::AuditOperation,
        child_context: Option<oxideterm_audit::AuditContext>,
    },
    ExportProgress {
        generation: u64,
        progress: OxideTransferProgress,
    },
    ExportDone {
        generation: u64,
        result: Result<Vec<u8>, String>,
        password: zeroize::Zeroizing<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OxideWorkerKey {
    Preview(u64),
    Import(u64),
    Export(u64),
}

enum OxideWorkspaceEffect {
    ApplyImport {
        generation: u64,
        core: OxideCoreImportResult,
        options: OxideClientStateImportOptions,
        audit: oxideterm_audit::AuditOperation,
        child_context: Option<oxideterm_audit::AuditContext>,
    },
    SaveExport {
        generation: u64,
        bytes: Vec<u8>,
        exported_count: usize,
    },
}

pub struct OxideWorkspaceEffects {
    effects: RefCell<VecDeque<OxideWorkspaceEffect>>,
}

impl OxideWorkspaceEffects {
    fn new(effects: VecDeque<OxideWorkspaceEffect>) -> Self {
        Self {
            effects: RefCell::new(effects),
        }
    }

    pub fn apply<C: DerefMut<Target = App>>(
        &self,
        manager: &Entity<SessionManagerState>,
        settings_path: &Path,
        i18n: &I18n,
        cx: &mut C,
        mut apply_import: impl FnMut(
            OxideCoreImportResult,
            OxideClientStateImportOptions,
            &mut C,
        ) -> (usize, usize),
    ) {
        let effects = std::mem::take(&mut *self.effects.borrow_mut());
        for effect in effects {
            match effect {
                OxideWorkspaceEffect::ApplyImport {
                    generation,
                    core,
                    options,
                    mut audit,
                    child_context,
                } => {
                    let still_current = manager
                        .read(&**cx)
                        .oxide_import_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.operation_generation == generation);
                    if !still_current {
                        audit.finish(AuditOutcome::Partial, AuditEvidence::Protocol, None, None);
                        continue;
                    }
                    let (applied, errors) =
                        AuditContext::with_sync_request(child_context.as_ref(), || {
                            apply_import(core, options, cx)
                        });
                    audit.summary(&format!("mode=oxide,applied={applied},errors={errors}"));
                    let outcome = if errors > 0 {
                        AuditOutcome::Partial
                    } else if applied == 0 {
                        AuditOutcome::Unchanged
                    } else {
                        AuditOutcome::Succeeded
                    };
                    audit.finish(outcome, AuditEvidence::Protocol, None, None);
                }
                OxideWorkspaceEffect::SaveExport {
                    generation,
                    bytes,
                    exported_count,
                } => {
                    if manager
                        .read(&**cx)
                        .oxide_export_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.operation_generation == generation)
                    {
                        manager.update(&mut **cx, |state, cx| {
                            state.prompt_save_oxide_export(
                                bytes,
                                exported_count,
                                settings_path.to_path_buf(),
                                i18n,
                                cx,
                            );
                        });
                    }
                }
            }
        }
    }
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OxideClientStateSnapshot {
    #[serde(default)]
    last_export_timestamp: Option<i64>,
}

impl SessionManagerState {
    pub fn initialize_oxide_delivery(&mut self, cx: &mut Context<Self>) {
        let (sender, receiver) = delivery::ActiveDeliverySender::channel();
        let wake = sender.wake();
        let release_wake = wake.clone();
        cx.on_release(move |_, _| {
            // Import/export workers may outlive the window, but their waiter
            // must stop with the Entity that owns the delivery receiver.
            release_wake.stop();
        })
        .detach();
        let task_wake = wake;
        let delivery_task = cx.spawn(async move |session_manager, cx| {
            loop {
                task_wake.wait().await;
                let should_drain = task_wake.take();
                let stopped = task_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = session_manager
                        .update(cx, |session_manager, cx| {
                            session_manager.drain_oxide_deliveries(cx)
                        })
                        .unwrap_or(false);
                    if backlog_remaining {
                        task_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        });
        self.oxide_worker_tx = Some(sender);
        self.oxide_worker_rx = Some(receiver);
        self.oxide_delivery_task = Some(delivery_task);
    }

    pub fn oxide_worker_sender(
        &self,
    ) -> Option<delivery::ActiveDeliverySender<OxideWorkerDelivery>> {
        self.oxide_worker_tx.clone()
    }

    fn finish_preview(
        &mut self,
        generation: u64,
        result: Result<ImportPreview, String>,
        password: zeroize::Zeroizing<String>,
    ) {
        let Some(dialog) = self.oxide_import_dialog.as_mut() else {
            return;
        };
        if dialog.operation_generation != generation {
            return;
        }
        // Restore the exact UI draft owner returned by the worker.
        dialog.password = password;
        dialog.busy = false;
        dialog.progress_stage = None;
        match result {
            Ok(preview) => {
                dialog.metadata = Some(preview.metadata.clone());
                dialog.selected_names = preview
                    .records
                    .iter()
                    .map(|record| record.name.clone())
                    .collect();
                if !preview.app_settings_section_ids.is_empty() {
                    dialog.selected_app_settings_sections =
                        preview.app_settings_section_ids.iter().cloned().collect();
                }
                dialog.import_app_settings = preview.has_app_settings;
                dialog.import_quick_commands = preview.has_quick_commands;
                dialog.import_serial_profiles = preview.serial_profiles_count > 0;
                dialog.import_telnet_profiles = preview.telnet_profiles_count > 0;
                dialog.import_mosh_profiles = preview.mosh_profiles_count > 0;
                dialog.import_plugin_settings = preview.plugin_settings_count > 0;
                dialog.import_forwards = preview.total_forwards > 0;
                dialog.import_portable_secrets = false;
                dialog.selected_plugin_ids =
                    preview.plugin_settings_by_plugin.keys().cloned().collect();
                dialog.expanded_app_settings_sections.clear();
                dialog.result = None;
                dialog.result_summary = None;
                dialog.preview = Some(Arc::new(preview));
                dialog.error = None;
            }
            Err(error) => dialog.error = Some(error),
        }
    }

    fn drain_oxide_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(receiver) = self.oxide_worker_rx.as_ref() else {
            return false;
        };
        let batch = delivery::drain_channel(receiver, delivery::USER_ACTION_DELIVERY_BUDGET);
        let mut effects = VecDeque::new();
        let changed = !batch.items.is_empty();
        for message in batch.items {
            match message {
                OxideWorkerDelivery::PreviewProgress {
                    generation,
                    progress,
                } => {
                    if let Some(dialog) = self.oxide_import_dialog.as_mut()
                        && dialog.busy
                        && dialog.operation_generation == generation
                    {
                        dialog.progress_stage = Some(progress);
                    }
                }
                OxideWorkerDelivery::ImportProgress {
                    generation,
                    progress,
                } => {
                    if let Some(dialog) = self.oxide_import_dialog.as_mut()
                        && dialog.busy
                        && dialog.operation_generation == generation
                    {
                        dialog.progress_stage = Some(progress);
                    }
                }
                OxideWorkerDelivery::ExportProgress {
                    generation,
                    progress,
                } => {
                    if let Some(dialog) = self.oxide_export_dialog.as_mut()
                        && dialog.busy
                        && dialog.operation_generation == generation
                    {
                        dialog.progress_stage = Some(progress);
                    }
                }
                OxideWorkerDelivery::PreviewDone {
                    generation,
                    result,
                    password,
                } => {
                    self.reap_oxide_worker(OxideWorkerKey::Preview(generation));
                    self.finish_preview(generation, result, password);
                }
                OxideWorkerDelivery::ImportDone {
                    generation,
                    result,
                    options,
                    password,
                    audit,
                    child_context,
                } => {
                    self.reap_oxide_worker(OxideWorkerKey::Import(generation));
                    let dialog = self
                        .oxide_import_dialog
                        .as_mut()
                        .filter(|dialog| dialog.operation_generation == generation);
                    match (dialog, result) {
                        (Some(_), Ok(core)) => {
                            drop(password);
                            effects.push_back(OxideWorkspaceEffect::ApplyImport {
                                generation,
                                core,
                                options,
                                audit,
                                child_context,
                            });
                        }
                        (None, Ok(_)) => {
                            audit.finish(
                                oxideterm_audit::AuditOutcome::Partial,
                                oxideterm_audit::AuditEvidence::Protocol,
                                None,
                                None,
                            );
                        }
                        (dialog, Err(error)) => {
                            audit.finish(
                                oxideterm_audit::AuditOutcome::Failed,
                                oxideterm_audit::AuditEvidence::Protocol,
                                None,
                                None,
                            );
                            if let Some(dialog) = dialog {
                                dialog.password = password;
                                dialog.busy = false;
                                dialog.progress_stage = None;
                                dialog.error = Some(error);
                            }
                        }
                    }
                }
                OxideWorkerDelivery::ExportDone {
                    generation,
                    result,
                    password,
                } => {
                    self.reap_oxide_worker(OxideWorkerKey::Export(generation));
                    let Some(dialog) = self
                        .oxide_export_dialog
                        .as_mut()
                        .filter(|dialog| dialog.operation_generation == generation)
                    else {
                        continue;
                    };
                    match result {
                        Ok(bytes) => {
                            drop(password);
                            let exported_count = oxide_export_connection_count(dialog);
                            dialog.progress_stage =
                                Some(OxideTransferProgress::new("writing", 1, 1));
                            effects.push_back(OxideWorkspaceEffect::SaveExport {
                                generation,
                                bytes,
                                exported_count,
                            });
                        }
                        Err(error) => {
                            dialog.password = password;
                            dialog.busy = false;
                            dialog.progress_stage = None;
                            dialog.error = Some(error);
                        }
                    }
                }
            }
        }
        if !effects.is_empty() {
            cx.emit(SessionManagerWorkspaceEvent::OxideEffectsReady(
                OxideWorkspaceEffects::new(effects),
            ));
        }
        if changed {
            cx.notify();
        }
        batch.outcome.backlog_remaining
    }

    pub fn retain_oxide_worker(
        &mut self,
        key: OxideWorkerKey,
        worker: std::thread::JoinHandle<()>,
    ) {
        if let Some(previous) = self.oxide_worker_threads.insert(key, worker) {
            // A generation key cannot be reused while its operation is active.
            // Reap a completed predecessor defensively without retaining it.
            let _ = previous.join();
        }
    }

    pub fn reap_oxide_worker(&mut self, key: OxideWorkerKey) {
        if let Some(worker) = self.oxide_worker_threads.remove(&key) {
            // Completion is the worker's final send, so this join only closes
            // the tiny gap between channel delivery and thread return.
            let _ = worker.join();
        }
    }

    pub fn begin_import_dialog_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.oxide_import_dialog.as_mut() else {
            return false;
        };
        if dialog.busy {
            return false;
        }
        let Some(generation) = dialog.presence.begin_exit() else {
            return false;
        };
        self.focused_input = None;
        if delay.is_zero() {
            self.finish_import_dialog_exit(generation, cx);
            return true;
        }
        self.import_dialog_exit_task = Some(cx.spawn(async move |session_manager, cx| {
            Timer::after(delay).await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                session_manager.finish_import_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    pub fn finish_import_dialog_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self
            .oxide_import_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.presence.finish_exit(generation))
        {
            return false;
        }
        self.oxide_import_dialog = None;
        self.import_dialog_exit_task = None;
        cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        cx.notify();
        true
    }

    pub fn begin_export_dialog_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.oxide_export_dialog.as_mut() else {
            return false;
        };
        if dialog.busy {
            return false;
        }
        let Some(generation) = dialog.presence.begin_exit() else {
            return false;
        };
        self.focused_input = None;
        if delay.is_zero() {
            self.finish_export_dialog_exit(generation, cx);
            return true;
        }
        self.export_dialog_exit_task = Some(cx.spawn(async move |session_manager, cx| {
            Timer::after(delay).await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                session_manager.finish_export_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    pub fn finish_export_dialog_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self
            .oxide_export_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.presence.finish_exit(generation))
        {
            return false;
        }
        self.oxide_export_dialog = None;
        self.export_dialog_exit_task = None;
        cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        cx.notify();
        true
    }

    pub fn start_import_file_picker(
        &mut self,
        selection: impl std::future::Future<Output = Option<Result<(PathBuf, Arc<[u8]>), String>>>
        + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.import_file_picker_task.is_some() {
            return;
        }
        self.import_file_picker_task = Some(cx.spawn(async move |session_manager, cx| {
            let result = selection.await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                session_manager.import_file_picker_task = None;
                let Some(result) = result else {
                    return;
                };
                let Some(dialog) = session_manager.oxide_import_dialog.as_mut() else {
                    return;
                };
                match result {
                    Ok((path, bytes)) => match OxideFile::from_bytes(bytes.as_ref()) {
                        Ok(file) => {
                            let metadata = file.metadata;
                            dialog.file_path = Some(path);
                            dialog.file_data = Some(bytes);
                            dialog.metadata_summary = metadata.as_ref().map(|metadata| {
                                format!(
                                    "{} 个连接 · {}",
                                    metadata.num_connections,
                                    metadata
                                        .exported_at
                                        .with_timezone(&Local)
                                        .format("%Y-%m-%d %H:%M")
                                )
                            });
                            dialog.selected_names = metadata
                                .as_ref()
                                .map(|metadata| metadata.connection_names.iter().cloned().collect())
                                .unwrap_or_default();
                            dialog.expanded_app_settings_sections.clear();
                            dialog.metadata = metadata;
                            dialog.preview = None;
                            dialog.error = None;
                            dialog.result_summary = None;
                            dialog.result = None;
                        }
                        Err(error) => {
                            dialog.metadata = None;
                            dialog.error = Some(error.to_string());
                        }
                    },
                    Err(error) => dialog.error = Some(error),
                }
                cx.notify();
            });
        }));
    }

    pub fn start_export_file_picker(
        &mut self,
        save: impl std::future::Future<Output = Option<Result<PathBuf, String>>> + 'static,
        settings_path: PathBuf,
        success_template: String,
        exported_count: usize,
        cx: &mut Context<Self>,
    ) {
        if self.export_file_picker_task.is_some() {
            return;
        }
        self.export_file_picker_task = Some(cx.spawn(async move |session_manager, cx| {
            let result = save.await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                session_manager.export_file_picker_task = None;
                let Some(result) = result else {
                    if let Some(dialog) = session_manager.oxide_export_dialog.as_mut() {
                        dialog.busy = false;
                        dialog.progress_stage = None;
                    }
                    cx.notify();
                    return;
                };
                match result {
                    Ok(path) => {
                        let _ = persist_oxide_last_export_timestamp(&settings_path);
                        let summary = success_template
                            .replace("{{count}}", &exported_count.to_string())
                            .replace("{{path}}", path.to_string_lossy().as_ref());
                        session_manager.status = Some(summary);
                        session_manager.oxide_export_dialog = None;
                        session_manager.focused_input = None;
                        cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
                    }
                    Err(error) => {
                        if let Some(dialog) = session_manager.oxide_export_dialog.as_mut() {
                            dialog.busy = false;
                            dialog.progress_stage = None;
                            dialog.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        }));
    }

    pub fn schedule_import_auto_close(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(generation) = self
            .oxide_import_dialog
            .as_ref()
            .map(|dialog| dialog.operation_generation)
        else {
            return;
        };
        self.dialog_auto_close_task = Some(cx.spawn(async move |session_manager, cx| {
            Timer::after(delay).await;
            let _ = session_manager.update(cx, |session_manager, cx| {
                let should_close =
                    session_manager
                        .oxide_import_dialog
                        .as_ref()
                        .is_some_and(|dialog| {
                            dialog.operation_generation == generation
                                && dialog.error.is_none()
                                && dialog.result_summary.is_some()
                        });
                if should_close {
                    session_manager.oxide_import_dialog = None;
                    session_manager.focused_input = None;
                    cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
                }
                session_manager.dialog_auto_close_task = None;
                cx.notify();
            });
        }));
    }
}

pub fn oxide_file_error_message(error: OxideFileError, i18n: &oxideterm_i18n::I18n) -> String {
    match error {
        OxideFileError::DecryptionFailed => i18n.t("modals.import.error_password"),
        OxideFileError::ChecksumMismatch => i18n.t("modals.import.error_tampered"),
        OxideFileError::PasswordTooShort => i18n.t("export.error_password_too_short"),
        other => other.to_string(),
    }
}

pub fn persist_oxide_last_export_timestamp(settings_path: &std::path::Path) -> Result<(), String> {
    let path = oxide_client_state_path(settings_path);
    let mut snapshot = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|contents| serde_json::from_str::<OxideClientStateSnapshot>(&contents).ok())
            .unwrap_or_default()
    } else {
        OxideClientStateSnapshot::default()
    };
    snapshot.last_export_timestamp = Some(Utc::now().timestamp_millis());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(&snapshot).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

pub fn load_oxide_last_export_timestamp(settings_path: &std::path::Path) -> Option<i64> {
    let path = oxide_client_state_path(settings_path);
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str::<OxideClientStateSnapshot>(&contents)
        .ok()
        .and_then(|snapshot| snapshot.last_export_timestamp)
}

pub fn oxide_client_state_path(settings_path: &std::path::Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join("oxide-client-state.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_manager::OxideImportDialogState;
    use gpui::{AppContext, TestAppContext};

    #[gpui::test]
    fn oxide_delivery_updates_progress_without_a_workspace(cx: &mut TestAppContext) {
        let session_manager = cx.new(SessionManagerState::new);
        let sender = session_manager.read_with(cx, |session_manager, _cx| {
            session_manager
                .oxide_worker_sender()
                .expect("oxide worker sender")
        });
        session_manager.update(cx, |session_manager, _cx| {
            let mut dialog = OxideImportDialogState::default();
            dialog.busy = true;
            dialog.operation_generation = 7;
            session_manager.oxide_import_dialog = Some(dialog);
        });

        sender
            .send(OxideWorkerDelivery::PreviewProgress {
                generation: 7,
                progress: OxideTransferProgress::new("decrypting", 2, 4),
            })
            .expect("preview progress delivery");

        // The Entity drains progress even when no WorkspaceApp or page is mounted.
        cx.run_until_parked();

        session_manager.read_with(cx, |session_manager, _cx| {
            let progress = session_manager
                .oxide_import_dialog
                .as_ref()
                .and_then(|dialog| dialog.progress_stage.as_ref())
                .expect("oxide progress");
            assert_eq!(progress.current, 2);
            assert_eq!(progress.total, 4);
        });
    }

    #[gpui::test]
    fn preview_failure_restores_the_original_secret_without_a_workspace(cx: &mut TestAppContext) {
        let manager = cx.new(SessionManagerState::new);
        let sender = manager.update(cx, |manager, _| {
            manager.oxide_import_dialog = Some(OxideImportDialogState {
                busy: true,
                operation_generation: 7,
                ..Default::default()
            });
            manager
                .oxide_worker_sender()
                .expect("preview worker sender")
        });
        let password = zeroize::Zeroizing::new("delivery-secret".to_owned());
        let allocation = password.as_ptr();
        sender
            .send(OxideWorkerDelivery::PreviewDone {
                generation: 7,
                result: Err("Could not decrypt the file".to_owned()),
                password,
            })
            .expect("preview completion delivery");
        cx.run_until_parked();
        manager.read_with(cx, |manager, _| {
            let dialog = manager.oxide_import_dialog.as_ref().unwrap();
            assert!(!dialog.busy);
            assert_eq!(dialog.error.as_deref(), Some("Could not decrypt the file"));
            assert_eq!(dialog.password.as_ptr(), allocation);
        });
    }
}
