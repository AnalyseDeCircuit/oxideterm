// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand};

use super::{JsonArgs, WriteArgs};

#[derive(Debug, Args)]
#[command(
    long_about = "Inspect and manage plugin state and plugin settings for headless workflows."
)]
pub struct PluginsCommand {
    #[command(subcommand)]
    pub action: PluginsAction,
}

#[derive(Debug, Subcommand)]
pub enum PluginsAction {
    #[command(about = "List installed/enabled plugin state when available")]
    List(JsonArgs),
    #[command(about = "Plugin settings commands")]
    Settings(PluginSettingsCommand),
    #[command(about = "Enable a plugin in the headless plugin-state file")]
    Enable(PluginStateWriteArgs),
    #[command(about = "Disable a plugin in the headless plugin-state file")]
    Disable(PluginStateWriteArgs),
}

#[derive(Debug, Args)]
pub struct PluginStateWriteArgs {
    #[arg(help = "Plugin id")]
    pub plugin_id: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct PluginSettingsCommand {
    #[command(subcommand)]
    pub action: PluginSettingsAction,
}

#[derive(Debug, Subcommand)]
pub enum PluginSettingsAction {
    #[command(about = "List plugin settings keys")]
    List(JsonArgs),
    #[command(about = "Get one plugin setting")]
    Get(PluginSettingKeyArgs),
    #[command(about = "Set one plugin setting serialized value")]
    Set(PluginSettingSetArgs),
    #[command(about = "Unset one plugin setting")]
    Unset(PluginSettingUnsetArgs),
    #[command(about = "Export plugin settings")]
    Export(JsonArgs),
    #[command(about = "Import plugin settings")]
    Import(PluginSettingsImportArgs),
}

#[derive(Debug, Args)]
pub struct PluginSettingKeyArgs {
    #[arg(help = "Plugin setting storage key")]
    pub key: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PluginSettingSetArgs {
    #[arg(help = "Plugin setting storage key")]
    pub key: String,
    #[arg(long = "value-json", help = "Serialized JSON value to store")]
    pub value_json: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct PluginSettingUnsetArgs {
    #[arg(help = "Plugin setting storage key")]
    pub key: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct PluginSettingsImportArgs {
    #[arg(help = "Path to plugin-settings JSON")]
    pub path: String,
    #[command(flatten)]
    pub write: WriteArgs,
}
