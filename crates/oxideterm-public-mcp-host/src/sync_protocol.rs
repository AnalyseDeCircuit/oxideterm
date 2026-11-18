use super::{
    ConnectionStore, PublicMcpBackground, PublicMcpHost, PublicMcpLocalSyncCheckpoint,
    PublicMcpSyncPlan, PublicMcpSyncPlanKind, PublicMcpSyncUndo, finish_serialized,
};
use oxideterm_cloud_sync::{
    BackendType, CloudSyncSettings, OXIDE_APP_SETTINGS_SECTION_IDS, RawSyncScope, secret_keys,
    service::CloudSyncLocalSnapshot,
};
use oxideterm_cloud_sync::{
    StructuredLocalState,
    operation::{
        AppliedSync, CloudSyncOperationService, PreparedSync, SyncOutcome, SyncPlanSummary,
    },
    secrets::CloudSyncKeychainSecretProvider,
    service::build_local_snapshot,
    state::CloudSyncPersistedState,
};
use oxideterm_forwarding::ForwardingRegistry;
use oxideterm_public_mcp::{DomainRequest, PublicSyncSection, SyncSelection, UndoRef};
use oxideterm_settings::SettingsStore;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

const SYNC_CANCELLED_ERROR: &str = "cancelled";

pub fn public_sync_scope(
    base: &RawSyncScope,
    selection: &SyncSelection,
) -> Result<(RawSyncScope, Vec<PublicSyncSection>), String> {
    let Some(selected) = selection.sections.as_ref() else {
        let scope = base.clone();
        let sections = raw_scope_sections(&scope);
        return Ok((scope, sections));
    };
    if selected.is_empty() {
        return Err("At least one Cloud Sync section must be selected".to_owned());
    }
    let selected = selected.iter().copied().collect::<BTreeSet<_>>();
    if selected.contains(&PublicSyncSection::SensitiveCredentials)
        && !selected.contains(&PublicSyncSection::Connections)
    {
        return Err("Sensitive credentials require the connections section".to_owned());
    }
    let contains = |section| selected.contains(&section);
    let scope = RawSyncScope {
        sync_connections: Some(contains(PublicSyncSection::Connections)),
        sync_forwards: Some(contains(PublicSyncSection::Forwards)),
        sync_quick_commands: Some(contains(PublicSyncSection::QuickCommands)),
        sync_serial_profiles: Some(contains(PublicSyncSection::SerialProfiles)),
        sync_ble_profiles: Some(contains(PublicSyncSection::BleProfiles)),
        sync_telnet_profiles: Some(contains(PublicSyncSection::TelnetProfiles)),
        sync_mosh_profiles: Some(contains(PublicSyncSection::MoshProfiles)),
        sync_remote_desktop_profiles: Some(contains(PublicSyncSection::RemoteDesktopProfiles)),
        sync_sensitive_credentials: Some(contains(PublicSyncSection::SensitiveCredentials)),
        sync_app_settings: Some(contains(PublicSyncSection::AppSettings)),
        app_settings_sections: base.app_settings_sections.clone(),
        include_local_terminal_env_vars: Some(
            contains(PublicSyncSection::AppSettings)
                && base.include_local_terminal_env_vars.unwrap_or(false),
        ),
        sync_plugin_settings: Some(contains(PublicSyncSection::PluginSettings)),
        plugin_ids: base.plugin_ids.clone(),
    };
    Ok((scope, selected.into_iter().collect()))
}

pub fn raw_scope_sections(scope: &RawSyncScope) -> Vec<PublicSyncSection> {
    let mut sections = Vec::new();
    push_enabled_sync_section(
        &mut sections,
        scope.sync_connections,
        PublicSyncSection::Connections,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_forwards,
        PublicSyncSection::Forwards,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_quick_commands,
        PublicSyncSection::QuickCommands,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_serial_profiles,
        PublicSyncSection::SerialProfiles,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_ble_profiles,
        PublicSyncSection::BleProfiles,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_telnet_profiles,
        PublicSyncSection::TelnetProfiles,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_mosh_profiles,
        PublicSyncSection::MoshProfiles,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_remote_desktop_profiles,
        PublicSyncSection::RemoteDesktopProfiles,
    );
    if scope.sync_sensitive_credentials.unwrap_or(false) {
        sections.push(PublicSyncSection::SensitiveCredentials);
    }
    push_enabled_sync_section(
        &mut sections,
        scope.sync_app_settings,
        PublicSyncSection::AppSettings,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_plugin_settings,
        PublicSyncSection::PluginSettings,
    );
    sections
}

pub fn push_enabled_sync_section(
    sections: &mut Vec<PublicSyncSection>,
    enabled: Option<bool>,
    section: PublicSyncSection,
) {
    if enabled.unwrap_or(true) {
        sections.push(section);
    }
}

pub fn full_sync_scope() -> RawSyncScope {
    RawSyncScope {
        sync_connections: Some(true),
        sync_forwards: Some(true),
        sync_quick_commands: Some(true),
        sync_serial_profiles: Some(true),
        sync_ble_profiles: Some(true),
        sync_telnet_profiles: Some(true),
        sync_mosh_profiles: Some(true),
        sync_remote_desktop_profiles: Some(true),
        sync_sensitive_credentials: Some(true),
        sync_app_settings: Some(true),
        app_settings_sections: Some(
            OXIDE_APP_SETTINGS_SECTION_IDS
                .iter()
                .map(|section| (*section).to_owned())
                .collect(),
        ),
        include_local_terminal_env_vars: Some(true),
        sync_plugin_settings: Some(true),
        plugin_ids: None,
    }
}

pub fn dirty_sections_projection(snapshot: &CloudSyncLocalSnapshot) -> Vec<&'static str> {
    let dirty = &snapshot.dirty.dirty_sections;
    let mut sections = Vec::new();
    if dirty.connections {
        sections.push("connections");
    }
    if dirty.forwards {
        sections.push("forwards");
    }
    if dirty.quick_commands {
        sections.push("quick_commands");
    }
    if dirty.serial_profiles {
        sections.push("serial_profiles");
    }
    if dirty.ble_profiles {
        sections.push("ble_profiles");
    }
    if dirty.telnet_profiles {
        sections.push("telnet_profiles");
    }
    if dirty.mosh_profiles {
        sections.push("mosh_profiles");
    }
    if dirty.remote_desktop_profiles {
        sections.push("remote_desktop_profiles");
    }
    if dirty.sensitive_credentials {
        sections.push("sensitive_credentials");
    }
    if dirty.app_settings.values().any(|value| *value) {
        sections.push("app_settings");
    }
    if dirty.plugin_settings.values().any(|value| *value) {
        sections.push("plugin_settings");
    }
    sections
}

pub fn cloud_sync_is_configured(
    settings: &CloudSyncSettings,
    secret_hints: &BTreeMap<String, bool>,
) -> bool {
    match settings.backend_type {
        BackendType::S3 => !settings.s3_bucket.trim().is_empty(),
        BackendType::Git => !settings.git_repository.trim().is_empty(),
        BackendType::GithubGist => {
            !settings.git_repository.trim().is_empty()
                || secret_hints
                    .get(secret_keys::GIT_TOKEN)
                    .copied()
                    .unwrap_or(false)
        }
        BackendType::OneDrive => secret_hints
            .get(secret_keys::MICROSOFT_REFRESH_TOKEN)
            .copied()
            .unwrap_or(false),
        BackendType::GoogleDrive => secret_hints
            .get(secret_keys::GOOGLE_REFRESH_TOKEN)
            .copied()
            .unwrap_or(false),
        BackendType::Webdav | BackendType::HttpJson | BackendType::Dropbox => {
            !settings.endpoint.trim().is_empty()
        }
    }
}

fn public_cloud_sync_error(error: &str) -> &'static str {
    let code = error.split_once(':').map_or(error, |(code, _)| code);
    match code.trim() {
        SYNC_CANCELLED_ERROR => "The Cloud Sync operation was cancelled",
        "remote_changed_after_preview" | "remote_changed_before_upload" => {
            "The remote Cloud Sync revision changed after the plan was created"
        }
        "remote_not_found" => "The remote Cloud Sync snapshot does not exist",
        "missing_sync_password" => "The Cloud Sync password is not configured",
        "worker_busy" | "operation_in_progress" => {
            "Another Cloud Sync operation is already running"
        }
        "worker_stopped" => "The Cloud Sync worker stopped before completion",
        _ => "The Cloud Sync operation failed",
    }
}

pub struct PublicMcpSyncFailure {
    pub display_error: &'static str,
    pub message: &'static str,
}

impl PublicMcpSyncFailure {
    pub fn from_error(error: String) -> Self {
        let error = zeroize::Zeroizing::new(error);
        let code = error.split(':').next().unwrap_or_default().trim();
        let message = public_cloud_sync_error(&error);
        let display_error = match code {
            "cancelled" => "cancelled",
            "remote_changed_after_preview" => "remote_changed_after_preview",
            "remote_changed_before_upload" => "remote_changed_before_upload",
            "remote_not_found" => "remote_not_found",
            "missing_sync_password" => "missing_sync_password",
            "sync_password_mismatch" => "sync_password_mismatch",
            "worker_busy" | "operation_in_progress" => "operation_in_progress",
            "missing_endpoint" => "missing_endpoint",
            "missing_backend_token" => "missing_backend_token",
            "network_request_failed" => "network_request_failed",
            _ => message,
        };
        Self {
            display_error,
            message,
        }
    }
}

pub fn status_projection(
    state: &CloudSyncPersistedState,
    local_snapshot: &CloudSyncLocalSnapshot,
    operation_in_flight: bool,
) -> serde_json::Value {
    let secret_hints = &state.secret_hints;
    json!({
        "backend_type": state.settings.backend_type,
        "configured": cloud_sync_is_configured(&state.settings, secret_hints),
        "status": state.status,
        "operation_in_flight": operation_in_flight,
        "remote_exists": state.remote_exists,
        "remote_revision": state.last_known_remote_revision,
        "local_dirty": local_snapshot.dirty.has_dirty,
        "dirty_sections": dirty_sections_projection(local_snapshot),
        "last_sync_at": state.last_sync_at,
        "last_upload_at": state.last_upload_at,
        "last_check_at": state.last_check_at,
        "blocked_by_conflict": state.auto_upload_blocked_by_conflict,
        "has_sync_password": secret_hints
            .get(state.settings.password_secret_key())
            .copied()
            .unwrap_or(false),
        "has_backend_credentials": secret_hints.iter().any(|(key, present)| {
            key != secret_keys::SYNC_PASSWORD && !key.starts_with("sync-v3-") && *present
        }),
    })
}

pub struct PublicMcpSyncPreview {
    prepared: PreparedSync,
    summary: SyncPlanSummary,
    pub secret_hints: BTreeMap<String, bool>,
}

async fn prepare_preview(
    service: CloudSyncOperationService,
    mut connections: ConnectionStore,
    forwards: ForwardingRegistry,
    mut settings: SettingsStore,
    state: CloudSyncPersistedState,
    scope: RawSyncScope,
    force: bool,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<PublicMcpSyncPreview, String> {
    let scope = oxideterm_cloud_sync::normalize_sync_scope(Some(&scope), &[]);
    let mut provider = CloudSyncKeychainSecretProvider::new(state.secret_hints);
    let prepared = tokio::select! {
        _ = cancellation.cancelled() => return Err(SYNC_CANCELLED_ERROR.to_owned()),
        result = service.prepare_sync(&mut connections, &forwards, &mut settings, &state.settings, &mut provider, scope, Default::default()) => result,
    };
    let mut prepared = prepared.map_err(|error| error.to_string())?;
    if force {
        prepared
            .choose_local_conflicts()
            .map_err(|error| error.to_string())?;
    }
    let summary = prepared.summary().map_err(|error| error.to_string())?;
    Ok(PublicMcpSyncPreview {
        prepared,
        summary,
        secret_hints: provider.hints().clone(),
    })
}

pub fn local_snapshot(
    connections: &ConnectionStore,
    forwards: &ForwardingRegistry,
    settings: &SettingsStore,
    state: &CloudSyncPersistedState,
) -> Result<CloudSyncLocalSnapshot, String> {
    build_local_snapshot(
        connections,
        forwards,
        settings,
        state.last_synced_structured_state.as_ref(),
        Some(&state.sync_scope),
    )
    .map_err(|_| "The local Cloud Sync snapshot could not be prepared".to_owned())
}

pub fn full_local_state(
    connections: &ConnectionStore,
    forwards: &ForwardingRegistry,
    settings: &SettingsStore,
) -> Result<StructuredLocalState, String> {
    build_local_snapshot(
        connections,
        forwards,
        settings,
        None,
        Some(&full_sync_scope()),
    )
    .map(|snapshot| snapshot.dirty.current_state)
    .map_err(|_| "The local Cloud Sync revision could not be calculated".to_owned())
}

fn capture_checkpoint(
    connections: &ConnectionStore,
    forwards: &ForwardingRegistry,
    settings: &SettingsStore,
    cloud_state: &CloudSyncPersistedState,
) -> Result<PublicMcpLocalSyncCheckpoint, String> {
    let settings_path = settings.path().to_path_buf();
    Ok(PublicMcpLocalSyncCheckpoint {
        connection_store: connections.create_checkpoint().map_err(|_| {
            "The connection store could not be checkpointed for Cloud Sync".to_owned()
        })?,
        saved_forwards: forwards
            .checkpoint_saved_forwards()
            .map_err(|_| "Saved forwards could not be checkpointed for Cloud Sync".to_owned())?,
        quick_commands: oxideterm_quick_commands::capture_checkpoint(&settings_path)
            .map_err(|_| "Quick Commands could not be checkpointed for Cloud Sync".to_owned())?,
        plugin_settings: oxideterm_cloud_sync::plugin_settings::checkpoint_plugin_settings(
            &settings_path,
        )
        .map_err(|_| "Plugin settings could not be checkpointed for Cloud Sync".to_owned())?,
        settings_store: settings
            .create_checkpoint()
            .map_err(|_| "App settings could not be checkpointed for Cloud Sync".to_owned())?,
        cloud_state: cloud_state.clone(),
        settings_path,
    })
}

fn restore_checkpoint(
    connections: &mut ConnectionStore,
    forwards: &ForwardingRegistry,
    settings: &mut SettingsStore,
    checkpoint: &PublicMcpLocalSyncCheckpoint,
) -> Result<(), String> {
    connections
        .restore_checkpoint(&checkpoint.connection_store)
        .map_err(|_| "The connection store could not be restored".to_owned())?;
    if let Some(saved_forwards) = checkpoint.saved_forwards.as_ref() {
        forwards
            .restore_saved_forwards(saved_forwards)
            .map_err(|_| "Saved forwards could not be restored".to_owned())?;
    }
    oxideterm_quick_commands::restore_checkpoint(
        &checkpoint.settings_path,
        &checkpoint.quick_commands,
    )
    .map_err(|_| "Quick Commands could not be restored".to_owned())?;
    oxideterm_cloud_sync::plugin_settings::restore_plugin_settings(
        &checkpoint.settings_path,
        &checkpoint.plugin_settings,
    )
    .map_err(|_| "Plugin settings could not be restored".to_owned())?;
    settings
        .restore_checkpoint(&checkpoint.settings_store)
        .map_err(|_| "App settings could not be restored".to_owned())?;
    Ok(())
}

impl PublicMcpHost {
    pub fn finish_sync_preview(
        &self,
        request: DomainRequest,
        preview: PublicMcpSyncPreview,
        local_state: StructuredLocalState,
        sections: Vec<PublicSyncSection>,
    ) {
        let plan_ref = self.insert_sync_plan(PublicMcpSyncPlan {
            client_ref: request.client_ref.clone(),
            created_at: std::time::Instant::now(),
            local_state,
            sections: sections.clone(),
            kind: PublicMcpSyncPlanKind::Causal(preview.prepared),
        });
        finish_serialized(
            request,
            json!({
                "sync_plan_ref": plan_ref,
                "sections": sections,
                "mode": "causal_merge",
                "changed_fields": preview.summary.changed_fields,
                "conflicts": preview.summary.conflicts.len(),
                "upgrading": preview.summary.upgrading,
                "requires_confirmation": true,
            }),
        );
    }
}

pub fn capture_undo_checkpoint(
    sections: &[PublicSyncSection],
    connections: &ConnectionStore,
    forwards: &ForwardingRegistry,
    settings: &SettingsStore,
    cloud_state: &CloudSyncPersistedState,
) -> Result<Option<PublicMcpLocalSyncCheckpoint>, String> {
    if sections.iter().all(|section| {
        matches!(
            section,
            PublicSyncSection::QuickCommands | PublicSyncSection::PluginSettings
        )
    }) {
        capture_checkpoint(connections, forwards, settings, cloud_state).map(Some)
    } else {
        Ok(None)
    }
}

impl PublicMcpSyncUndo {
    pub fn restore(
        &self,
        connections: &mut ConnectionStore,
        forwards: &ForwardingRegistry,
        settings: &mut SettingsStore,
        cloud_state: &CloudSyncPersistedState,
    ) -> Result<(), String> {
        if full_local_state(connections, forwards, settings)? != self.post_apply_state {
            return Err(
                "Local synchronized data changed after the undo handle was created".to_owned(),
            );
        }
        let compensation = capture_checkpoint(connections, forwards, settings, cloud_state)?;
        if let Err(error) = restore_checkpoint(connections, forwards, settings, &self.checkpoint) {
            let _ = restore_checkpoint(connections, forwards, settings, &compensation);
            return Err(error);
        }
        Ok(())
    }
}

pub fn finish_sync_publication(
    request: DomainRequest,
    outcome: &oxideterm_cloud_sync::operation::SyncOutcome,
    undo_ref: Option<UndoRef>,
) {
    finish_serialized(
        request,
        json!({
            "applied": outcome.applied,
            "published": outcome.published,
            "conflicts": outcome.conflicts.len(),
            "cleanup_pending": outcome.cleanup_pending,
            "undo_ref": undo_ref,
            "requires_publish": outcome.publication_pending,
        }),
    );
}

impl PublicMcpBackground {
    #[expect(
        clippy::too_many_arguments,
        reason = "preview uses the current owner stores and request cancellation"
    )]
    pub fn spawn_sync_preview(
        &self,
        service: CloudSyncOperationService,
        connections: ConnectionStore,
        forwards: ForwardingRegistry,
        settings: SettingsStore,
        state: CloudSyncPersistedState,
        scope: RawSyncScope,
        force: bool,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> tokio::task::JoinHandle<Result<PublicMcpSyncPreview, PublicMcpSyncFailure>> {
        self.forwarding_runtime.spawn(async move {
            prepare_preview(
                service,
                connections,
                forwards,
                settings,
                state,
                scope,
                force,
                cancellation,
            )
            .await
            .map_err(PublicMcpSyncFailure::from_error)
        })
    }

    pub fn spawn_sync_publication(
        &self,
        applied: AppliedSync,
    ) -> tokio::task::JoinHandle<Result<SyncOutcome, PublicMcpSyncFailure>> {
        // Local application already committed. Publication must finish even if the caller disconnects.
        self.forwarding_runtime.spawn(async move {
            applied
                .publish()
                .await
                .map_err(|error| PublicMcpSyncFailure::from_error(error.to_string()))
        })
    }
}
