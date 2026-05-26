// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{
    ConnectionInfo, ConnectionStore, SavedConnectionsConflictStrategy,
    SavedConnectionsSyncSnapshot, validate_group_name,
};
use serde::Serialize;
use serde_json::Value;
use std::fs;

use crate::{
    args::{
        ConnectionCreateArgs, ConnectionEditArgs, ConnectionSearchArgs, ConnectionShowArgs,
        ConnectionsAction, ConnectionsApplyStrategy, ConnectionsCommand, ConnectionsExportArgs,
        ConnectionsExportFormat, ConnectionsGroupAction, ConnectionsGroupCommand, JsonArgs,
        WriteArgs,
    },
    connections_validate,
    error::{CliError, CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::default_connections_path,
    write_guard::{self, WriteGuardPlan},
};

mod spec;

#[cfg(test)]
mod tests;

use spec::{connection_request_from_spec, read_connection_spec};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionsListResponse {
    path: String,
    count: usize,
    connections: Vec<ConnectionInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionGroupsResponse {
    path: String,
    count: usize,
    groups: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionShowResponse {
    path: String,
    connection: ConnectionInfo,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RawSafeConnectionsExportResponse {
    path: String,
    format: &'static str,
    count: usize,
    groups: Vec<String>,
    connections: Vec<ConnectionInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionsWriteResponse {
    path: String,
    applied: bool,
    dry_run: bool,
    backup_path: Option<String>,
    backup_size_bytes: Option<u64>,
    changes: Vec<ConnectionsChange>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionsChange {
    action: &'static str,
    target: String,
    before: Option<String>,
    after: Option<String>,
}

pub fn run(command: ConnectionsCommand) -> CliResult<i32> {
    match command.action {
        ConnectionsAction::List(args) => {
            list_connections(args)?;
            Ok(0)
        }
        ConnectionsAction::Show(args) => {
            show_connection(args)?;
            Ok(0)
        }
        ConnectionsAction::Groups(args) => {
            list_groups(args)?;
            Ok(0)
        }
        ConnectionsAction::Search(args) => {
            search_connections(args)?;
            Ok(0)
        }
        ConnectionsAction::Export(args) => {
            export_connections(args)?;
            Ok(0)
        }
        ConnectionsAction::Validate(args) => connections_validate::run(args),
        ConnectionsAction::Create(args) => create_connection(args),
        ConnectionsAction::Edit(args) => edit_connection(args),
        ConnectionsAction::Delete(args) => delete_connection(args.query, args.write),
        ConnectionsAction::Rename(args) => rename_connection(args.query, args.name, args.write),
        ConnectionsAction::Import(args) | ConnectionsAction::ApplySnapshot(args) => {
            apply_connections_snapshot(args.path, args.strategy, args.write)
        }
        ConnectionsAction::Group(command) => run_group_command(command),
    }
}

fn create_connection(args: ConnectionCreateArgs) -> CliResult<i32> {
    let spec = read_connection_spec(&args.spec_path, args.write.json)?;
    let request = connection_request_from_spec(spec, None, args.write.json)?;
    let connection_name = request.name.clone();
    let change = ConnectionsChange {
        action: "create",
        target: connection_name.clone(),
        before: None,
        after: Some(format!(
            "{}@{}:{}",
            request.username, request.host, request.port
        )),
    };
    finish_connections_write(args.write, vec![change], |store| {
        store.upsert(request).map(|_| ())
    })
}

fn edit_connection(args: ConnectionEditArgs) -> CliResult<i32> {
    let spec = read_connection_spec(&args.spec_path, args.write.json)?;
    let store = load_connection_store(args.write.json)?;
    let Some(connection) = find_connection(&store.connection_infos(), &args.query) else {
        return Err(CliError::new(
            "connection_not_found",
            format!("connection '{}' was not found", args.query),
            args.write.json,
        ));
    };
    let existing = store.get(&connection.id).cloned().ok_or_else(|| {
        CliError::new(
            "connection_not_found",
            format!("connection '{}' was not found", args.query),
            args.write.json,
        )
    })?;
    let request = connection_request_from_spec(spec, Some(&existing), args.write.json)?;
    let mut changes = Vec::new();
    push_connection_field_change(
        &mut changes,
        &connection.id,
        "name",
        &existing.name,
        &request.name,
    );
    push_connection_field_change(
        &mut changes,
        &connection.id,
        "host",
        &existing.host,
        &request.host,
    );
    push_connection_field_change(
        &mut changes,
        &connection.id,
        "username",
        &existing.username,
        &request.username,
    );
    if existing.port != request.port {
        changes.push(ConnectionsChange {
            action: "editPort",
            target: connection.id.clone(),
            before: Some(existing.port.to_string()),
            after: Some(request.port.to_string()),
        });
    }
    if existing.group != request.group {
        changes.push(ConnectionsChange {
            action: "editGroup",
            target: connection.id.clone(),
            before: existing.group.clone(),
            after: request.group.clone(),
        });
    }
    if existing.auth.auth_type() != request.auth.auth_type() {
        changes.push(ConnectionsChange {
            action: "editAuth",
            target: connection.id.clone(),
            before: Some(existing.auth.auth_type().as_str().to_string()),
            after: Some(request.auth.auth_type().as_str().to_string()),
        });
    }
    if existing.proxy_chain.len() != request.proxy_chain.len() {
        changes.push(ConnectionsChange {
            action: "editProxyChain",
            target: connection.id.clone(),
            before: Some(existing.proxy_chain.len().to_string()),
            after: Some(request.proxy_chain.len().to_string()),
        });
    }
    finish_connections_write(args.write, changes, |store| {
        store.upsert(request).map(|_| ())
    })
}

fn delete_connection(query: String, write: WriteArgs) -> CliResult<i32> {
    let store = load_connection_store(write.json)?;
    let Some(connection) = find_connection(&store.connection_infos(), &query) else {
        return Err(CliError::new(
            "connection_not_found",
            format!("connection '{}' was not found", query),
            write.json,
        ));
    };
    let change = ConnectionsChange {
        action: "delete",
        target: connection.id.clone(),
        before: Some(connection.name.clone()),
        after: None,
    };
    finish_connections_write(write, vec![change], |store| {
        store.delete(&connection.id).map(|_| ())
    })
}

fn rename_connection(query: String, name: String, write: WriteArgs) -> CliResult<i32> {
    let store = load_connection_store(write.json)?;
    let Some(connection) = find_connection(&store.connection_infos(), &query) else {
        return Err(CliError::new(
            "connection_not_found",
            format!("connection '{}' was not found", query),
            write.json,
        ));
    };
    let changes = if connection.name == name {
        Vec::new()
    } else {
        vec![ConnectionsChange {
            action: "rename",
            target: connection.id.clone(),
            before: Some(connection.name.clone()),
            after: Some(name.clone()),
        }]
    };
    finish_connections_write(write, changes, |store| {
        store.rename_connection(&connection.id, name).map(|_| ())
    })
}

fn run_group_command(command: ConnectionsGroupCommand) -> CliResult<i32> {
    match command.action {
        ConnectionsGroupAction::Add(args) => group_add(args.name, args.write),
        ConnectionsGroupAction::Remove(args) => group_remove(args.name, args.write),
        ConnectionsGroupAction::Rename(args) => {
            group_rename(args.old_name, args.new_name, args.write)
        }
    }
}

fn apply_connections_snapshot(
    path: String,
    strategy: ConnectionsApplyStrategy,
    write: WriteArgs,
) -> CliResult<i32> {
    let snapshot = read_connections_snapshot(&path, write.json)?;
    let changes = connections_snapshot_changes(&snapshot);
    finish_connections_write(write, changes, |store| {
        store
            .apply_saved_connections_snapshot(snapshot, saved_connections_strategy(strategy))
            .map(|_| ())
    })
}

fn group_add(name: String, write: WriteArgs) -> CliResult<i32> {
    validate_group_name(&name).map_err(|error| runtime_error(error, write.json))?;
    let store = load_connection_store(write.json)?;
    let changes = if store.groups().contains(&name) {
        Vec::new()
    } else {
        vec![ConnectionsChange {
            action: "groupAdd",
            target: name.clone(),
            before: None,
            after: Some(name.clone()),
        }]
    };
    finish_connections_write(write, changes, |store| store.create_group(name).map(|_| ()))
}

fn group_remove(name: String, write: WriteArgs) -> CliResult<i32> {
    let store = load_connection_store(write.json)?;
    let affected = store
        .connection_infos()
        .iter()
        .filter(|connection| connection.group.as_deref() == Some(name.as_str()))
        .count();
    let changes = if !store.groups().contains(&name) && affected == 0 {
        Vec::new()
    } else {
        vec![ConnectionsChange {
            action: "groupRemove",
            target: name.clone(),
            before: Some(format!("connections={affected}")),
            after: None,
        }]
    };
    finish_connections_write(write, changes, |store| {
        store.delete_group(&name).map(|_| ())
    })
}

fn group_rename(old_name: String, new_name: String, write: WriteArgs) -> CliResult<i32> {
    validate_group_name(&new_name).map_err(|error| runtime_error(error, write.json))?;
    let store = load_connection_store(write.json)?;
    let affected = store
        .connection_infos()
        .iter()
        .filter(|connection| connection.group.as_deref() == Some(old_name.as_str()))
        .count();
    let changes = if old_name == new_name || (!store.groups().contains(&old_name) && affected == 0)
    {
        Vec::new()
    } else {
        vec![ConnectionsChange {
            action: "groupRename",
            target: old_name.clone(),
            before: Some(format!("{old_name} connections={affected}")),
            after: Some(new_name.clone()),
        }]
    };
    finish_connections_write(write, changes, |store| {
        store.rename_group(&old_name, new_name).map(|_| ())
    })
}

fn push_connection_field_change(
    changes: &mut Vec<ConnectionsChange>,
    target: &str,
    action: &'static str,
    before: &str,
    after: &str,
) {
    if before != after {
        changes.push(ConnectionsChange {
            action,
            target: target.to_string(),
            before: Some(before.to_string()),
            after: Some(after.to_string()),
        });
    }
}

fn finish_connections_write<E: std::fmt::Display>(
    write: WriteArgs,
    changes: Vec<ConnectionsChange>,
    apply: impl FnOnce(&mut ConnectionStore) -> Result<(), E>,
) -> CliResult<i32> {
    let mut guard = write_guard::prepare_write(&write, !changes.is_empty())?;
    if !write.dry_run && !changes.is_empty() {
        let mut store = load_connection_store_for_write(write.json)?;
        apply(&mut store).map_err(|error| runtime_error(error, write.json))?;
        write_guard::mark_applied(&mut guard);
    }
    let response = connections_write_response(
        default_connections_path().display().to_string(),
        guard,
        changes,
    );
    let ok = response.applied || response.dry_run || response.changes.is_empty();
    match output::format_from_flag(write.json) {
        OutputFormat::Json => output::write_json_with_ok(&response, ok),
        OutputFormat::Text => {
            output::write_text(format_connections_write_text(&response));
            Ok(())
        }
    }?;
    Ok(if ok { 0 } else { 1 })
}

fn read_connections_snapshot(path: &str, json: bool) -> CliResult<SavedConnectionsSyncSnapshot> {
    let contents = fs::read_to_string(path).map_err(|error| {
        CliError::new(
            "connections_import_read_failed",
            format!("failed to read connections snapshot {path}: {error}"),
            json,
        )
    })?;
    let value = serde_json::from_str::<Value>(&contents).map_err(|error| {
        CliError::new(
            "connections_import_parse_failed",
            format!("failed to parse connections snapshot {path}: {error}"),
            json,
        )
    })?;
    let snapshot_value = value.get("snapshot").cloned().unwrap_or(value);
    serde_json::from_value::<SavedConnectionsSyncSnapshot>(snapshot_value).map_err(|error| {
        CliError::new(
            "connections_import_parse_failed",
            format!("failed to decode connections snapshot {path}: {error}"),
            json,
        )
    })
}

fn saved_connections_strategy(
    strategy: ConnectionsApplyStrategy,
) -> SavedConnectionsConflictStrategy {
    match strategy {
        ConnectionsApplyStrategy::Skip => SavedConnectionsConflictStrategy::Skip,
        ConnectionsApplyStrategy::Replace => SavedConnectionsConflictStrategy::Replace,
        ConnectionsApplyStrategy::Merge => SavedConnectionsConflictStrategy::Merge,
    }
}

fn connections_snapshot_changes(snapshot: &SavedConnectionsSyncSnapshot) -> Vec<ConnectionsChange> {
    // This is a preview of incoming records; final conflict counts come from the core store apply path.
    snapshot
        .records
        .iter()
        .map(|record| ConnectionsChange {
            action: if record.deleted {
                "snapshotDelete"
            } else {
                "snapshotUpsert"
            },
            target: record.id.clone(),
            before: None,
            after: record
                .payload
                .as_ref()
                .map(|connection| connection.name.clone()),
        })
        .collect()
}

fn list_connections(args: JsonArgs) -> CliResult<()> {
    let store = load_connection_store(args.json)?;
    let connections = store.connection_infos();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ConnectionsListResponse {
            path: store.path().display().to_string(),
            count: connections.len(),
            connections,
        }),
        OutputFormat::Text => {
            if connections.is_empty() {
                output::write_text("No saved connections");
            } else {
                for connection in connections {
                    output::write_text(format_connection_row(&connection));
                }
            }
            Ok(())
        }
    }
}

fn list_groups(args: JsonArgs) -> CliResult<()> {
    let store = load_connection_store(args.json)?;
    let groups = store.groups().to_vec();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ConnectionGroupsResponse {
            path: store.path().display().to_string(),
            count: groups.len(),
            groups,
        }),
        OutputFormat::Text => {
            if groups.is_empty() {
                output::write_text("No connection groups");
            } else {
                for group in groups {
                    output::write_text(group);
                }
            }
            Ok(())
        }
    }
}

fn search_connections(args: ConnectionSearchArgs) -> CliResult<()> {
    let store = load_connection_store(args.json)?;
    let connections = filter_connections(&store.connection_infos(), &args.query);
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ConnectionsListResponse {
            path: store.path().display().to_string(),
            count: connections.len(),
            connections,
        }),
        OutputFormat::Text => {
            if connections.is_empty() {
                output::write_text("No matching connections");
            } else {
                for connection in connections {
                    output::write_text(format_connection_row(&connection));
                }
            }
            Ok(())
        }
    }
}

fn show_connection(args: ConnectionShowArgs) -> CliResult<()> {
    let store = load_connection_store(args.json)?;
    let connections = store.connection_infos();
    let Some(connection) = find_connection(&connections, &args.query) else {
        return Err(CliError::new(
            "connection_not_found",
            format!("connection '{}' was not found", args.query),
            args.json,
        ));
    };
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ConnectionShowResponse {
            path: store.path().display().to_string(),
            connection,
        }),
        OutputFormat::Text => {
            output::write_text(format_connection_details(&connection));
            Ok(())
        }
    }
}

fn export_connections(args: ConnectionsExportArgs) -> CliResult<()> {
    let store = load_connection_store(args.json)?;
    if args.format == ConnectionsExportFormat::RawSafe {
        return export_raw_safe_connections(args, &store);
    }
    // Sync export mirrors cloud-sync payload shape and omits credential material.
    let snapshot = store
        .export_saved_connections_snapshot()
        .map_err(|error| runtime_error(error, args.json))?;
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&serde_json::json!({
            "path": store.path().display().to_string(),
            "format": "sync",
            "revision": snapshot.revision,
            "exportedAt": snapshot.exported_at,
            "count": snapshot.records.len(),
            "snapshot": snapshot,
        })),
        OutputFormat::Text => {
            output::write_text(serde_json::to_string_pretty(&snapshot).map_err(|error| {
                CliError::new("serialization_failed", error.to_string(), args.json)
            })?);
            Ok(())
        }
    }
}

fn export_raw_safe_connections(
    args: ConnectionsExportArgs,
    store: &ConnectionStore,
) -> CliResult<()> {
    // Raw-safe means user-facing connection metadata only; no keychain or password values are read.
    let response = RawSafeConnectionsExportResponse {
        path: store.path().display().to_string(),
        format: "raw-safe",
        count: store.connections().len(),
        groups: store.groups().to_vec(),
        connections: store.connection_infos(),
    };
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(serde_json::to_string_pretty(&response).map_err(|error| {
                CliError::new("serialization_failed", error.to_string(), args.json)
            })?);
            Ok(())
        }
    }
}

fn load_connection_store(json: bool) -> CliResult<ConnectionStore> {
    ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, json))
}

fn load_connection_store_for_write(json: bool) -> CliResult<ConnectionStore> {
    ConnectionStore::load(default_connections_path()).map_err(|error| runtime_error(error, json))
}

fn connections_write_response(
    path: String,
    guard: WriteGuardPlan,
    changes: Vec<ConnectionsChange>,
) -> ConnectionsWriteResponse {
    ConnectionsWriteResponse {
        path,
        applied: guard.applied,
        dry_run: guard.dry_run,
        backup_path: guard.backup_path,
        backup_size_bytes: guard.backup_size_bytes,
        changes,
    }
}

fn format_connections_write_text(response: &ConnectionsWriteResponse) -> String {
    let mut lines = vec![format!(
        "applied: {} dryRun={} changes={} backup={}",
        response.applied,
        response.dry_run,
        response.changes.len(),
        response.backup_path.as_deref().unwrap_or("-")
    )];
    for change in &response.changes {
        lines.push(format!(
            "{}\t{}\t{}\t=>\t{}",
            change.action,
            change.target,
            change.before.as_deref().unwrap_or("-"),
            change.after.as_deref().unwrap_or("-")
        ));
    }
    lines.join("\n")
}

fn filter_connections(connections: &[ConnectionInfo], query: &str) -> Vec<ConnectionInfo> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return connections.to_vec();
    }
    connections
        .iter()
        .filter(|connection| {
            connection.id.to_ascii_lowercase().contains(&query)
                || connection.name.to_ascii_lowercase().contains(&query)
                || connection.host.to_ascii_lowercase().contains(&query)
                || connection.username.to_ascii_lowercase().contains(&query)
                || connection
                    .group
                    .as_ref()
                    .is_some_and(|group| group.to_ascii_lowercase().contains(&query))
                || connection
                    .tags
                    .iter()
                    .any(|tag| tag.to_ascii_lowercase().contains(&query))
        })
        .cloned()
        .collect()
}

fn find_connection(connections: &[ConnectionInfo], query: &str) -> Option<ConnectionInfo> {
    connections
        .iter()
        .find(|connection| connection.id == query || connection.name == query)
        .cloned()
        .or_else(|| {
            let lower = query.to_ascii_lowercase();
            connections
                .iter()
                .find(|connection| connection.name.to_ascii_lowercase() == lower)
                .cloned()
        })
}

fn format_connection_row(connection: &ConnectionInfo) -> String {
    format!(
        "{}\t{}@{}:{}\t{}",
        connection.name, connection.username, connection.host, connection.port, connection.id
    )
}

fn format_connection_details(connection: &ConnectionInfo) -> String {
    let group = connection.group.as_deref().unwrap_or("-");
    let tags = if connection.tags.is_empty() {
        "-".to_string()
    } else {
        connection.tags.join(",")
    };
    format!(
        "id: {}\nname: {}\ngroup: {}\nhost: {}\nport: {}\nusername: {}\nauth: {:?}\ntags: {}",
        connection.id,
        connection.name,
        group,
        connection.host,
        connection.port,
        connection.username,
        connection.auth_type,
        tags
    )
}
