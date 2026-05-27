// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::io::{self, Read};

use oxideterm_portable_runtime::{
    PortableStatusSnapshot, acquire_portable_instance_lock, initialize_portable_runtime,
    portable_status_snapshot,
};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::{
    args::{
        PortableAction, PortableChangePasswordArgs, PortableCommand, PortablePasswordArgs,
        PortableResetArgs, PortableStatusArgs,
    },
    error::{CliError, CliResult},
    output,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PortableCommandResponse {
    action: &'static str,
    status: PortableStatusSnapshot,
}

pub fn run(command: PortableCommand) -> CliResult<i32> {
    match command.action {
        PortableAction::Status(args) => {
            status(args)?;
            Ok(0)
        }
        PortableAction::Setup(args) => {
            setup(args)?;
            Ok(0)
        }
        PortableAction::Unlock(args) => {
            unlock(args)?;
            Ok(0)
        }
        PortableAction::ChangePassword(args) => {
            change_password(args)?;
            Ok(0)
        }
        PortableAction::Reset(args) => {
            reset(args)?;
            Ok(0)
        }
    }
}

fn status(args: PortableStatusArgs) -> CliResult<()> {
    initialize_portable(args.json)?;
    write_response(args.json, "status")
}

fn setup(args: PortablePasswordArgs) -> CliResult<()> {
    initialize_portable(args.json)?;
    acquire_lock(args.json)?;
    let password =
        read_single_password(args.password_stdin, args.password_env.as_deref(), args.json)?;
    oxideterm_portable_runtime::keystore::create_portable_keystore(password.as_str())
        .map_err(|error| CliError::new("portable_setup_failed", error.to_string(), args.json))?;
    write_response(args.json, "setup")
}

fn unlock(args: PortablePasswordArgs) -> CliResult<()> {
    initialize_portable(args.json)?;
    acquire_lock(args.json)?;
    let password =
        read_single_password(args.password_stdin, args.password_env.as_deref(), args.json)?;
    oxideterm_portable_runtime::keystore::unlock_portable_keystore(password.as_str())
        .map_err(|error| CliError::new("portable_unlock_failed", error.to_string(), args.json))?;
    write_response(args.json, "unlock")
}

fn change_password(args: PortableChangePasswordArgs) -> CliResult<()> {
    initialize_portable(args.json)?;
    acquire_lock(args.json)?;
    let (current_password, new_password) = read_password_pair(&args)?;
    oxideterm_portable_runtime::keystore::change_portable_keystore_password(
        current_password.as_str(),
        new_password.as_str(),
    )
    .map_err(|error| {
        CliError::new(
            "portable_change_password_failed",
            error.to_string(),
            args.json,
        )
    })?;
    write_response(args.json, "changePassword")
}

fn reset(args: PortableResetArgs) -> CliResult<()> {
    initialize_portable(args.json)?;
    acquire_lock(args.json)?;
    if !args.yes {
        return Err(CliError::new(
            "confirmation_required",
            "pass --yes to delete the portable keystore",
            args.json,
        ));
    }
    oxideterm_portable_runtime::keystore::delete_portable_keystore()
        .map_err(|error| CliError::new("portable_reset_failed", error.to_string(), args.json))?;
    write_response(args.json, "reset")
}

fn initialize_portable(json: bool) -> CliResult<()> {
    initialize_portable_runtime()
        .map(|_| ())
        .map_err(|error| CliError::new("portable_runtime_failed", error.to_string(), json))
}

fn acquire_lock(json: bool) -> CliResult<()> {
    acquire_portable_instance_lock()
        .map_err(|error| CliError::new("portable_instance_lock_failed", error.to_string(), json))
}

fn write_response(json: bool, action: &'static str) -> CliResult<()> {
    let status = portable_status_snapshot()
        .map_err(|error| CliError::new("portable_status_failed", error.to_string(), json))?;
    let response = PortableCommandResponse { action, status };
    if json {
        output::write_json(&response)
    } else {
        output::write_text(format!(
            "portable: {} status={:?} dataDir={} keystore={}",
            response.action,
            response.status.status,
            response.status.data_dir,
            response
                .status
                .keystore_path
                .as_deref()
                .unwrap_or("unavailable")
        ));
        Ok(())
    }
}

fn read_single_password(
    stdin: bool,
    env: Option<&str>,
    json: bool,
) -> CliResult<Zeroizing<String>> {
    match (stdin, env) {
        (true, None) => read_stdin_password(json),
        (false, Some(var)) => read_env_password(var, json),
        (false, None) => Err(CliError::new(
            "password_source_required",
            "pass --password-stdin or --password-env",
            json,
        )),
        (true, Some(_)) => Err(CliError::new(
            "password_source_conflict",
            "choose only one password source",
            json,
        )),
    }
}

fn read_password_pair(
    args: &PortableChangePasswordArgs,
) -> CliResult<(Zeroizing<String>, Zeroizing<String>)> {
    if args.password_stdin {
        if args.current_password_env.is_some() || args.new_password_env.is_some() {
            return Err(CliError::new(
                "password_source_conflict",
                "choose stdin or environment variables, not both",
                args.json,
            ));
        }
        let input = read_stdin_password(args.json)?;
        let mut parts = input.lines();
        let current = parts.next().unwrap_or_default().to_string();
        let new = parts.next().unwrap_or_default().to_string();
        if current.is_empty() || new.is_empty() {
            return Err(CliError::new(
                "password_source_required",
                "--password-stdin must provide current and new password lines",
                args.json,
            ));
        }
        return Ok((Zeroizing::new(current), Zeroizing::new(new)));
    }

    let current_var = args.current_password_env.as_deref().ok_or_else(|| {
        CliError::new(
            "password_source_required",
            "pass --current-password-env or --password-stdin",
            args.json,
        )
    })?;
    let new_var = args.new_password_env.as_deref().ok_or_else(|| {
        CliError::new(
            "password_source_required",
            "pass --new-password-env or --password-stdin",
            args.json,
        )
    })?;
    Ok((
        read_env_password(current_var, args.json)?,
        read_env_password(new_var, args.json)?,
    ))
}

fn read_stdin_password(json: bool) -> CliResult<Zeroizing<String>> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| CliError::new("stdin_read_failed", error.to_string(), json))?;
    trim_trailing_newlines(&mut input);
    Ok(Zeroizing::new(input))
}

fn read_env_password(var: &str, json: bool) -> CliResult<Zeroizing<String>> {
    std::env::var(var)
        .map(Zeroizing::new)
        .map_err(|_| CliError::new("env_secret_missing", format!("{var} is not set"), json))
}

fn trim_trailing_newlines(value: &mut String) {
    while value.ends_with(['\n', '\r']) {
        value.pop();
    }
}
