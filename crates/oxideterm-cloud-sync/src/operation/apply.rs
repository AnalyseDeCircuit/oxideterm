// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncOperationService {
    pub fn apply_structured_preview(
        &self,
        connection_store: &mut ConnectionStore,
        forwarding_registry: &ForwardingRegistry,
        settings_store: &mut SettingsStore,
        _settings: &CloudSyncSettings,
        mut preview: StructuredPreview,
        selection: StructuredApplySelection,
        conflict_strategy: ConflictStrategy,
        sync_password: Option<&str>,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> Result<Option<ApplyStructuredPreviewOutcome>> {
        let Some(_permit) = self
            .guard
            .begin(CloudSyncOperationKind::ApplyPreview, false)?
        else {
            return Ok(None);
        };
        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        let app_settings_entry_ids = selection
            .app_settings_sections
            .iter()
            .filter(|section_id| preview.app_settings_entries.contains_key(*section_id))
            .cloned()
            .collect::<Vec<_>>();
        let plugin_entry_ids = selection
            .plugin_ids
            .iter()
            .filter(|plugin_id| preview.plugin_settings_entries.contains_key(*plugin_id))
            .cloned()
            .collect::<Vec<_>>();
        let apply_quick_commands =
            selection.quick_commands && preview.quick_commands_snapshot_json.is_some();
        let apply_serial_profiles =
            selection.serial_profiles && preview.serial_profiles_snapshot.is_some();
        let apply_sensitive_credentials =
            selection.sensitive_credentials && preview.sensitive_credentials_entry.is_some();
        let needs_password = !app_settings_entry_ids.is_empty() || !plugin_entry_ids.is_empty();
        let needs_password = needs_password || apply_sensitive_credentials;
        let sync_password = if needs_password {
            Some(required_sync_password(sync_password)?)
        } else {
            None
        };

        let total = (app_settings_entry_ids.len()
            + plugin_entry_ids.len()
            + usize::from(selection.connections && preview.connections_snapshot.is_some())
            + usize::from(selection.forwards && preview.forwards_snapshot.is_some())
            + usize::from(apply_quick_commands)
            + usize::from(apply_serial_profiles)
            + usize::from(apply_sensitive_credentials))
        .max(1);
        report_progress(progress, CloudSyncProgressStage::Importing, 0, total);

        let mut completed = 0usize;
        let content_summary = CloudSyncHistorySummary {
            connections: preview
                .connections_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or(0),
            forwards: preview
                .forwards_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or(0),
            quick_commands: preview
                .quick_commands_snapshot_json
                .as_deref()
                .and_then(|json| {
                    serde_json::from_str::<oxideterm_quick_commands::QuickCommandsSnapshot>(json)
                        .ok()
                        .map(|snapshot| snapshot.commands.len())
                })
                .unwrap_or(0),
            serial_profiles: preview
                .serial_profiles_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or(0),
            sensitive_credentials: preview
                .sensitive_credentials_preview
                .as_ref()
                .map(|preview| preview.total_connections + preview.portable_secret_count)
                .unwrap_or(0),
            has_app_settings: !preview.app_settings_entries.is_empty(),
            plugin_settings_count: preview
                .plugin_settings_counts
                .values()
                .copied()
                .sum::<usize>()
                .max(preview.plugin_settings_entries.len()),
        };
        let mut app_settings_snapshots = std::collections::BTreeMap::new();
        let mut plugin_settings_snapshot = Vec::new();
        let mut sensitive_credentials_envelope = None;
        if let Some(password) = sync_password {
            if apply_sensitive_credentials {
                if let Some(bytes) = preview.sensitive_credentials_entry.as_ref() {
                    let envelope = apply_oxide_import_with_options_with_progress(
                        connection_store,
                        bytes,
                        password,
                        OxideImportOptions {
                            selected_names: None,
                            selected_forward_ids: None,
                            conflict_strategy: ImportConflictStrategy::Merge,
                            import_forwards: false,
                            import_serial_profiles: false,
                            import_portable_secrets: true,
                            restore_managed_keys: true,
                            restore_managed_key_passphrases: true,
                        },
                        |stage, current, import_total| {
                            let fraction = fractional_import_progress(current, import_total);
                            report_fractional_progress(
                                progress,
                                host_import_progress_stage(stage, false),
                                (completed as f64 + fraction).min(total as f64),
                                total as f64,
                            );
                        },
                    )
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    sensitive_credentials_envelope = Some(envelope);
                    completed += 1;
                    report_progress(
                        progress,
                        CloudSyncProgressStage::Importing,
                        completed,
                        total,
                    );
                }
            }
            for section_id in &app_settings_entry_ids {
                let Some(bytes) = preview.app_settings_entries.get(section_id) else {
                    continue;
                };
                let envelope = apply_oxide_import_with_options_with_progress(
                    connection_store,
                    bytes,
                    password,
                    OxideImportOptions {
                        selected_names: Some(Vec::new()),
                        selected_forward_ids: None,
                        conflict_strategy: ImportConflictStrategy::Replace,
                        import_forwards: false,
                        import_portable_secrets: false,
                        ..OxideImportOptions::default()
                    },
                    |stage, current, import_total| {
                        let fraction = fractional_import_progress(current, import_total);
                        report_fractional_progress(
                            progress,
                            host_import_progress_stage(stage, false),
                            (completed as f64 + fraction).min(total as f64),
                            total as f64,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                if let Some(app_settings_json) = envelope.app_settings_json {
                    app_settings_snapshots.insert(section_id.clone(), app_settings_json);
                }
                completed += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::Importing,
                    completed,
                    total,
                );
            }

            for plugin_id in &plugin_entry_ids {
                let Some(bytes) = preview.plugin_settings_entries.get(plugin_id) else {
                    continue;
                };
                let envelope = apply_oxide_import_with_options_with_progress(
                    connection_store,
                    bytes,
                    password,
                    OxideImportOptions {
                        selected_names: Some(Vec::new()),
                        selected_forward_ids: None,
                        conflict_strategy: ImportConflictStrategy::Replace,
                        import_forwards: false,
                        import_portable_secrets: false,
                        ..OxideImportOptions::default()
                    },
                    |stage, current, import_total| {
                        let fraction = fractional_import_progress(current, import_total);
                        report_fractional_progress(
                            progress,
                            host_import_progress_stage(stage, false),
                            (completed as f64 + fraction).min(total as f64),
                            total as f64,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                plugin_settings_snapshot.extend(envelope.plugin_settings);
                completed += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::Importing,
                    completed,
                    total,
                );
            }
        }

        let requires_upload_after_merge = merge_structured_preview_fields(
            connection_store,
            forwarding_registry,
            settings_store,
            &mut preview,
            &selection,
            &conflict_strategy,
        )?;

        let connections_snapshot = if selection.connections {
            preview.connections_snapshot
        } else {
            None
        };
        let forwards_snapshot = if selection.forwards {
            preview.forwards_snapshot
        } else {
            None
        };
        let quick_commands_snapshot_json = if selection.quick_commands {
            preview.quick_commands_snapshot_json
        } else {
            None
        };
        let serial_profiles_snapshot = if selection.serial_profiles {
            preview.serial_profiles_snapshot
        } else {
            None
        };
        let connection_conflict_strategy = match conflict_strategy {
            ConflictStrategy::Skip => SavedConnectionsConflictStrategy::Skip,
            ConflictStrategy::Replace => SavedConnectionsConflictStrategy::Replace,
            ConflictStrategy::Merge | ConflictStrategy::Rename => {
                SavedConnectionsConflictStrategy::Merge
            }
        };
        let applied = apply_structured_snapshots(
            connection_store,
            forwarding_registry,
            settings_store,
            connections_snapshot,
            forwards_snapshot,
            quick_commands_snapshot_json,
            serial_profiles_snapshot,
            app_settings_snapshots,
            plugin_settings_snapshot,
            connection_conflict_strategy,
        )?;
        completed += usize::from(applied.connections.is_some())
            + usize::from(applied.forwards.is_some())
            + usize::from(apply_quick_commands)
            + usize::from(apply_serial_profiles);
        report_progress(
            progress,
            CloudSyncProgressStage::Importing,
            completed.min(total),
            total,
        );

        let local_snapshot = build_local_snapshot(
            connection_store,
            forwarding_registry,
            settings_store,
            None,
            None,
        )?;
        report_progress(progress, CloudSyncProgressStage::Done, total, total);

        let applied_selection = StructuredApplySelection {
            connections: applied.connections.as_ref().is_some_and(|outcome| {
                outcome.result.skipped == 0 && outcome.result.conflicts == 0
            }),
            forwards: applied
                .forwards
                .as_ref()
                .is_some_and(|outcome| outcome.skipped == 0),
            quick_commands: apply_quick_commands,
            serial_profiles: apply_serial_profiles,
            sensitive_credentials: apply_sensitive_credentials,
            app_settings_sections: app_settings_entry_ids,
            plugin_ids: plugin_entry_ids,
        };

        Ok(Some(ApplyStructuredPreviewOutcome {
            local_snapshot,
            applied,
            sensitive_credentials_envelope,
            content_summary,
            manifest: preview.manifest,
            remote_metadata: preview.remote_metadata,
            selection: applied_selection,
            requires_upload_after_merge,
        }))
    }

    pub fn apply_legacy_preview(
        &self,
        connection_store: &mut ConnectionStore,
        _settings: &CloudSyncSettings,
        preview: &LegacyPreview,
        sync_password: Option<&str>,
        import_connections: bool,
        selected_connection_names: Option<Vec<String>>,
        import_forwards: bool,
        import_portable_secrets: bool,
        conflict_strategy: ConflictStrategy,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> Result<Option<ApplyLegacyPreviewOutcome>> {
        let Some(_permit) = self
            .guard
            .begin(CloudSyncOperationKind::ApplyPreview, false)?
        else {
            return Ok(None);
        };
        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        let total = 1.0;
        report_fractional_progress(progress, CloudSyncProgressStage::Importing, 0.0, total);
        let password = required_sync_password(sync_password)?;
        let mut import_progress = |stage: &str, current: usize, total: usize| {
            report_fractional_progress(
                progress,
                host_import_progress_stage(stage, false),
                fractional_import_progress(current, total),
                1.0,
            );
        };
        let envelope = apply_oxide_import_with_options_with_progress(
            connection_store,
            &preview.bytes,
            password,
            OxideImportOptions {
                selected_names: legacy_preview_selected_names(
                    import_connections,
                    selected_connection_names,
                ),
                conflict_strategy: import_strategy_from_cloud(conflict_strategy),
                import_forwards,
                import_portable_secrets,
                ..OxideImportOptions::default()
            },
            &mut import_progress,
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        report_fractional_progress(progress, CloudSyncProgressStage::Done, total, total);
        Ok(Some(ApplyLegacyPreviewOutcome { envelope }))
    }
}
