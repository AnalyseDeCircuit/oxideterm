// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Command builders. Every value that reaches a command line is either a
//! literal written here or a dynamic value that travels as data: base64 decoded
//! into a PowerShell variable on Windows, one single-quoted argument parsed by
//! /bin/sh on Unix. The account shell never parses the dynamic values directly.

use super::HERDR_READ_LINES;
use super::ids::{validated_id, validated_name};
use crate::shell::{
    PowerShellData, encoded_posix_shell_command, posix_shell_command, powershell_bind,
    powershell_script, push_shell_quoted, script_arg, shell_quoted_len, windows_encoded_command,
};
use std::fmt;
use zeroize::Zeroizing;

#[derive(Clone, Eq, PartialEq)]
pub struct HerdrCaptureCommand {
    pub command: String,
}

impl std::fmt::Debug for HerdrCaptureCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A capture line embeds server-authored session names, so the derived
        // form would put them in logs and in panic messages.
        formatter
            .debug_struct("HerdrCaptureCommand")
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum HerdrActionKind<'a> {
    NewWorkspace {
        label: String,
        cwd: String,
    },
    CloseWorkspace {
        workspace_id: String,
    },
    RenameWorkspace {
        workspace_id: String,
        label: String,
    },
    FocusWorkspace {
        workspace_id: String,
    },
    CreateTab {
        workspace_id: String,
    },
    FocusTab {
        tab_id: String,
    },
    StopSession {
        name: String,
    },
    DeleteSession {
        name: String,
    },
    ClosePane {
        pane_id: String,
    },
    RenamePane {
        pane_id: String,
        label: String,
    },
    RenameTab {
        tab_id: String,
        label: String,
    },
    CloseTab {
        tab_id: String,
    },
    SplitPane {
        pane_id: String,
    },
    RunPane {
        pane_id: String,
        command: PaneCommand<'a>,
    },
}

/// A borrowed command that must not inherit its owner's Debug implementation.
pub struct PaneCommand<'a>(pub &'a str);

impl fmt::Debug for PaneCommand<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

pub struct HerdrActionCommand {
    pub command: Zeroizing<String>,
}

impl std::fmt::Debug for HerdrActionCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The command line embeds the user's typed pane command and
        // server-authored names, so the derived form would put them in logs
        // and in panic messages.
        formatter
            .debug_struct("HerdrActionCommand")
            .finish_non_exhaustive()
    }
}

/// Where an interactive herdr attach lands; all of these go to a terminal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HerdrAttachTarget {
    /// Launch or attach the default persistent session.
    Default,
    Session {
        name: String,
    },
    /// `--session <name>` uses the named session or creates it.
    NewSession {
        name: String,
    },
    Agent {
        session: String,
        pane_id: String,
    },
}

fn is_windows_os(os_type: &str) -> bool {
    matches!(os_type, "Windows" | "windows")
}

/// One remote- or user-authored label, session or workspace name, validated
/// then rendered as one argument.
fn name_arg(windows: bool, data: &mut Vec<PowerShellData>, name: &str) -> Result<String, String> {
    let name = validated_name(name)?;
    Ok(script_arg(windows, data, &name))
}

/// One object id (`wA:tB:p1`), validated then rendered as an argument.
///
/// The id grammar excludes every shell metacharacter, so a POSIX line keeps it
/// bare; a Windows line binds it anyway, because the transport cannot tell a
/// validated value from a forged one once the text is written.
fn object_arg(windows: bool, data: &mut Vec<PowerShellData>, id: &str) -> Result<String, String> {
    let id = validated_id(id)?;
    Ok(if windows {
        powershell_bind(data, &id)
    } else {
        id
    })
}

/// `herdr --session <session>`: workspace, tab and pane verbs all read that
/// session's own socket, so the session is part of every one of them.
fn session_prefix(
    windows: bool,
    data: &mut Vec<PowerShellData>,
    session: &str,
) -> Result<String, String> {
    Ok(format!(
        "herdr --session {}",
        name_arg(windows, data, session)?
    ))
}

/// The JSON envelope is the only success/failure signal, so stderr is
/// discarded and each list prints its own markers around one JSON document.
const HERDR_SNAPSHOT_UNIX: &str = concat!(
    "echo '===HERDR==='; ",
    "if command -v herdr >/dev/null 2>&1; then ",
    "echo '===HERDR_SESSIONS==='; herdr session list --json 2>/dev/null; ",
    "else echo '__OXIDE_HERDR_UNAVAILABLE__'; fi; ",
    "echo '===HERDR_END==='"
);

fn build_windows_herdr_snapshot_command() -> String {
    let script = concat!(
        "Write-Output '===HERDR===';",
        "if(Get-Command herdr -ErrorAction SilentlyContinue){",
        "Write-Output '===HERDR_SESSIONS===';",
        "herdr session list --json 2>$null;",
        "}else{Write-Output '__OXIDE_HERDR_UNAVAILABLE__'};",
        "Write-Output '===HERDR_END===';"
    );
    windows_encoded_command(&script)
}

pub fn build_herdr_snapshot_command(os_type: &str) -> HerdrCaptureCommand {
    if is_windows_os(os_type) {
        HerdrCaptureCommand {
            command: build_windows_herdr_snapshot_command(),
        }
    } else {
        HerdrCaptureCommand {
            command: posix_shell_command(HERDR_SNAPSHOT_UNIX),
        }
    }
}

/// Workspaces, panes and agents of one session. Every herdr workspace command
/// talks to that session's own socket, so the session is part of the command.
pub fn build_herdr_session_detail_command(
    os_type: &str,
    session: &str,
) -> Result<HerdrCaptureCommand, String> {
    let windows = is_windows_os(os_type);
    // The session names the socket every list reads, and it is server-authored,
    // so it is one argument the shell receives intact: bound as data on
    // Windows, quoted on POSIX. Only the two script bodies are literals.
    let mut data = Vec::new();
    let socket = script_arg(windows, &mut data, &validated_name(session)?);
    let command = if windows {
        let body = format!(
            concat!(
                "Write-Output '===HERDR===';",
                "if(Get-Command herdr -ErrorAction SilentlyContinue){{",
                "Write-Output '===HERDR_WORKSPACES===';",
                "herdr --session {socket} workspace list 2>$null;",
                "Write-Output '===HERDR_TABS===';",
                "herdr --session {socket} tab list 2>$null;",
                "Write-Output '===HERDR_PANES===';",
                "herdr --session {socket} pane list 2>$null;",
                "Write-Output '===HERDR_AGENTS===';",
                "herdr --session {socket} agent list 2>$null;",
                "}}else{{Write-Output '__OXIDE_HERDR_UNAVAILABLE__'}};",
                "Write-Output '===HERDR_END===';"
            ),
            socket = socket
        );
        windows_encoded_command(&powershell_script(&data, &body))
    } else {
        let body = format!(
            concat!(
                "echo '===HERDR==='; ",
                "if command -v herdr >/dev/null 2>&1; then ",
                "echo '===HERDR_WORKSPACES==='; herdr --session {socket} workspace list 2>/dev/null; ",
                "echo '===HERDR_TABS==='; herdr --session {socket} tab list 2>/dev/null; ",
                "echo '===HERDR_PANES==='; herdr --session {socket} pane list 2>/dev/null; ",
                "echo '===HERDR_AGENTS==='; herdr --session {socket} agent list 2>/dev/null; ",
                "else echo '__OXIDE_HERDR_UNAVAILABLE__'; fi; ",
                "echo '===HERDR_END==='"
            ),
            socket = socket
        );
        encoded_posix_shell_command(&body)
    };
    Ok(HerdrCaptureCommand { command })
}

pub fn build_herdr_read_command(
    os_type: &str,
    session: &str,
    pane_id: &str,
) -> Result<HerdrCaptureCommand, String> {
    let windows = is_windows_os(os_type);
    // Pane ids are per session, so the read must address that session's socket.
    let mut data = Vec::new();
    let socket = name_arg(windows, &mut data, session)?;
    let pane = object_arg(windows, &mut data, pane_id)?;
    let read = format!(
        "herdr --session {socket} pane read {pane} --lines {HERDR_READ_LINES} --source recent --format text 2>&1"
    );
    Ok(HerdrCaptureCommand {
        command: if windows {
            windows_encoded_command(&powershell_script(&data, &read))
        } else {
            encoded_posix_shell_command(&read)
        },
    })
}

pub fn build_herdr_attach_command(
    os_type: &str,
    target: &HerdrAttachTarget,
) -> Result<String, String> {
    // The attach line is typed into the terminal that will show the session, so
    // its shell is whatever the user or the sshd configuration chose: `cmd.exe`
    // keeps the quote characters it is handed and splits the line on `&`, so a
    // remote-authored name spelled into that line either fails the attach or
    // runs a second command. Windows therefore gets the encoded invocation form,
    // the one the Zellij attach already used, and only literals reach a parser.
    // The Unix wrapper inherits the terminal's PATH without asking its shell
    // to interpret the name.
    let windows = is_windows_os(os_type);
    let mut data = Vec::new();
    let invocation = match target {
        HerdrAttachTarget::Default => "herdr".to_string(),
        HerdrAttachTarget::Session { name } => {
            let name = name_arg(windows, &mut data, name)?;
            format!("herdr session attach {name}")
        }
        HerdrAttachTarget::NewSession { name } => {
            let name = name_arg(windows, &mut data, name)?;
            format!("herdr --session {name}")
        }
        HerdrAttachTarget::Agent { session, pane_id } => {
            let socket = name_arg(windows, &mut data, session)?;
            let pane = object_arg(windows, &mut data, pane_id)?;
            format!("herdr --session {socket} agent attach {pane}")
        }
    };
    Ok(if windows {
        // The preamble matches the Zellij attach: the terminal's encoding is set
        // before herdr takes the screen, and the exit code travels back.
        let script = format!(
            "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); & {invocation}; exit $LASTEXITCODE"
        );
        windows_encoded_command(&powershell_script(&data, &script))
    } else if matches!(target, HerdrAttachTarget::Default) {
        invocation
    } else {
        encoded_posix_shell_command(&invocation)
    })
}

/// Workspace, tab and pane mutations are addressed over one session's own
/// socket, so `session` becomes the global `--session` argument. Session level
/// actions (`stop`, `delete`) name their session directly instead.
pub fn build_herdr_action_command(
    os_type: &str,
    session: &str,
    action: &HerdrActionKind<'_>,
) -> Result<HerdrActionCommand, String> {
    // Herdr takes one argv per value, and every value here was authored by the
    // server or typed into a text field. Only the verbs are literals: a value is
    // bound as data on Windows and quoted on POSIX, so neither shell can read
    // one of them back as syntax.
    let windows = is_windows_os(os_type);
    let mut data = Vec::new();
    let payload = match action {
        HerdrActionKind::NewWorkspace { label, cwd } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let label = name_arg(windows, &mut data, label)?;
            let cwd = cwd.trim();
            if cwd.is_empty() {
                format!("{prefix} workspace create --label {label}")
            } else {
                let cwd = script_arg(windows, &mut data, cwd);
                format!("{prefix} workspace create --label {label} --cwd {cwd}")
            }
        }
        HerdrActionKind::CloseWorkspace { workspace_id } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, workspace_id)?;
            format!("{prefix} workspace close {id}")
        }
        HerdrActionKind::RenameWorkspace {
            workspace_id,
            label,
        } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, workspace_id)?;
            let label = name_arg(windows, &mut data, label)?;
            format!("{prefix} workspace rename {id} {label}")
        }
        HerdrActionKind::FocusWorkspace { workspace_id } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, workspace_id)?;
            format!("{prefix} workspace focus {id}")
        }
        HerdrActionKind::CreateTab { workspace_id } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, workspace_id)?;
            format!("{prefix} tab create --workspace {id}")
        }
        HerdrActionKind::FocusTab { tab_id } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, tab_id)?;
            format!("{prefix} tab focus {id}")
        }
        HerdrActionKind::StopSession { name } => {
            // A session-level verb names its session without the socket option,
            // and the name came from the server like every other value here.
            let name = name_arg(windows, &mut data, name)?;
            format!("herdr session stop {name}")
        }
        HerdrActionKind::DeleteSession { name } => {
            let name = name_arg(windows, &mut data, name)?;
            format!("herdr session delete {name}")
        }
        HerdrActionKind::ClosePane { pane_id } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, pane_id)?;
            format!("{prefix} pane close {id}")
        }
        HerdrActionKind::RenamePane { pane_id, label } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, pane_id)?;
            let label = name_arg(windows, &mut data, label)?;
            format!("{prefix} pane rename {id} {label}")
        }
        HerdrActionKind::RenameTab { tab_id, label } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, tab_id)?;
            let label = name_arg(windows, &mut data, label)?;
            format!("{prefix} tab rename {id} {label}")
        }
        HerdrActionKind::CloseTab { tab_id } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, tab_id)?;
            format!("{prefix} tab close {id}")
        }
        HerdrActionKind::SplitPane { pane_id } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, pane_id)?;
            format!("{prefix} pane split {id} --direction right")
        }
        HerdrActionKind::RunPane { pane_id, command } => {
            let prefix = session_prefix(windows, &mut data, session)?;
            let id = object_arg(windows, &mut data, pane_id)?;
            let command = command.0.trim();
            if command.is_empty() {
                return Err("A command is required.".to_string());
            }
            // herdr joins the remaining argv into the command it types into
            // the pane, and it does not consume a `--` separator: passing one
            // makes the pane run `-- <command>`. The command is the only
            // argument after the pane id.
            if windows {
                let command = powershell_bind(&mut data, command);
                format!("{prefix} pane run {id} {command}")
            } else {
                const REDIRECT: &str = " 2>&1";
                let head = format!("{prefix} pane run {id} ");
                // Reserve once: resizing could leave an unwiped copy of the command.
                let mut payload = Zeroizing::new(String::with_capacity(
                    head.len() + shell_quoted_len(command) + REDIRECT.len(),
                ));
                payload.push_str(&head);
                push_shell_quoted(&mut payload, command);
                payload.push_str(REDIRECT);
                return Ok(HerdrActionCommand {
                    command: Zeroizing::new(encoded_posix_shell_command(&payload)),
                });
            }
        }
    };
    Ok(HerdrActionCommand {
        command: Zeroizing::new(if windows {
            windows_encoded_command(&powershell_script(&data, &payload))
        } else {
            encoded_posix_shell_command(&format!("{payload} 2>&1"))
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::{
        HOSTILE_VALUE, assert_inert_windows_line as assert_inert_line,
        decoded_posix_script as posix_script, decoded_powershell_data,
        decoded_windows_script as windows_script,
    };

    fn action(session: &str, kind: HerdrActionKind<'_>) -> String {
        let line = build_herdr_action_command("Linux", session, &kind).unwrap();
        posix_script(&line.command)
    }

    /// The script a line actually runs, whichever shell it was built for.
    fn script_for(os_type: &str, command: &str) -> String {
        if is_windows_os(os_type) {
            windows_script(command)
        } else {
            posix_script(command)
        }
    }

    /// The PowerShell script text after every data binding. The bindings
    /// themselves are asserted as decoded values, so the command can be
    /// asserted without pasting base64 into every expectation.
    fn script_body(script: &str) -> &str {
        const DATA_END: &str = "'))";
        match script.rfind(DATA_END) {
            Some(stop) => &script[stop + DATA_END.len() + 1..],
            None => script,
        }
    }

    #[test]
    fn herdr_run_pane_passes_the_command_without_a_separator() {
        // herdr treats `--` as part of the typed command, so a separator
        // would make the pane run `-- ls`.
        let run = action(
            "default",
            HerdrActionKind::RunPane {
                pane_id: "wA:pC".to_string(),
                command: PaneCommand("ls"),
            },
        );
        assert_eq!(run, "herdr --session 'default' pane run wA:pC 'ls' 2>&1");

        let shell_metacharacters = action(
            "default",
            HerdrActionKind::RunPane {
                pane_id: "wA:pC".to_string(),
                command: PaneCommand("echo hi; rm -rf /"),
            },
        );
        assert_eq!(
            shell_metacharacters,
            "herdr --session 'default' pane run wA:pC 'echo hi; rm -rf /' 2>&1"
        );
    }

    #[test]
    fn herdr_command_builders_validate_ids_and_quote_inputs() {
        let rename = action(
            "default",
            HerdrActionKind::RenamePane {
                pane_id: "wA:pC".to_string(),
                label: "it's mine".to_string(),
            },
        );
        assert_eq!(
            rename,
            "herdr --session 'default' pane rename wA:pC 'it'\"'\"'s mine' 2>&1"
        );

        assert!(
            build_herdr_action_command(
                "Linux",
                "default",
                &HerdrActionKind::ClosePane {
                    pane_id: "wA; rm".to_string()
                }
            )
            .is_err()
        );
        assert!(
            build_herdr_action_command(
                "Linux",
                "default",
                &HerdrActionKind::RunPane {
                    pane_id: "wA:pC".to_string(),
                    command: PaneCommand("   ")
                }
            )
            .is_err()
        );
        assert!(
            build_herdr_action_command(
                "Linux",
                "default",
                &HerdrActionKind::RenamePane {
                    pane_id: "wA:pC".to_string(),
                    label: "line\nbreak".to_string()
                }
            )
            .is_err()
        );

        let attach = build_herdr_attach_command(
            "Linux",
            &HerdrAttachTarget::Agent {
                session: "default".to_string(),
                pane_id: "wA:pC".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            posix_script(&attach),
            "herdr --session 'default' agent attach wA:pC"
        );
        assert!(
            build_herdr_attach_command(
                "Linux",
                &HerdrAttachTarget::Agent {
                    session: "default".to_string(),
                    pane_id: "wA; rm".to_string()
                }
            )
            .is_err()
        );

        // Session names are typed into a terminal but parsed by /bin/sh.
        let session = build_herdr_attach_command(
            "Linux",
            &HerdrAttachTarget::Session {
                name: "work space".to_string(),
            },
        )
        .unwrap();
        assert_eq!(posix_script(&session), "herdr session attach 'work space'");
        let created = build_herdr_attach_command(
            "Linux",
            &HerdrAttachTarget::NewSession {
                name: "my work".to_string(),
            },
        )
        .unwrap();
        assert_eq!(posix_script(&created), "herdr --session 'my work'");
        assert_eq!(
            build_herdr_attach_command("Linux", &HerdrAttachTarget::Default).unwrap(),
            "herdr"
        );
        assert!(
            build_herdr_attach_command(
                "Linux",
                &HerdrAttachTarget::Session {
                    name: "line\nbreak".to_string()
                }
            )
            .is_err()
        );
        // A padded name addresses a different row than the one displayed, so
        // it is refused instead of quietly trimmed.
        assert!(
            build_herdr_attach_command(
                "Linux",
                &HerdrAttachTarget::Session {
                    name: " work".to_string()
                }
            )
            .is_err()
        );

        let stop = action(
            "default",
            HerdrActionKind::StopSession {
                name: "work space".to_string(),
            },
        );
        assert_eq!(stop, "herdr session stop 'work space' 2>&1");
        let delete = action(
            "default",
            HerdrActionKind::DeleteSession {
                name: "work space".to_string(),
            },
        );
        assert_eq!(delete, "herdr session delete 'work space' 2>&1");

        let tab_rename = action(
            "default",
            HerdrActionKind::RenameTab {
                tab_id: "wA:tB".to_string(),
                label: "it's mine".to_string(),
            },
        );
        assert_eq!(
            tab_rename,
            "herdr --session 'default' tab rename wA:tB 'it'\"'\"'s mine' 2>&1"
        );
        let tab_close = action(
            "default",
            HerdrActionKind::CloseTab {
                tab_id: "wA:tB".to_string(),
            },
        );
        assert_eq!(tab_close, "herdr --session 'default' tab close wA:tB 2>&1");
        assert!(
            build_herdr_action_command(
                "Linux",
                "default",
                &HerdrActionKind::CloseTab {
                    tab_id: "wA; rm".to_string()
                }
            )
            .is_err()
        );
        assert!(
            build_herdr_action_command(
                "Linux",
                "default",
                &HerdrActionKind::StopSession {
                    name: "line\nbreak".to_string()
                }
            )
            .is_err()
        );

        let session_scoped = action(
            "work space",
            HerdrActionKind::CloseWorkspace {
                workspace_id: "wA".to_string(),
            },
        );
        assert_eq!(
            session_scoped,
            "herdr --session 'work space' workspace close wA 2>&1"
        );

        let read = build_herdr_read_command("Linux", "default", "wA:pC").unwrap();
        assert_eq!(
            posix_script(&read.command),
            concat!(
                "herdr --session 'default' pane read wA:pC --lines 500 ",
                "--source recent --format text 2>&1"
            )
        );
        assert!(build_herdr_read_command("Linux", "default", "bad id").is_err());
        assert!(build_herdr_read_command("Linux", "default", "wA:pC ").is_err());
        assert!(build_herdr_read_command("Linux", " default", "wA:pC").is_err());
    }

    #[test]
    fn herdr_windows_read_line_addresses_session_and_pane_as_data() {
        let line = build_herdr_read_command("Windows", "dev", "wA:pC")
            .unwrap()
            .command;
        assert_inert_line(&line);
        // The base64 literals are "dev" and "wA:pC", encoded outside this file.
        assert_eq!(
            windows_script(&line),
            concat!(
                "$v0=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('ZGV2'));",
                "$v1=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('d0E6cEM='));",
                "herdr --session $v0 pane read $v1 --lines 500 --source recent --format text 2>&1"
            )
        );
    }

    #[test]
    fn herdr_windows_attach_names_the_session_as_data() {
        let line = build_herdr_attach_command(
            "Windows",
            &HerdrAttachTarget::Session {
                name: HOSTILE_VALUE.to_string(),
            },
        )
        .unwrap();
        assert_inert_line(&line);
        let script = windows_script(&line);
        assert_eq!(decoded_powershell_data(&script), [HOSTILE_VALUE]);
        assert_eq!(
            script_body(&script),
            concat!(
                "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); ",
                "& herdr session attach $v0; exit $LASTEXITCODE"
            )
        );

        let agent = build_herdr_attach_command(
            "Windows",
            &HerdrAttachTarget::Agent {
                session: HOSTILE_VALUE.to_string(),
                pane_id: "wA:pC".to_string(),
            },
        )
        .unwrap();
        assert_inert_line(&agent);
        let script = windows_script(&agent);
        assert_eq!(decoded_powershell_data(&script), [HOSTILE_VALUE, "wA:pC"]);
        assert_eq!(
            script_body(&script),
            concat!(
                "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); ",
                "& herdr --session $v0 agent attach $v1; exit $LASTEXITCODE"
            )
        );
    }

    /// Every dynamic value of a Windows action is one base64 binding, whatever
    /// it contains, and the script text is only verbs plus `$vN` references.
    #[test]
    fn herdr_windows_action_values_stay_data() {
        let rename = build_herdr_action_command(
            "Windows",
            HOSTILE_VALUE,
            &HerdrActionKind::RenameWorkspace {
                workspace_id: "wA".to_string(),
                label: HOSTILE_VALUE.to_string(),
            },
        )
        .unwrap();
        assert_inert_line(&rename.command);
        let script = windows_script(&rename.command);
        // Session socket, workspace, label: bound in command-line order.
        assert_eq!(
            decoded_powershell_data(&script),
            [HOSTILE_VALUE, "wA", HOSTILE_VALUE]
        );
        assert_eq!(
            script_body(&script),
            "herdr --session $v0 workspace rename $v1 $v2"
        );

        let create = build_herdr_action_command(
            "Windows",
            "dev",
            &HerdrActionKind::NewWorkspace {
                label: HOSTILE_VALUE.to_string(),
                cwd: HOSTILE_VALUE.to_string(),
            },
        )
        .unwrap();
        assert_inert_line(&create.command);
        let script = windows_script(&create.command);
        assert_eq!(
            decoded_powershell_data(&script),
            ["dev", HOSTILE_VALUE, HOSTILE_VALUE]
        );
        assert_eq!(
            script_body(&script),
            "herdr --session $v0 workspace create --label $v1 --cwd $v2"
        );

        let run = build_herdr_action_command(
            "Windows",
            "dev",
            &HerdrActionKind::RunPane {
                pane_id: "wA:pC".to_string(),
                command: PaneCommand(HOSTILE_VALUE),
            },
        )
        .unwrap();
        assert_inert_line(&run.command);
        let script = windows_script(&run.command);
        assert_eq!(
            decoded_powershell_data(&script),
            ["dev", "wA:pC", HOSTILE_VALUE]
        );
        assert_eq!(script_body(&script), "herdr --session $v0 pane run $v1 $v2");

        let stop = build_herdr_action_command(
            "Windows",
            "dev",
            &HerdrActionKind::StopSession {
                name: HOSTILE_VALUE.to_string(),
            },
        )
        .unwrap();
        assert_inert_line(&stop.command);
        let script = windows_script(&stop.command);
        assert_eq!(decoded_powershell_data(&script), [HOSTILE_VALUE]);
        assert_eq!(script_body(&script), "herdr session stop $v0");

        // No quoting branch is left to get wrong: the script text after the
        // bindings carries no quote character at all.
        for command in [&rename, &create, &run, &stop] {
            let script = windows_script(&command.command);
            assert!(!script_body(&script).contains('\''), "{script}");
        }
    }

    #[test]
    fn herdr_snapshot_command_lists_sessions_only() {
        for os_type in ["Linux", "Windows"] {
            let line = build_herdr_snapshot_command(os_type).command;
            let command = script_for(os_type, &line);
            assert!(command.contains("===HERDR_SESSIONS==="), "{os_type}");
            assert!(command.contains("session list --json"), "{os_type}");
            // Workspaces and panes need a session socket, so they are fetched
            // per session instead of in the snapshot.
            assert!(!command.contains("workspace list"), "{os_type}");
            assert!(!command.contains("pane list"), "{os_type}");
        }
    }

    #[test]
    fn herdr_session_detail_command_targets_one_session() {
        let posix = build_herdr_session_detail_command("Linux", "dev")
            .unwrap()
            .command;
        assert!(posix.starts_with("/bin/sh -c '"), "{posix}");
        assert_eq!(
            posix_script(&posix),
            concat!(
                "echo '===HERDR==='; if command -v herdr >/dev/null 2>&1; then ",
                "echo '===HERDR_WORKSPACES==='; herdr --session 'dev' workspace list 2>/dev/null; ",
                "echo '===HERDR_TABS==='; herdr --session 'dev' tab list 2>/dev/null; ",
                "echo '===HERDR_PANES==='; herdr --session 'dev' pane list 2>/dev/null; ",
                "echo '===HERDR_AGENTS==='; herdr --session 'dev' agent list 2>/dev/null; ",
                "else echo '__OXIDE_HERDR_UNAVAILABLE__'; fi; echo '===HERDR_END==='"
            )
        );

        let windows = build_herdr_session_detail_command("Windows", "dev")
            .unwrap()
            .command;
        assert_inert_line(&windows);
        assert_eq!(
            windows_script(&windows),
            concat!(
                "$v0=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('ZGV2'));",
                "Write-Output '===HERDR===';",
                "if(Get-Command herdr -ErrorAction SilentlyContinue){",
                "Write-Output '===HERDR_WORKSPACES===';",
                "herdr --session $v0 workspace list 2>$null;",
                "Write-Output '===HERDR_TABS===';",
                "herdr --session $v0 tab list 2>$null;",
                "Write-Output '===HERDR_PANES===';",
                "herdr --session $v0 pane list 2>$null;",
                "Write-Output '===HERDR_AGENTS===';",
                "herdr --session $v0 agent list 2>$null;",
                "}else{Write-Output '__OXIDE_HERDR_UNAVAILABLE__'};",
                "Write-Output '===HERDR_END===';"
            )
        );

        // A padded name would address another row than the one displayed.
        assert!(build_herdr_session_detail_command("Linux", "line\nbreak").is_err());
        assert!(build_herdr_session_detail_command("Windows", " dev").is_err());
    }

    #[test]
    fn herdr_navigation_verbs_address_exactly_one_object() {
        assert_eq!(
            action(
                "work space",
                HerdrActionKind::FocusWorkspace {
                    workspace_id: "wA".to_string()
                }
            ),
            "herdr --session 'work space' workspace focus wA 2>&1"
        );
        assert_eq!(
            action(
                "dev",
                HerdrActionKind::CreateTab {
                    workspace_id: "wA".to_string()
                }
            ),
            "herdr --session 'dev' tab create --workspace wA 2>&1"
        );
        assert_eq!(
            action(
                "dev",
                HerdrActionKind::FocusTab {
                    tab_id: "wA:tB".to_string()
                }
            ),
            "herdr --session 'dev' tab focus wA:tB 2>&1"
        );
        assert_eq!(
            action(
                "dev",
                HerdrActionKind::SplitPane {
                    pane_id: "wA:pC".to_string()
                }
            ),
            "herdr --session 'dev' pane split wA:pC --direction right 2>&1"
        );

        // Host-authored ids cannot smuggle a second verb or an option.
        for kind in [
            HerdrActionKind::FocusWorkspace {
                workspace_id: "wA; shutdown".to_string(),
            },
            HerdrActionKind::CreateTab {
                workspace_id: "wA --force".to_string(),
            },
            HerdrActionKind::FocusTab {
                tab_id: "wA tB".to_string(),
            },
            HerdrActionKind::SplitPane {
                pane_id: "wA:pC\nrm".to_string(),
            },
        ] {
            assert!(
                build_herdr_action_command("Linux", "dev", &kind).is_err(),
                "{kind:?}"
            );
        }
    }
}
