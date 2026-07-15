// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use chrono::Utc;
use oxideterm_cloud_sync::{
    CloudSyncStatus, ConflictStrategy,
    operation::{
        ApplyStructuredPreviewOutcome, CloudSyncOperationService, LegacyPreview, StructuredPreview,
        UploadOptions,
    },
    secrets::{CloudSyncKeychainSecretProvider, SecretReadMode, get_action_secrets},
    service::build_local_snapshot,
    state::{CloudSyncHistorySummary, CloudSyncStateStore},
    state_transitions::{
        LegacyApplyStateInput, finish_legacy_apply_state, finish_structured_apply_state,
        finish_upload_state,
    },
};
use oxideterm_connections::ConnectionStore;
use oxideterm_forwarding::{ForwardingRegistry, SavedForwardStore};
use oxideterm_settings::SettingsStore;
use serde::Serialize;
use zeroize::Zeroizing;

use crate::{
    args::{
        CloudSyncApplyArgs, CloudSyncApplySource, CloudSyncConflictStrategy, CloudSyncPullArgs,
        CloudSyncResolveArgs, CloudSyncResolveStrategy, CloudSyncWriteArgs, OxideImportStrategy,
        WriteArgs,
    },
    error::{CliError, CliResult, runtime_error},
    output::{self, OutputFormat},
    oxide,
    paths::{default_cloud_sync_path, default_connections_path, default_forwards_path},
    settings, write_guard,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncWriteResponse {
    action: &'static str,
    applied: bool,
    dry_run: bool,
    backup_path: Option<String>,
    backup_size_bytes: Option<u64>,
    summary: CloudSyncWriteSummary,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncWriteSummary {
    local_dirty: bool,
    connections: usize,
    forwards: usize,
    upload_units: usize,
    remote_revision: Option<String>,
    app_settings_sections: usize,
    plugin_settings: usize,
}

enum PullPreview {
    Structured(StructuredPreview),
    Legacy(LegacyPreview),
}

pub(crate) fn push(args: CloudSyncWriteArgs) -> CliResult<()> {
    run_push(args.write, false)
}

pub(crate) fn pull(args: CloudSyncPullArgs) -> CliResult<()> {
    run_pull(args.write, args.strategy, false)
}

pub(crate) fn apply(args: CloudSyncApplyArgs) -> CliResult<()> {
    match args.from {
        CloudSyncApplySource::Local => run_push(args.write, true),
        CloudSyncApplySource::Remote => run_pull(args.write, args.strategy, true),
    }
}

pub(crate) fn resolve(args: CloudSyncResolveArgs) -> CliResult<()> {
    match args.strategy {
        CloudSyncResolveStrategy::LocalWins => run_push(args.write, true),
        CloudSyncResolveStrategy::RemoteWins => run_pull(args.write, None, true),
    }
}

fn run_push(write: WriteArgs, force: bool) -> CliResult<()> {
    let json = write.json;
    let mut state_store = load_state_store(json)?;
    let connection_store = load_connection_store(json)?;
    let forwarding_registry = load_forwarding_registry(json)?;
    let settings_store = load_settings_store(json)?;
    let local = build_local_snapshot(
        &connection_store,
        &forwarding_registry,
        &settings_store,
        state_store.state().last_synced_structured_state.as_ref(),
        Some(&state_store.state().sync_scope),
    )
    .map_err(|error| runtime_error(error, json))?;
    let write = effective_cloud_sync_write(write);
    let mut guard = write_guard::prepare_write(&write, true)?;
    if write.dry_run {
        return write_response(
            json,
            CloudSyncWriteResponse {
                action: "push",
                applied: false,
                dry_run: true,
                backup_path: None,
                backup_size_bytes: None,
                summary: summary_from_local(
                    &local,
                    state_store.state().last_known_remote_revision.clone(),
                ),
            },
        );
    }

    let device_id = state_store.state_mut().ensure_device_id("cli");
    let revision_sequence = state_store.state_mut().next_revision_sequence();
    let mut provider =
        CloudSyncKeychainSecretProvider::new(state_store.state().secret_hints.clone());
    let service = CloudSyncOperationService::new();
    let portable_secrets = if local.scope.sync_sensitive_credentials {
        oxide::export_portable_secrets(json)?
    } else {
        Vec::new()
    };
    let options = UploadOptions {
        automatic: false,
        skip_if_busy: false,
        force,
        device_id,
        revision_sequence,
        previous_remote_revision: state_store.state().last_known_remote_revision.clone(),
        previous_remote_sections: state_store.state().last_synced_remote_sections.clone(),
        last_synced_structured_state: state_store.state().last_synced_structured_state.clone(),
        raw_sync_scope: Some(state_store.state().sync_scope.clone()),
        item_filter: Default::default(),
        portable_secrets,
    };
    let outcome = runtime(json)?.block_on(service.upload_now(
        &connection_store,
        &forwarding_registry,
        &settings_store,
        &state_store.state().settings,
        &mut provider,
        options,
        None,
    ));
    let outcome = match outcome {
        Ok(Some(outcome)) => outcome,
        Ok(None) => {
            return Err(CliError::new(
                "cloud_sync_busy",
                "cloud sync operation is already running",
                json,
            ));
        }
        Err(error) => {
            state_store.state_mut().last_error = Some(error.to_string());
            state_store.state_mut().status = CloudSyncStatus::Error;
            state_store.state_mut().secret_hints = provider.hints().clone();
            state_store
                .save()
                .map_err(|error| runtime_error(error, json))?;
            return Err(CliError::new(
                "cloud_sync_push_failed",
                error.to_string(),
                json,
            ));
        }
    };
    let revision = outcome.manifest.revision.clone();
    let summary = summary_from_local(&outcome.local_snapshot, Some(revision));
    {
        let state = state_store.state_mut();
        state.secret_hints = provider.hints().clone();
        finish_upload_state(state, &outcome);
    }
    state_store
        .save()
        .map_err(|error| runtime_error(error, json))?;
    write_guard::mark_applied(&mut guard);
    write_response(
        json,
        CloudSyncWriteResponse {
            action: "push",
            applied: guard.applied,
            dry_run: guard.dry_run,
            backup_path: guard.backup_path,
            backup_size_bytes: guard.backup_size_bytes,
            summary,
        },
    )
}

fn run_pull(
    write: WriteArgs,
    strategy: Option<CloudSyncConflictStrategy>,
    apply_remote: bool,
) -> CliResult<()> {
    let json = write.json;
    let mut state_store = load_state_store(json)?;
    let mut connection_store = load_connection_store(json)?;
    let forwarding_registry = load_forwarding_registry(json)?;
    let mut settings_store = load_settings_store(json)?;
    let mut provider =
        CloudSyncKeychainSecretProvider::new(state_store.state().secret_hints.clone());
    let service = CloudSyncOperationService::new();
    let preview = runtime(json)?.block_on(service.pull_structured_preview(
        &connection_store,
        &state_store.state().settings,
        &mut provider,
        state_store.state().last_synced_remote_sections.as_ref(),
        None,
    ));
    let preview = match preview {
        Ok(Some(preview)) => PullPreview::Structured(preview),
        Ok(None) => PullPreview::Legacy(
            runtime(json)?
                .block_on(service.pull_legacy_preview(
                    &connection_store,
                    &state_store.state().settings,
                    &mut provider,
                    conflict_strategy(strategy),
                    None,
                ))
                .map_err(|error| {
                    CliError::new("cloud_sync_pull_failed", error.to_string(), json)
                })?,
        ),
        Err(error) => {
            return Err(CliError::new(
                "cloud_sync_pull_failed",
                error.to_string(),
                json,
            ));
        }
    };
    let summary = summary_from_pull_preview(&preview);
    let write = effective_cloud_sync_write(write);
    let mut guard = write_guard::prepare_write(&write, apply_remote)?;
    if write.dry_run || !apply_remote {
        return write_response(
            json,
            CloudSyncWriteResponse {
                action: "pull",
                applied: false,
                dry_run: true,
                backup_path: None,
                backup_size_bytes: None,
                summary,
            },
        );
    }
    let previous_local_baseline = state_store.state().last_synced_structured_state.clone();
    let pull_result = apply_pull_preview(
        &service,
        &mut connection_store,
        &forwarding_registry,
        &mut settings_store,
        &state_store.state().settings,
        &mut provider,
        preview,
        strategy,
        json,
    )?;
    match pull_result {
        PullApplyResult::Structured(outcome) => {
            let local_snapshot = build_local_snapshot(
                &connection_store,
                &forwarding_registry,
                &settings_store,
                previous_local_baseline.as_ref(),
                Some(&state_store.state().sync_scope),
            )
            .unwrap_or_else(|_| outcome.local_snapshot.clone());
            let state = state_store.state_mut();
            state.secret_hints = provider.hints().clone();
            finish_structured_apply_state(
                state,
                &outcome,
                &local_snapshot,
                Utc::now().to_rfc3339(),
            );
        }
        PullApplyResult::Legacy(pull_result) => {
            let local_snapshot = build_local_snapshot(
                &connection_store,
                &forwarding_registry,
                &settings_store,
                None,
                Some(&state_store.state().sync_scope),
            )
            .ok();
            let applied_full_remote =
                !pull_result.skipped_app_settings && !pull_result.skipped_plugin_settings;
            let state = state_store.state_mut();
            state.secret_hints = provider.hints().clone();
            finish_legacy_apply_state(
                state,
                LegacyApplyStateInput {
                    remote_metadata: &pull_result.remote_metadata,
                    history_summary: pull_result.history_summary,
                    local_snapshot: local_snapshot.as_ref(),
                    now: Utc::now().to_rfc3339(),
                    applied_full_remote,
                    is_remote_source: true,
                    is_backup_source: false,
                },
            );
        }
    }
    state_store
        .save()
        .map_err(|error| runtime_error(error, json))?;
    write_guard::mark_applied(&mut guard);
    write_response(
        json,
        CloudSyncWriteResponse {
            action: "pull",
            applied: guard.applied,
            dry_run: guard.dry_run,
            backup_path: guard.backup_path,
            backup_size_bytes: guard.backup_size_bytes,
            summary,
        },
    )
}

enum PullApplyResult {
    Structured(ApplyStructuredPreviewOutcome),
    Legacy(LegacyPullApplyResult),
}

struct LegacyPullApplyResult {
    remote_metadata: oxideterm_cloud_sync::backend::RemoteMetadata,
    history_summary: CloudSyncHistorySummary,
    skipped_app_settings: bool,
    skipped_plugin_settings: bool,
}

fn apply_pull_preview(
    service: &CloudSyncOperationService,
    connection_store: &mut ConnectionStore,
    forwarding_registry: &ForwardingRegistry,
    settings_store: &mut SettingsStore,
    settings: &oxideterm_cloud_sync::CloudSyncSettings,
    provider: &mut CloudSyncKeychainSecretProvider,
    preview: PullPreview,
    strategy: Option<CloudSyncConflictStrategy>,
    json: bool,
) -> CliResult<PullApplyResult> {
    match preview {
        PullPreview::Structured(preview) => {
            let selection = preview.full_selection();
            // Encrypted structured sections share the sync password, including sensitive credentials.
            let needs_password = !preview.app_settings_entries.is_empty()
                || !preview.plugin_settings_entries.is_empty()
                || preview.sensitive_credentials_entry.is_some();
            let sync_password =
                read_sync_password_if_needed(settings, provider, needs_password, json)?;
            let outcome = service
                .apply_structured_preview(
                    connection_store,
                    forwarding_registry,
                    settings_store,
                    settings,
                    preview,
                    selection,
                    conflict_strategy(strategy),
                    sync_password.as_ref().map(|value| value.as_str()),
                    None,
                )
                .map_err(|error| CliError::new("cloud_sync_apply_failed", error.to_string(), json))?
                .ok_or_else(|| {
                    CliError::new("cloud_sync_apply_skipped", "apply was skipped", json)
                })?;
            Ok(PullApplyResult::Structured(outcome))
        }
        PullPreview::Legacy(preview) => {
            let sync_password = read_sync_password_if_needed(settings, provider, true, json)?;
            let history_summary = history_summary(
                preview.preview.total_connections,
                preview.preview.total_forwards,
                usize::from(preview.preview.has_app_settings),
                preview.preview.plugin_settings_count,
            );
            let mut outcome = service
                .apply_legacy_preview(
                    connection_store,
                    settings,
                    &preview,
                    sync_password.as_ref().map(|value| value.as_str()),
                    true,
                    None,
                    true,
                    true,
                    conflict_strategy(strategy),
                    None,
                )
                .map_err(|error| CliError::new("cloud_sync_apply_failed", error.to_string(), json))?
                .ok_or_else(|| {
                    CliError::new("cloud_sync_apply_skipped", "apply was skipped", json)
                })?;
            let imported_app_settings = oxide::apply_imported_app_settings(
                outcome.envelope.app_settings_json.as_deref(),
                None,
                json,
            )?;
            let _imported_quick_commands = oxide::apply_imported_quick_commands(
                outcome.envelope.quick_commands_json.as_deref(),
                OxideImportStrategy::Merge,
                json,
            )?;
            let imported_plugin_settings = oxide::apply_imported_plugin_settings(
                settings_store.path(),
                &outcome.envelope.plugin_settings,
                None,
                json,
            )?;
            oxide::apply_imported_portable_secrets(&mut outcome.envelope, json)?;
            if imported_app_settings {
                *settings_store = load_settings_store(json)?;
            }
            Ok(PullApplyResult::Legacy(LegacyPullApplyResult {
                remote_metadata: preview.remote_metadata,
                history_summary,
                skipped_app_settings: outcome.envelope.app_settings_json.is_some()
                    && !imported_app_settings,
                skipped_plugin_settings: !outcome.envelope.plugin_settings.is_empty()
                    && imported_plugin_settings == 0,
            }))
        }
    }
}

fn read_sync_password_if_needed(
    settings: &oxideterm_cloud_sync::CloudSyncSettings,
    provider: &mut CloudSyncKeychainSecretProvider,
    needed: bool,
    json: bool,
) -> CliResult<Option<Zeroizing<String>>> {
    if !needed {
        return Ok(None);
    }
    let secrets = get_action_secrets(settings, provider, true, SecretReadMode::Prompt)
        .map_err(|error| CliError::new("cloud_sync_secret_read_failed", error.to_string(), json))?;
    let password = secrets.sync_password.unwrap_or_default();
    if password.trim().is_empty() {
        return Err(CliError::new(
            "cloud_sync_secret_missing",
            "cloud sync password is required for this apply",
            json,
        ));
    }
    Ok(Some(password))
}

fn summary_from_pull_preview(preview: &PullPreview) -> CloudSyncWriteSummary {
    match preview {
        PullPreview::Structured(preview) => CloudSyncWriteSummary {
            local_dirty: true,
            connections: preview
                .connections_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or_default(),
            forwards: preview
                .forwards_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or_default(),
            upload_units: 0,
            remote_revision: Some(preview.manifest.revision.clone()),
            app_settings_sections: preview.app_settings_entries.len(),
            plugin_settings: preview.plugin_settings_entries.len(),
        },
        PullPreview::Legacy(preview) => CloudSyncWriteSummary {
            local_dirty: true,
            connections: preview.preview.total_connections,
            forwards: preview.preview.total_forwards,
            upload_units: 0,
            remote_revision: preview.remote_metadata.revision.clone(),
            app_settings_sections: usize::from(preview.preview.has_app_settings),
            plugin_settings: preview.preview.plugin_settings_count,
        },
    }
}

fn summary_from_local(
    local: &oxideterm_cloud_sync::service::CloudSyncLocalSnapshot,
    remote_revision: Option<String>,
) -> CloudSyncWriteSummary {
    CloudSyncWriteSummary {
        local_dirty: local.dirty.has_dirty,
        connections: local.connections_record_count,
        forwards: local.forwards_record_count,
        upload_units: local.upload_units,
        remote_revision,
        app_settings_sections: local.scope.app_settings_sections.len(),
        plugin_settings: local
            .scope
            .plugin_ids
            .as_ref()
            .map(Vec::len)
            .unwrap_or_default(),
    }
}

fn history_summary(
    connections: usize,
    forwards: usize,
    app_settings_sections: usize,
    plugin_settings: usize,
) -> CloudSyncHistorySummary {
    CloudSyncHistorySummary {
        connections,
        forwards,
        quick_commands: 0,
        serial_profiles: 0,
        raw_tcp_profiles: 0,
        raw_udp_profiles: 0,
        sensitive_credentials: 0,
        has_app_settings: app_settings_sections > 0,
        plugin_settings_count: plugin_settings,
    }
}

fn conflict_strategy(strategy: Option<CloudSyncConflictStrategy>) -> ConflictStrategy {
    match strategy.unwrap_or(CloudSyncConflictStrategy::Merge) {
        CloudSyncConflictStrategy::Merge => ConflictStrategy::Merge,
        CloudSyncConflictStrategy::Replace => ConflictStrategy::Replace,
        CloudSyncConflictStrategy::Skip => ConflictStrategy::Skip,
        CloudSyncConflictStrategy::Rename => ConflictStrategy::Rename,
    }
}

fn effective_cloud_sync_write(mut write: WriteArgs) -> WriteArgs {
    if !write.yes {
        write.dry_run = true;
    }
    write
}

fn load_state_store(json: bool) -> CliResult<CloudSyncStateStore> {
    CloudSyncStateStore::load(default_cloud_sync_path()).map_err(|error| runtime_error(error, json))
}

fn load_connection_store(json: bool) -> CliResult<ConnectionStore> {
    ConnectionStore::load(default_connections_path()).map_err(|error| runtime_error(error, json))
}

fn load_forwarding_registry(json: bool) -> CliResult<ForwardingRegistry> {
    let store = SavedForwardStore::load(default_forwards_path())
        .map_err(|error| runtime_error(error, json))?;
    Ok(ForwardingRegistry::new_with_store(store))
}

fn load_settings_store(json: bool) -> CliResult<SettingsStore> {
    let read_only = settings::load_settings_read_only(json)?;
    Ok(SettingsStore::from_read_only(
        read_only.path,
        read_only.settings,
    ))
}

fn runtime(json: bool) -> CliResult<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| CliError::new("runtime_error", error.to_string(), json))
}

fn write_response(json: bool, response: CloudSyncWriteResponse) -> CliResult<()> {
    match output::format_from_flag(json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format!(
                "{} applied={} dryRun={} localDirty={} remoteRevision={}",
                response.action,
                response.applied,
                response.dry_run,
                response.summary.local_dirty,
                response.summary.remote_revision.as_deref().unwrap_or("-")
            ));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_sync_write_defaults_to_dry_run_without_yes() {
        let write = effective_cloud_sync_write(WriteArgs {
            dry_run: false,
            yes: false,
            no_backup: false,
            backup_before_write: false,
            json: true,
            format: None,
        });

        assert!(write.dry_run);
    }
}
