// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};

use super::WriteArgs;

#[derive(Debug, Args)]
pub struct OxideCommand {
    #[command(subcommand)]
    pub action: OxideAction,
}

#[derive(Debug, Subcommand)]
pub enum OxideAction {
    Validate(OxidePathArgs),
    #[command(name = "preview-import")]
    PreviewImport(OxidePreviewImportArgs),
    Import(OxideImportArgs),
    Export(OxideExportArgs),
}

#[derive(Debug, Args)]
pub struct OxidePathArgs {
    pub path: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct OxidePreviewImportArgs {
    pub path: String,
    #[arg(long, value_enum, default_value_t = OxideImportStrategy::Rename)]
    pub strategy: OxideImportStrategy,
    #[command(flatten)]
    pub password: OxidePasswordArgs,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct OxideImportArgs {
    pub path: String,
    #[arg(long, value_enum, default_value_t = OxideImportStrategy::Rename)]
    pub strategy: OxideImportStrategy,
    #[arg(long = "name")]
    pub selected_names: Vec<String>,
    #[arg(long)]
    pub no_forwards: bool,
    #[arg(long = "forward")]
    pub forward_ids: Vec<String>,
    #[arg(long)]
    pub no_app_settings: bool,
    #[arg(long = "section")]
    pub sections: Vec<String>,
    #[arg(long)]
    pub no_quick_commands: bool,
    #[arg(long)]
    pub no_plugin_settings: bool,
    #[arg(long = "plugin")]
    pub plugin_ids: Vec<String>,
    #[arg(long)]
    pub import_portable_secrets: bool,
    #[command(flatten)]
    pub password: OxidePasswordArgs,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct OxideExportArgs {
    pub path: String,
    #[arg(long = "connection")]
    pub connection_queries: Vec<String>,
    #[arg(long = "forward")]
    pub forward_queries: Vec<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub embed_keys: bool,
    #[arg(long)]
    pub no_app_settings: bool,
    #[arg(long)]
    pub no_forwards: bool,
    #[arg(long)]
    pub no_quick_commands: bool,
    #[arg(long)]
    pub no_plugin_settings: bool,
    #[arg(long)]
    pub include_portable_secrets: bool,
    #[arg(long)]
    pub include_local_terminal_env_vars: bool,
    #[arg(long)]
    pub overwrite: bool,
    #[command(flatten)]
    pub password: OxidePasswordArgs,
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum OxideImportStrategy {
    Skip,
    Rename,
    Replace,
    Merge,
}

#[derive(Clone, Debug, Args)]
pub struct OxidePasswordArgs {
    #[arg(long, conflicts_with = "password_env")]
    pub password_stdin: bool,
    #[arg(
        long = "password-env",
        value_name = "VAR",
        conflicts_with = "password_stdin"
    )]
    pub password_env: Option<String>,
}
