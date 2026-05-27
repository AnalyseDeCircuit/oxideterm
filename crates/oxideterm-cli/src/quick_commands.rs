// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fs;

use oxideterm_quick_commands::{QuickCommand, QuickCommandsSnapshot, load_snapshot, save_snapshot};
use serde::Serialize;

use crate::{
    args::{
        JsonArgs, QuickCommandCreateArgs, QuickCommandDeleteArgs, QuickCommandEditArgs,
        QuickCommandImportArgs, QuickCommandShowArgs, QuickCommandsAction, QuickCommandsCommand,
        WriteArgs,
    },
    error::{CliError, CliResult},
    output::{self, OutputFormat},
    paths::default_settings_path,
    write_guard,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuickCommandsListResponse {
    path: String,
    count: usize,
    snapshot: QuickCommandsSnapshot,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuickCommandShowResponse {
    path: String,
    command: QuickCommand,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuickCommandChange {
    action: &'static str,
    target: String,
    before: Option<String>,
    after: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuickCommandWriteResponse {
    path: String,
    applied: bool,
    dry_run: bool,
    backup_path: Option<String>,
    backup_size_bytes: Option<u64>,
    changes: Vec<QuickCommandChange>,
}

pub fn run(command: QuickCommandsCommand) -> CliResult<i32> {
    match command.action {
        QuickCommandsAction::List(args) => {
            list(args)?;
            Ok(0)
        }
        QuickCommandsAction::Show(args) => {
            show(args)?;
            Ok(0)
        }
        QuickCommandsAction::Create(args) => create(args),
        QuickCommandsAction::Edit(args) => edit(args),
        QuickCommandsAction::Delete(args) => delete(args),
        QuickCommandsAction::Export(args) => {
            export(args)?;
            Ok(0)
        }
        QuickCommandsAction::Import(args) => import(args),
    }
}

fn list(args: JsonArgs) -> CliResult<()> {
    let settings_path = default_settings_path();
    let snapshot = read_snapshot(args.json)?;
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&QuickCommandsListResponse {
            path: settings_path.display().to_string(),
            count: snapshot.commands.len(),
            snapshot,
        }),
        OutputFormat::Text => {
            if snapshot.commands.is_empty() {
                output::write_text("No Quick Commands");
            } else {
                for command in snapshot.commands {
                    output::write_text(format_quick_command_row(&command));
                }
            }
            Ok(())
        }
    }
}

fn show(args: QuickCommandShowArgs) -> CliResult<()> {
    let settings_path = default_settings_path();
    let snapshot = read_snapshot(args.json)?;
    let command = find_command(&snapshot.commands, &args.query).ok_or_else(|| {
        CliError::new(
            "quick_command_not_found",
            format!("Quick Command '{}' was not found", args.query),
            args.json,
        )
    })?;
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&QuickCommandShowResponse {
            path: settings_path.display().to_string(),
            command,
        }),
        OutputFormat::Text => {
            output::write_text(format_quick_command_details(&command));
            Ok(())
        }
    }
}

fn create(args: QuickCommandCreateArgs) -> CliResult<i32> {
    let mut snapshot = read_snapshot(args.write.json)?;
    let now = now_ms();
    let command = QuickCommand {
        id: format!("qc-cli-{now}"),
        name: args.name,
        command: args.command,
        category: args.category,
        description: args.description,
        host_pattern: args.host_pattern,
        created_at: now,
        updated_at: now,
    };
    let change = QuickCommandChange {
        action: "create",
        target: command.id.clone(),
        before: None,
        after: Some(command.name.clone()),
    };
    snapshot.commands.push(command);
    snapshot.updated_at = now;
    finish_write(args.write, vec![change], snapshot)
}

fn edit(args: QuickCommandEditArgs) -> CliResult<i32> {
    let mut snapshot = read_snapshot(args.write.json)?;
    let command = find_command_mut(&mut snapshot.commands, &args.query).ok_or_else(|| {
        CliError::new(
            "quick_command_not_found",
            format!("Quick Command '{}' was not found", args.query),
            args.write.json,
        )
    })?;
    let before = format_quick_command_identity(command);
    if let Some(name) = args.name {
        command.name = name;
    }
    if let Some(command_text) = args.command {
        command.command = command_text;
    }
    if let Some(category) = args.category {
        command.category = category;
    }
    if let Some(description) = args.description {
        command.description = Some(description);
    }
    if let Some(host_pattern) = args.host_pattern {
        command.host_pattern = Some(host_pattern);
    }
    command.updated_at = now_ms();
    let after = format_quick_command_identity(command);
    snapshot.updated_at = command.updated_at;
    let changes = (before != after)
        .then(|| QuickCommandChange {
            action: "edit",
            target: command.id.clone(),
            before: Some(before),
            after: Some(after),
        })
        .into_iter()
        .collect();
    finish_write(args.write, changes, snapshot)
}

fn delete(args: QuickCommandDeleteArgs) -> CliResult<i32> {
    let mut snapshot = read_snapshot(args.write.json)?;
    let Some(index) = snapshot.commands.iter().position(|command| {
        command.id == args.query || command.name.eq_ignore_ascii_case(&args.query)
    }) else {
        return Err(CliError::new(
            "quick_command_not_found",
            format!("Quick Command '{}' was not found", args.query),
            args.write.json,
        ));
    };
    let command = snapshot.commands.remove(index);
    snapshot.updated_at = now_ms();
    finish_write(
        args.write,
        vec![QuickCommandChange {
            action: "delete",
            target: command.id,
            before: Some(command.name),
            after: None,
        }],
        snapshot,
    )
}

fn export(args: JsonArgs) -> CliResult<()> {
    let snapshot = read_snapshot(args.json)?;
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&snapshot),
        OutputFormat::Text => {
            output::write_text(serde_json::to_string_pretty(&snapshot).map_err(|error| {
                CliError::new("serialization_failed", error.to_string(), args.json)
            })?);
            Ok(())
        }
    }
}

fn import(args: QuickCommandImportArgs) -> CliResult<i32> {
    let contents = fs::read_to_string(&args.path).map_err(|error| {
        CliError::new(
            "quick_commands_import_failed",
            format!(
                "failed to read Quick Commands snapshot {}: {error}",
                args.path
            ),
            args.write.json,
        )
    })?;
    let snapshot = serde_json::from_str::<QuickCommandsSnapshot>(&contents).map_err(|error| {
        CliError::new(
            "quick_commands_import_failed",
            format!(
                "failed to parse Quick Commands snapshot {}: {error}",
                args.path
            ),
            args.write.json,
        )
    })?;
    let count = snapshot.commands.len();
    finish_write(
        args.write,
        vec![QuickCommandChange {
            action: "import",
            target: args.path,
            before: None,
            after: Some(format!("commands={count}")),
        }],
        snapshot,
    )
}

fn finish_write(
    write: WriteArgs,
    changes: Vec<QuickCommandChange>,
    snapshot: QuickCommandsSnapshot,
) -> CliResult<i32> {
    let mut guard = write_guard::prepare_write(&write, !changes.is_empty())?;
    if !write.dry_run && !changes.is_empty() {
        save_snapshot(&default_settings_path(), &snapshot)
            .map_err(|error| CliError::new("quick_commands_write_failed", error, write.json))?;
        write_guard::mark_applied(&mut guard);
    }
    let response = QuickCommandWriteResponse {
        path: default_settings_path().display().to_string(),
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

fn read_snapshot(json: bool) -> CliResult<QuickCommandsSnapshot> {
    load_snapshot(&default_settings_path())
        .map_err(|error| CliError::new("quick_commands_read_failed", error, json))
}

fn find_command(commands: &[QuickCommand], query: &str) -> Option<QuickCommand> {
    commands
        .iter()
        .find(|command| command.id == query || command.name.eq_ignore_ascii_case(query))
        .cloned()
}

fn find_command_mut<'a>(
    commands: &'a mut [QuickCommand],
    query: &str,
) -> Option<&'a mut QuickCommand> {
    commands
        .iter_mut()
        .find(|command| command.id == query || command.name.eq_ignore_ascii_case(query))
}

fn format_quick_command_row(command: &QuickCommand) -> String {
    format!("{}\t{}\t{}", command.id, command.category, command.name)
}

fn format_quick_command_details(command: &QuickCommand) -> String {
    format!(
        "id: {}\nname: {}\ncategory: {}\ncommand: {}\ndescription: {}",
        command.id,
        command.name,
        command.category,
        command.command,
        command.description.as_deref().unwrap_or("-")
    )
}

fn format_quick_command_identity(command: &QuickCommand) -> String {
    format!("{}:{}:{}", command.category, command.name, command.command)
}

fn format_write_text(response: &QuickCommandWriteResponse) -> String {
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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
