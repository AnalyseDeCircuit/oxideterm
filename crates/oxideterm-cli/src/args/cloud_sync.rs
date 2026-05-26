// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};

use super::{JsonArgs, WriteArgs};

#[derive(Debug, Args)]
pub struct CloudSyncCommand {
    #[command(subcommand)]
    pub action: CloudSyncAction,
}

#[derive(Debug, Subcommand)]
pub enum CloudSyncAction {
    Status(JsonArgs),
    Configure(CloudSyncConfigureArgs),
    Preview(JsonArgs),
    Diff(CloudSyncDiffArgs),
    Push(CloudSyncWriteArgs),
    Pull(CloudSyncPullArgs),
    Apply(CloudSyncApplyArgs),
    Resolve(CloudSyncResolveArgs),
    State(CloudSyncStateCommand),
    History(CloudSyncHistoryArgs),
    Backups(JsonArgs),
    Secrets(CloudSyncSecretsCommand),
}

#[derive(Debug, Args)]
pub struct CloudSyncWriteArgs {
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct CloudSyncConfigureArgs {
    #[arg(long, value_enum)]
    pub backend: Option<CloudSyncBackendArg>,
    #[arg(long, value_enum)]
    pub auth_mode: Option<CloudSyncAuthModeArg>,
    #[arg(long)]
    pub endpoint: Option<String>,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long)]
    pub s3_bucket: Option<String>,
    #[arg(long)]
    pub s3_region: Option<String>,
    #[arg(long)]
    pub git_repository: Option<String>,
    #[arg(long)]
    pub git_branch: Option<String>,
    #[arg(long)]
    pub auto_upload_enabled: Option<bool>,
    #[arg(long)]
    pub auto_upload_interval_mins: Option<f64>,
    #[arg(long, value_enum)]
    pub default_conflict_strategy: Option<CloudSyncConflictStrategy>,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CloudSyncBackendArg {
    Webdav,
    HttpJson,
    Dropbox,
    S3,
    Git,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CloudSyncAuthModeArg {
    Bearer,
    Basic,
    None,
}

#[derive(Debug, Args)]
pub struct CloudSyncPullArgs {
    #[arg(long, value_enum)]
    pub strategy: Option<CloudSyncConflictStrategy>,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct CloudSyncApplyArgs {
    #[arg(long, value_enum)]
    pub from: CloudSyncApplySource,
    #[arg(long, value_enum)]
    pub strategy: Option<CloudSyncConflictStrategy>,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct CloudSyncResolveArgs {
    #[arg(long, value_enum)]
    pub strategy: CloudSyncResolveStrategy,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CloudSyncApplySource {
    Local,
    Remote,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CloudSyncConflictStrategy {
    Merge,
    Replace,
    Skip,
    Rename,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CloudSyncResolveStrategy {
    LocalWins,
    RemoteWins,
}

#[derive(Debug, Args)]
pub struct CloudSyncHistoryArgs {
    #[arg(long)]
    pub failed_only: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncSecretsCommand {
    #[command(subcommand)]
    pub action: CloudSyncSecretsAction,
}

#[derive(Debug, Subcommand)]
pub enum CloudSyncSecretsAction {
    Status(JsonArgs),
    Set(CloudSyncSecretSetArgs),
    Clear(CloudSyncSecretKeyArgs),
    Import(CloudSyncSecretsImportArgs),
}

#[derive(Debug, Args)]
pub struct CloudSyncSecretSetArgs {
    pub key: String,
    #[arg(long, conflicts_with = "env")]
    pub stdin: bool,
    #[arg(long = "env", value_name = "VAR", conflicts_with = "stdin")]
    pub env: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncSecretKeyArgs {
    pub key: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncSecretsImportArgs {
    pub path: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncStateCommand {
    #[command(subcommand)]
    pub action: CloudSyncStateAction,
}

#[derive(Debug, Subcommand)]
pub enum CloudSyncStateAction {
    Show(JsonArgs),
    Get(CloudSyncStateGetArgs),
}

#[derive(Debug, Args)]
pub struct CloudSyncStateGetArgs {
    pub key: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncDiffArgs {
    #[arg(long)]
    pub dirty_only: bool,
    #[arg(long, value_enum)]
    pub category: Option<CloudSyncDiffCategory>,
    #[arg(long, value_enum)]
    pub format: Option<CloudSyncDiffFormat>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CloudSyncDiffCategory {
    Connections,
    Forwards,
    AppSettings,
    PluginSettings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CloudSyncDiffFormat {
    Table,
    Json,
}
