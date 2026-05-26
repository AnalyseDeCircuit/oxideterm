// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::Serialize;
use serde_json::json;

use crate::error::{CliError, CliResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Text,
    Json,
}

pub fn format_from_flag(json: bool) -> OutputFormat {
    if json {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    }
}

pub fn write_json<T: Serialize>(value: &T) -> CliResult<()> {
    // Scripts get a stable envelope, even when individual command payloads grow.
    let envelope = json!({
        "ok": true,
        "data": value,
    });
    let text = serde_json::to_string_pretty(&envelope)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), true))?;
    println!("{text}");
    Ok(())
}

pub fn write_text(value: impl AsRef<str>) {
    println!("{}", value.as_ref());
}

pub fn write_error(format: OutputFormat, error: &CliError) {
    match format {
        OutputFormat::Text => eprintln!("{}: {}", error.code, error.message),
        OutputFormat::Json => {
            let envelope = json!({
                "ok": false,
                "error": {
                    "code": error.code,
                    "message": error.message,
                },
            });
            let fallback = format!(
                r#"{{"ok":false,"error":{{"code":"{}","message":"{}"}}}}"#,
                error.code, error.message
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&envelope).unwrap_or(fallback)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_flag_selects_json_format() {
        assert_eq!(format_from_flag(true), OutputFormat::Json);
        assert_eq!(format_from_flag(false), OutputFormat::Text);
    }
}
