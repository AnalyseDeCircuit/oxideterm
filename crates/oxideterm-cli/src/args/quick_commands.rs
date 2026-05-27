// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand};

use super::{JsonArgs, WriteArgs};

#[derive(Debug, Args)]
#[command(
    name = "quick-commands",
    long_about = "Manage terminal Quick Commands independently from portable .oxide bundles."
)]
#[command(
    after_help = "Examples:\n  oxideterm quick-commands list\n  oxideterm quick-commands create --name Uptime --command uptime --category system --yes\n  oxideterm quick-commands export --json"
)]
pub struct QuickCommandsCommand {
    #[command(subcommand)]
    pub action: QuickCommandsAction,
}

#[derive(Debug, Subcommand)]
pub enum QuickCommandsAction {
    #[command(about = "List Quick Commands")]
    List(JsonArgs),
    #[command(about = "Show one Quick Command")]
    Show(QuickCommandShowArgs),
    #[command(about = "Create a Quick Command")]
    Create(QuickCommandCreateArgs),
    #[command(about = "Edit a Quick Command")]
    Edit(QuickCommandEditArgs),
    #[command(about = "Delete a Quick Command")]
    Delete(QuickCommandDeleteArgs),
    #[command(about = "Export Quick Commands as a snapshot")]
    Export(JsonArgs),
    #[command(about = "Import a Quick Commands snapshot")]
    Import(QuickCommandImportArgs),
}

#[derive(Debug, Args)]
pub struct QuickCommandShowArgs {
    #[arg(help = "Command query: id or name")]
    pub query: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct QuickCommandCreateArgs {
    #[arg(long, help = "Command name")]
    pub name: String,
    #[arg(long, help = "Shell command text")]
    pub command: String,
    #[arg(long, default_value = "custom", help = "Category id")]
    pub category: String,
    #[arg(long, help = "Optional description")]
    pub description: Option<String>,
    #[arg(long, help = "Optional host pattern")]
    pub host_pattern: Option<String>,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct QuickCommandEditArgs {
    #[arg(help = "Command query: id or name")]
    pub query: String,
    #[arg(long, help = "Command name")]
    pub name: Option<String>,
    #[arg(long, help = "Shell command text")]
    pub command: Option<String>,
    #[arg(long, help = "Category id")]
    pub category: Option<String>,
    #[arg(long, help = "Optional description")]
    pub description: Option<String>,
    #[arg(long, help = "Optional host pattern")]
    pub host_pattern: Option<String>,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct QuickCommandDeleteArgs {
    #[arg(help = "Command query: id or name")]
    pub query: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct QuickCommandImportArgs {
    #[arg(help = "Path to a Quick Commands snapshot JSON file")]
    pub path: String,
    #[command(flatten)]
    pub write: WriteArgs,
}
