// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};

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
    #[arg(long, action = clap::ArgAction::Append, help = "Allowed host pattern; repeat for multiple patterns")]
    pub host_pattern: Vec<String>,
    #[arg(long, value_enum, action = clap::ArgAction::Append, help = "Allowed target protocol; repeat for multiple protocols")]
    pub protocol: Vec<QuickCommandProtocolArg>,
    #[arg(long, help = "JSON array of Quick Command parameter definitions")]
    pub parameters_json: Option<String>,
    #[arg(
        long,
        value_enum,
        default_value = "inherit",
        help = "Execution confirmation policy"
    )]
    pub confirmation: QuickCommandConfirmationArg,
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
    #[arg(
        long,
        conflicts_with = "clear_description",
        help = "Optional description"
    )]
    pub description: Option<String>,
    #[arg(long, help = "Remove the description")]
    pub clear_description: bool,
    #[arg(long, action = clap::ArgAction::Append, conflicts_with = "clear_host_patterns", help = "Replace allowed host patterns; repeat for multiple patterns")]
    pub host_pattern: Vec<String>,
    #[arg(long, help = "Remove all host restrictions")]
    pub clear_host_patterns: bool,
    #[arg(long, value_enum, action = clap::ArgAction::Append, conflicts_with = "clear_protocols", help = "Replace allowed target protocols; repeat for multiple protocols")]
    pub protocol: Vec<QuickCommandProtocolArg>,
    #[arg(long, help = "Allow every target protocol")]
    pub clear_protocols: bool,
    #[arg(
        long,
        conflicts_with = "clear_parameters",
        help = "JSON array replacing parameter definitions"
    )]
    pub parameters_json: Option<String>,
    #[arg(long, help = "Remove all parameter definitions")]
    pub clear_parameters: bool,
    #[arg(long, value_enum, help = "Execution confirmation policy")]
    pub confirmation: Option<QuickCommandConfirmationArg>,
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

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum QuickCommandProtocolArg {
    Local,
    Ssh,
    Mosh,
    Telnet,
    Serial,
    Tmux,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum QuickCommandConfirmationArg {
    Inherit,
    Always,
}
