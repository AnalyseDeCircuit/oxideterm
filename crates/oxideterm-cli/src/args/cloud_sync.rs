// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};

use super::{CliOutputFormat, JsonArgs, WriteArgs};

#[derive(Debug, Args)]
#[command(
    long_about = "Inspect, configure, and operate OxideTerm cloud sync. Write operations default to dry-run unless confirmed with --yes, and secret commands print only hints/status."
)]
#[command(
    after_help = "Examples:\n  oxideterm cloud-sync status --json\n  oxideterm cloud-sync configure --backend webdav --endpoint https://example.invalid/sync --dry-run\n  oxideterm cloud-sync diff --dirty-only --format table\n  oxideterm cloud-sync push --dry-run --json\n  oxideterm cloud-sync apply --from remote --strategy merge --yes\n  oxideterm cloud-sync secrets set token --stdin"
)]
pub struct CloudSyncCommand {
    #[command(subcommand)]
    pub action: CloudSyncAction,
}

#[derive(Debug, Subcommand)]
pub enum CloudSyncAction {
    #[command(about = "Show cloud-sync status")]
    Status(JsonArgs),
    #[command(about = "Update cloud-sync configuration")]
    Configure(CloudSyncConfigureArgs),
    #[command(about = "Preview local and remote sync state")]
    Preview(JsonArgs),
    #[command(about = "Show differences between local and remote sync state")]
    Diff(CloudSyncDiffArgs),
    #[command(about = "Push local state to the configured remote")]
    Push(CloudSyncWriteArgs),
    #[command(about = "Pull remote state into local files")]
    Pull(CloudSyncPullArgs),
    #[command(about = "Apply either local or remote state through the sync engine")]
    Apply(CloudSyncApplyArgs),
    #[command(about = "Resolve a cloud-sync conflict")]
    Resolve(CloudSyncResolveArgs),
    #[command(about = "Inspect cloud-sync persisted state")]
    State(CloudSyncStateCommand),
    #[command(about = "List cloud-sync operation history")]
    History(CloudSyncHistoryArgs),
    #[command(about = "List cloud-sync rollback backups")]
    Backups(JsonArgs),
    #[command(about = "Inspect or update cloud-sync secrets")]
    Secrets(CloudSyncSecretsCommand),
    #[command(about = "Configure cloud-sync backends with backend-specific validation")]
    Backend(CloudSyncBackendCommand),
}

#[derive(Debug, Args)]
pub struct CloudSyncWriteArgs {
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct CloudSyncConfigureArgs {
    #[arg(
        long,
        value_enum,
        help = "Backend type: webdav, http-json, dropbox, github-gist, s3, or git"
    )]
    pub backend: Option<CloudSyncBackendArg>,
    #[arg(long, value_enum, help = "Authentication mode for the backend")]
    pub auth_mode: Option<CloudSyncAuthModeArg>,
    #[arg(long, help = "Backend endpoint URL")]
    pub endpoint: Option<String>,
    #[arg(long, help = "Remote namespace/prefix")]
    pub namespace: Option<String>,
    #[arg(long, help = "S3 bucket name")]
    pub s3_bucket: Option<String>,
    #[arg(long, help = "S3 region")]
    pub s3_region: Option<String>,
    #[arg(long, help = "Git repository URL")]
    pub git_repository: Option<String>,
    #[arg(long, help = "Git branch")]
    pub git_branch: Option<String>,
    #[arg(long, help = "GitHub OAuth client ID for Gist device login")]
    pub github_oauth_client_id: Option<String>,
    #[arg(long, help = "Enable or disable automatic upload")]
    pub auto_upload_enabled: Option<bool>,
    #[arg(long, help = "Automatic upload interval in minutes")]
    pub auto_upload_interval_mins: Option<f64>,
    #[arg(long, value_enum, help = "Default conflict strategy")]
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
    GithubGist,
    S3,
    Git,
}

#[derive(Debug, Args)]
pub struct CloudSyncBackendCommand {
    #[command(subcommand)]
    pub action: CloudSyncBackendAction,
}

#[derive(Debug, Subcommand)]
pub enum CloudSyncBackendAction {
    Webdav(CloudSyncBackendConfigureCommand),
    GithubGist(CloudSyncBackendConfigureCommand),
    S3(CloudSyncBackendConfigureCommand),
    Git(CloudSyncBackendConfigureCommand),
}

#[derive(Debug, Args)]
pub struct CloudSyncBackendConfigureCommand {
    #[command(subcommand)]
    pub action: CloudSyncBackendConfigureAction,
}

#[derive(Debug, Subcommand)]
pub enum CloudSyncBackendConfigureAction {
    Configure(CloudSyncConfigureArgs),
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
    #[arg(long, value_enum, help = "Conflict strategy for local changes")]
    pub strategy: Option<CloudSyncConflictStrategy>,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct CloudSyncApplyArgs {
    #[arg(long, value_enum, help = "State source to apply: local or remote")]
    pub from: CloudSyncApplySource,
    #[arg(long, value_enum, help = "Conflict strategy for applying changes")]
    pub strategy: Option<CloudSyncConflictStrategy>,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct CloudSyncResolveArgs {
    #[arg(
        long,
        value_enum,
        help = "Resolution strategy: local-wins or remote-wins"
    )]
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
    #[arg(long, help = "Show only failed operations")]
    pub failed_only: bool,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
    #[arg(long, value_enum, help = "Output format: text, table, or json")]
    pub format: Option<CliOutputFormat>,
}

#[derive(Debug, Args)]
pub struct CloudSyncSecretsCommand {
    #[command(subcommand)]
    pub action: CloudSyncSecretsAction,
}

#[derive(Debug, Subcommand)]
pub enum CloudSyncSecretsAction {
    #[command(about = "Show configured secret hints without secret values")]
    Status(JsonArgs),
    #[command(about = "Set one cloud-sync secret from stdin or an environment variable")]
    Set(CloudSyncSecretSetArgs),
    #[command(about = "Clear one cloud-sync secret")]
    Clear(CloudSyncSecretKeyArgs),
    #[command(about = "Import cloud-sync secrets from a JSON file")]
    Import(CloudSyncSecretsImportArgs),
}

#[derive(Debug, Args)]
pub struct CloudSyncSecretSetArgs {
    #[arg(help = "Secret key to set")]
    pub key: String,
    #[arg(
        long,
        conflicts_with = "env",
        help = "Read the secret value from stdin"
    )]
    pub stdin: bool,
    #[arg(
        long = "env",
        value_name = "VAR",
        conflicts_with = "stdin",
        help = "Read the secret value from environment variable VAR"
    )]
    pub env: Option<String>,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncSecretKeyArgs {
    #[arg(help = "Secret key to clear")]
    pub key: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncSecretsImportArgs {
    #[arg(help = "Path to a cloud-sync secrets import JSON file")]
    pub path: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncStateCommand {
    #[command(subcommand)]
    pub action: CloudSyncStateAction,
}

#[derive(Debug, Subcommand)]
pub enum CloudSyncStateAction {
    #[command(about = "Print cloud-sync persisted state")]
    Show(JsonArgs),
    #[command(about = "Read a value from cloud-sync state by JSON path")]
    Get(CloudSyncStateGetArgs),
}

#[derive(Debug, Args)]
pub struct CloudSyncStateGetArgs {
    #[arg(help = "Cloud-sync state JSON path")]
    pub key: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncDiffArgs {
    #[arg(long, help = "Show only dirty sections")]
    pub dirty_only: bool,
    #[arg(long, value_enum, help = "Limit diff to one category")]
    pub category: Option<CloudSyncDiffCategory>,
    #[arg(long, value_enum, help = "Output format: table or json")]
    pub format: Option<CloudSyncDiffFormat>,
    #[arg(long, help = "Print machine-readable JSON output")]
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
