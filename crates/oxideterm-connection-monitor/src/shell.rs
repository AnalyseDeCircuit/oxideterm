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
///
/// Only safe for values this crate generated itself. A PowerShell single-quoted
/// string is also terminated by U+2018/U+2019, so doubling the ASCII apostrophe
/// does not make a remote- or user-authored value inert: use [`powershell_bind`]
/// for anything that did not come from a literal written here.
pub(crate) fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// One dynamic value bound to a variable inside a generated PowerShell script.
pub(crate) struct PowerShellData {
    /// `$vN=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('…'))`
    assignment: String,
}

/// Bind `value` to the next data variable of one script and return the `$vN`
/// fragment the script body must use for it.
///
/// The value travels as base64 of its UTF-8 bytes and the script decodes it, so
/// none of its characters reach the PowerShell parser: neither an ASCII
/// apostrophe nor a smart quote can terminate the base64 literal, and
/// `& | ^ % $ ( )` are ordinary characters inside it. PowerShell then hands the
/// decoded string to the native command as one argument, which is the argument
/// parser's problem and never script syntax.
pub(crate) fn powershell_bind(data: &mut Vec<PowerShellData>, value: &str) -> String {
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::STANDARD.encode(value.as_bytes());
    let variable = format!("$v{}", data.len());
    data.push(PowerShellData {
        assignment: format!(
            "{variable}=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}'))"
        ),
    });
    variable
}

/// Render one dynamic value as a single argument for the shell that will parse
/// the generated line.
///
/// This is the only escape strategy generated commands may use: on Windows the
/// value is bound to a script variable with [`powershell_bind`] and never
/// spelled into the text, on POSIX it is one quoted argument. POSIX keeps
/// quoting because that line must run in the user's own interactive shell,
/// which is the only place the command is on `PATH` with the right environment.
pub(crate) fn script_arg(windows: bool, data: &mut Vec<PowerShellData>, value: &str) -> String {
    if windows {
        powershell_bind(data, value)
    } else {
        shell_quote(value)
    }
}

/// Assemble a script: every binding decodes first, then `body` runs.
///
/// `body` is script text owned by the caller; the only dynamic content in it may
/// be the references returned by [`powershell_bind`].
pub(crate) fn powershell_script(data: &[PowerShellData], body: &str) -> String {
    let mut script = String::new();
    for item in data {
        script.push_str(&item.assignment);
        script.push(';');
    }
    script.push_str(body);
    script
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

/// A value that would break out of a quoted PowerShell argument and out of a
/// `cmd.exe` command line: both apostrophes PowerShell accepts as string
/// delimiters, the cmd.exe and PowerShell metacharacters, and a substitution.
/// Shared fixture for every builder that must carry such a value as data.
#[cfg(test)]
pub(crate) const HOSTILE_VALUE: &str = "it's ’ \"w\" & calc.exe | whoami ^ %TEMP% $(id) end";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quotes_preserve_spaces_and_apostrophes() {
        assert_eq!(shell_quote("Oxide Term"), "'Oxide Term'");
        assert_eq!(shell_quote("it's-ready"), "'it'\"'\"'s-ready'");
        assert_eq!(powershell_quote("it's-ready"), "'it''s-ready'");
    }

    #[test]
    fn powershell_data_decodes_to_the_exact_value() {
        let mut data = Vec::new();
        let label = powershell_bind(&mut data, HOSTILE_VALUE);
        let session = powershell_bind(&mut data, "dev");
        let body = format!("herdr --session {session} pane rename {label}");
        let script = powershell_script(&data, &body);

        assert_eq!(decoded_powershell_data(&script), [HOSTILE_VALUE, "dev"]);
        // The value is present only as base64 data, so nothing in it can be
        // parsed: the fragments a shell would act on never appear as text.
        for fragment in [HOSTILE_VALUE, "’", "& calc", "| whoami", "$(id)", "%TEMP%"] {
            assert!(
                !script.contains(fragment),
                "{fragment:?} reached the script text"
            );
        }
        assert!(script.contains("$v0") && script.contains("$v1"), "{script}");
    }

    #[test]
    fn powershell_script_without_data_is_the_body_itself() {
        assert_eq!(
            powershell_script(&[], "Write-Output 'ok'"),
            "Write-Output 'ok'"
        );
    }
}

/// Every value a PowerShell script carries as data, in binding order.
///
/// Test-only: the generated script is a transport for these values, and the
/// assertions belong on the values, not on their base64 form.
#[cfg(test)]
pub(crate) fn decoded_powershell_data(script: &str) -> Vec<String> {
    use base64::Engine as _;
    const BEGIN: &str = "[Convert]::FromBase64String('";
    const END: &str = "'))";
    let mut values = Vec::new();
    let mut rest = script;
    while let Some(start) = rest.find(BEGIN) {
        let payload = &rest[start + BEGIN.len()..];
        let stop = payload.find(END).expect("terminated data literal");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&payload[..stop])
            .expect("base64 data");
        values.push(String::from_utf8(bytes).expect("utf-8 data"));
        rest = &payload[stop + END.len()..];
    }
    values
}

/// The script a Windows transport line actually runs.
///
/// Test-only, and shared by every module that builds Windows lines: the
/// assertions belong on the decoded script, not on the base64 of the transport.
#[cfg(test)]
pub(crate) fn decoded_windows_script(line: &str) -> String {
    use base64::Engine as _;
    let encoded = line.trim_end().rsplit(' ').next().expect("encoded payload");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("base64 payload");
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&units).expect("utf-16 payload")
}

/// Asserts a Windows line is safe to type into a shell this crate did not
/// choose: `cmd.exe` splits on `&` and expands `%`, and PowerShell rewrites `$`
/// and quotes, so nothing but the flag and the base64 payload may appear.
#[cfg(test)]
pub(crate) fn assert_inert_windows_line(line: &str) {
    assert!(
        line.starts_with("powershell -NoProfile -NonInteractive -EncodedCommand "),
        "{line}"
    );
    for shell_character in ['\'', '"', '$', '&', '|', '^', '%', '(', ')', ';', '<', '>'] {
        assert!(
            !line.contains(shell_character),
            "{shell_character:?} in {line}"
        );
    }
}
