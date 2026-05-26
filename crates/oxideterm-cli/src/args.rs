// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Parser, Subcommand, ValueEnum};

// CLI parsing is intentionally UI-free so the same command surface can later
// feed a GUI IPC client without linking GPUI into this crate.
#[derive(Debug, Parser)]
#[command(name = "oxideterm")]
#[command(about = "OxideTerm headless management CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Settings(SettingsCommand),
    Connections(ConnectionsCommand),
    #[command(name = "cloud-sync")]
    CloudSync(CloudSyncCommand),
    Paths(OutputArgs),
    Diagnose(OutputArgs),
    Doctor(OutputArgs),
    Backup(BackupCommand),
}

#[derive(Debug, Args)]
pub struct SettingsCommand {
    #[command(subcommand)]
    pub action: SettingsAction,
}

#[derive(Debug, Subcommand)]
pub enum SettingsAction {
    Path(OutputArgs),
    Sections(JsonArgs),
    Show(JsonArgs),
    Get(SettingsGetArgs),
    Export(SettingsExportArgs),
}

#[derive(Debug, Args)]
pub struct SettingsGetArgs {
    pub key: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SettingsExportArgs {
    #[arg(long = "section")]
    pub sections: Vec<String>,
    #[arg(long)]
    pub include_local_terminal_env_vars: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ConnectionsCommand {
    #[command(subcommand)]
    pub action: ConnectionsAction,
}

#[derive(Debug, Subcommand)]
pub enum ConnectionsAction {
    List(JsonArgs),
    Show(ConnectionShowArgs),
    Groups(JsonArgs),
    Search(ConnectionSearchArgs),
    Export(JsonArgs),
    Validate(JsonArgs),
}

#[derive(Debug, Args)]
pub struct ConnectionShowArgs {
    pub query: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ConnectionSearchArgs {
    pub query: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncCommand {
    #[command(subcommand)]
    pub action: CloudSyncAction,
}

#[derive(Debug, Subcommand)]
pub enum CloudSyncAction {
    Status(JsonArgs),
    Preview(JsonArgs),
    Diff(CloudSyncDiffArgs),
    State(CloudSyncStateCommand),
    History(JsonArgs),
    Backups(JsonArgs),
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
pub struct BackupCommand {
    #[command(subcommand)]
    pub action: BackupAction,
}

#[derive(Debug, Subcommand)]
pub enum BackupAction {
    Preview(JsonArgs),
    Create(JsonArgs),
    List(JsonArgs),
    Inspect(BackupInspectArgs),
    Verify(BackupInspectArgs),
}

#[derive(Debug, Args)]
pub struct BackupInspectArgs {
    pub query: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CloudSyncDiffArgs {
    #[arg(long)]
    pub dirty_only: bool,
    #[arg(long, value_enum)]
    pub category: Option<CloudSyncDiffCategory>,
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

#[derive(Debug, Args)]
pub struct JsonArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct OutputArgs {
    #[arg(long)]
    pub json: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_connections_show_json() {
        let cli = Cli::parse_from(["oxideterm", "connections", "show", "prod", "--json"]);
        match cli.command {
            Command::Connections(command) => match command.action {
                ConnectionsAction::Show(args) => {
                    assert_eq!(args.query, "prod");
                    assert!(args.json);
                }
                _ => panic!("expected show command"),
            },
            _ => panic!("expected connections command"),
        }
    }

    #[test]
    fn parses_cloud_sync_status() {
        let cli = Cli::parse_from(["oxideterm", "cloud-sync", "status", "--json"]);
        match cli.command {
            Command::CloudSync(command) => match command.action {
                CloudSyncAction::Status(args) => assert!(args.json),
                _ => panic!("expected status command"),
            },
            _ => panic!("expected cloud-sync command"),
        }
    }

    #[test]
    fn parses_cloud_sync_diff() {
        let cli = Cli::parse_from([
            "oxideterm",
            "cloud-sync",
            "diff",
            "--dirty-only",
            "--category",
            "app-settings",
            "--json",
        ]);
        match cli.command {
            Command::CloudSync(command) => match command.action {
                CloudSyncAction::Diff(args) => {
                    assert!(args.dirty_only);
                    assert_eq!(args.category, Some(CloudSyncDiffCategory::AppSettings));
                    assert!(args.json);
                }
                _ => panic!("expected diff command"),
            },
            _ => panic!("expected cloud-sync command"),
        }
    }

    #[test]
    fn parses_cloud_sync_state_get() {
        let cli = Cli::parse_from([
            "oxideterm",
            "cloud-sync",
            "state",
            "get",
            "settings.namespace",
            "--json",
        ]);
        match cli.command {
            Command::CloudSync(command) => match command.action {
                CloudSyncAction::State(command) => match command.action {
                    CloudSyncStateAction::Get(args) => {
                        assert_eq!(args.key, "settings.namespace");
                        assert!(args.json);
                    }
                    _ => panic!("expected get command"),
                },
                _ => panic!("expected state command"),
            },
            _ => panic!("expected cloud-sync command"),
        }
    }

    #[test]
    fn parses_connections_search() {
        let cli = Cli::parse_from(["oxideterm", "connections", "search", "prod", "--json"]);
        match cli.command {
            Command::Connections(command) => match command.action {
                ConnectionsAction::Search(args) => {
                    assert_eq!(args.query, "prod");
                    assert!(args.json);
                }
                _ => panic!("expected search command"),
            },
            _ => panic!("expected connections command"),
        }
    }

    #[test]
    fn parses_settings_export_sections() {
        let cli = Cli::parse_from([
            "oxideterm",
            "settings",
            "export",
            "--section",
            "general",
            "--include-local-terminal-env-vars",
            "--json",
        ]);
        match cli.command {
            Command::Settings(command) => match command.action {
                SettingsAction::Export(args) => {
                    assert_eq!(args.sections, ["general"]);
                    assert!(args.include_local_terminal_env_vars);
                    assert!(args.json);
                }
                _ => panic!("expected export command"),
            },
            _ => panic!("expected settings command"),
        }
    }

    #[test]
    fn parses_settings_sections() {
        let cli = Cli::parse_from(["oxideterm", "settings", "sections", "--json"]);
        match cli.command {
            Command::Settings(command) => match command.action {
                SettingsAction::Sections(args) => assert!(args.json),
                _ => panic!("expected sections command"),
            },
            _ => panic!("expected settings command"),
        }
    }

    #[test]
    fn parses_connections_validate() {
        let cli = Cli::parse_from(["oxideterm", "connections", "validate", "--json"]);
        match cli.command {
            Command::Connections(command) => match command.action {
                ConnectionsAction::Validate(args) => assert!(args.json),
                _ => panic!("expected validate command"),
            },
            _ => panic!("expected connections command"),
        }
    }

    #[test]
    fn parses_top_level_diagnostics() {
        let cli = Cli::parse_from(["oxideterm", "diagnose", "--json"]);
        match cli.command {
            Command::Diagnose(args) => assert!(args.json),
            _ => panic!("expected diagnose command"),
        }
    }

    #[test]
    fn parses_doctor() {
        let cli = Cli::parse_from(["oxideterm", "doctor", "--json"]);
        match cli.command {
            Command::Doctor(args) => assert!(args.json),
            _ => panic!("expected doctor command"),
        }
    }

    #[test]
    fn parses_backup_inspect() {
        let cli = Cli::parse_from(["oxideterm", "backup", "inspect", "backup.json", "--json"]);
        match cli.command {
            Command::Backup(command) => match command.action {
                BackupAction::Inspect(args) => {
                    assert_eq!(args.query, "backup.json");
                    assert!(args.json);
                }
                _ => panic!("expected inspect command"),
            },
            _ => panic!("expected backup command"),
        }
    }

    #[test]
    fn parses_backup_preview() {
        let cli = Cli::parse_from(["oxideterm", "backup", "preview", "--json"]);
        match cli.command {
            Command::Backup(command) => match command.action {
                BackupAction::Preview(args) => assert!(args.json),
                _ => panic!("expected preview command"),
            },
            _ => panic!("expected backup command"),
        }
    }

    #[test]
    fn parses_backup_verify() {
        let cli = Cli::parse_from(["oxideterm", "backup", "verify", "backup.json", "--json"]);
        match cli.command {
            Command::Backup(command) => match command.action {
                BackupAction::Verify(args) => {
                    assert_eq!(args.query, "backup.json");
                    assert!(args.json);
                }
                _ => panic!("expected verify command"),
            },
            _ => panic!("expected backup command"),
        }
    }
}
