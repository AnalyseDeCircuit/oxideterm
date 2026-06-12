// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

mod backup;
mod batch;
mod cloud_sync;
mod common;
mod connections;
mod diagnostics;
mod forwards;
mod oxide;
mod plugins;
mod portable;
mod quick_commands;
mod secrets;
mod settings;

#[cfg(test)]
mod tests;

pub use backup::*;
pub use batch::*;
pub use cloud_sync::*;
pub use common::*;
pub use connections::*;
pub use diagnostics::*;
pub use forwards::*;
pub use oxide::*;
pub use plugins::*;
pub use portable::*;
pub use quick_commands::*;
pub use secrets::*;
pub use settings::*;

// Root CLI parsing stays UI-free. Domain-specific argument DTOs live in
// sibling modules so each command surface owns its own schema.
#[derive(Debug, Parser)]
#[command(name = "oxideterm")]
#[command(about = "OxideTerm headless management CLI")]
#[command(
    long_about = "OxideTerm headless management CLI for settings, saved connections, portable .oxide bundles, cloud sync, backups, and support diagnostics."
)]
#[command(
    after_help = "Examples:\n  oxideterm doctor --strict\n  oxideterm settings validate --strict --json\n  oxideterm connections search prod\n  oxideterm backup create --output ./oxideterm-backup.json --json\n  oxideterm oxide export ./profile.oxide --connection prod --password-stdin\n  oxideterm cloud-sync push --dry-run --json\n  oxideterm portable status --json\n  oxideterm completion zsh > ~/.zfunc/_oxideterm"
)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        env = "OXIDETERM_CONFIG_DIR",
        help = "Use PATH as the OxideTerm config directory"
    )]
    pub config_dir: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        value_name = "NAME",
        help = "Use a named profile under the config directory"
    )]
    pub profile: Option<String>,
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn normalize_output_format(&mut self) {
        normalize_command_output_format(&mut self.command);
    }
}

fn normalize_command_output_format(command: &mut Command) {
    match command {
        Command::Settings(command) => match &mut command.action {
            SettingsAction::Path(args) => normalize_output_args(args),
            SettingsAction::Sections(args) | SettingsAction::Show(args) => {
                normalize_json_args(args)
            }
            SettingsAction::Set(args) => normalize_write_args(&mut args.write),
            SettingsAction::Unset(args) => normalize_write_args(&mut args.write),
            SettingsAction::Apply(args) => normalize_write_args(&mut args.write),
            SettingsAction::Import(args) => normalize_write_args(&mut args.write),
            _ => {}
        },
        Command::Connections(command) => match &mut command.action {
            ConnectionsAction::List(args) | ConnectionsAction::Groups(args) => {
                normalize_json_args(args)
            }
            ConnectionsAction::Create(args) => normalize_write_args(&mut args.write),
            ConnectionsAction::Edit(args) => normalize_write_args(&mut args.write),
            ConnectionsAction::Delete(args) => normalize_write_args(&mut args.write),
            ConnectionsAction::Rename(args) => normalize_write_args(&mut args.write),
            ConnectionsAction::Import(args) | ConnectionsAction::ApplySnapshot(args) => {
                normalize_write_args(&mut args.write)
            }
            ConnectionsAction::Group(group) => match &mut group.action {
                ConnectionsGroupAction::Add(args) | ConnectionsGroupAction::Remove(args) => {
                    normalize_write_args(&mut args.write)
                }
                ConnectionsGroupAction::Rename(args) => normalize_write_args(&mut args.write),
            },
            _ => {}
        },
        Command::Forwards(command) => match &mut command.action {
            ForwardsAction::List(args)
            | ForwardsAction::Validate(args)
            | ForwardsAction::Export(args) => normalize_json_args(args),
            ForwardsAction::Create(args) => normalize_write_args(&mut args.write),
            ForwardsAction::Edit(args) => normalize_write_args(&mut args.write),
            ForwardsAction::Delete(args) => normalize_write_args(&mut args.write),
            ForwardsAction::Import(args) => normalize_write_args(&mut args.write),
            ForwardsAction::Show(_) => {}
        },
        Command::QuickCommands(command) => match &mut command.action {
            QuickCommandsAction::List(args) | QuickCommandsAction::Export(args) => {
                normalize_json_args(args)
            }
            QuickCommandsAction::Create(args) => normalize_write_args(&mut args.write),
            QuickCommandsAction::Edit(args) => normalize_write_args(&mut args.write),
            QuickCommandsAction::Delete(args) => normalize_write_args(&mut args.write),
            QuickCommandsAction::Import(args) => normalize_write_args(&mut args.write),
            QuickCommandsAction::Show(_) => {}
        },
        Command::Plugins(command) => match &mut command.action {
            PluginsAction::List(args) => normalize_json_args(args),
            PluginsAction::Enable(args) | PluginsAction::Disable(args) => {
                normalize_write_args(&mut args.write)
            }
            PluginsAction::Settings(settings) => match &mut settings.action {
                PluginSettingsAction::List(args) | PluginSettingsAction::Export(args) => {
                    normalize_json_args(args)
                }
                PluginSettingsAction::Set(args) => normalize_write_args(&mut args.write),
                PluginSettingsAction::Unset(args) => normalize_write_args(&mut args.write),
                PluginSettingsAction::Import(args) => normalize_write_args(&mut args.write),
                PluginSettingsAction::Get(_) => {}
            },
        },
        Command::Portable(_) => {}
        Command::Secrets(_) => {}
        Command::CloudSync(command) => match &mut command.action {
            CloudSyncAction::Status(args)
            | CloudSyncAction::Preview(args)
            | CloudSyncAction::Backups(args) => normalize_json_args(args),
            CloudSyncAction::Configure(args) => normalize_write_args(&mut args.write),
            CloudSyncAction::Push(args) => normalize_write_args(&mut args.write),
            CloudSyncAction::Pull(args) => normalize_write_args(&mut args.write),
            CloudSyncAction::Apply(args) => normalize_write_args(&mut args.write),
            CloudSyncAction::Resolve(args) => normalize_write_args(&mut args.write),
            CloudSyncAction::State(state) => match &mut state.action {
                CloudSyncStateAction::Show(args) => normalize_json_args(args),
                CloudSyncStateAction::Get(_) => {}
            },
            CloudSyncAction::History(args) => {
                if args.format == Some(CliOutputFormat::Json) {
                    args.json = true;
                }
            }
            CloudSyncAction::Secrets(secrets) => match &mut secrets.action {
                CloudSyncSecretsAction::Status(args) => normalize_json_args(args),
                _ => {}
            },
            CloudSyncAction::Backend(command) => match &mut command.action {
                CloudSyncBackendAction::Webdav(command)
                | CloudSyncBackendAction::OneDrive(command)
                | CloudSyncBackendAction::GithubGist(command)
                | CloudSyncBackendAction::S3(command)
                | CloudSyncBackendAction::Git(command) => match &mut command.action {
                    CloudSyncBackendConfigureAction::Configure(args) => {
                        normalize_write_args(&mut args.write)
                    }
                },
            },
            _ => {}
        },
        Command::Backup(command) => match &mut command.action {
            BackupAction::Preview(args) | BackupAction::List(args) => normalize_json_args(args),
            BackupAction::Restore(args) => normalize_write_args(&mut args.write),
            _ => {}
        },
        Command::Batch(command) => match &mut command.action {
            BatchAction::Apply(args) => normalize_write_args(&mut args.write),
        },
        Command::Paths(args) | Command::Diagnose(args) => normalize_output_args(args),
        Command::Doctor(args) => {
            if args.format == Some(CliOutputFormat::Json) {
                args.json = true;
            }
        }
        Command::Report(args) => {
            if args.format == Some(CliOutputFormat::Json) {
                args.json = true;
            }
        }
        _ => {}
    }
}

fn normalize_json_args(args: &mut JsonArgs) {
    if args.format == Some(CliOutputFormat::Json) {
        args.json = true;
    }
}

fn normalize_output_args(args: &mut OutputArgs) {
    if args.format == Some(CliOutputFormat::Json) {
        args.json = true;
    }
}

fn normalize_write_args(args: &mut WriteArgs) {
    if args.format == Some(CliOutputFormat::Json) {
        args.json = true;
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Read, validate, import, export, and edit OxideTerm settings")]
    Settings(SettingsCommand),
    #[command(about = "Inspect and manage saved SSH connections and groups")]
    Connections(ConnectionsCommand),
    #[command(about = "Inspect and manage saved SSH port forwards")]
    Forwards(ForwardsCommand),
    #[command(name = "quick-commands")]
    #[command(about = "Inspect and manage terminal Quick Commands")]
    QuickCommands(QuickCommandsCommand),
    #[command(about = "Inspect and manage plugins and plugin settings")]
    Plugins(PluginsCommand),
    #[command(about = "Inspect and unlock the portable runtime")]
    Portable(PortableCommand),
    #[command(about = "Inspect and manage keychain-backed secrets")]
    Secrets(SecretsCommand),
    #[command(about = "Validate, import, and export portable .oxide bundles")]
    Oxide(OxideCommand),
    #[command(name = "cloud-sync")]
    #[command(about = "Inspect, configure, and operate cloud sync")]
    CloudSync(CloudSyncCommand),
    #[command(about = "Print resolved OxideTerm config and data paths")]
    Paths(OutputArgs),
    #[command(about = "Print a read-only diagnostics snapshot")]
    Diagnose(OutputArgs),
    #[command(about = "Run health checks for settings, connections, and cloud sync")]
    Doctor(DoctorArgs),
    #[command(about = "Create, inspect, verify, and restore local backups")]
    Backup(BackupCommand),
    #[command(about = "Apply multi-step CLI plans")]
    Batch(BatchCommand),
    #[command(about = "Generate a redacted support report")]
    Report(ReportArgs),
    #[command(about = "Generate shell completion scripts")]
    Completion(CompletionArgs),
    #[command(about = "List machine-readable CLI error codes")]
    Errors(ErrorCatalogArgs),
}

#[derive(Debug, Args)]
#[command(long_about = "List stable CLI error codes for scripts and CI integrations.")]
#[command(after_help = "Examples:\n  oxideterm errors --json\n  oxideterm errors")]
pub struct ErrorCatalogArgs {
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
#[command(
    long_about = "Generate a shell completion script for OxideTerm. The script is printed to stdout so callers can redirect it to the location required by their shell."
)]
#[command(
    after_help = "Examples:\n  oxideterm completion zsh > ~/.zfunc/_oxideterm\n  oxideterm completion bash > ~/.local/share/bash-completion/completions/oxideterm\n  oxideterm completion fish > ~/.config/fish/completions/oxideterm.fish"
)]
pub struct CompletionArgs {
    #[command(subcommand)]
    pub action: Option<CompletionAction>,
    #[arg(value_enum, help = "Shell to generate completions for")]
    pub shell: Option<CompletionShell>,
}

#[derive(Debug, Subcommand)]
pub enum CompletionAction {
    #[command(about = "Print a shell completion script to stdout")]
    Generate(CompletionShellArgs),
    #[command(about = "Print the recommended completion install path")]
    Path(CompletionShellArgs),
    #[command(about = "Install a shell completion script at the recommended path")]
    Install(CompletionInstallArgs),
}

#[derive(Debug, Args)]
pub struct CompletionShellArgs {
    #[arg(value_enum, help = "Shell to generate completions for")]
    pub shell: CompletionShell,
}

#[derive(Debug, Args)]
pub struct CompletionInstallArgs {
    #[arg(value_enum, help = "Shell to install completions for")]
    pub shell: CompletionShell,
    #[arg(long, help = "Overwrite an existing completion file")]
    pub force: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    PowerShell,
    Zsh,
}
