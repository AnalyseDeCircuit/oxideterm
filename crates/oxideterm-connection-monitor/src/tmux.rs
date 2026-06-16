use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTmuxSession {
    pub id: String,
    pub name: String,
    pub windows: usize,
    pub attached: bool,
    pub created: String,
    pub activity: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTmuxWindow {
    pub session_id: String,
    pub id: String,
    pub index: usize,
    pub name: String,
    pub active: bool,
    pub panes: usize,
    pub layout: String,
    pub activity: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTmuxPane {
    pub session_id: String,
    pub window_id: String,
    pub id: String,
    pub index: usize,
    pub command: String,
    pub path: String,
    pub active: bool,
    pub pid: String,
    pub size: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TmuxCommandCapability {
    #[default]
    Unknown,
    Full,
    Partial,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceTmuxStatus {
    #[default]
    Unknown,
    Available {
        capability: TmuxCommandCapability,
        platform: String,
        version: String,
    },
    Unavailable,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTmuxSnapshot {
    pub status: ResourceTmuxStatus,
    pub sessions: Vec<ResourceTmuxSession>,
    pub windows: Vec<ResourceTmuxWindow>,
    pub panes: Vec<ResourceTmuxPane>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TmuxActionKind {
    KillSession { target: String },
    KillWindow { target: String },
    KillPane { target: String },
    RenameSession { target: String, name: String },
    RenameWindow { target: String, name: String },
    SendPaneCommand { target: String, command: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxActionCommand {
    pub command: String,
    pub capability: TmuxCommandCapability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxCaptureCommand {
    pub command: String,
    pub capability: TmuxCommandCapability,
}

const TMUX_UNAVAILABLE_MARKER: &str = "__OXIDE_TMUX_UNAVAILABLE__";
const TMUX_ERROR_MARKER: &str = "__OXIDE_TMUX_ERROR__";
const TMUX_CAPABILITY_MARKER: &str = "__OXIDE_TMUX_CAPABILITY__";
const TMUX_FIELD_SEPARATOR: char = '\u{1f}';

const TMUX_SESSION_FORMAT: &str = concat!(
    "SESSION\u{1f}",
    "#{session_id}\u{1f}",
    "#{session_name}\u{1f}",
    "#{session_windows}\u{1f}",
    "#{session_attached}\u{1f}",
    "#{session_created}\u{1f}",
    "#{session_activity}"
);

const TMUX_WINDOW_FORMAT: &str = concat!(
    "WINDOW\u{1f}",
    "#{session_id}\u{1f}",
    "#{window_id}\u{1f}",
    "#{window_index}\u{1f}",
    "#{window_name}\u{1f}",
    "#{window_active}\u{1f}",
    "#{window_panes}\u{1f}",
    "#{window_layout}\u{1f}",
    "#{window_activity}"
);

const TMUX_PANE_FORMAT: &str = concat!(
    "PANE\u{1f}",
    "#{session_id}\u{1f}",
    "#{window_id}\u{1f}",
    "#{pane_id}\u{1f}",
    "#{pane_index}\u{1f}",
    "#{pane_current_command}\u{1f}",
    "#{pane_current_path}\u{1f}",
    "#{pane_active}\u{1f}",
    "#{pane_pid}\u{1f}",
    "#{pane_width}x#{pane_height}"
);

pub fn build_tmux_snapshot_command(os_type: &str) -> TmuxCaptureCommand {
    let command = if is_windows_os(os_type) {
        build_windows_tmux_snapshot_command()
    } else {
        build_unix_tmux_snapshot_command()
    };
    TmuxCaptureCommand {
        command,
        capability: TmuxCommandCapability::Unknown,
    }
}

pub fn build_tmux_action_command(
    os_type: &str,
    action: TmuxActionKind,
) -> Result<TmuxActionCommand, String> {
    let command = match action {
        TmuxActionKind::KillSession { target } => {
            let target = validated_tmux_target(&target, "tmux session")?;
            if is_windows_os(os_type) {
                format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"tmux kill-session -t {}\"",
                    powershell_quote(&target)
                )
            } else {
                format!("tmux kill-session -t {}", shell_quote(&target))
            }
        }
        TmuxActionKind::KillWindow { target } => {
            let target = validated_tmux_target(&target, "tmux window")?;
            if is_windows_os(os_type) {
                format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"tmux kill-window -t {}\"",
                    powershell_quote(&target)
                )
            } else {
                format!("tmux kill-window -t {}", shell_quote(&target))
            }
        }
        TmuxActionKind::KillPane { target } => {
            let target = validated_tmux_target(&target, "tmux pane")?;
            if is_windows_os(os_type) {
                format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"tmux kill-pane -t {}\"",
                    powershell_quote(&target)
                )
            } else {
                format!("tmux kill-pane -t {}", shell_quote(&target))
            }
        }
        TmuxActionKind::RenameSession { target, name } => {
            let target = validated_tmux_target(&target, "tmux session")?;
            let name = validated_tmux_name(&name, "tmux session name")?;
            if is_windows_os(os_type) {
                format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"tmux rename-session -t {} {}\"",
                    powershell_quote(&target),
                    powershell_quote(&name)
                )
            } else {
                format!(
                    "tmux rename-session -t {} {}",
                    shell_quote(&target),
                    shell_quote(&name)
                )
            }
        }
        TmuxActionKind::RenameWindow { target, name } => {
            let target = validated_tmux_target(&target, "tmux window")?;
            let name = validated_tmux_name(&name, "tmux window name")?;
            if is_windows_os(os_type) {
                format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"tmux rename-window -t {} {}\"",
                    powershell_quote(&target),
                    powershell_quote(&name)
                )
            } else {
                format!(
                    "tmux rename-window -t {} {}",
                    shell_quote(&target),
                    shell_quote(&name)
                )
            }
        }
        TmuxActionKind::SendPaneCommand { target, command } => {
            let target = validated_tmux_target(&target, "tmux pane")?;
            let command = validated_tmux_send_command(&command)?;
            if is_windows_os(os_type) {
                format!(
                    concat!(
                        "powershell -NoProfile -ExecutionPolicy Bypass -Command \"",
                        "tmux send-keys -t {} -l -- {}; ",
                        "if($LASTEXITCODE -eq 0){{tmux send-keys -t {} Enter}}",
                        "\""
                    ),
                    powershell_quote(&target),
                    powershell_quote(&command),
                    powershell_quote(&target)
                )
            } else {
                format!(
                    "tmux send-keys -t {} -l -- {} && tmux send-keys -t {} Enter",
                    shell_quote(&target),
                    shell_quote(&command),
                    shell_quote(&target)
                )
            }
        }
    };
    Ok(TmuxActionCommand {
        command,
        capability: TmuxCommandCapability::Unknown,
    })
}

pub fn build_tmux_attach_command(os_type: &str, target: &str) -> Result<String, String> {
    let target = validated_tmux_target(target, "tmux session")?;
    if is_windows_os(os_type) {
        Ok(format!(
            "tmux attach-session -t {}",
            powershell_quote(&target)
        ))
    } else {
        Ok(format!("tmux attach-session -t {}", shell_quote(&target)))
    }
}

pub fn build_tmux_new_session_command(os_type: &str, name: Option<&str>) -> Result<String, String> {
    let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) else {
        return Ok("tmux new-session".to_string());
    };
    let name = validated_tmux_target(name, "tmux session name")?;
    if is_windows_os(os_type) {
        Ok(format!(
            "tmux new-session -A -s {}",
            powershell_quote(&name)
        ))
    } else {
        Ok(format!("tmux new-session -A -s {}", shell_quote(&name)))
    }
}

pub fn parse_tmux_snapshot(output: &str) -> ResourceTmuxSnapshot {
    let Some(section) = extract_section(output, "TMUX") else {
        return ResourceTmuxSnapshot::default();
    };

    let mut sessions = Vec::new();
    let mut windows = Vec::new();
    let mut panes = Vec::new();
    let mut capability = TmuxCommandCapability::Unknown;
    let mut platform = "unknown".to_string();
    let mut version = String::new();

    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line == TMUX_UNAVAILABLE_MARKER {
            return ResourceTmuxSnapshot {
                status: ResourceTmuxStatus::Unavailable,
                sessions: Vec::new(),
                windows: Vec::new(),
                panes: Vec::new(),
            };
        }
        if let Some(message) = line.strip_prefix(TMUX_ERROR_MARKER) {
            return ResourceTmuxSnapshot {
                status: ResourceTmuxStatus::Error {
                    message: clean_marker_message(message, "tmux command failed."),
                },
                sessions: Vec::new(),
                windows: Vec::new(),
                panes: Vec::new(),
            };
        }
        if let Some((next_capability, next_platform, next_version)) =
            parse_tmux_capability_line(line)
        {
            capability = next_capability;
            platform = next_platform;
            version = next_version;
            continue;
        }
        if line.starts_with("SESSION") {
            match parse_tmux_session_line(line) {
                Some(session) => sessions.push(session),
                None => return malformed_tmux_snapshot(),
            }
            continue;
        }
        if line.starts_with("WINDOW") {
            match parse_tmux_window_line(line) {
                Some(window) => windows.push(window),
                None => return malformed_tmux_snapshot(),
            }
            continue;
        }
        if line.starts_with("PANE") {
            match parse_tmux_pane_line(line) {
                Some(pane) => panes.push(pane),
                None => return malformed_tmux_snapshot(),
            }
        }
    }

    sessions.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    windows.sort_by(|left, right| {
        left.session_id
            .cmp(&right.session_id)
            .then(left.index.cmp(&right.index))
    });
    panes.sort_by(|left, right| {
        left.session_id
            .cmp(&right.session_id)
            .then(left.window_id.cmp(&right.window_id))
            .then(left.index.cmp(&right.index))
    });

    ResourceTmuxSnapshot {
        status: ResourceTmuxStatus::Available {
            capability,
            platform,
            version,
        },
        sessions,
        windows,
        panes,
    }
}

pub fn visible_tmux_session_rows(
    snapshot: &ResourceTmuxSnapshot,
    query: &str,
) -> Vec<ResourceTmuxSession> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return snapshot.sessions.clone();
    }
    snapshot
        .sessions
        .iter()
        .filter(|session| tmux_session_matches_query(snapshot, session, &query))
        .cloned()
        .collect()
}

pub fn tmux_session_row_signature(session: &ResourceTmuxSession) -> u64 {
    let mut hasher = DefaultHasher::new();
    session.id.hash(&mut hasher);
    session.name.hash(&mut hasher);
    session.windows.hash(&mut hasher);
    session.attached.hash(&mut hasher);
    session.created.hash(&mut hasher);
    session.activity.hash(&mut hasher);
    hasher.finish()
}

pub fn tmux_action_succeeded(exit_code: Option<i32>) -> bool {
    exit_code.unwrap_or(0) == 0
}

pub fn tmux_action_success_message(stdout: &str, stderr: &str) -> String {
    stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("tmux action completed.")
        .to_string()
}

pub fn tmux_action_failure_message(stdout: &str, stderr: &str, exit_code: Option<i32>) -> String {
    let reason = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("tmux action failed.");
    match exit_code {
        Some(code) => format!("{reason} (exit {code})"),
        None => reason.to_string(),
    }
}

fn build_unix_tmux_snapshot_command() -> String {
    format!(
        concat!(
            "echo '===TMUX==='; ",
            "if command -v tmux >/dev/null 2>&1; then ",
            "oxide_tmux_version=$(tmux -V 2>/dev/null | head -n 1 | tr '\\t' ' '); ",
            "printf '__OXIDE_TMUX_CAPABILITY__\\tfull\\ttmux_cli\\t%s\\n' \"$oxide_tmux_version\"; ",
            "oxide_tmux_sessions=$(tmux list-sessions -F {session_format} 2>&1); ",
            "oxide_tmux_status=$?; ",
            "if [ \"$oxide_tmux_status\" -ne 0 ]; then ",
            "if printf '%s' \"$oxide_tmux_sessions\" | grep -qi 'no server running'; then :; ",
            "else printf '__OXIDE_TMUX_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_tmux_sessions\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
            "else ",
            "printf '%s\\n' \"$oxide_tmux_sessions\"; ",
            "oxide_tmux_windows=$(tmux list-windows -a -F {window_format} 2>&1); ",
            "oxide_tmux_window_status=$?; ",
            "if [ \"$oxide_tmux_window_status\" -eq 0 ]; then printf '%s\\n' \"$oxide_tmux_windows\"; ",
            "else printf '__OXIDE_TMUX_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_tmux_windows\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
            "oxide_tmux_panes=$(tmux list-panes -a -F {pane_format} 2>&1); ",
            "oxide_tmux_pane_status=$?; ",
            "if [ \"$oxide_tmux_pane_status\" -eq 0 ]; then printf '%s\\n' \"$oxide_tmux_panes\"; ",
            "else printf '__OXIDE_TMUX_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_tmux_panes\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
            "fi; ",
            "else echo '__OXIDE_TMUX_UNAVAILABLE__'; fi; ",
            "echo '===TMUX_END==='"
        ),
        session_format = shell_quote(TMUX_SESSION_FORMAT),
        window_format = shell_quote(TMUX_WINDOW_FORMAT),
        pane_format = shell_quote(TMUX_PANE_FORMAT),
    )
}

fn build_windows_tmux_snapshot_command() -> String {
    let script = concat!(
        "Write-Output '===TMUX===';",
        "if(Get-Command tmux -ErrorAction SilentlyContinue){",
        "$version=(& tmux -V 2>$null|Select-Object -First 1);",
        "Write-Output ('__OXIDE_TMUX_CAPABILITY__'+[char]9+'unknown'+[char]9+'windows_tmux'+[char]9+$version);",
        "$d=[char]31;",
        "$sf='SESSION'+$d+'#{session_id}'+$d+'#{session_name}'+$d+'#{session_windows}'+$d+'#{session_attached}'+$d+'#{session_created}'+$d+'#{session_activity}';",
        "$wf='WINDOW'+$d+'#{session_id}'+$d+'#{window_id}'+$d+'#{window_index}'+$d+'#{window_name}'+$d+'#{window_active}'+$d+'#{window_panes}'+$d+'#{window_layout}'+$d+'#{window_activity}';",
        "$pf='PANE'+$d+'#{session_id}'+$d+'#{window_id}'+$d+'#{pane_id}'+$d+'#{pane_index}'+$d+'#{pane_current_command}'+$d+'#{pane_current_path}'+$d+'#{pane_active}'+$d+'#{pane_pid}'+$d+'#{pane_width}x#{pane_height}';",
        "$sessions=& tmux list-sessions -F $sf 2>&1;",
        "if($LASTEXITCODE -ne 0){",
        "$msg=($sessions|Select-Object -First 1);",
        "if(([string]$msg) -notmatch 'no server running'){Write-Output ('__OXIDE_TMUX_ERROR__'+[char]9+$msg)}",
        "}else{",
        "$sessions|ForEach-Object{Write-Output $_};",
        "$windows=& tmux list-windows -a -F $wf 2>&1;",
        "if($LASTEXITCODE -eq 0){$windows|ForEach-Object{Write-Output $_}}else{Write-Output ('__OXIDE_TMUX_ERROR__'+[char]9+($windows|Select-Object -First 1))};",
        "$panes=& tmux list-panes -a -F $pf 2>&1;",
        "if($LASTEXITCODE -eq 0){$panes|ForEach-Object{Write-Output $_}}else{Write-Output ('__OXIDE_TMUX_ERROR__'+[char]9+($panes|Select-Object -First 1))};",
        "}",
        "}else{Write-Output '__OXIDE_TMUX_UNAVAILABLE__'};",
        "Write-Output '===TMUX_END===';"
    );
    format!("powershell -NoProfile -ExecutionPolicy Bypass -Command \"{script}\"")
}

fn parse_tmux_capability_line(line: &str) -> Option<(TmuxCommandCapability, String, String)> {
    let payload = line.strip_prefix(TMUX_CAPABILITY_MARKER)?;
    let parts = payload
        .trim_start_matches('\t')
        .split('\t')
        .collect::<Vec<_>>();
    let capability = match parts.first().copied().unwrap_or("unknown") {
        "full" => TmuxCommandCapability::Full,
        "partial" => TmuxCommandCapability::Partial,
        _ => TmuxCommandCapability::Unknown,
    };
    Some((
        capability,
        parts
            .get(1)
            .copied()
            .unwrap_or("unknown")
            .trim()
            .to_string(),
        parts.get(2).copied().unwrap_or_default().trim().to_string(),
    ))
}

fn parse_tmux_session_line(line: &str) -> Option<ResourceTmuxSession> {
    let payload = line
        .strip_prefix("SESSION")?
        .strip_prefix(TMUX_FIELD_SEPARATOR)?;
    let parts = split_tmux_fields(payload, 6)?;
    if parts.len() != 6 {
        return None;
    }
    Some(ResourceTmuxSession {
        id: clean_field(parts[0]),
        name: clean_field(parts[1]),
        windows: parse_usize(parts[2]),
        attached: parse_bool(parts[3]),
        created: clean_field(parts[4]),
        activity: clean_field(parts[5]),
    })
}

fn parse_tmux_window_line(line: &str) -> Option<ResourceTmuxWindow> {
    let payload = line
        .strip_prefix("WINDOW")?
        .strip_prefix(TMUX_FIELD_SEPARATOR)?;
    let parts = split_tmux_fields(payload, 8)?;
    if parts.len() != 8 {
        return None;
    }
    Some(ResourceTmuxWindow {
        session_id: clean_field(parts[0]),
        id: clean_field(parts[1]),
        index: parse_usize(parts[2]),
        name: clean_field(parts[3]),
        active: parse_bool(parts[4]),
        panes: parse_usize(parts[5]),
        layout: clean_field(parts[6]),
        activity: clean_field(parts[7]),
    })
}

fn parse_tmux_pane_line(line: &str) -> Option<ResourceTmuxPane> {
    let payload = line
        .strip_prefix("PANE")?
        .strip_prefix(TMUX_FIELD_SEPARATOR)?;
    let parts = split_tmux_fields(payload, 9)?;
    if parts.len() != 9 {
        return None;
    }
    Some(ResourceTmuxPane {
        session_id: clean_field(parts[0]),
        window_id: clean_field(parts[1]),
        id: clean_field(parts[2]),
        index: parse_usize(parts[3]),
        command: clean_field(parts[4]),
        path: clean_field(parts[5]),
        active: parse_bool(parts[6]),
        pid: clean_field(parts[7]),
        size: clean_field(parts[8]),
    })
}

fn malformed_tmux_snapshot() -> ResourceTmuxSnapshot {
    ResourceTmuxSnapshot {
        status: ResourceTmuxStatus::Error {
            message: "tmux output did not include the required fields.".to_string(),
        },
        sessions: Vec::new(),
        windows: Vec::new(),
        panes: Vec::new(),
    }
}

fn tmux_session_matches_query(
    snapshot: &ResourceTmuxSnapshot,
    session: &ResourceTmuxSession,
    query: &str,
) -> bool {
    session.id.to_lowercase().contains(query)
        || session.name.to_lowercase().contains(query)
        || snapshot
            .windows
            .iter()
            .filter(|window| window.session_id == session.id)
            .any(|window| {
                window.id.to_lowercase().contains(query)
                    || window.name.to_lowercase().contains(query)
                    || window.layout.to_lowercase().contains(query)
            })
        || snapshot
            .panes
            .iter()
            .filter(|pane| pane.session_id == session.id)
            .any(|pane| {
                pane.id.to_lowercase().contains(query)
                    || pane.command.to_lowercase().contains(query)
                    || pane.path.to_lowercase().contains(query)
                    || pane.pid.to_lowercase().contains(query)
            })
}

fn split_tmux_fields(payload: &str, expected: usize) -> Option<Vec<&str>> {
    // tmux fields are separated by ASCII Unit Separator so names and commands do not fight tabs.
    let parts = payload.split(TMUX_FIELD_SEPARATOR).collect::<Vec<_>>();
    (parts.len() == expected).then_some(parts)
}

fn validated_tmux_target(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} cannot be empty."));
    }
    if trimmed.len() > 256 {
        return Err(format!("{label} is too long."));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(format!("{label} contains unsupported control characters."));
    }
    Ok(trimmed.to_string())
}

fn validated_tmux_name(value: &str, label: &str) -> Result<String, String> {
    let trimmed = validated_tmux_target(value, label)?;
    if trimmed.contains(TMUX_FIELD_SEPARATOR) {
        return Err(format!(
            "{label} contains unsupported separator characters."
        ));
    }
    Ok(trimmed)
}

fn validated_tmux_send_command(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("tmux command cannot be empty.".to_string());
    }
    if trimmed.len() > 4096 {
        return Err("tmux command is too long.".to_string());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("tmux command must be a single printable line.".to_string());
    }
    Ok(trimmed.to_string())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn is_windows_os(os_type: &str) -> bool {
    matches!(os_type, "Windows" | "windows")
}

fn clean_field(value: &str) -> String {
    value.trim().to_string()
}

fn clean_marker_message(message: &str, fallback: &str) -> String {
    let cleaned = message.trim_start_matches('\t').trim();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn parse_usize(value: &str) -> usize {
    value.trim().parse::<usize>().unwrap_or_default()
}

fn extract_section<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("==={name}===");
    let end = format!("==={name}_END===");
    let after_start = output.split_once(&start)?.1;
    Some(
        after_start
            .split_once(&end)
            .map_or(after_start, |(section, _)| section),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmux_row(kind: &str, fields: &[&str]) -> String {
        let separator = TMUX_FIELD_SEPARATOR.to_string();
        format!("{kind}{separator}{}", fields.join(&separator))
    }

    #[test]
    fn parses_tmux_sessions_windows_and_panes() {
        let output = format!(
            "===TMUX===\n__OXIDE_TMUX_CAPABILITY__\tfull\ttmux_cli\ttmux 3.4\n{}\n{}\n{}\n{}\n===TMUX_END===\n",
            tmux_row(
                "SESSION",
                &["$1", "work\t中文", "2", "1", "1713990000", "1713990300",]
            ),
            tmux_row(
                "WINDOW",
                &[
                    "$1",
                    "@1",
                    "0",
                    "editor",
                    "1",
                    "2",
                    "layout text",
                    "1713990300",
                ]
            ),
            tmux_row(
                "PANE",
                &[
                    "$1",
                    "@1",
                    "%1",
                    "0",
                    "nvim",
                    "/home/me/project",
                    "1",
                    "1234",
                    "120x30",
                ]
            ),
            tmux_row(
                "PANE",
                &[
                    "$1",
                    "@1",
                    "%2",
                    "1",
                    "bash",
                    "/home/me/project",
                    "0",
                    "1235",
                    "80x30",
                ]
            )
        );

        let snapshot = parse_tmux_snapshot(&output);
        let rows = visible_tmux_session_rows(&snapshot, "project");

        assert_eq!(
            snapshot.status,
            ResourceTmuxStatus::Available {
                capability: TmuxCommandCapability::Full,
                platform: "tmux_cli".to_string(),
                version: "tmux 3.4".to_string(),
            }
        );
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].name, "work\t中文");
        assert_eq!(snapshot.windows[0].name, "editor");
        assert_eq!(snapshot.panes[0].command, "nvim");
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn parser_rejects_tab_delimited_rows_before_ui_layout_gets_blamed() {
        let output = concat!(
            "===TMUX===\n",
            "__OXIDE_TMUX_CAPABILITY__\tfull\ttmux_cli\ttmux 3.4\n",
            "SESSION\t$1\tlegacy\t1\t0\t1713990000\t1713990300\n",
            "===TMUX_END===\n"
        );

        let snapshot = parse_tmux_snapshot(&output);

        assert!(matches!(snapshot.status, ResourceTmuxStatus::Error { .. }));
    }

    #[test]
    fn no_server_running_is_available_empty() {
        let output = concat!(
            "===TMUX===\n",
            "__OXIDE_TMUX_CAPABILITY__\tfull\ttmux_cli\ttmux 3.4\n",
            "===TMUX_END===\n"
        );

        let snapshot = parse_tmux_snapshot(&output);

        assert!(matches!(
            snapshot.status,
            ResourceTmuxStatus::Available { .. }
        ));
        assert!(snapshot.sessions.is_empty());
    }

    #[test]
    fn tmux_not_installed_is_unavailable() {
        let output = concat!(
            "===TMUX===\n",
            "__OXIDE_TMUX_UNAVAILABLE__\n",
            "===TMUX_END===\n"
        );

        let snapshot = parse_tmux_snapshot(&output);

        assert_eq!(snapshot.status, ResourceTmuxStatus::Unavailable);
    }

    #[test]
    fn malformed_old_format_is_error() {
        let output = format!(
            "===TMUX===\n__OXIDE_TMUX_CAPABILITY__\tpartial\ttmux_cli\ttmux 1.8\n{}\n===TMUX_END===\n",
            tmux_row("SESSION", &["$1", "missing-fields"])
        );

        let snapshot = parse_tmux_snapshot(&output);

        assert!(matches!(snapshot.status, ResourceTmuxStatus::Error { .. }));
    }

    #[test]
    fn tmux_commands_validate_and_quote_targets() {
        let attach = build_tmux_attach_command("Linux", "$1; rm -rf /").unwrap();
        let windows = build_tmux_action_command(
            "Windows",
            TmuxActionKind::KillSession {
                target: "work's".to_string(),
            },
        )
        .unwrap();
        let invalid = build_tmux_new_session_command("Linux", Some("bad\nname"));

        assert_eq!(attach, "tmux attach-session -t '$1; rm -rf /'");
        assert!(windows.command.contains("'work''s'"));
        assert!(invalid.is_err());
    }

    #[test]
    fn tmux_window_pane_and_mutation_commands_are_quoted() {
        let kill_window = build_tmux_action_command(
            "Linux",
            TmuxActionKind::KillWindow {
                target: "@1; rm -rf /".to_string(),
            },
        )
        .unwrap();
        let kill_pane = build_tmux_action_command(
            "Linux",
            TmuxActionKind::KillPane {
                target: "%2".to_string(),
            },
        )
        .unwrap();
        let rename = build_tmux_action_command(
            "Linux",
            TmuxActionKind::RenameWindow {
                target: "@1".to_string(),
                name: "build's logs".to_string(),
            },
        )
        .unwrap();
        let send = build_tmux_action_command(
            "Linux",
            TmuxActionKind::SendPaneCommand {
                target: "%3".to_string(),
                command: "echo 'safe' && pwd".to_string(),
            },
        )
        .unwrap();
        let invalid_send = build_tmux_action_command(
            "Linux",
            TmuxActionKind::SendPaneCommand {
                target: "%3".to_string(),
                command: "echo safe\nrm -rf /".to_string(),
            },
        );

        assert_eq!(kill_window.command, "tmux kill-window -t '@1; rm -rf /'");
        assert_eq!(kill_pane.command, "tmux kill-pane -t '%2'");
        assert_eq!(
            rename.command,
            "tmux rename-window -t '@1' 'build'\\''s logs'"
        );
        assert_eq!(
            send.command,
            "tmux send-keys -t '%3' -l -- 'echo '\\''safe'\\'' && pwd' && tmux send-keys -t '%3' Enter"
        );
        assert!(invalid_send.is_err());
    }

    #[test]
    fn tmux_windows_mutations_keep_powershell_separate() {
        let rename = build_tmux_action_command(
            "Windows",
            TmuxActionKind::RenameSession {
                target: "$1".to_string(),
                name: "work's".to_string(),
            },
        )
        .unwrap();
        let send = build_tmux_action_command(
            "Windows",
            TmuxActionKind::SendPaneCommand {
                target: "%1".to_string(),
                command: "Write-Host 'ok'".to_string(),
            },
        )
        .unwrap();

        assert!(rename.command.contains("powershell -NoProfile"));
        assert!(rename.command.contains("'work''s'"));
        assert!(send.command.contains("tmux send-keys -t '%1' -l --"));
        assert!(send.command.contains("tmux send-keys -t '%1' Enter"));
    }

    #[test]
    fn snapshot_commands_are_remote_capability_driven() {
        let linux = build_tmux_snapshot_command("Linux");
        let mac = build_tmux_snapshot_command("macOS");
        let windows = build_tmux_snapshot_command("Windows");

        assert!(linux.command.contains("command -v tmux"));
        assert_eq!(linux.command, mac.command);
        assert!(windows.command.contains("Get-Command tmux"));
        assert_eq!(linux.capability, TmuxCommandCapability::Unknown);
    }
}
