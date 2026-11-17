use std::sync::Arc;

use super::*;

pub struct CloudSyncSources<'a> {
    pub cloud_sync: &'a Entity<CloudSyncWorkspaceEntity>,
    pub connection_store: &'a ConnectionStore,
    pub settings_store: &'a SettingsStore,
    pub forwarding_registry: &'a ForwardingRegistry,
}

pub struct CloudSyncDiffRenderData {
    pub local_snapshot: Result<Arc<CloudSyncLocalSnapshot>, Arc<str>>,
    pub local_field_diff: Arc<CloudSyncLocalFieldDiffSnapshot>,
    pub upload_diff_items: Arc<Vec<CloudSyncSectionDiffItem>>,
    pub upload_sensitive_summary: Option<String>,
}

impl CloudSyncSources<'_> {
    pub fn diff_render_data(
        &self,
        keys: &oxideterm_ai::AiProviderKeyStore,
        i18n: &I18n,
        cx: &App,
    ) -> CloudSyncDiffRenderData {
        let sync = self.cloud_sync.read(cx);
        let state = sync.controller.store.state();
        let local_snapshot = self.cloud_sync_local_snapshot(state, cx);
        let upload_diff_items = local_snapshot
            .as_ref()
            .map(|snapshot| self.cloud_sync_upload_diff_items_cached(snapshot, state, cx))
            .unwrap_or_default();
        let upload_sensitive_summary = sync.view.upload_selection.as_ref().and_then(|selection| {
            cloud_sync_upload_sensitive_summary(
                selection,
                self.connection_store,
                self.settings_store.settings(),
                keys,
                i18n,
            )
        });
        CloudSyncDiffRenderData {
            local_snapshot: local_snapshot.map(Arc::new).map_err(Arc::<str>::from),
            local_field_diff: Arc::new(self.cloud_sync_local_field_diff_snapshot(cx)),
            upload_diff_items: Arc::new(upload_diff_items),
            upload_sensitive_summary,
        }
    }

    pub(super) fn cloud_sync_local_snapshot(
        &self,
        state: &CloudSyncPersistedState,
        cx: &App,
    ) -> std::result::Result<CloudSyncLocalSnapshot, String> {
        let generation = self
            .cloud_sync
            .read(cx)
            .view
            .snapshot_cache_generation
            .get();
        if let Some(cache) = self
            .cloud_sync
            .read(cx)
            .view
            .local_snapshot_cache
            .borrow()
            .as_ref()
        {
            if cache.generation == generation {
                return cache.result.clone();
            }
        }
        let result = build_local_snapshot(
            self.connection_store,
            self.forwarding_registry,
            self.settings_store,
            state.last_synced_structured_state.as_ref(),
            Some(&state.sync_scope),
        )
        .map_err(|error| error.to_string());
        *self
            .cloud_sync
            .read(cx)
            .view
            .local_snapshot_cache
            .borrow_mut() = Some(CloudSyncLocalSnapshotCache {
            generation,
            result: result.clone(),
        });
        result
    }

    fn cloud_sync_upload_diff_items_cached(
        &self,
        snapshot: &CloudSyncLocalSnapshot,
        state: &CloudSyncPersistedState,
        cx: &App,
    ) -> Vec<CloudSyncSectionDiffItem> {
        let generation = self
            .cloud_sync
            .read(cx)
            .view
            .snapshot_cache_generation
            .get();
        if let Some(cache) = self
            .cloud_sync
            .read(cx)
            .view
            .upload_diff_cache
            .borrow()
            .as_ref()
        {
            if cache.generation == generation {
                return cache.items.clone();
            }
        }
        let items = cloud_sync_upload_diff_items(snapshot, state);
        *self.cloud_sync.read(cx).view.upload_diff_cache.borrow_mut() =
            Some(CloudSyncUploadDiffCache {
                generation,
                items: items.clone(),
            });
        items
    }

    pub fn invalidate_cloud_sync_snapshot_caches(&self, cx: &App) {
        // Source mutations advance one explicit generation so render-time cache
        // hits never need to rescan stores or query filesystem metadata.
        self.cloud_sync.read(cx).view.snapshot_cache_generation.set(
            self.cloud_sync
                .read(cx)
                .view
                .snapshot_cache_generation
                .get()
                .wrapping_add(1),
        );
        self.cloud_sync
            .read(cx)
            .view
            .local_snapshot_cache
            .borrow_mut()
            .take();
        self.cloud_sync
            .read(cx)
            .view
            .upload_diff_cache
            .borrow_mut()
            .take();
    }

    pub(super) fn cloud_sync_local_field_diff_snapshot(
        &self,
        cx: &App,
    ) -> CloudSyncLocalFieldDiffSnapshot {
        let scope = normalize_sync_scope(
            Some(&self.cloud_sync.read(cx).controller.store.state().sync_scope),
            &[],
        );
        let app_settings_sections = if scope.sync_app_settings {
            scope
                .app_settings_sections
                .iter()
                .filter_map(|section_id| {
                    let selected = std::collections::HashSet::from([section_id.clone()]);
                    oxideterm_settings::export_oxide_settings_snapshot_json(
                        self.settings_store.settings(),
                        Some(&selected),
                        scope.include_local_terminal_env_vars,
                    )
                    .ok()
                    .and_then(|json| {
                        oxideterm_connections::oxide_file::preview_oxide_app_settings_sections(
                            &json,
                        )
                        .into_iter()
                        .find(|section| section.id == *section_id)
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let quick_commands =
            oxideterm_quick_commands::export_snapshot_json(self.settings_store.path())
                .ok()
                .and_then(|json| oxideterm_quick_commands::decode_snapshot_json(&json).ok());
        CloudSyncLocalFieldDiffSnapshot {
            connections: self
                .connection_store
                .export_saved_connections_snapshot()
                .ok(),
            forwards: self
                .forwarding_registry
                .export_saved_forwards_snapshot()
                .ok(),
            quick_commands,
            serial_profiles: self.connection_store.export_serial_profiles_snapshot().ok(),
            ble_profiles: self.connection_store.export_ble_profiles_snapshot().ok(),
            standalone_sftp_profiles: self
                .connection_store
                .export_standalone_sftp_profiles_snapshot()
                .ok(),
            telnet_profiles: self.connection_store.export_telnet_profiles_snapshot().ok(),
            mosh_profiles: self.connection_store.export_mosh_profiles_snapshot().ok(),
            remote_desktop_profiles: self
                .connection_store
                .export_remote_desktop_profiles_snapshot()
                .ok(),
            app_settings_sections,
        }
    }
}

fn cloud_sync_upload_sensitive_summary(
    selection: &CloudSyncUploadSelection,
    connections: &ConnectionStore,
    settings: &PersistedSettings,
    keys: &oxideterm_ai::AiProviderKeyStore,
    i18n: &I18n,
) -> Option<String> {
    if !selection.sync_sensitive_credentials {
        return None;
    }
    let connection_ids = connections
        .connections()
        .iter()
        .map(|connection| connection.id.clone())
        .filter(|connection_id| {
            selection.sync_connections
                && selection
                    .selected_connection_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(connection_id))
        })
        .collect::<Vec<_>>();
    let portable_secret_count = oxideterm_ai::provider_views(&settings.ai.providers)
        .into_iter()
        .filter(|provider| keys.has_provider_key(&provider.id))
        .count()
        + oxideterm_cloud_sync::profile_credential_count_for_scope(
            connections,
            settings,
            &oxideterm_cloud_sync::normalize_sync_scope(
                Some(&selection.raw_scope(&Default::default())),
                &[],
            ),
            &selection.item_filter(),
        );
    let preflight = oxideterm_connections::oxide_file::preflight_export(
        connections,
        &connection_ids,
        true,
        true,
        portable_secret_count,
    );
    let summary = replace(
        i18n,
        "plugin.cloud_sync.preview.sensitive_upload_summary",
        &[
            (
                "passwords",
                preflight.connections_with_passwords.to_string(),
            ),
            ("keyPassphrases", preflight.key_passphrase_count.to_string()),
            ("managedKeys", preflight.managed_key_count.to_string()),
            (
                "managedKeyPassphrases",
                preflight.managed_key_passphrase_count.to_string(),
            ),
            (
                "portableSecrets",
                preflight.portable_secret_count.to_string(),
            ),
        ],
    );
    let common_count = if selection.sync_connections && selection.selected_connection_ids.is_none()
    {
        connections
            .list_privilege_credentials(oxideterm_connections::SSH_COMMON_PRIVILEGE_SCOPE_ID)
            .map(|credentials| credentials.len())
            .unwrap_or(0)
    } else {
        0
    };
    Some(if common_count > 0 {
        format!(
            "{summary}\n{}: {common_count}",
            i18n.t("settings_view.privilege_credentials.ssh_common_scope")
        )
    } else {
        summary
    })
}
