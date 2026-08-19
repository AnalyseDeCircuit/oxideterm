// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncOperationService {
    pub(super) async fn build_structured_upload_plan(
        &self,
        connection_store: &ConnectionStore,
        forwarding_registry: &ForwardingRegistry,
        settings_store: &SettingsStore,
        local_snapshot: &CloudSyncLocalSnapshot,
        revision: &str,
        uploaded_at: &str,
        device_id: &str,
        sync_password: Option<&str>,
        portable_secrets: Vec<EncryptedPortableSecret>,
        item_filter: &StructuredUploadItemFilter,
        progress: &mut dyn CloudSyncProgressSink,
        total: usize,
    ) -> Result<StructuredUploadPlan> {
        let mut manifest = crate::create_manifest_base(
            revision.to_string(),
            uploaded_at.to_string(),
            device_id.to_string(),
            local_snapshot.scope.clone(),
        );
        let mut objects = Vec::new();
        let mut completed_exports = 0usize;
        let has_encrypted_objects = local_snapshot.scope.sync_sensitive_credentials
            || (local_snapshot.scope.sync_app_settings
                && !local_snapshot.scope.app_settings_sections.is_empty())
            || (local_snapshot.scope.sync_plugin_settings
                && !local_snapshot.metadata.plugin_settings_revisions.is_empty());
        let encryption_context = if has_encrypted_objects {
            let password =
                sync_password.context("missing_sync_password: cloud sync password is required")?;
            // A cloud upload contains multiple independently authenticated files.
            // Reuse one zeroizing batch key while retaining a fresh nonce per file.
            Some(
                OxideBatchEncryptionContext::new(password)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            )
        } else {
            None
        };

        if local_snapshot.scope.sync_connections {
            let mut snapshot = connection_store.export_saved_connections_snapshot()?;
            filter_saved_connection_snapshot(&mut snapshot, item_filter.connection_ids.as_ref());
            let bytes = serde_json::to_vec(&snapshot)?;
            let connections_revision = local_snapshot
                .metadata
                .saved_connections_revision
                .clone()
                .unwrap_or_else(|| snapshot.revision.clone());
            let path = connections_object_path(&connections_revision);
            manifest.sections.connections = Some(crate::StructuredObjectEntry {
                revision: connections_revision,
                path: path.clone(),
                record_count: Some(snapshot.records.len()),
                content_type: "application/json".to_string(),
            });
            objects.push(StructuredUploadObject {
                path,
                bytes,
                content_type: "application/json".to_string(),
            });
            completed_exports += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::Exporting,
                2 + completed_exports,
                total,
            );
        }

        if local_snapshot.scope.sync_forwards {
            let mut snapshot = forwarding_registry.export_saved_forwards_snapshot()?;
            filter_saved_forwards_snapshot(&mut snapshot, item_filter.forward_ids.as_ref());
            let bytes = serde_json::to_vec(&snapshot)?;
            let path = forwards_object_path(&snapshot.revision);
            manifest.sections.forwards = Some(crate::StructuredObjectEntry {
                revision: snapshot.revision.clone(),
                path: path.clone(),
                record_count: Some(snapshot.records.len()),
                content_type: "application/json".to_string(),
            });
            objects.push(StructuredUploadObject {
                path,
                bytes,
                content_type: "application/json".to_string(),
            });
            completed_exports += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::Exporting,
                2 + completed_exports,
                total,
            );
        }

        if local_snapshot.scope.sync_quick_commands {
            if let Some(revision) = local_snapshot.metadata.quick_commands_revision.as_ref() {
                let mut snapshot_json =
                    oxideterm_quick_commands::export_snapshot_json(settings_store.path())
                        .map_err(anyhow::Error::msg)?;
                let record_count = filter_quick_commands_snapshot_json(
                    &mut snapshot_json,
                    item_filter.quick_command_ids.as_ref(),
                );
                let path = quick_commands_object_path(revision);
                manifest.sections.quick_commands = Some(crate::StructuredObjectEntry {
                    revision: revision.clone(),
                    path: path.clone(),
                    record_count: Some(record_count),
                    content_type: "application/json".to_string(),
                });
                objects.push(StructuredUploadObject {
                    path,
                    bytes: snapshot_json.into_bytes(),
                    content_type: "application/json".to_string(),
                });
                completed_exports += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::Exporting,
                    2 + completed_exports,
                    total,
                );
            }
        }

        if local_snapshot.scope.sync_serial_profiles {
            let mut snapshot = connection_store.export_serial_profiles_snapshot()?;
            filter_serial_profiles_snapshot(&mut snapshot, item_filter.serial_profile_ids.as_ref());
            let bytes = serde_json::to_vec(&snapshot)?;
            let path = serial_profiles_object_path(&snapshot.revision);
            manifest.sections.serial_profiles = Some(crate::StructuredObjectEntry {
                revision: snapshot.revision.clone(),
                path: path.clone(),
                record_count: Some(snapshot.records.len()),
                content_type: "application/json".to_string(),
            });
            objects.push(StructuredUploadObject {
                path,
                bytes,
                content_type: "application/json".to_string(),
            });
            completed_exports += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::Exporting,
                2 + completed_exports,
                total,
            );
        }

        if local_snapshot.scope.sync_telnet_profiles {
            let mut snapshot = connection_store.export_telnet_profiles_snapshot()?;
            filter_telnet_profiles_snapshot(&mut snapshot, item_filter.telnet_profile_ids.as_ref());
            let bytes = serde_json::to_vec(&snapshot)?;
            let path = telnet_profiles_object_path(&snapshot.revision);
            manifest.sections.telnet_profiles = Some(crate::StructuredObjectEntry {
                revision: snapshot.revision.clone(),
                path: path.clone(),
                record_count: Some(snapshot.records.len()),
                content_type: "application/json".to_string(),
            });
            objects.push(StructuredUploadObject {
                path,
                bytes,
                content_type: "application/json".to_string(),
            });
            completed_exports += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::Exporting,
                2 + completed_exports,
                total,
            );
        }

        if local_snapshot.scope.sync_mosh_profiles {
            let mut snapshot = connection_store.export_mosh_profiles_snapshot()?;
            filter_mosh_profiles_snapshot(&mut snapshot, item_filter.mosh_profile_ids.as_ref());
            let bytes = serde_json::to_vec(&snapshot)?;
            let path = mosh_profiles_object_path(&snapshot.revision);
            manifest.sections.mosh_profiles = Some(crate::StructuredObjectEntry {
                revision: snapshot.revision.clone(),
                path: path.clone(),
                record_count: Some(snapshot.records.len()),
                content_type: "application/json".to_string(),
            });
            objects.push(StructuredUploadObject {
                path,
                bytes,
                content_type: "application/json".to_string(),
            });
            completed_exports += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::Exporting,
                2 + completed_exports,
                total,
            );
        }

        if local_snapshot.scope.sync_connections {
            let snapshot = connection_store.export_standalone_sftp_profiles_snapshot()?;
            let bytes = serde_json::to_vec(&snapshot)?;
            let connections_revision = local_snapshot
                .metadata
                .saved_connections_revision
                .clone()
                .unwrap_or_else(|| snapshot.revision.clone());
            let path = standalone_sftp_profiles_object_path(&connections_revision);
            manifest.sections.standalone_sftp_profiles = Some(crate::StructuredObjectEntry {
                revision: connections_revision,
                path: path.clone(),
                record_count: Some(snapshot.records.len()),
                content_type: "application/json".to_string(),
            });
            objects.push(StructuredUploadObject {
                path,
                bytes,
                content_type: "application/json".to_string(),
            });
            completed_exports += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::Exporting,
                2 + completed_exports,
                total,
            );
        }

        if local_snapshot.scope.sync_remote_desktop_profiles {
            let mut snapshot = connection_store.export_remote_desktop_profiles_snapshot()?;
            filter_remote_desktop_profiles_snapshot(
                &mut snapshot,
                item_filter.remote_desktop_profile_ids.as_ref(),
            );
            strip_remote_desktop_credential_refs(&mut snapshot);
            // Cloud profile objects are non-secret metadata; credentials and device-local
            // protected-store references never enter this plaintext structured object.
            let bytes = serde_json::to_vec(&snapshot)?;
            let path = remote_desktop_profiles_object_path(&snapshot.revision);
            manifest.sections.remote_desktop_profiles = Some(crate::StructuredObjectEntry {
                revision: snapshot.revision.clone(),
                path: path.clone(),
                record_count: Some(snapshot.records.len()),
                content_type: "application/json".to_string(),
            });
            objects.push(StructuredUploadObject {
                path,
                bytes,
                content_type: "application/json".to_string(),
            });
            completed_exports += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::Exporting,
                2 + completed_exports,
                total,
            );
        }

        if local_snapshot.scope.sync_sensitive_credentials {
            if let Some(revision) = local_snapshot
                .metadata
                .sensitive_credentials_revision
                .as_ref()
            {
                let connection_ids = connection_store
                    .connections()
                    .iter()
                    .map(|connection| connection.id.clone())
                    .filter(|connection_id| {
                        item_filter
                            .connection_ids
                            .as_ref()
                            .is_none_or(|ids| ids.contains(connection_id))
                    })
                    .collect::<Vec<_>>();
                let bytes = export_connections_to_oxide_with_context_and_progress(
                    connection_store,
                    &connection_ids,
                    OxideExportOptions {
                        description: Some("Cloud Sync sensitive credentials".to_string()),
                        embed_keys: false,
                        include_passwords: true,
                        include_key_passphrases: true,
                        include_managed_keys: true,
                        include_managed_key_passphrases: true,
                        portable_secrets,
                        ..OxideExportOptions::default()
                    },
                    encryption_context
                        .as_ref()
                        .context("missing cloud sync encryption context")?,
                    |_stage, current, export_total| {
                        report_fractional_progress(
                            progress,
                            CloudSyncProgressStage::Exporting,
                            export_progress_current(completed_exports, current, export_total),
                            total as f64,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let path = sensitive_credentials_object_path(revision);
                manifest.sections.sensitive_credentials = Some(crate::StructuredObjectEntry {
                    revision: revision.clone(),
                    path: path.clone(),
                    record_count: Some(local_snapshot.sensitive_credentials_record_count),
                    content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                });
                objects.push(StructuredUploadObject {
                    path,
                    bytes,
                    content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                });
                completed_exports += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::Exporting,
                    2 + completed_exports,
                    total,
                );
            }
        }

        if local_snapshot.scope.sync_app_settings {
            for section_id in &local_snapshot.scope.app_settings_sections {
                let Some(section_revision) = local_snapshot
                    .metadata
                    .app_settings_section_revisions
                    .get(section_id)
                else {
                    continue;
                };
                let selected = std::collections::HashSet::from([section_id.clone()]);
                let app_settings_json = export_oxide_settings_snapshot_json(
                    settings_store.settings(),
                    Some(&selected),
                    local_snapshot.scope.include_local_terminal_env_vars,
                )?;
                let bytes = export_connections_to_oxide_with_context_and_progress(
                    connection_store,
                    &[],
                    OxideExportOptions {
                        description: Some(format!("Cloud Sync app settings {section_id}")),
                        embed_keys: false,
                        app_settings_json: Some(app_settings_json),
                        ..OxideExportOptions::default()
                    },
                    encryption_context
                        .as_ref()
                        .context("missing cloud sync encryption context")?,
                    |_stage, current, export_total| {
                        report_fractional_progress(
                            progress,
                            CloudSyncProgressStage::Exporting,
                            export_progress_current(completed_exports, current, export_total),
                            total as f64,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let path = crate::app_settings_object_path(section_id, section_revision);
                manifest.sections.app_settings.insert(
                    section_id.clone(),
                    crate::StructuredObjectEntry {
                        revision: section_revision.clone(),
                        path: path.clone(),
                        record_count: None,
                        content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                    },
                );
                objects.push(StructuredUploadObject {
                    path,
                    bytes,
                    content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                });
                completed_exports += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::Exporting,
                    2 + completed_exports,
                    total,
                );
            }
        }

        if local_snapshot.scope.sync_plugin_settings {
            let entries = crate::plugin_settings::load_plugin_settings(settings_store.path())
                .map_err(anyhow::Error::msg)?;
            for plugin_id in scoped_plugin_ids(local_snapshot) {
                let Some(plugin_revision) = local_snapshot
                    .metadata
                    .plugin_settings_revisions
                    .get(&plugin_id)
                else {
                    continue;
                };
                let plugin_settings = entries
                    .iter()
                    .filter(|entry| {
                        plugin_id_from_setting_storage_key(&entry.storage_key).as_deref()
                            == Some(plugin_id.as_str())
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let bytes = export_connections_to_oxide_with_context_and_progress(
                    connection_store,
                    &[],
                    OxideExportOptions {
                        description: Some(format!("Cloud Sync plugin settings {plugin_id}")),
                        embed_keys: false,
                        plugin_settings,
                        ..OxideExportOptions::default()
                    },
                    encryption_context
                        .as_ref()
                        .context("missing cloud sync encryption context")?,
                    |_stage, current, export_total| {
                        report_fractional_progress(
                            progress,
                            CloudSyncProgressStage::Exporting,
                            export_progress_current(completed_exports, current, export_total),
                            total as f64,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let path = crate::plugin_settings_object_path(&plugin_id, plugin_revision);
                manifest.sections.plugin_settings.insert(
                    plugin_id.clone(),
                    crate::StructuredObjectEntry {
                        revision: plugin_revision.clone(),
                        path: path.clone(),
                        record_count: None,
                        content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                    },
                );
                objects.push(StructuredUploadObject {
                    path,
                    bytes,
                    content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                });
                completed_exports += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::Exporting,
                    2 + completed_exports,
                    total,
                );
            }
        }

        manifest.section_revisions = crate::build_manifest_section_revisions(&manifest);
        Ok(StructuredUploadPlan { manifest, objects })
    }
}
