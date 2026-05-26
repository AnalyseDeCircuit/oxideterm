// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand};

use super::{JsonArgs, OutputArgs, WriteArgs};

#[derive(Debug, Args)]
pub struct SettingsCommand {
    #[command(subcommand)]
    pub action: SettingsAction,
}

#[derive(Debug, Subcommand)]
pub enum SettingsAction {
    Path(OutputArgs),
    Sections(JsonArgs),
    Validate(SettingsValidateArgs),
    Show(JsonArgs),
    Get(SettingsGetArgs),
    Set(SettingsSetArgs),
    Unset(SettingsUnsetArgs),
    Apply(SettingsApplyArgs),
    Import(SettingsImportArgs),
    Export(SettingsExportArgs),
}

#[derive(Debug, Args)]
pub struct SettingsGetArgs {
    pub key: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SettingsSetArgs {
    pub key: String,
    pub value: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct SettingsUnsetArgs {
    pub key: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct SettingsApplyArgs {
    pub path: String,
    #[command(flatten)]
    pub write: WriteArgs,
}

#[derive(Debug, Args)]
pub struct SettingsImportArgs {
    pub path: String,
    #[arg(long = "section")]
    pub sections: Vec<String>,
    #[command(flatten)]
    pub write: WriteArgs,
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
pub struct SettingsValidateArgs {
    #[arg(long)]
    pub strict: bool,
    #[arg(long)]
    pub json: bool,
}
