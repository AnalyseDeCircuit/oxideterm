// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand};

use super::{JsonArgs, OutputArgs, WriteArgs};

#[derive(Debug, Args)]
#[command(
    long_about = "Read, validate, import, export, and edit OxideTerm settings without launching the GPUI app. Write commands default to dry-run unless confirmed with --yes."
)]
#[command(
    after_help = "Examples:\n  oxideterm settings validate --strict\n  oxideterm settings get ai.providers --json\n  oxideterm settings set terminal.fontSize 14 --dry-run\n  oxideterm settings import ./settings-snapshot.json --section appearance --yes\n  oxideterm settings export --section general --json"
)]
pub struct SettingsCommand {
    #[command(subcommand)]
    pub action: SettingsAction,
}

#[derive(Debug, Subcommand)]
pub enum SettingsAction {
    #[command(about = "Print the settings file path")]
    Path(OutputArgs),
    #[command(about = "List exportable settings sections")]
    Sections(JsonArgs),
    #[command(about = "Validate settings parsing, sanitization, and section coverage")]
    Validate(SettingsValidateArgs),
    #[command(about = "Print sanitized settings")]
    Show(JsonArgs),
    #[command(about = "Read a settings value by JSON path")]
    Get(SettingsGetArgs),
    #[command(about = "Set an existing settings value by JSON path")]
    Set(SettingsSetArgs),
    #[command(about = "Unset an existing settings value by JSON path")]
    Unset(SettingsUnsetArgs),
    #[command(about = "Apply a complete settings snapshot")]
    Apply(SettingsApplyArgs),
    #[command(about = "Import selected sections from a settings snapshot")]
    Import(SettingsImportArgs),
    #[command(about = "Export settings as an .oxide-compatible snapshot")]
    Export(SettingsExportArgs),
    #[command(about = "Diff current settings against a settings snapshot")]
    Diff(SettingsDiffArgs),
}

#[derive(Debug, Args)]
pub struct SettingsGetArgs {
    #[arg(help = "Settings JSON path, for example ai.providers")]
    pub key: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SettingsSetArgs {
    #[arg(help = "Existing settings JSON path to update")]
    pub key: String,
    #[arg(help = "JSON value to write, for example true or '{\"enabled\":true}'")]
    pub value: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct SettingsUnsetArgs {
    #[arg(help = "Existing settings JSON path to remove")]
    pub key: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct SettingsApplyArgs {
    #[arg(help = "Path to a settings snapshot JSON file")]
    pub path: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct SettingsImportArgs {
    #[arg(help = "Path to a settings snapshot JSON file")]
    pub path: String,
    #[arg(
        long = "section",
        help = "Section id to import; repeat to select multiple sections"
    )]
    pub sections: Vec<String>,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct SettingsExportArgs {
    #[arg(
        long = "section",
        help = "Section id to export; repeat to select multiple sections"
    )]
    pub sections: Vec<String>,
    #[arg(
        long,
        help = "Include local terminal environment variables in the export"
    )]
    pub include_local_terminal_env_vars: bool,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SettingsDiffArgs {
    #[arg(help = "Path to a settings snapshot JSON file")]
    pub path: String,
    #[arg(
        long = "section",
        help = "Section id to diff; repeat to select multiple sections"
    )]
    pub sections: Vec<String>,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SettingsValidateArgs {
    #[arg(long, help = "Treat validation warnings as failures")]
    pub strict: bool,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}
