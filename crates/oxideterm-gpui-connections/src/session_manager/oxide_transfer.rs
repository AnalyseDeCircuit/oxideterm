use super::*;
use gpui::{App, Entity};
use oxideterm_connections::oxide_file::OxideForwardRecord;
use oxideterm_connections::{
    ConnectionStore,
    oxide_file::{
        ExportPreflightResult, ImportConflictStrategy, OxideExportOptions, OxideImportOptions,
        apply_oxide_import_with_options_with_progress, export_connections_to_oxide_with_progress,
        preflight_export, preview_oxide_import_with_progress,
    },
};
use oxideterm_forwarding::{ForwardType, PersistedForward};
use oxideterm_i18n::I18n;
use oxideterm_quick_commands::QuickCommandImportStrategy;
use oxideterm_settings::export_oxide_settings_snapshot_json;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

pub fn oxide_export_draft(
    store: &ConnectionStore,
    forwards: Vec<oxideterm_forwarding::PersistedForward>,
    plugin_ids: impl IntoIterator<Item = String>,
    settings_path: &std::path::Path,
    portable_migration: bool,
) -> OxideExportDialogState {
    let available_forwards = exportable_saved_forwards(store, forwards);
    let connection_rows = store
        .connections()
        .iter()
        .map(OxideExportConnectionRow::from)
        .chain(
            store
                .plugin_profiles()
                .iter()
                .map(OxideExportConnectionRow::from),
        )
        .collect::<Vec<_>>()
        .into();
    let forward_group_rows =
        oxide_export_forward_group_rows(store.connections(), &available_forwards);
    let selected_forward_ids = available_forwards
        .iter()
        .map(|forward| forward.id.clone())
        .collect();
    let mut plugin_groups = std::collections::HashMap::new();
    for id in plugin_ids {
        *plugin_groups.entry(id).or_insert(0) += 1;
    }
    let selected_plugin_ids = plugin_groups.keys().cloned().collect();
    OxideExportDialogState {
        connection_rows,
        available_forwards,
        forward_group_rows,
        selected_forward_ids,
        plugin_groups,
        selected_plugin_ids,
        last_export_timestamp: load_oxide_last_export_timestamp(settings_path),
        include_portable_secrets: portable_migration,
        embed_keys: portable_migration,
        include_managed_key_passphrases: portable_migration,
        ..OxideExportDialogState::default()
    }
}

impl SessionManagerState {
    pub fn show_oxide_import_dialog(
        &mut self,
        portable_migration: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.oxide_import_dialog = Some(OxideImportDialogState {
            import_portable_secrets: portable_migration,
            restore_managed_key_passphrases: portable_migration,
            ..OxideImportDialogState::default()
        });
        self.focused_input = None;
        self.status = None;
        cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        cx.notify();
    }

    pub fn show_oxide_export_dialog(
        &mut self,
        dialog: OxideExportDialogState,
        cx: &mut gpui::Context<Self>,
    ) {
        self.oxide_export_dialog = Some(dialog);
        self.focused_input = None;
        self.status = None;
        cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        cx.notify();
    }
}

fn exportable_saved_forwards(
    store: &ConnectionStore,
    forwards: Vec<oxideterm_forwarding::PersistedForward>,
) -> Vec<PersistedForward> {
    let connection_ids = store
        .connections()
        .iter()
        .map(|connection| connection.id.clone())
        .collect::<HashSet<_>>();
    let mut forwards_by_key = HashMap::<String, PersistedForward>::new();

    for forward in forwards {
        let Some(owner_id) = forward.owner_connection_id.as_ref() else {
            continue;
        };
        if !connection_ids.contains(owner_id) {
            continue;
        }

        let key = oxide_forward_export_identity(&forward);
        match forwards_by_key.get(&key) {
            Some(existing) if existing.sync_updated_at() >= forward.sync_updated_at() => {}
            _ => {
                forwards_by_key.insert(key, forward);
            }
        }
    }

    let mut forwards = forwards_by_key.into_values().collect::<Vec<_>>();
    forwards.sort_by_key(|forward| {
        (
            forward.owner_connection_id.clone().unwrap_or_default(),
            forward.created_at,
        )
    });
    forwards
}

fn oxide_forward_export_identity(forward: &PersistedForward) -> String {
    if forward.tunnel.is_some() {
        return format!("tunnel:{}", forward.id);
    }
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        forward.owner_connection_id.as_deref().unwrap_or_default(),
        match forward.forward_type {
            ForwardType::Local => "local",
            ForwardType::Remote => "remote",
            ForwardType::Dynamic => "dynamic",
        },
        forward.rule.bind_address,
        forward.rule.bind_port,
        forward.rule.target_host,
        forward.rule.target_port
    )
}

fn oxide_export_has_content(dialog: &OxideExportDialogState) -> bool {
    !oxide_export_target_ids(dialog).is_empty()
        || (dialog.include_app_settings && !dialog.selected_app_settings_sections.is_empty())
        || dialog.include_quick_commands
        || dialog.include_serial_profiles
        || dialog.include_telnet_profiles
        || dialog.include_mosh_profiles
        || dialog.include_remote_desktop_profiles
        || dialog.include_ble_profiles
        || (dialog.include_plugin_settings && !dialog.selected_plugin_ids.is_empty())
        || dialog.include_portable_secrets
}

pub fn oxide_export_validation_error(dialog: &OxideExportDialogState) -> Option<&'static str> {
    if !oxide_export_has_content(dialog) {
        Some("export.error_select_something")
    } else if dialog.password.len() < 6 {
        Some("export.error_password_too_short")
    } else if dialog.password != dialog.confirm_password {
        Some("export.error_password_mismatch")
    } else if dialog
        .preflight
        .as_ref()
        .is_some_and(|preflight| !preflight.can_export)
    {
        Some("export.error_managed_keys_required")
    } else {
        None
    }
}
pub fn oxide_export_preflight(
    store: &ConnectionStore,
    dialog: &OxideExportDialogState,
    portable_secret_count: usize,
) -> ExportPreflightResult {
    let selected_ids = oxide_export_target_ids(dialog)
        .into_iter()
        .collect::<Vec<_>>();
    preflight_export(
        store,
        &selected_ids,
        dialog.embed_keys,
        dialog.include_managed_keys,
        portable_secret_count,
    )
}

pub fn oxide_export_target_ids(dialog: &OxideExportDialogState) -> HashSet<String> {
    let mut ids = dialog.selected_ids.clone();
    if dialog.include_forwards {
        for forward in &dialog.available_forwards {
            if dialog.selected_forward_ids.contains(&forward.id) {
                if let Some(owner_id) = &forward.owner_connection_id {
                    ids.insert(owner_id.clone());
                }
            }
        }
    }
    ids
}

pub fn oxide_export_options(
    dialog: &OxideExportDialogState,
    store: &ConnectionStore,
    settings: &oxideterm_settings::PersistedSettings,
    settings_path: &std::path::Path,
    plugin_settings: Vec<oxideterm_connections::oxide_file::EncryptedPluginSetting>,
    portable_secrets: Vec<oxideterm_connections::oxide_file::EncryptedPortableSecret>,
) -> Result<OxideExportOptions, String> {
    let app_settings_json = if dialog.include_app_settings {
        Some(
            export_oxide_settings_snapshot_json(
                settings,
                Some(&dialog.selected_app_settings_sections),
                dialog.include_local_terminal_env_vars,
            )
            .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let quick_commands_json = if dialog.include_quick_commands {
        Some(oxideterm_quick_commands::export_snapshot_json(
            settings_path,
        )?)
    } else {
        None
    };
    let serial_profiles_json = if dialog.include_serial_profiles {
        Some(
            serde_json::to_string_pretty(
                &store
                    .export_serial_profiles_snapshot()
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let telnet_profiles_json = if dialog.include_telnet_profiles {
        Some(
            serde_json::to_string_pretty(
                &store
                    .export_telnet_profiles_snapshot()
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let mosh_profiles_json = if dialog.include_mosh_profiles {
        Some(
            serde_json::to_string_pretty(
                &store
                    .export_mosh_profiles_snapshot()
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let remote_desktop_profiles_json = if dialog.include_remote_desktop_profiles {
        Some(
            serde_json::to_string_pretty(
                &store
                    .export_remote_desktop_profiles_snapshot()
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let ble_profiles_json = if dialog.include_ble_profiles {
        Some(
            serde_json::to_string_pretty(
                &store
                    .export_ble_profiles_snapshot()
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let selected_ids = oxide_export_target_ids(dialog);
    let forwards = if dialog.include_forwards {
        dialog
            .available_forwards
            .iter()
            .cloned()
            .filter_map(|forward| {
                let owner_id = forward.owner_connection_id?;
                (selected_ids.contains(&owner_id)
                    && dialog.selected_forward_ids.contains(&forward.id))
                .then(|| OxideForwardRecord {
                    id: Some(forward.id),
                    connection_id: owner_id,
                    forward_type: match forward.forward_type {
                        ForwardType::Local => "local".to_string(),
                        ForwardType::Remote => "remote".to_string(),
                        ForwardType::Dynamic => "dynamic".to_string(),
                    },
                    bind_address: forward.rule.bind_address,
                    bind_port: forward.rule.bind_port,
                    target_host: forward.rule.target_host,
                    target_port: forward.rule.target_port,
                    description: Some(forward.rule.description),
                    auto_start: forward.auto_start,
                    tunnel: forward.tunnel.map(|profile| {
                        oxideterm_connections::oxide_file::OxideTunnelProfile {
                            name: profile.name,
                            group: profile.group,
                            connect_on_startup: profile.connect_on_startup,
                        }
                    }),
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(OxideExportOptions {
        description: (!dialog.description.trim().is_empty())
            .then(|| dialog.description.trim().to_string()),
        embed_keys: dialog.embed_keys,
        include_passwords: dialog.include_passwords,
        include_key_passphrases: dialog.include_key_passphrases,
        include_managed_keys: dialog.include_managed_keys,
        include_managed_key_passphrases: dialog.include_managed_key_passphrases,
        app_settings_json,
        quick_commands_json,
        serial_profiles_json,
        telnet_profiles_json,
        mosh_profiles_json,
        remote_desktop_profiles_json,
        ble_profiles_json,
        plugin_settings,
        portable_secrets,
        forwards,
        ..OxideExportOptions::default()
    })
}

pub struct OxideTransfer<'a> {
    pub manager: &'a Entity<SessionManagerState>,
    pub store: &'a ConnectionStore,
    pub i18n: &'a I18n,
}

pub fn quick_command_strategy_from_oxide(
    strategy: ImportConflictStrategy,
) -> QuickCommandImportStrategy {
    match strategy {
        ImportConflictStrategy::Rename => QuickCommandImportStrategy::Rename,
        ImportConflictStrategy::Skip => QuickCommandImportStrategy::Skip,
        ImportConflictStrategy::Replace => QuickCommandImportStrategy::Replace,
        ImportConflictStrategy::Merge => QuickCommandImportStrategy::Merge,
    }
}

impl OxideTransfer<'_> {
    pub fn preview_oxide_import_dialog(&self, cx: &mut App) {
        let missing_file_error = self.i18n.t("modals.import.select_file");
        let missing_password_error = self.i18n.t("modals.import.error_enter_password");
        let Some((bytes, password, conflict_strategy, generation, sender)) =
            self.manager.update(cx, |session_manager, cx| {
                let Some(sender) = session_manager.oxide_worker_sender() else {
                    return None;
                };
                let Some(dialog) = session_manager.oxide_import_dialog.as_mut() else {
                    return None;
                };
                let Some(bytes) = dialog.file_data.clone() else {
                    dialog.error = Some(missing_file_error);
                    cx.notify();
                    return None;
                };
                if dialog.password.is_empty() {
                    dialog.error = Some(missing_password_error);
                    cx.notify();
                    return None;
                }
                // Move the sole password owner into the worker. The completion
                // delivery returns this same zeroizing allocation to the dialog.
                let password = std::mem::take(&mut dialog.password);
                dialog.busy = true;
                dialog.operation_generation = dialog.operation_generation.wrapping_add(1);
                dialog.progress_stage = Some(OxideTransferProgress::new("parsing_file", 1, 8));
                dialog.error = None;
                cx.notify();
                Some((
                    bytes,
                    password,
                    dialog.conflict_strategy,
                    dialog.operation_generation,
                    sender,
                ))
            })
        else {
            return;
        };
        let store = self.store.clone();
        let i18n = self.i18n.clone();
        let worker = std::thread::spawn(move || {
            let result = preview_oxide_import_with_progress(
                &store,
                bytes.as_ref(),
                &password,
                conflict_strategy,
                |stage, current, total| {
                    let _ = sender.send(OxideWorkerDelivery::PreviewProgress {
                        generation,
                        progress: OxideTransferProgress::new(stage, current, total),
                    });
                },
            );
            let _ = sender.send(OxideWorkerDelivery::PreviewDone {
                generation,
                result: result.map_err(|error| oxide_file_error_message(error, &i18n)),
                password,
            });
        });
        self.manager.update(cx, |session_manager, _cx| {
            session_manager.retain_oxide_worker(OxideWorkerKey::Preview(generation), worker);
        });
    }

    pub fn apply_oxide_import_dialog(&self, cx: &mut App) {
        let missing_file_error = self.i18n.t("modals.import.select_file");
        let missing_password_error = self.i18n.t("modals.import.error_enter_password");
        let Some((bytes, password, options, generation, sender)) =
            self.manager.update(cx, |session_manager, cx| {
                let Some(sender) = session_manager.oxide_worker_sender() else {
                    return None;
                };
                let Some(dialog) = session_manager.oxide_import_dialog.as_mut() else {
                    return None;
                };
                let Some(bytes) = dialog.file_data.clone() else {
                    dialog.error = Some(missing_file_error);
                    cx.notify();
                    return None;
                };
                if dialog.password.is_empty() {
                    dialog.error = Some(missing_password_error);
                    cx.notify();
                    return None;
                }
                let options = OxideClientStateImportOptions {
                    oxide_options: OxideImportOptions {
                        selected_names: Some(dialog.selected_names.iter().cloned().collect()),
                        selected_forward_ids: None,
                        conflict_strategy: dialog.conflict_strategy,
                        import_forwards: dialog.import_forwards,
                        import_serial_profiles: dialog.import_serial_profiles,
                        import_telnet_profiles: dialog.import_telnet_profiles,
                        import_mosh_profiles: dialog.import_mosh_profiles,
                        import_portable_secrets: dialog.import_portable_secrets,
                        restore_managed_keys: dialog.restore_managed_keys,
                        restore_managed_key_passphrases: dialog.restore_managed_key_passphrases,
                        ..OxideImportOptions::default()
                    },
                    import_quick_commands: dialog.import_quick_commands,
                    quick_command_strategy: quick_command_strategy_from_oxide(
                        dialog.conflict_strategy,
                    ),
                    import_plugin_settings: dialog.import_plugin_settings,
                    selected_plugin_ids: Some(dialog.selected_plugin_ids.clone()),
                    import_app_settings: dialog.import_app_settings
                        && !dialog.selected_app_settings_sections.is_empty(),
                    selected_app_settings_sections: Some(
                        dialog.selected_app_settings_sections.clone(),
                    ),
                };
                // The worker becomes the only password owner until completion.
                let password = std::mem::take(&mut dialog.password);
                dialog.busy = true;
                dialog.operation_generation = dialog.operation_generation.wrapping_add(1);
                dialog.progress_stage = Some(OxideTransferProgress::new("parsing_file", 1, 10));
                dialog.error = None;
                cx.notify();
                Some((
                    bytes,
                    password,
                    options,
                    dialog.operation_generation,
                    sender,
                ))
            })
        else {
            return;
        };
        let i18n = self.i18n.clone();
        let mut store = self.store.clone();
        let oxide_options = options.oxide_options.clone();
        let mut audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::Configuration,
            "configuration_import",
            None,
            Some("oxide_file"),
        );
        audit.summary(&format!(
            "mode=oxide,selected_connections={}",
            oxide_options
                .selected_names
                .as_ref()
                .map_or(0, |names| names.len())
        ));
        let child_context = oxideterm_audit::AuditContext::current_request()
            .or_else(oxideterm_audit::AuditContext::current)
            .map(|mut context| {
                context.parent_id = audit.id().map(str::to_owned);
                context
            });
        let worker = std::thread::spawn(move || {
            let result =
                oxideterm_audit::AuditContext::with_sync_request(child_context.as_ref(), || {
                    apply_oxide_import_with_options_with_progress(
                        &mut store,
                        bytes.as_ref(),
                        &password,
                        oxide_options,
                        |stage, current, total| {
                            let _ = sender.send(OxideWorkerDelivery::ImportProgress {
                                generation,
                                progress: OxideTransferProgress::new(stage, current, total),
                            });
                        },
                    )
                })
                .map(|envelope| OxideCoreImportResult { store, envelope })
                .map_err(|error| oxide_file_error_message(error, &i18n));
            let _ = sender.send(OxideWorkerDelivery::ImportDone {
                generation,
                result,
                options,
                password,
                audit,
                child_context,
            });
        });
        self.manager.update(cx, |session_manager, _cx| {
            session_manager.retain_oxide_worker(OxideWorkerKey::Import(generation), worker);
        });
    }

    pub fn present_oxide_import_result(&self, result: OxideClientStateImportResult, cx: &mut App) {
        let result_view = OxideImportResultView {
            imported: result.envelope.imported,
            skipped: result.envelope.skipped,
            merged: result.envelope.merged,
            replaced: result.envelope.replaced,
            renamed: result.envelope.renamed,
            renames: result.envelope.renames.clone(),
            errors: result.envelope.errors.clone(),
            imported_forwards: result.envelope.imported_forwards,
            skipped_forwards: result.envelope.skipped_forwards,
            imported_app_settings: result.imported_app_settings,
            skipped_app_settings: result.skipped_app_settings,
            imported_quick_commands: result.imported_quick_commands,
            skipped_quick_commands: result.skipped_quick_commands,
            imported_serial_profiles: result.envelope.imported_serial_profiles,
            skipped_serial_profiles: result.envelope.skipped_serial_profiles,
            imported_telnet_profiles: result.envelope.imported_telnet_profiles,
            skipped_telnet_profiles: result.envelope.skipped_telnet_profiles,
            imported_mosh_profiles: result.envelope.imported_mosh_profiles,
            skipped_mosh_profiles: result.envelope.skipped_mosh_profiles,
            quick_commands_errors: result.quick_commands_errors.clone(),
            imported_plugin_settings: result.imported_plugin_settings,
            skipped_plugin_settings: result.skipped_plugin_settings,
            imported_portable_secrets: result.envelope.imported_portable_secrets,
            skipped_portable_secrets: result.envelope.skipped_portable_secrets,
        };

        let mut parts = vec![
            self.i18n
                .t("modals.import.success")
                .replace("{{count}}", &result_view.imported.to_string()),
        ];
        if result_view.imported_forwards > 0 {
            parts.push(
                self.i18n
                    .t("modals.import.section_forwards")
                    .replace("{{count}}", &result_view.imported_forwards.to_string()),
            );
        }
        if result_view.imported_app_settings {
            parts.push(self.i18n.t("modals.import.imported_app_settings"));
        }
        if result_view.imported_quick_commands > 0 {
            parts.push(
                self.i18n
                    .t("modals.import.imported_quick_commands")
                    .replace(
                        "{{count}}",
                        &result_view.imported_quick_commands.to_string(),
                    ),
            );
        }
        if result_view.imported_serial_profiles > 0 {
            parts.push(
                self.i18n
                    .t("modals.import.imported_serial_profiles")
                    .replace(
                        "{{count}}",
                        &result_view.imported_serial_profiles.to_string(),
                    ),
            );
        }
        if result_view.imported_telnet_profiles > 0 {
            parts.push(
                self.i18n
                    .t("modals.import.imported_telnet_profiles")
                    .replace(
                        "{{count}}",
                        &result_view.imported_telnet_profiles.to_string(),
                    ),
            );
        }
        if result_view.imported_mosh_profiles > 0 {
            parts.push(
                self.i18n
                    .t("modals.import.imported_mosh_profiles")
                    .replace("{{count}}", &result_view.imported_mosh_profiles.to_string()),
            );
        }
        if result_view.imported_plugin_settings > 0 {
            parts.push(
                self.i18n
                    .t("modals.import.imported_plugin_settings")
                    .replace(
                        "{{count}}",
                        &result_view.imported_plugin_settings.to_string(),
                    ),
            );
        }
        if result_view.imported_portable_secrets > 0 {
            parts.push(
                self.i18n
                    .t("modals.import.imported_portable_secrets")
                    .replace(
                        "{{count}}",
                        &result_view.imported_portable_secrets.to_string(),
                    ),
            );
        }
        let auto_close_import_dialog = result_view.errors.is_empty();
        let result_error = (!result_view.errors.is_empty()).then(|| result_view.errors.join("; "));
        let result_summary = parts.join(" · ");
        self.manager.update(cx, |session_manager, cx| {
            let Some(dialog) = session_manager.oxide_import_dialog.as_mut() else {
                return;
            };
            dialog.busy = false;
            dialog.progress_stage = None;
            dialog.error = result_error;
            dialog.result_summary = Some(result_summary.clone());
            dialog.result = Some(Arc::new(result_view));
            session_manager.status = Some(result_summary);
            if auto_close_import_dialog {
                session_manager.schedule_import_auto_close(Duration::from_secs(2), cx);
            }
            cx.notify();
        });
    }

    pub fn start_export(
        &self,
        selected_ids: Vec<String>,
        preflight: ExportPreflightResult,
        options: OxideExportOptions,
        cx: &mut App,
    ) {
        let Some((password, generation, sender)) =
            self.manager.update(cx, |session_manager, cx| {
                let sender = session_manager.oxide_worker_sender()?;
                let dialog = session_manager.oxide_export_dialog.as_mut()?;
                // Both drafts are zeroizing owners. Move them out directly so
                // no plaintext password copy survives the submission boundary.
                let password = std::mem::take(&mut dialog.password);
                let confirm_password = std::mem::take(&mut dialog.confirm_password);
                dialog.busy = true;
                dialog.operation_generation = dialog.operation_generation.wrapping_add(1);
                dialog.progress_stage =
                    Some(OxideTransferProgress::new("collecting_connections", 0, 1));
                dialog.error = None;
                dialog.preflight = Some(preflight);
                drop(confirm_password);
                cx.notify();
                Some((password, dialog.operation_generation, sender))
            })
        else {
            return;
        };
        let store = self.store.clone();
        let i18n = self.i18n.clone();
        let worker = std::thread::spawn(move || {
            let result = export_connections_to_oxide_with_progress(
                &store,
                &selected_ids,
                &password,
                options,
                |stage, current, total| {
                    let _ = sender.send(OxideWorkerDelivery::ExportProgress {
                        generation,
                        progress: OxideTransferProgress::new(stage, current, total),
                    });
                },
            );
            let _ = sender.send(OxideWorkerDelivery::ExportDone {
                generation,
                result: result.map_err(|error| oxide_file_error_message(error, &i18n)),
                password,
            });
        });
        self.manager.update(cx, |session_manager, _cx| {
            session_manager.retain_oxide_worker(OxideWorkerKey::Export(generation), worker);
        });
    }
}
