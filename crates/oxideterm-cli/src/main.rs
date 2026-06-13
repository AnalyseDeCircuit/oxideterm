// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod args;
mod backup;
mod batch;
mod cloud_sync;
mod cloud_sync_preview;
mod cloud_sync_secrets;
mod cloud_sync_state;
mod cloud_sync_write;
mod completion;
mod connections;
mod connections_validate;
mod diagnose;
mod doctor;
mod error;
mod errors;
mod forwards;
mod json_query;
mod output;
mod oxide;
mod paths;
mod plugins;
mod portable;
mod quick_commands;
mod report;
mod secrets;
mod settings;
mod ssh;
mod write_guard;

use clap::Parser;

use crate::{
    args::{Cli, Command},
    error::CliResult,
    output::OutputFormat,
};

fn main() {
    let mut cli = Cli::parse();
    cli.normalize_output_format();
    let result = run(cli);
    match result {
        Ok(0) => {}
        Ok(exit_code) => std::process::exit(exit_code),
        Err(error) => {
            let format = if error.json {
                OutputFormat::Json
            } else {
                OutputFormat::Text
            };
            output::write_error(format, &error);
            std::process::exit(error.exit_code());
        }
    }
}

fn run(cli: Cli) -> CliResult<i32> {
    // Keep dispatch thin: command modules own domain-specific loading and output mapping.
    paths::set_cli_path_context(cli.config_dir, cli.profile);
    match cli.command {
        Command::Settings(command) => settings::run(command),
        Command::Connections(command) => connections::run(command),
        Command::Ssh(args) => ssh::run(args),
        Command::Forwards(command) => forwards::run(command),
        Command::QuickCommands(command) => quick_commands::run(command),
        Command::Plugins(command) => plugins::run(command),
        Command::Portable(command) => portable::run(command),
        Command::Secrets(command) => secrets::run(command),
        Command::Oxide(command) => oxide::run(command),
        Command::CloudSync(command) => {
            cloud_sync::run(command)?;
            Ok(0)
        }
        Command::Paths(args) => {
            diagnose::show_paths(args)?;
            Ok(0)
        }
        Command::Diagnose(args) => {
            diagnose::diagnose(args)?;
            Ok(0)
        }
        Command::Doctor(args) => doctor::run(args),
        Command::Backup(command) => {
            backup::run(command)?;
            Ok(0)
        }
        Command::Batch(command) => batch::run(command),
        Command::Report(args) => report::run(args),
        Command::Completion(args) => completion::run(args).map(|_| 0),
        Command::Errors(args) => errors::run(args),
    }
}
