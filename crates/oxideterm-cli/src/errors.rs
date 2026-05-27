// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::Serialize;

use crate::{
    args::ErrorCatalogArgs,
    error::CliResult,
    output::{self, OutputFormat},
};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorCodeDoc {
    code: &'static str,
    exit_code: i32,
    meaning: &'static str,
}

const ERROR_CODES: &[ErrorCodeDoc] = &[
    ErrorCodeDoc {
        code: "runtime_error",
        exit_code: 1,
        meaning: "An underlying store, filesystem, backend, or domain operation failed.",
    },
    ErrorCodeDoc {
        code: "serialization_failed",
        exit_code: 1,
        meaning: "The CLI could not serialize a response.",
    },
    ErrorCodeDoc {
        code: "connection_not_found",
        exit_code: 2,
        meaning: "A connection query did not match an existing saved connection.",
    },
    ErrorCodeDoc {
        code: "settings_key_not_found",
        exit_code: 2,
        meaning: "A settings JSON path did not exist.",
    },
    ErrorCodeDoc {
        code: "write_confirmation_required",
        exit_code: 1,
        meaning: "A real write was requested without --yes.",
    },
    ErrorCodeDoc {
        code: "completion_install_exists",
        exit_code: 1,
        meaning: "A completion file already exists and --force was not provided.",
    },
    ErrorCodeDoc {
        code: "completion_install_failed",
        exit_code: 1,
        meaning: "The CLI could not create or write the completion file.",
    },
    ErrorCodeDoc {
        code: "home_dir_not_found",
        exit_code: 1,
        meaning: "The CLI could not resolve a home directory for shell integration.",
    },
];

pub(crate) fn run(args: ErrorCatalogArgs) -> CliResult<i32> {
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ERROR_CODES)?,
        OutputFormat::Text => {
            for doc in ERROR_CODES {
                output::write_text(format!(
                    "{}\texit={}\t{}",
                    doc.code, doc.exit_code, doc.meaning
                ));
            }
        }
    }
    Ok(0)
}
