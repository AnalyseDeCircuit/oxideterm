// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs,
    io::{self, Read},
    path::Path,
};

use zeroize::Zeroizing;

use crate::{
    args::OxidePasswordArgs,
    error::{CliError, CliResult},
};

pub(super) fn read_oxide_file(path: &str, json: bool) -> CliResult<Vec<u8>> {
    fs::read(path).map_err(|error| {
        CliError::new(
            "oxide_read_failed",
            format!("failed to read .oxide file {path}: {error}"),
            json,
        )
    })
}

pub(super) fn read_password(args: &OxidePasswordArgs, json: bool) -> CliResult<Zeroizing<String>> {
    if let Some(name) = args.password_env.as_ref() {
        let value = std::env::var(name).map_err(|error| {
            CliError::new(
                "oxide_password_missing",
                format!("failed to read password from env var {name}: {error}"),
                json,
            )
        })?;
        return Ok(Zeroizing::new(value));
    }
    if args.password_stdin {
        let mut password = String::new();
        io::stdin().read_to_string(&mut password).map_err(|error| {
            CliError::new(
                "oxide_password_read_failed",
                format!("failed to read password from stdin: {error}"),
                json,
            )
        })?;
        trim_line_endings(&mut password);
        return Ok(Zeroizing::new(password));
    }
    Err(CliError::new(
        "oxide_password_required",
        "provide --password-stdin or --password-env VAR; passwords are not accepted as command arguments",
        json,
    ))
}

pub(super) fn ensure_output_path(path: &str, overwrite: bool, json: bool) -> CliResult<()> {
    if Path::new(path).exists() && !overwrite {
        return Err(CliError::new(
            "oxide_export_exists",
            format!("output file {path} already exists; pass --overwrite to replace it"),
            json,
        ));
    }
    Ok(())
}

pub(super) fn write_output_file(path: &str, bytes: &[u8], json: bool) -> CliResult<()> {
    let path = Path::new(path);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::new(
                "oxide_export_failed",
                format!("failed to create output dir {}: {error}", parent.display()),
                json,
            )
        })?;
    }
    fs::write(path, bytes).map_err(|error| {
        CliError::new(
            "oxide_export_failed",
            format!("failed to write .oxide file {}: {error}", path.display()),
            json,
        )
    })
}

fn trim_line_endings(value: &mut String) {
    while value.ends_with('\n') || value.ends_with('\r') {
        value.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_stdin_trims_line_endings() {
        let mut value = "secret\r\n".to_string();

        trim_line_endings(&mut value);

        assert_eq!(value, "secret");
    }
}
