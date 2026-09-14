// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncOperationService {
    /// Local archives use the same scope and item selection as remote uploads.
    pub fn export_local_file(
        &self,
        connections: &ConnectionStore,
        forwarding: &ForwardingRegistry,
        settings: &SettingsStore,
        password: &str,
        scope: &crate::SyncScope,
        filter: &StructuredUploadItemFilter,
        mut secrets: Vec<EncryptedPortableSecret>,
    ) -> Result<Vec<u8>> {
        let Some(_permit) = self.guard.begin(CloudSyncOperationKind::Upload, false)? else {
            unreachable!()
        };
        let ids = connections
            .connections()
            .iter()
            .filter(|connection| {
                scope.sync_connections
                    && filter
                        .connection_ids
                        .as_ref()
                        .is_none_or(|ids| ids.contains(&connection.id))
            })
            .map(|connection| connection.id.clone())
            .collect::<Vec<_>>();
        let mut options = OxideExportOptions {
            include_passwords: scope.sync_sensitive_credentials,
            include_key_passphrases: scope.sync_sensitive_credentials,
            include_managed_keys: scope.sync_sensitive_credentials,
            include_managed_key_passphrases: scope.sync_sensitive_credentials,
            ..OxideExportOptions::default()
        };
        if scope.sync_sensitive_credentials {
            secrets.extend(crate::credentials::export_profile_credentials(
                connections,
                settings.settings(),
                scope,
                filter,
            )?);
            options.portable_secrets = secrets;
        }
        if scope.sync_app_settings {
            options.app_settings_json = Some(export_oxide_settings_snapshot_json(
                settings.settings(),
                Some(&scope.app_settings_sections.iter().cloned().collect()),
                scope.include_local_terminal_env_vars,
            )?);
        }
        if scope.sync_plugin_settings {
            options.plugin_settings = crate::plugin_settings::load_plugin_settings(settings.path())
                .map_err(anyhow::Error::msg)?
                .into_iter()
                .filter(|setting| {
                    crate::plugin_settings::plugin_id_from_setting_storage_key(&setting.storage_key)
                        .is_some_and(|id| {
                            !crate::get_syncable_plugin_ids(std::slice::from_ref(&id)).is_empty()
                                && scope
                                    .plugin_ids
                                    .as_ref()
                                    .is_none_or(|ids| ids.contains(&id))
                        })
                })
                .collect();
        }
        if scope.sync_quick_commands {
            let mut json = oxideterm_quick_commands::export_snapshot_json(settings.path())
                .map_err(anyhow::Error::msg)?;
            filter_quick_commands_snapshot_json(&mut json, filter.quick_command_ids.as_ref());
            options.quick_commands_json = Some(json);
        }
        if scope.sync_serial_profiles {
            let mut snapshot = connections.export_serial_profiles_snapshot()?;
            filter_serial_profiles_snapshot(&mut snapshot, filter.serial_profile_ids.as_ref());
            options.serial_profiles_json = Some(serde_json::to_string(&snapshot)?);
        }
        if scope.sync_telnet_profiles {
            let mut snapshot = connections.export_telnet_profiles_snapshot()?;
            filter_telnet_profiles_snapshot(&mut snapshot, filter.telnet_profile_ids.as_ref());
            options.telnet_profiles_json = Some(serde_json::to_string(&snapshot)?);
        }
        if scope.sync_mosh_profiles {
            let mut snapshot = connections.export_mosh_profiles_snapshot()?;
            filter_mosh_profiles_snapshot(&mut snapshot, filter.mosh_profile_ids.as_ref());
            options.mosh_profiles_json = Some(serde_json::to_string(&snapshot)?);
        }
        if scope.sync_connections {
            options.standalone_sftp_profiles_json = Some(serde_json::to_string(
                &connections.export_standalone_sftp_profiles_snapshot()?,
            )?);
        }
        if scope.sync_remote_desktop_profiles {
            let mut snapshot = connections.export_remote_desktop_profiles_snapshot()?;
            filter_remote_desktop_profiles_snapshot(
                &mut snapshot,
                filter.remote_desktop_profile_ids.as_ref(),
            );
            options.remote_desktop_profiles_json = Some(serde_json::to_string(&snapshot)?);
        }
        if scope.sync_forwards {
            let selected_forwards = forwarding.list_all_saved_forwards();
            if selected_forwards.iter().any(|forward| {
                filter
                    .forward_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&forward.id))
                    && forward
                        .owner_connection_id
                        .as_ref()
                        .is_none_or(|owner| !ids.contains(owner))
            }) {
                bail!("local_forward_requires_connection");
            }
            options.forwards = forwarding
                .list_all_saved_forwards()
                .into_iter()
                .filter_map(|forward| {
                    let owner = forward.owner_connection_id?;
                    if !ids.contains(&owner)
                        || filter
                            .forward_ids
                            .as_ref()
                            .is_some_and(|ids| !ids.contains(&forward.id))
                    {
                        return None;
                    }
                    Some(oxideterm_connections::oxide_file::OxideForwardRecord {
                        id: Some(forward.id),
                        connection_id: owner,
                        forward_type: match forward.forward_type {
                            oxideterm_forwarding::ForwardType::Local => "local",
                            oxideterm_forwarding::ForwardType::Remote => "remote",
                            oxideterm_forwarding::ForwardType::Dynamic => "dynamic",
                        }
                        .into(),
                        bind_address: forward.rule.bind_address,
                        bind_port: forward.rule.bind_port,
                        target_host: forward.rule.target_host,
                        target_port: forward.rule.target_port,
                        description: Some(forward.rule.description),
                        auto_start: forward.auto_start,
                    })
                })
                .collect();
        }
        oxideterm_connections::oxide_file::export_connections_to_oxide(
            connections,
            &ids,
            password,
            options,
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    pub fn preview_local_file(
        &self,
        connections: &ConnectionStore,
        bytes: Vec<u8>,
        password: &str,
        strategy: ConflictStrategy,
    ) -> Result<LegacyPreview> {
        let Some(_permit) = self.guard.begin(CloudSyncOperationKind::Pull, false)? else {
            unreachable!()
        };
        let metadata = OxideFile::from_bytes(&bytes)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?
            .metadata;
        let preview = preview_oxide_import_with_progress(
            connections,
            &bytes,
            password,
            import_strategy_from_cloud(strategy),
            |_, _, _| {},
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(LegacyPreview {
            bytes,
            metadata,
            preview,
            remote_metadata: RemoteMetadata::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_file_scope_and_selection_restore_only_selected_profiles() {
        let directory =
            std::env::temp_dir().join(format!("oxide-local-file-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut source = ConnectionStore::load(directory.join("source.json")).unwrap();
        let sftp = oxideterm_connections::StandaloneSftpProfile::new(
            "Files",
            "files.test",
            22,
            "ops",
            oxideterm_connections::SavedAuth::Agent,
        );
        source
            .apply_standalone_sftp_profiles_snapshot(StandaloneSftpProfilesSyncSnapshot {
                revision: "files".into(),
                exported_at: Utc::now().to_rfc3339(),
                records: vec![sftp],
            })
            .unwrap();
        let first = oxideterm_connections::RemoteDesktopProfile::new(
            "Keep",
            serde_json::from_str("\"vnc\"").unwrap(),
            "keep.test",
            5900,
        );
        let second = oxideterm_connections::RemoteDesktopProfile::new(
            "Exclude",
            serde_json::from_str("\"vnc\"").unwrap(),
            "exclude.test",
            5900,
        );
        let filter = StructuredUploadItemFilter {
            remote_desktop_profile_ids: Some([first.id.clone()].into_iter().collect()),
            ..Default::default()
        };
        source
            .apply_remote_desktop_profiles_snapshot(RemoteDesktopProfilesSyncSnapshot {
                revision: "desktops".into(),
                exported_at: Utc::now().to_rfc3339(),
                records: vec![first, second],
            })
            .unwrap();
        let settings = SettingsStore::load_from_path(directory.join("settings.json")).unwrap();
        let service = CloudSyncOperationService::new();
        for include_connections in [true, false] {
            let scope = crate::SyncScope {
                sync_connections: include_connections,
                sync_app_settings: false,
                sync_plugin_settings: false,
                ..Default::default()
            };
            let bytes = service
                .export_local_file(
                    &source,
                    &ForwardingRegistry::new(),
                    &settings,
                    "test archive password",
                    &scope,
                    &filter,
                    Vec::new(),
                )
                .unwrap();
            let preview = service
                .preview_local_file(
                    &source,
                    bytes.clone(),
                    "test archive password",
                    ConflictStrategy::Merge,
                )
                .unwrap();
            assert_eq!(preview.metadata.remote_desktop_profiles_count, Some(1));
            assert_eq!(
                preview.metadata.standalone_sftp_profiles_count.unwrap_or(0),
                usize::from(include_connections)
            );
            assert!(!preview.preview.has_app_settings);
            let mut target =
                ConnectionStore::load(directory.join(format!("target-{include_connections}.json")))
                    .unwrap();
            oxideterm_connections::oxide_file::apply_oxide_import_with_options(
                &mut target,
                &bytes,
                "test archive password",
                OxideImportOptions::default(),
            )
            .unwrap();
            assert_eq!(
                target
                    .remote_desktop_profiles()
                    .iter()
                    .map(|profile| profile.host.as_str())
                    .collect::<Vec<_>>(),
                ["keep.test"]
            );
            assert_eq!(
                target
                    .standalone_sftp_profiles()
                    .iter()
                    .map(|profile| profile.host.as_str())
                    .collect::<Vec<_>>(),
                if include_connections {
                    vec!["files.test"]
                } else {
                    vec![]
                }
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
