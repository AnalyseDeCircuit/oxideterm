// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::Args;

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long, help = "Treat warnings as failures")]
    pub strict: bool,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    #[arg(
        long,
        value_name = "PATH",
        help = "Write a redacted report bundle to PATH"
    )]
    pub bundle: Option<String>,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}
