// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand};

use super::WriteArgs;

#[derive(Debug, Args)]
#[command(
    long_about = "Apply a multi-step OxideTerm CLI plan. The plan can combine settings import, connections snapshot import, and cloud-sync configuration in one dry-run or confirmed write."
)]
#[command(
    after_help = "Example plan:\n  {\n    \"settings\": { \"path\": \"settings.json\", \"sections\": [\"appearance\"] },\n    \"connections\": { \"path\": \"connections.json\", \"strategy\": \"merge\" },\n    \"cloudSync\": { \"configure\": { \"backend\": \"webdav\", \"endpoint\": \"https://example.invalid/sync\" } }\n  }\n\nExamples:\n  oxideterm batch apply ./plan.json --dry-run\n  oxideterm batch apply ./plan.json --yes --json"
)]
pub struct BatchCommand {
    #[command(subcommand)]
    pub action: BatchAction,
}

#[derive(Debug, Subcommand)]
pub enum BatchAction {
    #[command(about = "Apply a JSON batch plan")]
    Apply(BatchApplyArgs),
}

#[derive(Debug, Args)]
pub struct BatchApplyArgs {
    #[arg(help = "Path to a batch plan JSON file")]
    pub path: String,
    #[command(flatten)]
    pub write: WriteArgs,
}
