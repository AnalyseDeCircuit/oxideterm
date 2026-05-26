// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Parser, Subcommand};

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
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Settings(SettingsCommand),
    Connections(ConnectionsCommand),
    Oxide(OxideCommand),
    #[command(name = "cloud-sync")]
    CloudSync(CloudSyncCommand),
    Paths(OutputArgs),
    Diagnose(OutputArgs),
    Doctor(DoctorArgs),
    Backup(BackupCommand),
    Report(ReportArgs),
}
