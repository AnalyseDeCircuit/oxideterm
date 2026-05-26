// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};

use super::{JsonArgs, WriteArgs};

#[derive(Debug, Args)]
pub struct ConnectionsCommand {
    #[command(subcommand)]
    pub action: ConnectionsAction,
}

#[derive(Debug, Subcommand)]
pub enum ConnectionsAction {
    List(JsonArgs),
    Show(ConnectionShowArgs),
    Groups(JsonArgs),
    Search(ConnectionSearchArgs),
    Export(ConnectionsExportArgs),
    Validate(ConnectionsValidateArgs),
    Create(ConnectionCreateArgs),
    Edit(ConnectionEditArgs),
    Delete(ConnectionDeleteArgs),
    Rename(ConnectionRenameArgs),
    Import(ConnectionsApplySnapshotArgs),
    #[command(name = "apply-snapshot")]
    ApplySnapshot(ConnectionsApplySnapshotArgs),
    Group(ConnectionsGroupCommand),
}

#[derive(Debug, Args)]
pub struct ConnectionShowArgs {
    pub query: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ConnectionSearchArgs {
    pub query: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ConnectionCreateArgs {
    #[arg(long = "spec")]
    pub spec_path: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionEditArgs {
    pub query: String,
    #[arg(long = "spec")]
    pub spec_path: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionDeleteArgs {
    pub query: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionRenameArgs {
    pub query: String,
    pub name: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionsApplySnapshotArgs {
    pub path: String,
    #[arg(long, value_enum, default_value_t = ConnectionsApplyStrategy::Skip)]
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
    Add(ConnectionsGroupNameArgs),
    Remove(ConnectionsGroupNameArgs),
    Rename(ConnectionsGroupRenameArgs),
}

#[derive(Debug, Args)]
pub struct ConnectionsGroupNameArgs {
    pub name: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionsGroupRenameArgs {
    pub old_name: String,
    pub new_name: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ConnectionsExportArgs {
    #[arg(long, value_enum, default_value_t = ConnectionsExportFormat::Sync)]
    pub format: ConnectionsExportFormat,
    #[arg(long)]
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
    #[arg(long)]
    pub strict: bool,
    #[arg(long)]
    pub json: bool,
}
