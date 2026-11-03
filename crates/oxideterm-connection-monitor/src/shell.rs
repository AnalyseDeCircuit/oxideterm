// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/// Quote one value for use as a single POSIX shell argument.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Run a generated Unix probe with a stable POSIX parser instead of the
/// account's interactive login shell, which may be Fish, Csh, Tcsh, or Nushell.
pub(crate) fn posix_shell_command(script: &str) -> String {
    format!("/bin/sh -c {}", shell_quote(script))
}

/// Quote one value for use as a single PowerShell string argument.
pub(crate) fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Run a generated script on a Windows target.
///
/// Used for lines that are typed into an interactive shell (follow streams):
/// the user's local shell may be `cmd.exe`, PowerShell, or a POSIX shell from
/// Git Bash, and each rewrites `$` and quotes before `powershell.exe` parses
/// the script. base64 UTF-16LE carries the script past all of them.
pub(crate) fn windows_encoded_command(script: &str) -> String {
    use base64::Engine as _;
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("powershell -NoProfile -NonInteractive -EncodedCommand {encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quotes_preserve_spaces_and_apostrophes() {
        assert_eq!(shell_quote("Oxide Term"), "'Oxide Term'");
        assert_eq!(shell_quote("it's-ready"), "'it'\"'\"'s-ready'");
        assert_eq!(powershell_quote("it's-ready"), "'it''s-ready'");
    }
}
