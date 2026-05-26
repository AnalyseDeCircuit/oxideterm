// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod args;
mod backup;
mod cloud_sync;
mod cloud_sync_preview;
mod cloud_sync_state;
mod connections;
mod connections_validate;
mod diagnose;
mod doctor;
mod error;
mod json_query;
mod output;
mod paths;
mod settings;

use clap::Parser;

use crate::{
    args::{Cli, Command},
    error::CliResult,
    output::OutputFormat,
};

fn main() {
    let cli = Cli::parse();
    let result = run(cli);
    if let Err(error) = result {
        let format = if error.json {
            OutputFormat::Json
        } else {
            OutputFormat::Text
        };
        output::write_error(format, &error);
        std::process::exit(error.exit_code());
    }
}

fn run(cli: Cli) -> CliResult<()> {
    // Keep dispatch thin: command modules own domain-specific loading and output mapping.
    match cli.command {
        Command::Settings(command) => settings::run(command),
        Command::Connections(command) => connections::run(command),
        Command::CloudSync(command) => cloud_sync::run(command),
        Command::Paths(args) => diagnose::show_paths(args),
        Command::Diagnose(args) => diagnose::diagnose(args),
        Command::Doctor(args) => doctor::run(args),
        Command::Backup(command) => backup::run(command),
    }
}
