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
