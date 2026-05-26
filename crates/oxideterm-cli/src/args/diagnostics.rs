// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::Args;

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    pub strict: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    #[arg(long)]
    pub bundle: Option<String>,
    #[arg(long)]
    pub json: bool,
}
