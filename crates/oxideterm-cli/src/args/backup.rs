// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};

use super::{JsonArgs, WriteArgs};

#[derive(Debug, Args)]
pub struct BackupCommand {
    #[command(subcommand)]
    pub action: BackupAction,
}

#[derive(Debug, Subcommand)]
pub enum BackupAction {
    Preview(JsonArgs),
    Create(BackupCreateArgs),
    List(JsonArgs),
    Inspect(BackupInspectArgs),
    Verify(BackupInspectArgs),
    Restore(BackupRestoreArgs),
}

#[derive(Debug, Args)]
pub struct BackupCreateArgs {
    #[arg(long)]
    pub output: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct BackupInspectArgs {
    pub query: String,
    #[arg(long, value_enum)]
    pub section: Option<BackupInspectSection>,
    #[arg(long, conflicts_with = "full")]
    pub summary: bool,
    #[arg(long, conflicts_with = "summary")]
    pub full: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct BackupRestoreArgs {
    pub query: String,
    #[arg(long, value_enum)]
    pub section: Option<BackupInspectSection>,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum BackupInspectSection {
    Connections,
    Settings,
    CloudSync,
}
