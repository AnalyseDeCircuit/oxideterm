// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};

use super::{JsonArgs, WriteArgs};

#[derive(Debug, Args)]
#[command(
    long_about = "Manage saved SSH port forwards independently from portable .oxide bundles. Write commands default to dry-run unless confirmed with --yes."
)]
#[command(
    after_help = "Examples:\n  oxideterm forwards list\n  oxideterm forwards create --type local --bind-port 8080 --target-host localhost --target-port 80 --connection prod --yes\n  oxideterm forwards edit web --bind-port 8081 --yes\n  oxideterm forwards export --json"
)]
pub struct ForwardsCommand {
    #[command(subcommand)]
    pub action: ForwardsAction,
}

#[derive(Debug, Subcommand)]
pub enum ForwardsAction {
    #[command(about = "List saved port forwards")]
    List(JsonArgs),
    #[command(about = "Show one saved port forward")]
    Show(ForwardShowArgs),
    #[command(about = "Create a saved port forward")]
    Create(ForwardCreateArgs),
    #[command(about = "Edit a saved port forward")]
    Edit(ForwardEditArgs),
    #[command(about = "Delete a saved port forward")]
    Delete(ForwardDeleteArgs),
    #[command(about = "Validate saved port forwards")]
    Validate(JsonArgs),
    #[command(about = "Export saved port forwards as a sync snapshot")]
    Export(JsonArgs),
    #[command(about = "Import a saved port forwards sync snapshot")]
    Import(ForwardsImportArgs),
}

#[derive(Debug, Args)]
pub struct ForwardShowArgs {
    #[arg(help = "Forward query: id, description, bind port, or target host")]
    pub query: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ForwardCreateArgs {
    #[arg(long = "type", value_enum, default_value_t = ForwardTypeArg::Local, help = "Forward type")]
    pub forward_type: ForwardTypeArg,
    #[arg(long, default_value = "127.0.0.1", help = "Bind address")]
    pub bind_address: String,
    #[arg(long, help = "Bind port")]
    pub bind_port: u16,
    #[arg(long, help = "Target host; required for local/remote forwards")]
    pub target_host: Option<String>,
    #[arg(long, help = "Target port; required for local/remote forwards")]
    pub target_port: Option<u16>,
    #[arg(long, help = "Saved connection id/name to own this forward")]
    pub connection: Option<String>,
    #[arg(long, help = "Forward session id for runtime association")]
    pub session_id: Option<String>,
    #[arg(long, help = "Forward description")]
    pub description: Option<String>,
    #[arg(long, help = "Auto-start this forward when the owner connection opens")]
    pub auto_start: bool,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ForwardEditArgs {
    #[arg(help = "Forward query: id, description, bind port, or target host")]
    pub query: String,
    #[arg(long = "type", value_enum, help = "Forward type")]
    pub forward_type: Option<ForwardTypeArg>,
    #[arg(long, help = "Bind address")]
    pub bind_address: Option<String>,
    #[arg(long, help = "Bind port")]
    pub bind_port: Option<u16>,
    #[arg(long, help = "Target host")]
    pub target_host: Option<String>,
    #[arg(long, help = "Target port")]
    pub target_port: Option<u16>,
    #[arg(long, help = "Forward description")]
    pub description: Option<String>,
    #[arg(long, help = "Enable or disable auto-start")]
    pub auto_start: Option<bool>,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ForwardDeleteArgs {
    #[arg(help = "Forward query: id, description, bind port, or target host")]
    pub query: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct ForwardsImportArgs {
    #[arg(help = "Path to a saved-forwards sync snapshot JSON file")]
    pub path: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ForwardTypeArg {
    Local,
    Remote,
    Dynamic,
}
