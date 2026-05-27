// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Parser, Subcommand, ValueEnum};

mod backup;
mod cloud_sync;
mod common;
mod connections;
mod diagnostics;
mod oxide;
mod settings;

#[cfg(test)]
mod tests;

pub use backup::*;
pub use cloud_sync::*;
pub use common::*;
pub use connections::*;
pub use diagnostics::*;
pub use oxide::*;
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
    after_help = "Examples:\n  oxideterm doctor --strict\n  oxideterm settings validate --strict --json\n  oxideterm connections search prod\n  oxideterm backup create --output ./oxideterm-backup.json --json\n  oxideterm oxide export ./profile.oxide --connection prod --password-stdin\n  oxideterm cloud-sync push --dry-run --json\n  oxideterm completion zsh > ~/.zfunc/_oxideterm"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Read, validate, import, export, and edit OxideTerm settings")]
    Settings(SettingsCommand),
    #[command(about = "Inspect and manage saved SSH connections and groups")]
    Connections(ConnectionsCommand),
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
    #[command(about = "Generate a redacted support report")]
    Report(ReportArgs),
    #[command(about = "Generate shell completion scripts")]
    Completion(CompletionArgs),
}

#[derive(Debug, Args)]
#[command(
    long_about = "Generate a shell completion script for OxideTerm. The script is printed to stdout so callers can redirect it to the location required by their shell."
)]
#[command(
    after_help = "Examples:\n  oxideterm completion zsh > ~/.zfunc/_oxideterm\n  oxideterm completion bash > ~/.local/share/bash-completion/completions/oxideterm\n  oxideterm completion fish > ~/.config/fish/completions/oxideterm.fish"
)]
pub struct CompletionArgs {
    #[arg(value_enum, help = "Shell to generate completions for")]
    pub shell: CompletionShell,
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
