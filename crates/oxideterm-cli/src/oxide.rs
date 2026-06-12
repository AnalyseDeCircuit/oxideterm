// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, path::Path};

use oxideterm_connections::{
    ConnectionInfo, ConnectionStore,
    oxide_file::{
        EncryptedPluginSetting, ImportConflictStrategy, ImportPreview, ImportResultEnvelope,
        OxideExportOptions, OxideFile, OxideForwardRecord, OxideImportOptions, OxideMetadata,
        export_connections_to_oxide, preflight_export, preview_oxide_import_with_options,
    },
};
use oxideterm_forwarding::{ForwardType, OwnedForwardImportRecord, SavedForwardStore};
use oxideterm_settings::{
    export_oxide_settings_snapshot_json, merge_oxide_settings_snapshot, save_settings_to_path,
};
use serde::Serialize;

use crate::{
    args::{
        OxideAction, OxideCommand, OxideExportArgs, OxideImportArgs, OxideImportStrategy,
        OxidePathArgs, OxidePreviewImportArgs, WriteArgs,
    },
    error::{CliError, CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::{default_connections_path, default_forwards_path, default_settings_path},
    settings, write_guard,
};

mod files;

#[cfg(test)]
mod tests;

use files::{ensure_output_path, read_oxide_file, read_password, write_output_file};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OxideValidateResponse {
    path: String,
    metadata: OxideMetadata,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OxidePreviewImportResponse {
    path: String,
    strategy: &'static str,
    preview: ImportPreview,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OxideImportResponse {
    path: String,
    applied: bool,
    dry_run: bool,
    backup_path: Option<String>,
    backup_size_bytes: Option<u64>,
    strategy: &'static str,
    preview: Option<ImportPreview>,
    result: Option<ImportResultEnvelope>,
    imported_app_settings: bool,
    imported_quick_commands: usize,
    imported_plugin_settings: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OxideExportResponse {
    path: String,
    size_bytes: u64,
    connection_count: usize,
    forward_count: usize,
    plugin_settings_count: usize,
    portable_secret_count: usize,
    metadata: OxideMetadata,
}

pub(crate) fn run(command: OxideCommand) -> CliResult<i32> {
    match command.action {
        OxideAction::Validate(args) => validate(args),
        OxideAction::PreviewImport(args) => preview_import(args),
        OxideAction::Diff(args) => preview_import(args),
        OxideAction::Import(args) => import(args),
        OxideAction::Export(args) => export(args),
    }
}

fn validate(args: OxidePathArgs) -> CliResult<i32> {
    let bytes = read_oxide_file(&args.path, args.json)?;
    let file = OxideFile::from_bytes(&bytes)
        .map_err(|error| CliError::new("oxide_validate_failed", error.to_string(), args.json))?;
    let response = OxideValidateResponse {
        path: args.path,
        metadata: file.metadata,
    };
    write_value(args.json, &response, format_validate_text(&response))?;
    Ok(0)
}

fn preview_import(args: OxidePreviewImportArgs) -> CliResult<i32> {
    let bytes = read_oxide_file(&args.path, args.json)?;
    let password = read_password(&args.password, args.json)?;
    let store = ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, args.json))?;
    let strategy = import_strategy(args.strategy);
    let preview = preview_oxide_import_with_options(
        &store,
        &bytes,
        &password,
        OxideImportOptions {
            conflict_strategy: strategy,
            ..OxideImportOptions::default()
        },
    )
    .map_err(|error| CliError::new("oxide_preview_failed", error.to_string(), args.json))?;
    let response = OxidePreviewImportResponse {
        path: args.path,
        strategy: strategy_name(args.strategy),
        preview,
    };
    write_value(args.json, &response, format_preview_text(&response))?;
    Ok(0)
}

fn import(args: OxideImportArgs) -> CliResult<i32> {
    let bytes = read_oxide_file(&args.path, args.write.json)?;
    let password = read_password(&args.password, args.write.json)?;
    let strategy = import_strategy(args.strategy);
    let read_only_store = ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, args.write.json))?;
    let selected_connection_names = selected_names(&args.selected_names);
    let selected_forward_ids = selected_names(&args.forward_ids);
    let preview = preview_oxide_import_with_options(
        &read_only_store,
        &bytes,
        &password,
        OxideImportOptions {
            selected_names: selected_connection_names.clone(),
            selected_forward_ids: selected_forward_ids.clone(),
            conflict_strategy: strategy,
            import_forwards: !args.no_forwards,
            import_portable_secrets: args.import_portable_secrets,
            ..OxideImportOptions::default()
        },
    )
    .map_err(|error| CliError::new("oxide_preview_failed", error.to_string(), args.write.json))?;
    let import_filter = OxideImportFilter::from_args(&args);
    let has_changes = preview_has_changes(&preview, &import_filter);
    let write = effective_import_write(args.write);
    let mut guard = write_guard::prepare_write(&write, has_changes)?;

    if write.dry_run || !has_changes {
        let response = OxideImportResponse {
            path: args.path,
            applied: guard.applied,
            dry_run: guard.dry_run,
            backup_path: guard.backup_path,
            backup_size_bytes: guard.backup_size_bytes,
            strategy: strategy_name(args.strategy),
            preview: Some(preview),
            result: None,
            imported_app_settings: false,
            imported_quick_commands: 0,
            imported_plugin_settings: 0,
        };
        let ok = response.dry_run || response.changes_applied();
        write_value(write.json, &response, format_import_text(&response))?;
        return Ok(if ok { 0 } else { 1 });
    }

    let mut store = ConnectionStore::load(default_connections_path())
        .map_err(|error| runtime_error(error, write.json))?;
    let mut result = oxideterm_connections::oxide_file::apply_oxide_import_with_options(
        &mut store,
        &bytes,
        &password,
        OxideImportOptions {
            selected_names: selected_connection_names,
            selected_forward_ids,
            conflict_strategy: strategy,
            import_forwards: !args.no_forwards,
            import_portable_secrets: args.import_portable_secrets,
            ..OxideImportOptions::default()
        },
    )
    .map_err(|error| CliError::new("oxide_import_failed", error.to_string(), write.json))?;
    let imported_app_settings = if args.no_app_settings {
        false
    } else {
        apply_imported_app_settings(
            result.app_settings_json.as_deref(),
            import_filter.settings_sections.as_ref(),
            write.json,
        )?
    };
    let imported_quick_commands = if args.no_quick_commands {
        0
    } else {
        apply_imported_quick_commands(
            result.quick_commands_json.as_deref(),
            args.strategy,
            write.json,
        )?
    };
    let imported_plugin_settings = if args.no_plugin_settings {
        0
    } else {
        apply_imported_plugin_settings(
            &default_settings_path(),
            &result.plugin_settings,
            import_filter.plugin_ids.as_ref(),
            write.json,
        )?
    };
    if args.import_portable_secrets {
        apply_imported_portable_secrets(&mut result, write.json)?;
    }
    apply_imported_forward_records(&mut result, write.json)?;
    write_guard::mark_applied(&mut guard);
    let response = OxideImportResponse {
        path: args.path,
        applied: guard.applied,
        dry_run: guard.dry_run,
        backup_path: guard.backup_path,
        backup_size_bytes: guard.backup_size_bytes,
        strategy: strategy_name(args.strategy),
        preview: None,
        result: Some(result),
        imported_app_settings,
        imported_quick_commands,
        imported_plugin_settings,
    };
    write_value(write.json, &response, format_import_text(&response))?;
    Ok(0)
}

fn export(args: OxideExportArgs) -> CliResult<i32> {
    let password = read_password(&args.password, args.json)?;
    let store = ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, args.json))?;
    let mut connection_ids = selected_connection_ids(&store, &args.connection_queries, args.json)?;
    let forwards = if args.no_forwards {
        Vec::new()
    } else {
        load_export_forwards(
            &store,
            &mut connection_ids,
            &args.forward_queries,
            args.json,
        )?
    };
    let app_settings_json = if args.no_app_settings {
        None
    } else {
        let current = settings::load_settings_read_only(args.json)?;
        Some(
            export_oxide_settings_snapshot_json(
                &current.settings,
                None,
                args.include_local_terminal_env_vars,
            )
            .map_err(|error| {
                CliError::new("settings_export_failed", error.to_string(), args.json)
            })?,
        )
    };
    let plugin_settings = if args.no_plugin_settings {
        Vec::new()
    } else {
        oxideterm_cloud_sync::plugin_settings::load_plugin_settings(&default_settings_path())
            .map_err(|error| CliError::new("plugin_settings_export_failed", error, args.json))?
    };
    let quick_commands_json = if args.no_quick_commands {
        None
    } else {
        Some(
            oxideterm_quick_commands::export_snapshot_json(&default_settings_path())
                .map_err(|error| CliError::new("quick_commands_export_failed", error, args.json))?,
        )
    };
    let portable_secrets = if args.include_portable_secrets {
        export_portable_secrets(args.json)?
    } else {
        Vec::new()
    };
    let forward_count = forwards.len();
    let preflight = preflight_export(
        &store,
        &connection_ids,
        args.embed_keys,
        true,
        portable_secrets.len(),
    );
    if !preflight.can_export {
        return Err(CliError::new(
            "oxide_export_preflight_failed",
            "selected connections cannot be exported; run with --json for missing key details",
            args.json,
        ));
    }
    ensure_output_path(&args.path, args.overwrite, args.json)?;
    let bytes = export_connections_to_oxide(
        &store,
        &connection_ids,
        &password,
        OxideExportOptions {
            description: args.description.clone(),
            embed_keys: args.embed_keys,
            app_settings_json,
            quick_commands_json,
            plugin_settings,
            portable_secrets,
            forwards,
            ..OxideExportOptions::default()
        },
    )
    .map_err(|error| CliError::new("oxide_export_failed", error.to_string(), args.json))?;
    write_output_file(&args.path, &bytes, args.json)?;
    let metadata = OxideFile::from_bytes(&bytes)
        .map_err(|error| CliError::new("oxide_export_failed", error.to_string(), args.json))?
        .metadata;
    let response = OxideExportResponse {
        path: args.path,
        size_bytes: bytes.len() as u64,
        connection_count: connection_ids.len(),
        forward_count,
        plugin_settings_count: metadata.plugin_settings_count.unwrap_or_default(),
        portable_secret_count: metadata.portable_secret_count.unwrap_or_default(),
        metadata,
    };
    write_value(args.json, &response, format_export_text(&response))?;
    Ok(0)
}

pub(crate) fn apply_imported_quick_commands(
    snapshot_json: Option<&str>,
    strategy: crate::args::OxideImportStrategy,
    json: bool,
) -> CliResult<usize> {
    let Some(snapshot_json) = snapshot_json else {
        return Ok(0);
    };
    let result = oxideterm_quick_commands::apply_snapshot_json(
        &default_settings_path(),
        snapshot_json,
        quick_command_strategy(strategy),
    );
    if !result.errors.is_empty() {
        return Err(CliError::new(
            "quick_commands_import_failed",
            result.errors.join("; "),
            json,
        ));
    }
    Ok(result.imported)
}

fn quick_command_strategy(
    strategy: crate::args::OxideImportStrategy,
) -> oxideterm_quick_commands::QuickCommandImportStrategy {
    match strategy {
        crate::args::OxideImportStrategy::Skip => {
            oxideterm_quick_commands::QuickCommandImportStrategy::Skip
        }
        crate::args::OxideImportStrategy::Rename => {
            oxideterm_quick_commands::QuickCommandImportStrategy::Rename
        }
        crate::args::OxideImportStrategy::Replace => {
            oxideterm_quick_commands::QuickCommandImportStrategy::Replace
        }
        crate::args::OxideImportStrategy::Merge => {
            oxideterm_quick_commands::QuickCommandImportStrategy::Merge
        }
    }
}

fn load_export_forwards(
    connection_store: &ConnectionStore,
    connection_ids: &mut Vec<String>,
    forward_queries: &[String],
    json: bool,
) -> CliResult<Vec<OxideForwardRecord>> {
    let store = SavedForwardStore::load(default_forwards_path())
        .map_err(|error| CliError::new("forwards_export_failed", error.to_string(), json))?;
    let requested = forward_queries
        .iter()
        .map(|query| query.trim())
        .filter(|query| !query.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let mut matched = HashSet::new();
    let mut forwards = Vec::new();
    for forward in store.load_syncable_forwards() {
        if !requested.is_empty() && !requested.contains(&forward.id) {
            continue;
        }
        matched.insert(forward.id.clone());
        let Some(owner_connection_id) = forward.owner_connection_id.clone() else {
            continue;
        };
        if connection_store.get(&owner_connection_id).is_none() {
            continue;
        }
        if !connection_ids.contains(&owner_connection_id) {
            connection_ids.push(owner_connection_id.clone());
        }
        forwards.push(OxideForwardRecord {
            id: Some(forward.id),
            connection_id: owner_connection_id,
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
        });
    }
    if !requested.is_empty() {
        let missing = requested.difference(&matched).cloned().collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(CliError::new(
                "forwards_export_failed",
                format!("saved forward id(s) not found: {}", missing.join(", ")),
                json,
            ));
        }
    }
    Ok(forwards)
}

fn apply_imported_forward_records(
    result: &mut ImportResultEnvelope,
    json: bool,
) -> CliResult<usize> {
    if result.forward_records.is_empty() {
        return Ok(0);
    }
    let records = result
        .forward_records
        .iter()
        .map(owned_forward_import_record)
        .collect::<Vec<_>>();
    let replace_owner_ids = result
        .forward_replace_owner_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let merge_owner_ids = result
        .forward_merge_owner_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let store = SavedForwardStore::load(default_forwards_path())
        .map_err(|error| CliError::new("forwards_import_failed", error.to_string(), json))?;
    let imported = store
        .apply_owned_forward_import_records(&records, &replace_owner_ids, &merge_owner_ids)
        .map_err(|error| CliError::new("forwards_import_failed", error.to_string(), json))?;
    // The envelope initially contains planned forward records; after persistence, report actual writes.
    result.imported_forwards = imported;
    Ok(imported)
}

fn owned_forward_import_record(record: &OxideForwardRecord) -> OwnedForwardImportRecord {
    OwnedForwardImportRecord {
        owner_connection_id: record.connection_id.clone(),
        forward_type: record.forward_type.clone(),
        bind_address: record.bind_address.clone(),
        bind_port: record.bind_port,
        target_host: record.target_host.clone(),
        target_port: record.target_port,
        description: record.description.clone(),
        auto_start: record.auto_start,
    }
}

pub(crate) fn export_portable_secrets(
    json: bool,
) -> CliResult<Vec<oxideterm_connections::oxide_file::EncryptedPortableSecret>> {
    let settings = settings::load_settings_read_only(json)?;
    let key_store = oxideterm_ai::AiProviderKeyStore::new();
    let provider_ids = oxideterm_ai::provider_views(&settings.settings.ai.providers)
        .into_iter()
        .map(|provider| provider.id)
        .filter(|provider_id| key_store.has_provider_key(provider_id))
        .collect::<Vec<_>>();
    let keys = key_store
        .get_provider_keys(&provider_ids)
        .map_err(|error| CliError::new("portable_secret_export_failed", error.to_string(), json))?;
    Ok(keys
        .into_iter()
        .map(
            |(id, secret)| oxideterm_connections::oxide_file::EncryptedPortableSecret {
                kind: "ai_provider_key".to_string(),
                id,
                secret,
            },
        )
        .collect())
}

pub(crate) fn apply_imported_app_settings(
    snapshot_json: Option<&str>,
    sections: Option<&HashSet<String>>,
    json: bool,
) -> CliResult<bool> {
    let Some(snapshot_json) = snapshot_json else {
        return Ok(false);
    };
    let current = settings::load_settings_read_only(json)?;
    let merged = merge_oxide_settings_snapshot(&current.settings, snapshot_json, sections)
        .map_err(|error| CliError::new("settings_import_failed", error.to_string(), json))?;
    let saved = save_settings_to_path(&default_settings_path(), merged)
        .map_err(|error| CliError::new("settings_import_failed", error.to_string(), json))?;
    if !saved.validation_warnings.is_empty() {
        return Err(CliError::new(
            "settings_import_failed",
            format!(
                "settings save produced validation warnings: {}",
                saved.validation_warnings.join("; ")
            ),
            json,
        ));
    }
    Ok(true)
}

pub(crate) fn apply_imported_plugin_settings(
    settings_path: &Path,
    plugin_settings: &[EncryptedPluginSetting],
    selected_plugin_ids: Option<&HashSet<String>>,
    json: bool,
) -> CliResult<usize> {
    let filtered = selected_plugin_ids.map_or_else(
        || plugin_settings.to_vec(),
        |selected| {
            plugin_settings
                .iter()
                .filter(|setting| {
                    oxideterm_cloud_sync::plugin_settings::plugin_id_from_setting_storage_key(
                        &setting.storage_key,
                    )
                    .is_some_and(|plugin_id| selected.contains(&plugin_id))
                })
                .cloned()
                .collect()
        },
    );
    oxideterm_cloud_sync::plugin_settings::upsert_plugin_settings(settings_path, &filtered)
        .map_err(|error| CliError::new("plugin_settings_import_failed", error, json))
}

pub(crate) fn apply_imported_portable_secrets(
    envelope: &mut ImportResultEnvelope,
    json: bool,
) -> CliResult<()> {
    let total = envelope.portable_secrets.len();
    if total == 0 {
        return Ok(());
    }

    let key_store = oxideterm_ai::AiProviderKeyStore::new();
    let mut imported = 0usize;
    for secret in envelope.portable_secrets.drain(..) {
        if secret.kind != "ai_provider_key" || secret.id.trim().is_empty() {
            envelope.errors.push(format!(
                "unsupported portable secret kind '{}' for id '{}'",
                secret.kind, secret.id
            ));
            continue;
        }

        // The decrypted portable secret is moved directly into the AI key store's
        // Zeroizing boundary, matching GPUI import without ever printing the value.
        key_store
            .store_provider_key(&secret.id, secret.secret)
            .map_err(|error| {
                CliError::new(
                    "portable_secret_import_failed",
                    format!("failed to import portable secret '{}': {error}", secret.id),
                    json,
                )
            })?;
        imported += 1;
    }

    envelope.imported_portable_secrets = imported;
    envelope.skipped_portable_secrets = total.saturating_sub(imported);
    Ok(())
}

fn selected_connection_ids(
    store: &ConnectionStore,
    queries: &[String],
    json: bool,
) -> CliResult<Vec<String>> {
    let connections = store.connection_infos();
    if queries.is_empty() {
        return Ok(connections
            .into_iter()
            .map(|connection| connection.id)
            .collect());
    }
    queries
        .iter()
        .map(|query| {
            find_connection(&connections, query)
                .map(|connection| connection.id.clone())
                .ok_or_else(|| {
                    CliError::new(
                        "connection_not_found",
                        format!("connection '{}' was not found", query),
                        json,
                    )
                })
        })
        .collect()
}

fn find_connection<'a>(
    connections: &'a [ConnectionInfo],
    query: &str,
) -> Option<&'a ConnectionInfo> {
    let query = query.trim();
    connections
        .iter()
        .find(|connection| connection.id == query || connection.name.eq_ignore_ascii_case(query))
}

fn selected_names(values: &[String]) -> Option<Vec<String>> {
    let selected = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    (!selected.is_empty()).then_some(selected)
}

fn import_strategy(strategy: OxideImportStrategy) -> ImportConflictStrategy {
    match strategy {
        OxideImportStrategy::Skip => ImportConflictStrategy::Skip,
        OxideImportStrategy::Rename => ImportConflictStrategy::Rename,
        OxideImportStrategy::Replace => ImportConflictStrategy::Replace,
        OxideImportStrategy::Merge => ImportConflictStrategy::Merge,
    }
}

fn strategy_name(strategy: OxideImportStrategy) -> &'static str {
    match strategy {
        OxideImportStrategy::Skip => "skip",
        OxideImportStrategy::Rename => "rename",
        OxideImportStrategy::Replace => "replace",
        OxideImportStrategy::Merge => "merge",
    }
}

fn effective_import_write(mut write: WriteArgs) -> WriteArgs {
    // Tauri import is user-confirmed in a modal; CLI mirrors that by previewing unless --yes is explicit.
    if !write.yes {
        write.dry_run = true;
    }
    write
}

struct OxideImportFilter {
    include_app_settings: bool,
    include_quick_commands: bool,
    include_plugin_settings: bool,
    settings_sections: Option<HashSet<String>>,
    plugin_ids: Option<HashSet<String>>,
}

impl OxideImportFilter {
    fn from_args(args: &OxideImportArgs) -> Self {
        Self {
            include_app_settings: !args.no_app_settings,
            include_quick_commands: !args.no_quick_commands,
            include_plugin_settings: !args.no_plugin_settings,
            settings_sections: selected_name_set(&args.sections),
            plugin_ids: selected_name_set(&args.plugin_ids),
        }
    }
}

fn selected_name_set(values: &[String]) -> Option<HashSet<String>> {
    let selected = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    (!selected.is_empty()).then_some(selected)
}

fn preview_has_changes(preview: &ImportPreview, filter: &OxideImportFilter) -> bool {
    preview.total_connections > 0
        || preview.total_forwards > 0
        || (filter.include_plugin_settings && preview.plugin_settings_count > 0)
        || (filter.include_quick_commands && preview.has_quick_commands)
        || preview.portable_secret_count > 0
        || (filter.include_app_settings && preview.has_app_settings)
}

impl OxideImportResponse {
    fn changes_applied(&self) -> bool {
        self.result.is_some()
            || self.imported_app_settings
            || self.imported_plugin_settings > 0
            || self.imported_quick_commands > 0
    }
}

fn write_value<T: Serialize>(json: bool, value: &T, text: String) -> CliResult<()> {
    match output::format_from_flag(json) {
        OutputFormat::Json => output::write_json(value),
        OutputFormat::Text => {
            output::write_text(text);
            Ok(())
        }
    }
}

fn format_validate_text(response: &OxideValidateResponse) -> String {
    format!(
        "valid: true connections={} appSettings={} pluginSettings={} portableSecrets={}",
        response.metadata.num_connections,
        response.metadata.has_app_settings.unwrap_or(false),
        response.metadata.plugin_settings_count.unwrap_or_default(),
        response.metadata.portable_secret_count.unwrap_or_default()
    )
}

fn format_preview_text(response: &OxidePreviewImportResponse) -> String {
    format!(
        "connections={} strategy={} rename={} skip={} replace={} merge={} appSettings={} forwards={} portableSecrets={}",
        response.preview.total_connections,
        response.strategy,
        response.preview.will_rename.len(),
        response.preview.will_skip.len(),
        response.preview.will_replace.len(),
        response.preview.will_merge.len(),
        response.preview.has_app_settings,
        response.preview.total_forwards,
        response.preview.portable_secret_count
    )
}

fn format_import_text(response: &OxideImportResponse) -> String {
    if let Some(preview) = response.preview.as_ref() {
        return format!(
            "applied: {} dryRun={} backup={} previewConnections={} appSettings={}",
            response.applied,
            response.dry_run,
            response.backup_path.as_deref().unwrap_or("-"),
            preview.total_connections,
            preview.has_app_settings
        );
    }
    let result = response.result.as_ref();
    format!(
        "applied: {} dryRun={} backup={} imported={} renamed={} replaced={} merged={} skipped={} appSettings={} quickCommands={} pluginSettings={} portableSecrets={}",
        response.applied,
        response.dry_run,
        response.backup_path.as_deref().unwrap_or("-"),
        result.map(|value| value.imported).unwrap_or_default(),
        result.map(|value| value.renamed).unwrap_or_default(),
        result.map(|value| value.replaced).unwrap_or_default(),
        result.map(|value| value.merged).unwrap_or_default(),
        result.map(|value| value.skipped).unwrap_or_default(),
        response.imported_app_settings,
        response.imported_quick_commands,
        response.imported_plugin_settings,
        result
            .map(|value| value.imported_portable_secrets)
            .unwrap_or_default()
    )
}

fn format_export_text(response: &OxideExportResponse) -> String {
    format!(
        "{}\tsize={} connections={} forwards={} pluginSettings={} portableSecrets={}",
        response.path,
        response.size_bytes,
        response.connection_count,
        response.forward_count,
        response.plugin_settings_count,
        response.portable_secret_count
    )
}
