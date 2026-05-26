// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::Args;

#[derive(Clone, Debug, Args)]
pub struct WriteArgs {
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(long, conflicts_with = "backup_before_write")]
    pub no_backup: bool,
    #[arg(long)]
    pub backup_before_write: bool,
    #[arg(long)]
    pub json: bool,
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
