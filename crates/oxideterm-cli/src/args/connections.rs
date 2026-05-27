// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};

use super::{JsonArgs, WriteArgs};

#[derive(Debug, Args)]
#[command(
    long_about = "Inspect and manage saved SSH connections and groups. Export and diagnostics output omit credential values; write commands default to dry-run unless confirmed with --yes."
)]
#[command(
    after_help = "Examples:\n  oxideterm connections list\n  oxideterm connections search prod --json\n  oxideterm connections create --spec ./connection.json --dry-run\n  oxideterm connections rename prod production --yes\n  oxideterm connections export --format raw-safe --json"
)]
pub struct ConnectionsCommand {
    #[command(subcommand)]
    pub action: ConnectionsAction,
}

#[derive(Debug, Subcommand)]
pub enum ConnectionsAction {
    #[command(about = "List saved connections")]
    List(JsonArgs),
    #[command(about = "Show one saved connection by id, name, host, or group")]
    Show(ConnectionShowArgs),
    #[command(about = "List connection groups")]
    Groups(JsonArgs),
    #[command(about = "Search saved connections")]
    Search(ConnectionSearchArgs),
    #[command(about = "Export connections without credential values")]
    Export(ConnectionsExportArgs),
    #[command(about = "Validate saved connections")]
    Validate(ConnectionsValidateArgs),
    #[command(about = "Create a connection from a JSON spec")]
    Create(ConnectionCreateArgs),
    #[command(about = "Edit a connection from a JSON spec")]
    Edit(ConnectionEditArgs),
    #[command(about = "Delete a saved connection")]
    Delete(ConnectionDeleteArgs),
    #[command(about = "Rename a saved connection")]
    Rename(ConnectionRenameArgs),
    #[command(about = "Import a saved-connections snapshot")]
    Import(ConnectionsApplySnapshotArgs),
    #[command(name = "apply-snapshot")]
    #[command(about = "Apply a saved-connections sync snapshot")]
    ApplySnapshot(ConnectionsApplySnapshotArgs),
    #[command(about = "Add, remove, or rename connection groups")]
    Group(ConnectionsGroupCommand),
}

#[derive(Debug, Args)]
pub struct ConnectionShowArgs {
    #[arg(help = "Connection query: id, name, host, or group")]
    pub query: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ConnectionSearchArgs {
    #[arg(help = "Text to match against connection fields")]
    pub query: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ConnectionCreateArgs {
    #[arg(
        long = "spec",
        value_name = "PATH",
        help = "Path to a connection JSON spec"
    )]
    pub spec_path: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionEditArgs {
    #[arg(help = "Connection query: id, name, host, or group")]
    pub query: String,
    #[arg(
        long = "spec",
        value_name = "PATH",
        help = "Path to a connection JSON spec"
    )]
    pub spec_path: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionDeleteArgs {
    #[arg(help = "Connection query: id, name, host, or group")]
    pub query: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionRenameArgs {
    #[arg(help = "Connection query: id, name, host, or group")]
    pub query: String,
    #[arg(help = "New connection name")]
    pub name: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionsApplySnapshotArgs {
    #[arg(help = "Path to a saved-connections snapshot JSON file")]
    pub path: String,
    #[arg(long, value_enum, default_value_t = ConnectionsApplyStrategy::Skip, help = "Conflict strategy for existing connections")]
    pub strategy: ConnectionsApplyStrategy,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ConnectionsApplyStrategy {
    Skip,
    Replace,
    Merge,
}

#[derive(Debug, Args)]
pub struct ConnectionsGroupCommand {
    #[command(subcommand)]
    pub action: ConnectionsGroupAction,
}

#[derive(Debug, Subcommand)]
pub enum ConnectionsGroupAction {
    #[command(about = "Create a connection group")]
    Add(ConnectionsGroupNameArgs),
    #[command(about = "Remove a connection group")]
    Remove(ConnectionsGroupNameArgs),
    #[command(about = "Rename a connection group")]
    Rename(ConnectionsGroupRenameArgs),
}

#[derive(Debug, Args)]
pub struct ConnectionsGroupNameArgs {
    #[arg(help = "Group name")]
    pub name: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionsGroupRenameArgs {
    #[arg(help = "Existing group name")]
    pub old_name: String,
    #[arg(help = "New group name")]
    pub new_name: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionsExportArgs {
    #[arg(long, value_enum, default_value_t = ConnectionsExportFormat::Sync, help = "Export format")]
    pub format: ConnectionsExportFormat,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ConnectionsExportFormat {
    Sync,
    RawSafe,
}

#[derive(Debug, Args)]
pub struct ConnectionsValidateArgs {
    #[arg(long, help = "Treat validation warnings as failures")]
    pub strict: bool,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}
