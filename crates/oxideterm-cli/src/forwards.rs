// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, fs};

use oxideterm_connections::ConnectionStore;
use oxideterm_forwarding::{
    ForwardRule, ForwardStatus, ForwardType, PersistedForward, SavedForwardStore,
    SavedForwardsSyncSnapshot,
};
use serde::Serialize;

use crate::{
    args::{
        ForwardCreateArgs, ForwardDeleteArgs, ForwardEditArgs, ForwardShowArgs, ForwardTypeArg,
        ForwardsAction, ForwardsCommand, ForwardsImportArgs, JsonArgs, WriteArgs,
    },
    error::{CliError, CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::{default_connections_path, default_forwards_path},
    write_guard,
};

const CLI_FORWARD_SESSION_ID: &str = "cli-managed";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForwardsListResponse {
    path: String,
    count: usize,
    forwards: Vec<PersistedForward>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForwardShowResponse {
    path: String,
    forward: PersistedForward,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ForwardChange {
    action: &'static str,
    target: String,
    before: Option<String>,
    after: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForwardsWriteResponse {
    path: String,
    applied: bool,
    dry_run: bool,
    backup_path: Option<String>,
    backup_size_bytes: Option<u64>,
    changes: Vec<ForwardChange>,
}

pub fn run(command: ForwardsCommand) -> CliResult<i32> {
    match command.action {
        ForwardsAction::List(args) => {
            list(args)?;
            Ok(0)
        }
        ForwardsAction::Show(args) => {
            show(args)?;
            Ok(0)
        }
        ForwardsAction::Create(args) => create(args),
        ForwardsAction::Edit(args) => edit(args),
        ForwardsAction::Delete(args) => delete(args),
        ForwardsAction::Validate(args) => validate(args),
        ForwardsAction::Export(args) => {
            export(args)?;
            Ok(0)
        }
        ForwardsAction::Import(args) => import(args),
    }
}

fn list(args: JsonArgs) -> CliResult<()> {
    let store = load_store(args.json)?;
    let forwards = store.load_syncable_forwards();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ForwardsListResponse {
            path: store.path().display().to_string(),
            count: forwards.len(),
            forwards,
        }),
        OutputFormat::Text => {
            if forwards.is_empty() {
                output::write_text("No saved forwards");
            } else {
                for forward in forwards {
                    output::write_text(format_forward_row(&forward));
                }
            }
            Ok(())
        }
    }
}

fn show(args: ForwardShowArgs) -> CliResult<()> {
    let store = load_store(args.json)?;
    let forward = find_forward(&store.load_syncable_forwards(), &args.query).ok_or_else(|| {
        CliError::new(
            "forward_not_found",
            format!("forward '{}' was not found", args.query),
            args.json,
        )
    })?;
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ForwardShowResponse {
            path: store.path().display().to_string(),
            forward,
        }),
        OutputFormat::Text => {
            output::write_text(format_forward_details(&forward));
            Ok(())
        }
    }
}

fn create(args: ForwardCreateArgs) -> CliResult<i32> {
    let forward_type = forward_type_from_arg(args.forward_type);
    let rule = forward_rule_from_create(&args, forward_type)?;
    validate_rule(&rule, args.write.json)?;
    let owner_connection_id =
        resolve_owner_connection_id(args.connection.as_deref(), args.write.json)?;
    let session_id = args
        .session_id
        .clone()
        .unwrap_or_else(|| CLI_FORWARD_SESSION_ID.to_string());
    let persisted = PersistedForward::new(
        rule.id.clone(),
        session_id,
        owner_connection_id,
        forward_type,
        rule,
        args.auto_start,
    );
    let change = ForwardChange {
        action: "create",
        target: persisted.id.clone(),
        before: None,
        after: Some(format_forward_identity(&persisted)),
    };
    finish_write(args.write, vec![change], |store| {
        store.persist_forward(persisted)
    })
}

fn edit(args: ForwardEditArgs) -> CliResult<i32> {
    let store = load_store(args.write.json)?;
    let mut forward =
        find_forward(&store.load_syncable_forwards(), &args.query).ok_or_else(|| {
            CliError::new(
                "forward_not_found",
                format!("forward '{}' was not found", args.query),
                args.write.json,
            )
        })?;
    let before = format_forward_identity(&forward);
    apply_forward_edit(&mut forward, &args);
    validate_rule(&forward.rule, args.write.json)?;
    let after = format_forward_identity(&forward);
    let changes = (before != after || args.auto_start.is_some())
        .then(|| ForwardChange {
            action: "edit",
            target: forward.id.clone(),
            before: Some(before),
            after: Some(after),
        })
        .into_iter()
        .collect();
    finish_write(args.write, changes, |store| store.persist_forward(forward))
}

fn delete(args: ForwardDeleteArgs) -> CliResult<i32> {
    let store = load_store(args.write.json)?;
    let forward = find_forward(&store.load_syncable_forwards(), &args.query).ok_or_else(|| {
        CliError::new(
            "forward_not_found",
            format!("forward '{}' was not found", args.query),
            args.write.json,
        )
    })?;
    let change = ForwardChange {
        action: "delete",
        target: forward.id.clone(),
        before: Some(format_forward_identity(&forward)),
        after: None,
    };
    finish_write(args.write, vec![change], |store| {
        store.delete_persisted_forward(&forward.id)
    })
}

fn validate(args: JsonArgs) -> CliResult<i32> {
    let store = load_store(args.json)?;
    let forwards = store.load_syncable_forwards();
    let errors = forwards
        .iter()
        .filter_map(|forward| {
            validate_rule(&forward.rule, args.json)
                .err()
                .map(|error| format!("{}: {}", forward.id, error.message))
        })
        .collect::<Vec<_>>();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json_with_ok(
            &serde_json::json!({
                "path": store.path().display().to_string(),
                "ok": errors.is_empty(),
                "count": forwards.len(),
                "errors": errors,
            }),
            errors.is_empty(),
        )?,
        OutputFormat::Text => {
            if errors.is_empty() {
                output::write_text(format!("ok: true forwards={}", forwards.len()));
            } else {
                output::write_text(errors.join("\n"));
            }
        }
    }
    Ok(if errors.is_empty() { 0 } else { 1 })
}

fn export(args: JsonArgs) -> CliResult<()> {
    let store = load_store(args.json)?;
    let snapshot = store
        .export_snapshot()
        .map_err(|error| runtime_error(error, args.json))?;
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&serde_json::json!({
            "path": store.path().display().to_string(),
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

fn import(args: ForwardsImportArgs) -> CliResult<i32> {
    let contents = fs::read_to_string(&args.path).map_err(|error| {
        CliError::new(
            "forwards_import_read_failed",
            format!("failed to read forwards snapshot {}: {error}", args.path),
            args.write.json,
        )
    })?;
    let value = serde_json::from_str::<serde_json::Value>(&contents).map_err(|error| {
        CliError::new(
            "forwards_import_parse_failed",
            format!("failed to parse forwards snapshot {}: {error}", args.path),
            args.write.json,
        )
    })?;
    let snapshot_value = value.get("snapshot").cloned().unwrap_or(value);
    let snapshot =
        serde_json::from_value::<SavedForwardsSyncSnapshot>(snapshot_value).map_err(|error| {
            CliError::new(
                "forwards_import_parse_failed",
                format!("failed to decode forwards snapshot {}: {error}", args.path),
                args.write.json,
            )
        })?;
    let changes = snapshot
        .records
        .iter()
        .map(|record| ForwardChange {
            action: if record.deleted {
                "snapshotDelete"
            } else {
                "snapshotUpsert"
            },
            target: record.id.clone(),
            before: None,
            after: record.payload.as_ref().map(|payload| {
                format!(
                    "{}:{} -> {}:{}",
                    payload.bind_address,
                    payload.bind_port,
                    payload.target_host,
                    payload.target_port
                )
            }),
        })
        .collect::<Vec<_>>();
    let valid_owner_ids = valid_connection_ids(args.write.json)?;
    finish_write(args.write, changes, |store| {
        store.apply_snapshot(snapshot, &valid_owner_ids).map(|_| ())
    })
}

fn finish_write<E: std::fmt::Display>(
    write: WriteArgs,
    changes: Vec<ForwardChange>,
    apply: impl FnOnce(&SavedForwardStore) -> Result<(), E>,
) -> CliResult<i32> {
    let mut guard = write_guard::prepare_write(&write, !changes.is_empty())?;
    if !write.dry_run && !changes.is_empty() {
        let store = load_store(write.json)?;
        apply(&store).map_err(|error| runtime_error(error, write.json))?;
        write_guard::mark_applied(&mut guard);
    }
    let response = ForwardsWriteResponse {
        path: default_forwards_path().display().to_string(),
        applied: guard.applied,
        dry_run: guard.dry_run,
        backup_path: guard.backup_path,
        backup_size_bytes: guard.backup_size_bytes,
        changes,
    };
    let ok = response.applied || response.dry_run || response.changes.is_empty();
    match output::format_from_flag(write.json) {
        OutputFormat::Json => output::write_json_with_ok(&response, ok),
        OutputFormat::Text => {
            output::write_text(format_write_text(&response));
            Ok(())
        }
    }?;
    Ok(if ok { 0 } else { 1 })
}

fn load_store(json: bool) -> CliResult<SavedForwardStore> {
    SavedForwardStore::load(default_forwards_path()).map_err(|error| runtime_error(error, json))
}

fn valid_connection_ids(json: bool) -> CliResult<HashSet<String>> {
    let store = ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, json))?;
    Ok(store
        .connection_infos()
        .into_iter()
        .map(|connection| connection.id)
        .collect())
}

fn resolve_owner_connection_id(query: Option<&str>, json: bool) -> CliResult<Option<String>> {
    let Some(query) = query else {
        return Ok(None);
    };
    let store = ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, json))?;
    store
        .connection_infos()
        .into_iter()
        .find(|connection| connection.id == query || connection.name.eq_ignore_ascii_case(query))
        .map(|connection| Some(connection.id))
        .ok_or_else(|| {
            CliError::new(
                "connection_not_found",
                format!("connection '{query}' was not found"),
                json,
            )
        })
}

fn forward_rule_from_create(
    args: &ForwardCreateArgs,
    forward_type: ForwardType,
) -> CliResult<ForwardRule> {
    let mut rule = match forward_type {
        ForwardType::Local => ForwardRule::local(
            args.bind_address.clone(),
            args.bind_port,
            required_target_host(args.target_host.as_deref(), args.write.json)?,
            required_target_port(args.target_port, args.write.json)?,
        ),
        ForwardType::Remote => ForwardRule::remote(
            args.bind_address.clone(),
            args.bind_port,
            required_target_host(args.target_host.as_deref(), args.write.json)?,
            required_target_port(args.target_port, args.write.json)?,
        ),
        ForwardType::Dynamic => ForwardRule::dynamic(args.bind_address.clone(), args.bind_port),
    };
    if let Some(description) = &args.description {
        rule.description = description.clone();
    }
    rule.status = ForwardStatus::Stopped;
    Ok(rule)
}

fn apply_forward_edit(forward: &mut PersistedForward, args: &ForwardEditArgs) {
    if let Some(forward_type) = args.forward_type {
        forward.forward_type = forward_type_from_arg(forward_type);
        forward.rule.forward_type = forward.forward_type;
    }
    if let Some(bind_address) = &args.bind_address {
        forward.rule.bind_address = bind_address.clone();
    }
    if let Some(bind_port) = args.bind_port {
        forward.rule.bind_port = bind_port;
    }
    if let Some(target_host) = &args.target_host {
        forward.rule.target_host = target_host.clone();
    }
    if let Some(target_port) = args.target_port {
        forward.rule.target_port = target_port;
    }
    if let Some(description) = &args.description {
        forward.rule.description = description.clone();
    }
    if let Some(auto_start) = args.auto_start {
        forward.auto_start = auto_start;
    }
    forward.mark_updated();
}

fn validate_rule(rule: &ForwardRule, json: bool) -> CliResult<()> {
    if rule.bind_port == 0 {
        return Err(CliError::new(
            "forward_invalid",
            "bind port must be greater than zero",
            json,
        ));
    }
    if !matches!(rule.forward_type, ForwardType::Dynamic)
        && (rule.target_host.trim().is_empty() || rule.target_port == 0)
    {
        return Err(CliError::new(
            "forward_invalid",
            "local and remote forwards require target host and target port",
            json,
        ));
    }
    Ok(())
}

fn required_target_host(value: Option<&str>, json: bool) -> CliResult<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| CliError::new("forward_invalid", "--target-host is required", json))
}

fn required_target_port(value: Option<u16>, json: bool) -> CliResult<u16> {
    value.filter(|port| *port > 0).ok_or_else(|| {
        CliError::new(
            "forward_invalid",
            "--target-port must be provided and greater than zero",
            json,
        )
    })
}

fn find_forward(forwards: &[PersistedForward], query: &str) -> Option<PersistedForward> {
    let lower = query.to_ascii_lowercase();
    forwards
        .iter()
        .find(|forward| {
            forward.id == query
                || forward
                    .rule
                    .description
                    .to_ascii_lowercase()
                    .contains(&lower)
                || forward.rule.bind_port.to_string() == query
                || forward
                    .rule
                    .target_host
                    .to_ascii_lowercase()
                    .contains(&lower)
        })
        .cloned()
}

fn forward_type_from_arg(value: ForwardTypeArg) -> ForwardType {
    match value {
        ForwardTypeArg::Local => ForwardType::Local,
        ForwardTypeArg::Remote => ForwardType::Remote,
        ForwardTypeArg::Dynamic => ForwardType::Dynamic,
    }
}

fn format_forward_row(forward: &PersistedForward) -> String {
    format!(
        "{}\t{}\t{}",
        forward.id,
        format_forward_identity(forward),
        forward.rule.description
    )
}

fn format_forward_details(forward: &PersistedForward) -> String {
    format!(
        "id: {}\ntype: {}\nbind: {}:{}\ntarget: {}:{}\nautoStart: {}\nownerConnectionId: {}\ndescription: {}",
        forward.id,
        forward.forward_type.as_str(),
        forward.rule.bind_address,
        forward.rule.bind_port,
        forward.rule.target_host,
        forward.rule.target_port,
        forward.auto_start,
        forward.owner_connection_id.as_deref().unwrap_or("-"),
        forward.rule.description
    )
}

fn format_forward_identity(forward: &PersistedForward) -> String {
    format!(
        "{} {}:{} -> {}:{}",
        forward.forward_type.as_str(),
        forward.rule.bind_address,
        forward.rule.bind_port,
        forward.rule.target_host,
        forward.rule.target_port
    )
}

fn format_write_text(response: &ForwardsWriteResponse) -> String {
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
