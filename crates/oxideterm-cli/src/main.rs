// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod args;
mod audit;
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
mod mcp;
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
mod uri;
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
    let audit_name = audit::command_name(&cli.command);
    let audit_service = audit_name.and_then(|_| {
        match oxideterm_audit::AuditService::start(
            paths::default_settings_path().with_file_name("audit.sqlite3"),
        ) {
            Ok(service) => Some(service),
            Err(_) => {
                eprintln!("Audit capture is unavailable.");
                None
            }
        }
    });
    let mut context = audit_service.as_ref().map(|service| {
        oxideterm_audit::AuditContext::new(service.client(), oxideterm_audit::AuditSource::Cli)
    });
    let operation = context.as_ref().zip(audit_name).map(|(context, name)| {
        context.operation(
            oxideterm_audit::AuditCategory::System,
            "cli_execute",
            Some(name),
        )
    });
    if let Some(context) = &mut context {
        context.parent_id = operation
            .as_ref()
            .and_then(|operation| operation.id().map(str::to_string));
    }
    let registration = context.map(oxideterm_audit::AuditContext::install);
    let dispatched = matches!(cli.command, Command::Ssh(_) | Command::Open(_));
    let result = (|| match cli.command {
        Command::Settings(command) => settings::run(command),
        Command::Connections(command) => connections::run(command),
        Command::Ssh(args) => ssh::run(args),
        Command::Open(args) => uri::run(args),
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
        Command::Mcp(command) => mcp::run(command),
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
    })();
    if let Some(operation) = operation {
        use oxideterm_audit::{AuditEvidence, AuditOutcome};
        let outcome = match &result {
            Ok(0) if dispatched => AuditOutcome::Sent,
            Ok(0) => AuditOutcome::Succeeded,
            _ => AuditOutcome::Failed,
        };
        operation.finish(
            outcome,
            if dispatched {
                AuditEvidence::Dispatch
            } else {
                AuditEvidence::ExitCode
            },
            result.as_ref().ok().copied(),
            None,
        );
    }
    drop(registration);
    let audit_client = audit_service.as_ref().map(|service| service.client());
    drop(audit_service);
    if audit_client.is_some_and(|client| {
        let health = client.health();
        health.error.is_some() || health.unrecorded > 0
    }) {
        eprintln!("Audit capture failed; some records may be missing.");
    }
    result
}
