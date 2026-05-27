// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};

use super::{JsonArgs, WriteArgs};

#[derive(Debug, Args)]
#[command(
    long_about = "Create, inspect, verify, and restore local OxideTerm backups. Restore supports section-limited dry-runs and requires --yes for real writes."
)]
#[command(
    after_help = "Examples:\n  oxideterm backup preview --json\n  oxideterm backup create --output ./oxideterm-backup.json\n  oxideterm backup inspect oxideterm-backup-123 --summary\n  oxideterm backup inspect ./backup.json --section connections --json\n  oxideterm backup restore ./backup.json --section settings --dry-run"
)]
pub struct BackupCommand {
    #[command(subcommand)]
    pub action: BackupAction,
}

#[derive(Debug, Subcommand)]
pub enum BackupAction {
    #[command(about = "Preview the backup document without writing it")]
    Preview(JsonArgs),
    #[command(about = "Create a backup of settings, connections, and cloud-sync metadata")]
    Create(BackupCreateArgs),
    #[command(about = "List local CLI backups")]
    List(JsonArgs),
    #[command(about = "Inspect a backup summary or section")]
    Inspect(BackupInspectArgs),
    #[command(about = "Verify a backup can be read and recognized")]
    Verify(BackupInspectArgs),
    #[command(about = "Restore settings, connections, or cloud-sync metadata from a backup")]
    Restore(BackupRestoreArgs),
}

#[derive(Debug, Args)]
pub struct BackupCreateArgs {
    #[arg(
        long,
        value_name = "PATH",
        help = "Write the backup to a specific path"
    )]
    pub output: Option<String>,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct BackupInspectArgs {
    #[arg(help = "Backup file path, file name, or backup id")]
    pub query: String,
    #[arg(long, value_enum, help = "Inspect only one backup section")]
    pub section: Option<BackupInspectSection>,
    #[arg(long, conflicts_with = "full", help = "Print only a compact summary")]
    pub summary: bool,
    #[arg(long, conflicts_with = "summary", help = "Print the full backup JSON")]
    pub full: bool,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct BackupRestoreArgs {
    #[arg(help = "Backup file path, file name, or backup id")]
    pub query: String,
    #[arg(long, value_enum, help = "Restore only one backup section")]
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
