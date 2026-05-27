// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};

use super::WriteArgs;

#[derive(Debug, Args)]
#[command(
    long_about = "Validate, preview, import, and export portable .oxide bundles. Passwords can be supplied through stdin or an environment variable so they do not appear in shell history."
)]
#[command(
    after_help = "Examples:\n  oxideterm oxide validate ./profile.oxide\n  oxideterm oxide preview-import ./profile.oxide --password-stdin --json\n  oxideterm oxide import ./profile.oxide --strategy merge --import-portable-secrets --yes\n  oxideterm oxide export ./profile.oxide --connection prod --include-portable-secrets --password-env OXIDE_PASSWORD\n  oxideterm oxide export ./profile.oxide --no-plugin-settings --overwrite"
)]
pub struct OxideCommand {
    #[command(subcommand)]
    pub action: OxideAction,
}

#[derive(Debug, Subcommand)]
pub enum OxideAction {
    #[command(about = "Validate a portable .oxide bundle")]
    Validate(OxidePathArgs),
    #[command(name = "preview-import")]
    #[command(about = "Preview importing a portable .oxide bundle")]
    PreviewImport(OxidePreviewImportArgs),
    #[command(about = "Diff a portable .oxide bundle against local state")]
    Diff(OxidePreviewImportArgs),
    #[command(about = "Import a portable .oxide bundle")]
    Import(OxideImportArgs),
    #[command(about = "Export portable .oxide bundle")]
    Export(OxideExportArgs),
}

#[derive(Debug, Args)]
pub struct OxidePathArgs {
    #[arg(help = "Path to a .oxide bundle")]
    pub path: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct OxidePreviewImportArgs {
    #[arg(help = "Path to a .oxide bundle")]
    pub path: String,
    #[arg(long, value_enum, default_value_t = OxideImportStrategy::Rename, help = "Conflict strategy for previewing import")]
    pub strategy: OxideImportStrategy,
    #[command(flatten)]
    pub password: OxidePasswordArgs,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct OxideImportArgs {
    #[arg(help = "Path to a .oxide bundle")]
    pub path: String,
    #[arg(long, value_enum, default_value_t = OxideImportStrategy::Rename, help = "Conflict strategy for existing records")]
    pub strategy: OxideImportStrategy,
    #[arg(
        long = "name",
        help = "Connection name to import; repeat to select multiple"
    )]
    pub selected_names: Vec<String>,
    #[arg(long, help = "Skip importing forwards")]
    pub no_forwards: bool,
    #[arg(
        long = "forward",
        help = "Forward id to import; repeat to select multiple"
    )]
    pub forward_ids: Vec<String>,
    #[arg(long, help = "Skip importing app settings")]
    pub no_app_settings: bool,
    #[arg(
        long = "section",
        help = "App settings section to import; repeat to select multiple"
    )]
    pub sections: Vec<String>,
    #[arg(long, help = "Skip importing quick commands")]
    pub no_quick_commands: bool,
    #[arg(long, help = "Skip importing plugin settings")]
    pub no_plugin_settings: bool,
    #[arg(
        long = "plugin",
        help = "Plugin id to import settings for; repeat to select multiple"
    )]
    pub plugin_ids: Vec<String>,
    #[arg(long, help = "Import portable secrets into the local secret store")]
    pub import_portable_secrets: bool,
    #[command(flatten)]
    pub password: OxidePasswordArgs,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct OxideExportArgs {
    #[arg(help = "Path to write the .oxide bundle")]
    pub path: String,
    #[arg(
        long = "connection",
        help = "Connection query to export; repeat to select multiple"
    )]
    pub connection_queries: Vec<String>,
    #[arg(
        long = "forward",
        help = "Forward query/id to export; repeat to select multiple"
    )]
    pub forward_queries: Vec<String>,
    #[arg(long, help = "Human-readable bundle description")]
    pub description: Option<String>,
    #[arg(long, help = "Embed private key files in the bundle")]
    pub embed_keys: bool,
    #[arg(long, help = "Skip app settings")]
    pub no_app_settings: bool,
    #[arg(long, help = "Skip forwards")]
    pub no_forwards: bool,
    #[arg(long, help = "Skip quick commands")]
    pub no_quick_commands: bool,
    #[arg(long, help = "Skip plugin settings")]
    pub no_plugin_settings: bool,
    #[arg(long, help = "Include portable secrets in encrypted form")]
    pub include_portable_secrets: bool,
    #[arg(long, help = "Include local terminal environment variables")]
    pub include_local_terminal_env_vars: bool,
    #[arg(long, help = "Overwrite an existing output file")]
    pub overwrite: bool,
    #[command(flatten)]
    pub password: OxidePasswordArgs,
    #[arg(long, help = "Print machine-readable JSON output")]
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
    #[arg(
        long,
        conflicts_with = "password_env",
        help = "Read .oxide password from stdin"
    )]
    pub password_stdin: bool,
    #[arg(
        long = "password-env",
        value_name = "VAR",
        conflicts_with = "password_stdin",
        help = "Read .oxide password from environment variable VAR"
    )]
    pub password_env: Option<String>,
}
