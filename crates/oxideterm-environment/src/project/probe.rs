// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use super::model::{ProjectManifestEntry, ProjectProbeError, ProjectProbeOutcome};
use super::parse::{interpret_project_manifest_entries, project_manifest_file_names};

pub const PROJECT_SHELL_PROBE_SENTINEL: &str = "OXIDETERM_PROJECT_PROBE_V1";
pub const PROJECT_PROBE_MAX_ANCESTORS: usize = 12;
pub const PROJECT_PROBE_MAX_FILE_BYTES: usize = 64 * 1024;

/// Build the POSIX shell command used by SSH node exec probes.
pub fn remote_shell_project_probe_command(cwd: &str) -> String {
    format!(
        "{}{}",
        remote_project_probe_cd_prelude(cwd),
        remote_project_probe_body()
    )
}

/// Interpret the NUL-delimited output from `remote_shell_project_probe_command`.
pub fn parse_remote_shell_project_probe_output(output: &str) -> ProjectProbeOutcome {
    let mut parts = output.split('\0');
    if parts.next() != Some(PROJECT_SHELL_PROBE_SENTINEL) {
        return ProjectProbeOutcome::Error(ProjectProbeError::new(
            "missing project probe sentinel",
        ));
    }

    let mut entries = Vec::new();
    let mut state = None;
    while let Some(kind) = parts.next() {
        match kind {
            "state" => {
                state = parts.next().map(str::to_string);
            }
            "file" => {
                let Some(path) = parts.next() else {
                    break;
                };
                let Some("content") = parts.next() else {
                    break;
                };
                let Some(content) = parts.next() else {
                    break;
                };
                if let Some(entry) = ProjectManifestEntry::new(path, content) {
                    entries.push(entry);
                }
            }
            "" => {}
            _ => {
                let _ = parts.next();
            }
        }
    }

    match state.as_deref() {
        Some("cwd_missing") => ProjectProbeOutcome::CwdMissing,
        Some("ok") => interpret_project_manifest_entries(entries),
        _ => ProjectProbeOutcome::Error(ProjectProbeError::new("invalid project probe state")),
    }
}

fn remote_project_probe_cd_prelude(cwd: &str) -> String {
    format!(
        "cd -- {} 2>/dev/null || {{ printf '{}\\0state\\0cwd_missing\\0'; exit 0; }}\n",
        remote_project_cd_target(cwd),
        PROJECT_SHELL_PROBE_SENTINEL,
    )
}

fn remote_project_probe_body() -> String {
    let names = project_manifest_file_names()
        .iter()
        .map(|name| shell_quote(name))
        .collect::<Vec<_>>()
        .join(" ");
    // The probe avoids SFTP and agents: it walks upward from the active shell
    // cwd through a short-lived exec channel and emits small manifest payloads.
    [
        format!("printf '{}\\0state\\0ok\\0'", PROJECT_SHELL_PROBE_SENTINEL),
        "dir=$(pwd -P 2>/dev/null || pwd)".to_string(),
        "depth=0".to_string(),
        format!(
            "while [ \"$depth\" -lt {} ]; do",
            PROJECT_PROBE_MAX_ANCESTORS
        ),
        format!("  for name in {names}; do"),
        "    file=\"$dir/$name\"".to_string(),
        "    if [ -f \"$file\" ]; then".to_string(),
        "      printf 'file\\0%s\\0content\\0' \"$file\"".to_string(),
        format!(
            "      head -c {} \"$file\" 2>/dev/null || true",
            PROJECT_PROBE_MAX_FILE_BYTES
        ),
        "      printf '\\0'".to_string(),
        "    fi".to_string(),
        "  done".to_string(),
        "  [ \"$dir\" = \"/\" ] && break".to_string(),
        "  parent=$(dirname \"$dir\")".to_string(),
        "  [ \"$parent\" = \"$dir\" ] && break".to_string(),
        "  dir=\"$parent\"".to_string(),
        "  depth=$((depth + 1))".to_string(),
        "done".to_string(),
    ]
    .join("\n")
        + "\n"
}

fn remote_project_cd_target(cwd: &str) -> String {
    let cwd = cwd.trim();
    if cwd == "~" {
        return "\"$HOME\"".to_string();
    }
    if let Some(rest) = cwd.strip_prefix("~/") {
        if rest.is_empty() {
            "\"$HOME\"".to_string()
        } else {
            format!("\"$HOME\"/{}", shell_quote(rest))
        }
    } else {
        shell_quote(cwd)
    }
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_project_probe_quotes_cwd_and_uses_manifest_names() {
        let command = remote_shell_project_probe_command("~/Project Dir");
        assert!(command.contains("cd -- \"$HOME\"/'Project Dir'"));
        assert!(command.contains(PROJECT_SHELL_PROBE_SENTINEL));
        assert!(command.contains("'package.json'"));
        assert!(command.contains("'Cargo.toml'"));
        assert!(command.contains("head -c 65536"));
    }

    #[test]
    fn remote_project_probe_parser_reads_files() {
        let output = format!(
            "{}\0state\0ok\0file\0/repo/package.json\0content\0{{\"scripts\":{{\"dev\":\"vite\"}}}}\0",
            PROJECT_SHELL_PROBE_SENTINEL
        );
        let outcome = parse_remote_shell_project_probe_output(&output);
        let ProjectProbeOutcome::Ready(snapshot) = outcome else {
            panic!("expected project");
        };
        assert_eq!(snapshot.root_path(), "/repo");
        assert_eq!(snapshot.display_label(), "Node");
    }

    #[test]
    fn remote_project_probe_parser_handles_missing_cwd() {
        let output = format!("{}\0state\0cwd_missing\0", PROJECT_SHELL_PROBE_SENTINEL);
        assert_eq!(
            parse_remote_shell_project_probe_output(&output),
            ProjectProbeOutcome::CwdMissing
        );
    }
}
