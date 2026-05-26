// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use oxideterm_cloud_sync::state::CloudSyncStateStore;
use oxideterm_connections::ConnectionStore;
use serde::Serialize;

use crate::{
    args::OutputArgs,
    error::CliResult,
    output::{self, OutputFormat},
    paths::{self, CliPaths},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnoseResponse {
    paths: CliPaths,
    files: Vec<FileDiagnostic>,
    loads: Vec<LoadDiagnostic>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileDiagnostic {
    name: &'static str,
    path: String,
    exists: bool,
    kind: &'static str,
    readable: bool,
    size_bytes: Option<u64>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LoadDiagnostic {
    name: &'static str,
    ok: bool,
    error: Option<String>,
}

pub fn show_paths(args: OutputArgs) -> CliResult<()> {
    let paths = paths::cli_paths();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&paths),
        OutputFormat::Text => {
            output::write_text(format_paths_text(&paths));
            Ok(())
        }
    }
}

pub fn diagnose(args: OutputArgs) -> CliResult<()> {
    let paths = paths::cli_paths();
    let response = DiagnoseResponse {
        files: file_diagnostics(&paths),
        loads: load_diagnostics(&paths),
        paths,
    };

    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format_diagnose_text(&response));
            Ok(())
        }
    }
}

fn file_diagnostics(paths: &CliPaths) -> Vec<FileDiagnostic> {
    vec![
        file_diagnostic("settings", Path::new(&paths.settings)),
        file_diagnostic("connections", Path::new(&paths.connections)),
        file_diagnostic("cloudSync", Path::new(&paths.cloud_sync)),
    ]
}

fn load_diagnostics(paths: &CliPaths) -> Vec<LoadDiagnostic> {
    vec![
        settings_json_diagnostic(Path::new(&paths.settings)),
        connection_store_diagnostic(PathBuf::from(&paths.connections)),
        cloud_sync_store_diagnostic(PathBuf::from(&paths.cloud_sync)),
    ]
}

fn file_diagnostic(name: &'static str, path: &Path) -> FileDiagnostic {
    match fs::metadata(path) {
        Ok(metadata) => {
            let readable_error = fs::File::open(path).err();
            FileDiagnostic {
                name,
                path: path.display().to_string(),
                exists: true,
                kind: file_kind(&metadata),
                readable: readable_error.is_none(),
                size_bytes: metadata.is_file().then_some(metadata.len()),
                error: readable_error.map(|error| error.to_string()),
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => FileDiagnostic {
            name,
            path: path.display().to_string(),
            exists: false,
            kind: "missing",
            readable: false,
            size_bytes: None,
            error: None,
        },
        Err(error) => FileDiagnostic {
            name,
            path: path.display().to_string(),
            exists: false,
            kind: "unknown",
            readable: false,
            size_bytes: None,
            error: Some(error.to_string()),
        },
    }
}

fn file_kind(metadata: &fs::Metadata) -> &'static str {
    if metadata.is_file() {
        "file"
    } else if metadata.is_dir() {
        "directory"
    } else {
        "other"
    }
}

fn settings_json_diagnostic(path: &Path) -> LoadDiagnostic {
    // SettingsStore::load_default currently writes sanitized data, so diagnose parses only.
    match fs::read_to_string(path) {
        Ok(contents) if contents.trim().is_empty() => LoadDiagnostic {
            name: "settingsJson",
            ok: true,
            error: None,
        },
        Ok(contents) => match serde_json::from_str::<serde_json::Value>(&contents) {
            Ok(_) => LoadDiagnostic {
                name: "settingsJson",
                ok: true,
                error: None,
            },
            Err(error) => LoadDiagnostic {
                name: "settingsJson",
                ok: false,
                error: Some(error.to_string()),
            },
        },
        Err(error) if error.kind() == ErrorKind::NotFound => LoadDiagnostic {
            name: "settingsJson",
            ok: true,
            error: None,
        },
        Err(error) => LoadDiagnostic {
            name: "settingsJson",
            ok: false,
            error: Some(error.to_string()),
        },
    }
}

fn connection_store_diagnostic(path: PathBuf) -> LoadDiagnostic {
    match ConnectionStore::load_read_only(path) {
        Ok(_) => LoadDiagnostic {
            name: "connectionsStore",
            ok: true,
            error: None,
        },
        Err(error) => LoadDiagnostic {
            name: "connectionsStore",
            ok: false,
            error: Some(error.to_string()),
        },
    }
}

fn cloud_sync_store_diagnostic(path: PathBuf) -> LoadDiagnostic {
    match CloudSyncStateStore::load(path) {
        Ok(_) => LoadDiagnostic {
            name: "cloudSyncStore",
            ok: true,
            error: None,
        },
        Err(error) => LoadDiagnostic {
            name: "cloudSyncStore",
            ok: false,
            error: Some(error.to_string()),
        },
    }
}

fn format_paths_text(paths: &CliPaths) -> String {
    format!(
        "settingsDir: {}\nsettings: {}\nconnections: {}\ncloudSync: {}",
        paths.settings_dir, paths.settings, paths.connections, paths.cloud_sync
    )
}

fn format_diagnose_text(response: &DiagnoseResponse) -> String {
    let mut lines = vec![format_paths_text(&response.paths), "files:".to_string()];
    for file in &response.files {
        lines.push(format_file_text(file));
    }
    lines.push("loads:".to_string());
    for load in &response.loads {
        lines.push(format_load_text(load));
    }
    lines.join("\n")
}

fn format_file_text(file: &FileDiagnostic) -> String {
    let size = file
        .size_bytes
        .map(|size| format!("{size} bytes"))
        .unwrap_or_else(|| "-".to_string());
    let error = file.error.as_deref().unwrap_or("-");
    format!(
        "  {}: kind={} exists={} readable={} size={} error={}",
        file.name, file.kind, file.exists, file.readable, size, error
    )
}

fn format_load_text(load: &LoadDiagnostic) -> String {
    let error = load.error.as_deref().unwrap_or("-");
    format!("  {}: ok={} error={}", load.name, load.ok, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_reported_without_error() {
        let path =
            std::env::temp_dir().join(format!("oxideterm-cli-missing-{}", std::process::id()));

        let diagnostic = file_diagnostic("missing", &path);

        assert!(!diagnostic.exists);
        assert_eq!(diagnostic.kind, "missing");
        assert!(diagnostic.error.is_none());
    }

    #[test]
    fn formats_load_status_line() {
        let load = LoadDiagnostic {
            name: "settingsJson",
            ok: true,
            error: None,
        };

        assert_eq!(format_load_text(&load), "  settingsJson: ok=true error=-");
    }
}
