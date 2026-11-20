// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Runs the POSIX generators on Windows hosts through a Git/MSYS `bash`.
//!
//! The container scripts are POSIX shell; on Windows the same work exists in
//! Git Bash (and MSYS2) without maintaining a second PowerShell dialect. The
//! script travels as base64 so quotes, tabs and `$()` survive PowerShell's
//! command line, and stdin (payloads) is inherited by the `bash -c` child.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

const BASH_CANDIDATES: &str = r#"'bash','C:\Program Files\Git\bin\bash.exe','C:\Program Files\Git\usr\bin\bash.exe','C:\Program Files (x86)\Git\bin\bash.exe','C:\msys64\usr\bin\bash.exe'"#;

pub(crate) fn posix_via_bash(script: &str, section: &str, missing: &str) -> String {
    let encoded = STANDARD.encode(script.as_bytes());
    // PowerShell 5.1 does not escape embedded double quotes when it forwards an
    // argument to a native exe, so `bash -c $script` mangles the script. Write
    // it to a short-lived temp file instead; stdin stays free for payloads.
    let bridge = format!(
        "$oxideScript=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')); \
         $oxideBash=$null; \
         foreach($oxideCandidate in @({BASH_CANDIDATES})){{ \
         if(Test-Path -LiteralPath $oxideCandidate -PathType Leaf){{ $oxideBash=$oxideCandidate; break }}; \
         $oxideCmd=Get-Command $oxideCandidate -ErrorAction SilentlyContinue; \
         if($oxideCmd){{ $oxideBash=$oxideCmd.Source; break }} }}; \
         if($oxideBash){{ \
         $oxidePath=[IO.Path]::Combine([IO.Path]::GetTempPath(), 'oxide-'+$PID+'-'+[Guid]::NewGuid().ToString('N')+'.sh'); \
         try {{ \
         [IO.File]::WriteAllText($oxidePath, $oxideScript, (New-Object System.Text.UTF8Encoding($false))); \
         & $oxideBash $oxidePath.Replace('\\','/') \
         }} finally {{ Remove-Item -LiteralPath $oxidePath -Force -ErrorAction SilentlyContinue }} \
         }} else {{ Write-Output '==={section}==='; Write-Output ('UNAVAILABLE'+[char]9+'{missing}'); Write-Output '==={section}_END===' }}"
    );
    powershell_command(&bridge)
}

/// Wraps generated PowerShell so a Windows target can run it from `cmd.exe`
/// (local captures and SSH exec both start there). The bridge only uses single
/// quotes, so a `-Command "…"` wrapper needs no escaping and stays inside
/// cmd.exe's 8191-character limit; `-EncodedCommand` would double the length
/// through UTF-16 and overflow it.
pub(crate) fn powershell_command(script: &str) -> String {
    debug_assert!(
        !script.contains('"'),
        "the -Command wrapper relies on single-quoted PowerShell"
    );
    format!("powershell -NoProfile -NonInteractive -Command \"{script}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Discovery turns a missing Bash into a distinct error, so the marker the
    /// bridge substitutes for the whole probe has to keep the exact
    /// `UNAVAILABLE<TAB><reason>` shape the reader recognizes.
    #[test]
    fn missing_shell_marker_is_a_single_row_without_an_engine() {
        let command = posix_via_bash(
            "echo '===CONTAINER_DISCOVER==='; echo '===CONTAINER_DISCOVER_END==='",
            "CONTAINER_DISCOVER",
            "Git Bash (bash.exe) not found",
        );
        assert!(command.contains("'UNAVAILABLE'+[char]9+'Git Bash (bash.exe) not found'"));
        assert!(command.contains("Write-Output '===CONTAINER_DISCOVER==='"));
        assert!(command.contains("Write-Output '===CONTAINER_DISCOVER_END==='"));
        // A single tab keeps the reason in one column, so the reader sees no
        // engine name and can classify the row as a missing shell.
        assert_eq!(command.matches("[char]9").count(), 1);
    }

    #[test]
    fn script_travels_base64_and_falls_back_with_the_section_markers() {
        let script = "echo '===X==='; printf '%s\\n' \"$(id -u)\"; echo '===X_END==='";
        let command = posix_via_bash(script, "X", "Git Bash not found");
        // cmd.exe receives only a quoted PowerShell invocation.
        assert!(command.starts_with("powershell -NoProfile -NonInteractive -Command \""));
        assert!(command.ends_with('"'));
        assert!(command.len() < 8191, "cmd.exe rejects longer command lines");
        assert!(command.contains("& $oxideBash $oxidePath.Replace"));
        assert!(command.contains("===X==="));
        assert!(command.contains("===X_END==="));
        // The encoded payload must decode to the original POSIX script.
        let start = command.find("FromBase64String('").unwrap() + "FromBase64String('".len();
        let end = command[start..].find('\'').unwrap() + start;
        let decoded = STANDARD.decode(&command[start..end]).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), script);
    }

    /// Windows targets start the line from cmd.exe, whose 8191-character limit
    /// applies to every generated capture. The Podman probe once pushed the
    /// combined sample over it, so the budget is pinned here for all engines.
    #[test]
    fn every_windows_container_command_fits_cmd_exe() {
        use crate::container::{
            ContainerEngine, ContainerInstance, ContainerTransport, discovery_command,
            registries_command, snapshot_command, stats_command,
        };
        let instances = [
            ContainerInstance::new(
                ContainerEngine::Docker,
                ContainerTransport::Socket {
                    endpoint: "npipe:////./pipe/docker_engine".into(),
                },
                None,
            ),
            ContainerInstance::new(ContainerEngine::Podman, ContainerTransport::Cli, None),
            ContainerInstance::new(ContainerEngine::Wslc, ContainerTransport::Cli, None),
        ];
        let discovery = discovery_command("Windows");
        assert!(discovery.len() < 8191, "discovery: {}", discovery.len());
        for instance in instances {
            for (label, command) in [
                ("snapshot", snapshot_command("Windows", &instance)),
                ("stats", stats_command("Windows", &instance)),
                ("registries", registries_command("Windows", &instance)),
            ] {
                assert!(
                    command.len() < 8191,
                    "{label} for {:?}: {} chars",
                    instance.engine,
                    command.len()
                );
            }
        }
    }
}
