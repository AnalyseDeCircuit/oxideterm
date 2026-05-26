// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

pub type CliResult<T> = Result<T, CliError>;

#[derive(Debug)]
pub struct CliError {
    pub code: &'static str,
    pub message: String,
    pub json: bool,
}

impl CliError {
    pub fn new(code: &'static str, message: impl Into<String>, json: bool) -> Self {
        Self {
            code,
            message: message.into(),
            json,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.code {
            "connection_not_found" | "settings_key_not_found" => 2,
            _ => 1,
        }
    }
}

pub fn runtime_error(error: impl std::fmt::Display, json: bool) -> CliError {
    CliError::new("runtime_error", error.to_string(), json)
}
