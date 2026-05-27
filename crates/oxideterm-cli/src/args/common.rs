// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, ValueEnum};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CliOutputFormat {
    Text,
    Table,
    Json,
}

#[derive(Clone, Debug, Args)]
pub struct WriteArgs {
    #[arg(long, help = "Preview planned changes without writing files")]
    pub dry_run: bool,
    #[arg(long, help = "Confirm a write operation; required for real writes")]
    pub yes: bool,
    #[arg(
        long,
        conflicts_with = "backup_before_write",
        help = "Skip automatic backup creation before writing"
    )]
    pub no_backup: bool,
    #[arg(long, help = "Force backup creation before writing")]
    pub backup_before_write: bool,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
    #[arg(long, value_enum, help = "Output format: text, table, or json")]
    pub format: Option<CliOutputFormat>,
}

#[derive(Debug, Args)]
pub struct JsonArgs {
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
    #[arg(long, value_enum, help = "Output format: text, table, or json")]
    pub format: Option<CliOutputFormat>,
}

#[derive(Debug, Args)]
pub struct OutputArgs {
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
    #[arg(long, value_enum, help = "Output format: text, table, or json")]
    pub format: Option<CliOutputFormat>,
}
