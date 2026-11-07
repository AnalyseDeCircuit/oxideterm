// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Envelope parsing. Each capture prints section markers around one JSON
//! document per list; the parsers walk those sections and read herdr's error
//! envelopes, which are the only failure signal herdr emits.
//!
//! Framing is positional. A line is framing only where the probe prints one:
//! the header at the stream start, each section marker in the order the
//! command lists them, the terminal sentinel after the last section, and the
//! missing-binary marker in the preamble. Labels, cwds and terminal titles
//! are server-authored, so a payload line that spells a marker stays data;
//! reading it as framing would let the remote choose which section its own
//! answer is parsed from and hide the rest.

use super::model::{
    HerdrAgent, HerdrPane, HerdrSession, HerdrSessionDetail, HerdrSnapshot, HerdrStatus, HerdrTab,
    HerdrWorkspace,
};

const HERDR_PROBE_HEADER: &str = "===HERDR===";
const HERDR_UNAVAILABLE_MARKER: &str = "__OXIDE_HERDR_UNAVAILABLE__";
const HERDR_SNAPSHOT_END: &str = "===HERDR_END===";
const HERDR_SESSIONS_SECTION: &str = "===HERDR_SESSIONS===";
const HERDR_WORKSPACES_SECTION: &str = "===HERDR_WORKSPACES===";
const HERDR_TABS_SECTION: &str = "===HERDR_TABS===";
const HERDR_PANES_SECTION: &str = "===HERDR_PANES===";
const HERDR_AGENTS_SECTION: &str = "===HERDR_AGENTS===";

/// The section markers of the session-list probe, in the order it prints them.
const SNAPSHOT_SECTIONS: &[&str] = &[HERDR_SESSIONS_SECTION];
/// The section markers of the per-session probe, in the order it prints them.
const DETAIL_SECTIONS: &[&str] = &[
    HERDR_WORKSPACES_SECTION,
    HERDR_TABS_SECTION,
    HERDR_PANES_SECTION,
    HERDR_AGENTS_SECTION,
];

const HERDR_ERROR_MESSAGE_MAX_CHARS: usize = 180;

/// Whether a capture's own answer proved the action worked.
///
/// A failed herdr action still exits zero and reports the failure as a JSON
/// envelope, so the envelope decides; the exit code only covers invocations
/// that printed no envelope at all. A truncated capture can hold half an
/// envelope, which decides nothing: calling it success would report the
/// outcome of text that was cut off mid-document.
pub fn herdr_action_outcome(stdout: &str, exit_code: Option<i32>, truncated: bool) -> bool {
    if truncated {
        return false;
    }
    let envelope = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|value| {
            if value.get("error").is_some_and(|error| !error.is_null()) {
                return Some(false);
            }
            (value.get("result").is_some() || value.get("id").is_some()).then_some(true)
        });
    match envelope {
        Some(true) => exit_code.is_none_or(|code| code == 0),
        Some(false) => false,
        // Nothing answered: only a known clean exit says the command ran.
        None => exit_code == Some(0),
    }
}

/// Payload lines of a capture, attributed to the section that printed them.
struct FramedCapture<'a> {
    /// One entry per expected marker, in probe order.
    sections: Vec<Vec<&'a str>>,
    /// The probe printed the missing-binary sentinel where only it can.
    missing_binary: bool,
    /// The capture holds text but no framing line anywhere the probe prints
    /// one, so nothing here can be attributed to herdr.
    complete: bool,
    /// The probe printed nothing at all.
    silent: bool,
}

fn frame_capture<'a>(output: &'a str, expected: &[&str]) -> FramedCapture<'a> {
    let mut framed = FramedCapture {
        sections: expected.iter().map(|_| Vec::new()).collect(),
        missing_binary: false,
        complete: false,
        // The probe's first word is its header, so a capture that is blank
        // after trimming was never answered.
        silent: output.trim().is_empty(),
    };
    let mut started = false;
    let mut next = 0usize;
    for line in output.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if !started {
            started = line == HERDR_PROBE_HEADER;
            continue;
        }
        if line == HERDR_UNAVAILABLE_MARKER && next == 0 {
            framed.missing_binary = true;
            break;
        }
        // Only the next expected marker can open a section.
        if expected.get(next).is_some_and(|marker| *marker == line) {
            next += 1;
            continue;
        }
        if line == HERDR_SNAPSHOT_END && next == expected.len() {
            framed.complete = true;
            break;
        }
        if next != 0 {
            framed.sections[next - 1].push(line);
        }
    }
    framed
}

/// Server-authored display text made safe for the row, toast or confirmation
/// it ends up in. A label carrying a newline forges an extra notification line
/// and an embedded escape sequence moves the cursor, so every control
/// character (newline, CR, tab, bell, ANSI introducer) becomes a space; the
/// caller's cap keeps a runaway value from pushing the row open.
fn display_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max_chars)
        .collect()
}

/// Optional display text: a value that is blank once sanitized is not a name
/// the tree has to render, so it stays `None` and the caller's fallback applies.
fn optional_display_text(value: &str, max_chars: usize) -> Option<String> {
    let sanitized: String = value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let text = sanitized.trim();
    (!text.is_empty()).then(|| text.chars().take(max_chars).collect())
}

fn bounded_error_message(message: &str) -> String {
    let cleaned: String = message
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(HERDR_ERROR_MESSAGE_MAX_CHARS)
        .collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty())
        .then(|| cleaned.to_string())
        .unwrap_or_else(|| "herdr command failed.".to_string())
}

/// A locally-detected structural complaint. It carries no message: the text
/// would be the app's own English sentence, and the panel already owns a
/// localized failure line for a capture it cannot trust.
const MALFORMED: HerdrStatus = HerdrStatus::Malformed;

/// The status one envelope declares. A success document may still carry
/// `"error": null`; only a populated error member makes it an answer about a
/// failure.
fn error_status(value: &serde_json::Value) -> Option<HerdrStatus> {
    let error = value.get("error").filter(|error| !error.is_null())?;
    if error.get("code").and_then(|code| code.as_str()) == Some("server_not_running") {
        return Some(HerdrStatus::ServerNotRunning);
    }
    let message = error
        .get("message")
        .and_then(|message| message.as_str())
        .unwrap_or_default();
    Some(HerdrStatus::Error {
        message: bounded_error_message(message),
    })
}

/// Where a section's list member sits on one document.
enum Carrier<'a> {
    List(&'a [serde_json::Value]),
    /// No list member: the document may still be a single row of its own.
    Absent,
    /// A member on the path exists with the wrong type. Reading that as an
    /// empty list would report a server that answered with nothing.
    Malformed,
}

fn carrier<'a>(value: &'a serde_json::Value, path: &[&str]) -> Carrier<'a> {
    let Some((last, parents)) = path.split_last() else {
        return Carrier::Absent;
    };
    let mut node = value;
    for key in parents {
        match node.get(*key) {
            None => return Carrier::Absent,
            Some(member) if member.is_object() => node = member,
            Some(_) => return Carrier::Malformed,
        }
    }
    match node.get(*last) {
        None => Carrier::Absent,
        Some(list) => match list.as_array() {
            Some(entries) => Carrier::List(entries),
            None => Carrier::Malformed,
        },
    }
}

fn parse_workspace(value: &serde_json::Value) -> Option<HerdrWorkspace> {
    let workspace_id = value.get("workspace_id")?.as_str()?.to_string();
    Some(HerdrWorkspace {
        label: value
            .get("label")
            .and_then(|label| label.as_str())
            .map(|label| display_text(label, 128))
            .unwrap_or_else(|| display_text(&workspace_id, 128)),
        number: value.get("number").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        focused: value
            .get("focused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        pane_count: value
            .get("pane_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        tab_count: value.get("tab_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        agent_status: value
            .get("agent_status")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(32).collect()),
        workspace_id,
    })
}

fn parse_tab(value: &serde_json::Value) -> Option<HerdrTab> {
    Some(HerdrTab {
        tab_id: value.get("tab_id")?.as_str()?.to_string(),
        workspace_id: value.get("workspace_id")?.as_str()?.to_string(),
        label: value
            .get("label")
            .and_then(|v| v.as_str())
            .and_then(|label| optional_display_text(label, 128)),
        number: value.get("number").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        pane_count: value
            .get("pane_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        focused: value
            .get("focused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        agent_status: value
            .get("agent_status")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(32).collect()),
    })
}

fn parse_pane(value: &serde_json::Value) -> Option<HerdrPane> {
    Some(HerdrPane {
        pane_id: value.get("pane_id")?.as_str()?.to_string(),
        workspace_id: value.get("workspace_id")?.as_str()?.to_string(),
        tab_id: value
            .get("tab_id")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(64).collect()),
        label: value
            .get("label")
            .and_then(|v| v.as_str())
            .and_then(|label| optional_display_text(label, 128)),
        terminal_title: value
            .get("terminal_title")
            .or_else(|| value.get("terminal_title_stripped"))
            .and_then(|v| v.as_str())
            .and_then(|title| optional_display_text(title, 160)),
        cwd: value
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(|cwd| display_text(cwd, 256)),
        focused: value
            .get("focused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        agent_status: value
            .get("agent_status")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(32).collect()),
        restore_failed: value
            .get("restore_error")
            .and_then(|v| v.as_str())
            .is_some_and(|error| !error.is_empty()),
    })
}

fn parse_session(value: &serde_json::Value) -> Option<HerdrSession> {
    let name = value.get("name")?.as_str()?;
    // The attach builder trims and length-limits this name, so a row whose
    // name would change there is dropped instead of attaching elsewhere.
    if name.is_empty()
        || name.trim() != name
        || name.chars().count() > 128
        || name.chars().any(char::is_control)
    {
        return None;
    }
    Some(HerdrSession {
        name: name.to_string(),
        running: value
            .get("running")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

fn parse_agent(value: &serde_json::Value) -> Option<HerdrAgent> {
    Some(HerdrAgent {
        // The pane reference is the only field the row joins on; an agent
        // without one cannot be attached to or rendered against a pane.
        pane_id: value
            .get("pane_id")
            .or_else(|| value.get("target"))
            .and_then(|v| v.as_str())?
            .to_string(),
        workspace_id: value
            .get("workspace_id")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(64).collect()),
        label: value
            .get("label")
            .or_else(|| value.get("name"))
            // herdr reports the agent kind under `agent` and the pane title
            // separately; either is a better row label than the raw pane id.
            .or_else(|| value.get("agent"))
            .or_else(|| value.get("terminal_title"))
            .and_then(|v| v.as_str())
            .map(|label| display_text(label, 128)),
        status: value
            .get("status")
            .or_else(|| value.get("agent_status"))
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(32).collect()),
    })
}

/// One section's rows plus the status its documents decided. Success
/// documents feed the list; the first error envelope or structurally wrong
/// document decides the status.
fn parse_herdr_section<T>(
    lines: &[&str],
    extract: impl Fn(&serde_json::Value) -> Option<T> + Copy,
    carrier_path: &[&str],
) -> (Vec<T>, Option<HerdrStatus>) {
    let mut items = Vec::new();
    let mut status = None;
    let mut answered = false;
    for line in lines {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            // Non-JSON noise (a shell banner between markers) is not an answer.
            continue;
        };
        if let Some(line_status) = error_status(&value) {
            status.get_or_insert(line_status);
            continue;
        }
        if !value.is_object() {
            status.get_or_insert(MALFORMED);
            continue;
        }
        match carrier(&value, carrier_path) {
            Carrier::List(entries) => {
                answered = true;
                for entry in entries {
                    if let Some(item) = extract(entry) {
                        items.push(item);
                    }
                }
            }
            Carrier::Absent => {
                if let Some(item) = extract(&value) {
                    answered = true;
                    items.push(item);
                }
            }
            Carrier::Malformed => {
                status.get_or_insert(MALFORMED);
            }
        }
    }
    (items, status.or_else(|| (!answered).then_some(MALFORMED)))
}

/// Folds the per-section verdicts into the status of the whole capture.
/// `server_not_running` outranks every other failure because it names the
/// action the user can actually take.
#[derive(Default)]
struct StatusFold {
    server_down: bool,
    error: Option<HerdrStatus>,
}

impl StatusFold {
    fn add(&mut self, section_status: Option<HerdrStatus>) {
        match section_status {
            Some(HerdrStatus::ServerNotRunning) => self.server_down = true,
            Some(status) if self.error.is_none() => self.error = Some(status),
            _ => {}
        }
    }

    fn finish(self, framed: &FramedCapture<'_>) -> HerdrStatus {
        if framed.missing_binary {
            return HerdrStatus::Unavailable;
        }
        if framed.silent {
            return HerdrStatus::Unknown;
        }
        if self.server_down {
            return HerdrStatus::ServerNotRunning;
        }
        if let Some(error) = self.error {
            return error;
        }
        if !framed.complete {
            return HerdrStatus::Malformed;
        }
        HerdrStatus::Available
    }
}

pub fn parse_herdr_snapshot(output: &str) -> HerdrSnapshot {
    let framed = frame_capture(output, SNAPSHOT_SECTIONS);
    let mut fold = StatusFold::default();
    let (sessions, status) = parse_herdr_section(&framed.sections[0], parse_session, &["sessions"]);
    fold.add(status);
    HerdrSnapshot {
        status: fold.finish(&framed),
        sessions,
    }
}

pub fn parse_herdr_session_detail(output: &str) -> HerdrSessionDetail {
    let framed = frame_capture(output, DETAIL_SECTIONS);
    let mut fold = StatusFold::default();

    let (workspaces, status) = parse_herdr_section(
        &framed.sections[0],
        parse_workspace,
        &["result", "workspaces"],
    );
    fold.add(status);

    let (tabs, status) = parse_herdr_section(&framed.sections[1], parse_tab, &["result", "tabs"]);
    fold.add(status);

    let (panes, status) =
        parse_herdr_section(&framed.sections[2], parse_pane, &["result", "panes"]);
    fold.add(status);

    let (agents, status) =
        parse_herdr_section(&framed.sections[3], parse_agent, &["result", "agents"]);
    fold.add(status);

    HerdrSessionDetail {
        status: fold.finish(&framed),
        workspaces,
        tabs,
        panes,
        agents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures below are captured verbatim from herdr 0.9.4-preview.
    const WORKSPACE_ENVELOPE: &str = r#"{"id":"cli:workspace:list","result":{"type":"workspace_list","workspaces":[{"active_tab_id":"wA:tB","agent_status":"unknown","focused":true,"label":"helphub","number":1,"pane_count":7,"tab_count":7,"workspace_id":"wA"},{"active_tab_id":"wD:t3","agent_status":"unknown","focused":false,"label":"dbx","number":2,"pane_count":2,"tab_count":2,"workspace_id":"wD"}]}}"#;
    const PANE_ENVELOPE: &str = r#"{"id":"cli:pane:list","result":{"panes":[{"agent":"omp","agent_status":"working","cwd":"C:\\Users\\me\\project\\helphub","focused":true,"pane_id":"wA:pC","revision":0,"tab_id":"wA:tB","terminal_id":"term_1","terminal_title":"omp","terminal_title_stripped":"omp","workspace_id":"wA"},{"agent_status":"unknown","cwd":"/gone/dir","label":"1324","focused":false,"pane_id":"wA:pB","restore_error":"Saved directory is unavailable. Restore the directory and restart this session.","revision":0,"tab_id":"wA:tA","terminal_id":"term_2","workspace_id":"wA"}],"type":"pane_list"}}"#;
    const AGENT_ENVELOPE: &str = r#"{"id":"cli:agent:list","result":{"agents":[{"label":"claude","pane_id":"wA:pC","status":"working","workspace_id":"wA"},{"label":"ghost","pane_id":"wZ:pZ","status":"blocked","workspace_id":"wZ"}],"type":"agent_list"}}"#;
    const AGENT_ENVELOPE_JOINED_ONLY: &str = r#"{"id":"cli:agent:list","result":{"agents":[{"label":"claude","pane_id":"wA:pC","status":"working","workspace_id":"wA"}],"type":"agent_list"}}"#;
    const TAB_ENVELOPE: &str = r#"{"id":"cli:tab:list","result":{"tabs":[{"agent_status":"idle","focused":true,"label":"backend","number":11,"pane_count":1,"tab_id":"wA:tB","workspace_id":"wA"},{"agent_status":"unknown","focused":false,"label":"terminal","number":12,"pane_count":1,"tab_id":"wA:tA","workspace_id":"wA"},{"agent_status":"unknown","focused":false,"label":"other","number":3,"pane_count":2,"tab_id":"wD:t3","workspace_id":"wD"}]}}"#;
    const SESSION_BARE: &str = r#"{"sessions":[{"default":true,"name":"default","running":true,"session_dir":"C:\\herdr","socket_path":"C:\\herdr\\herdr.sock"},{"default":false,"name":"work space","running":false}]}"#;
    const SERVER_DOWN_ENVELOPE: &str = r#"{"id":"cli:workspace:list","error":{"code":"server_not_running","message":"no herdr server is running at C:\\herdr\\herdr.sock; run `herdr` to start or attach it"}}"#;
    const EXTRA_WORKSPACE_ENVELOPE: &str = r#"{"id":"cli:workspace:list","result":{"workspaces":[{"workspace_id":"wE","label":"extra","number":9}]}}"#;
    const HOSTILE_LABEL_ENVELOPE: &str = r#"{"id":"cli:pane:list","result":{"panes":[{"pane_id":"wA:pH","workspace_id":"wA","tab_id":"wA:tH","label":"build\n\u001b[31mfailed","terminal_title":"first\r\tsecond","cwd":"/srv\u0007/api","agent_status":"unknown"}]}}"#;
    const HOSTILE_AGENT_ENVELOPE: &str = r#"{"id":"cli:agent:list","result":{"agents":[{"label":"clau\u0007de\n\u001b[0mextra","pane_id":"wA:pH","status":"working"}]}}"#;

    fn full_snapshot_output() -> String {
        format!(
            "{HERDR_PROBE_HEADER}\n{HERDR_SESSIONS_SECTION}\n{SESSION_BARE}\n{HERDR_SNAPSHOT_END}"
        )
    }

    /// The four detail sections in probe order, each with the document the
    /// probe prints for it.
    fn detail_sections() -> [(&'static str, &'static str); 4] {
        [
            (HERDR_WORKSPACES_SECTION, WORKSPACE_ENVELOPE),
            (HERDR_TABS_SECTION, TAB_ENVELOPE),
            (HERDR_PANES_SECTION, PANE_ENVELOPE),
            (HERDR_AGENTS_SECTION, AGENT_ENVELOPE_JOINED_ONLY),
        ]
    }

    /// A capture laid out the way the probe prints it: header, each section's
    /// marker and document in order, then the terminal sentinel.
    fn framed_detail(section_rows: &[(&str, &str)]) -> String {
        let mut output = String::from(HERDR_PROBE_HEADER);
        for (marker, envelope) in section_rows {
            output.push('\n');
            output.push_str(marker);
            output.push('\n');
            output.push_str(envelope);
        }
        output.push('\n');
        output.push_str(HERDR_SNAPSHOT_END);
        output
    }

    /// The real answer with one section's document replaced.
    fn detail_output_with(section: &str, envelope: &str) -> String {
        framed_detail(
            &detail_sections().map(|(marker, default)| {
                (marker, if marker == section { envelope } else { default })
            }),
        )
    }

    fn full_detail_output() -> String {
        framed_detail(&detail_sections())
    }

    #[test]
    fn herdr_parser_builds_the_tree_from_real_envelopes() {
        let snapshot = parse_herdr_snapshot(&full_snapshot_output());
        assert_eq!(snapshot.status, HerdrStatus::Available);
        assert_eq!(snapshot.sessions.len(), 2);
        assert_eq!(snapshot.sessions[0].name, "default");
        assert!(snapshot.sessions[0].running);
        assert!(!snapshot.sessions[1].running);

        let detail = parse_herdr_session_detail(&full_detail_output());
        assert_eq!(detail.status, HerdrStatus::Available);
        assert_eq!(detail.workspaces.len(), 2);
        assert_eq!(detail.workspaces[0].label, "helphub");
        assert_eq!(detail.workspaces[0].workspace_id, "wA");
        assert_eq!(detail.workspaces[0].tab_count, 7);
        assert_eq!(detail.workspaces[0].pane_count, 7);
        assert!(detail.workspaces[0].focused);
        // Every tab the list carries names its own workspace, which is the
        // grouping the tree joins on.
        assert_eq!(detail.tabs.len(), 3);
        assert_eq!(
            detail
                .tabs
                .iter()
                .filter(|tab| tab.workspace_id == "wA")
                .count(),
            2
        );
        assert_eq!(
            detail
                .tabs
                .iter()
                .filter(|tab| tab.workspace_id == "wD")
                .count(),
            1
        );
        assert_eq!(detail.tabs[0].label.as_deref(), Some("backend"));
        assert_eq!(detail.panes.len(), 2);
        assert_eq!(
            detail
                .panes
                .iter()
                .filter(|pane| pane.tab_id.as_deref() == Some("wA:tB"))
                .count(),
            1
        );
        assert_eq!(
            detail
                .panes
                .iter()
                .filter(|pane| pane.tab_id.as_deref() == Some("wA:tA"))
                .count(),
            1
        );
        assert_eq!(detail.panes[0].label, None);
        assert_eq!(detail.panes[0].terminal_title.as_deref(), Some("omp"));
        // `pane rename` writes `label`, which is the name herdr shows.
        assert_eq!(detail.panes[1].label.as_deref(), Some("1324"));
        assert_eq!(detail.panes[1].terminal_title, None);
        assert!(!detail.panes[0].restore_failed);
        assert!(detail.panes[1].restore_failed);
        assert_eq!(detail.agents.len(), 1);
        let join = detail.agent_join();
        assert_eq!(
            join.for_pane("wA:pC")
                .and_then(|agent| agent.label.clone())
                .as_deref(),
            Some("claude")
        );
        assert!(join.for_pane("wA:pB").is_none());
        assert!(join.orphans().is_empty());
    }

    #[test]
    fn herdr_agent_join_separates_joined_agents_from_orphans() {
        // Both agents arrive in the fixture above; only the pane the pane list
        // does not carry may land in the fallback group.
        let detail =
            parse_herdr_session_detail(&detail_output_with(HERDR_AGENTS_SECTION, AGENT_ENVELOPE));
        let join = detail.agent_join();
        assert_eq!(
            join.for_pane("wA:pC").map(|agent| agent.pane_id.as_str()),
            Some("wA:pC")
        );
        assert_eq!(
            join.for_pane("wZ:pZ").map(|agent| agent.pane_id.as_str()),
            Some("wZ:pZ")
        );
        assert_eq!(join.orphans().len(), 1);
        assert_eq!(join.orphans()[0].pane_id, "wZ:pZ");
        assert_eq!(join.orphans()[0].label.as_deref(), Some("ghost"));
    }

    #[test]
    fn herdr_parser_drops_session_names_attach_cannot_target() {
        let bare = r#"{"sessions":[{"name":" ok ","running":true},{"name":"","running":true},{"name":"line\nbreak","running":true},{"name":"work space","running":true}]}"#;
        let output =
            format!("{HERDR_PROBE_HEADER}\n{HERDR_SESSIONS_SECTION}\n{bare}\n{HERDR_SNAPSHOT_END}");
        let snapshot = parse_herdr_snapshot(&output);
        let names: Vec<&str> = snapshot
            .sessions
            .iter()
            .map(|session| session.name.as_str())
            .collect();
        assert_eq!(names, vec!["work space"]);
    }

    #[test]
    fn herdr_parser_reports_server_not_running_from_each_section() {
        // One case per section. The untouched sections still have to carry the
        // rows their envelope lists, so a section that stopped folding its own
        // answer fails its own case instead of hiding behind the others.
        const ROWS: [usize; 4] = [2, 3, 2, 1];
        for (index, marker) in DETAIL_SECTIONS.iter().enumerate() {
            let detail =
                parse_herdr_session_detail(&detail_output_with(marker, SERVER_DOWN_ENVELOPE));
            assert_eq!(
                detail.status,
                HerdrStatus::ServerNotRunning,
                "{marker} must be read"
            );
            assert_eq!(
                detail.workspaces.len(),
                if index == 0 { 0 } else { ROWS[0] },
                "{marker}: workspace rows"
            );
            assert_eq!(
                detail.tabs.len(),
                if index == 1 { 0 } else { ROWS[1] },
                "{marker}: tab rows"
            );
            assert_eq!(
                detail.panes.len(),
                if index == 2 { 0 } else { ROWS[2] },
                "{marker}: pane rows"
            );
            assert_eq!(
                detail.agents.len(),
                if index == 3 { 0 } else { ROWS[3] },
                "{marker}: agent rows"
            );
        }
    }

    #[test]
    fn herdr_parser_reports_missing_binary_and_noise_tolerance() {
        let unavailable =
            format!("{HERDR_PROBE_HEADER}\n{HERDR_UNAVAILABLE_MARKER}\n{HERDR_SNAPSHOT_END}");
        assert_eq!(
            parse_herdr_snapshot(&unavailable).status,
            HerdrStatus::Unavailable
        );
        assert_eq!(
            parse_herdr_session_detail(&unavailable).status,
            HerdrStatus::Unavailable
        );

        // A banner is not a row; an explicit empty list is a valid answer.
        let empty_agents = r#"{"result":{"agents":[]}}"#;
        let noisy = format!(
            "{HERDR_PROBE_HEADER}\nPowerShell banner noise\n{HERDR_WORKSPACES_SECTION}\n{WORKSPACE_ENVELOPE}\n{HERDR_TABS_SECTION}\n{TAB_ENVELOPE}\n{HERDR_PANES_SECTION}\n{PANE_ENVELOPE}\n{HERDR_AGENTS_SECTION}\n{empty_agents}\n{HERDR_SNAPSHOT_END}"
        );
        let detail = parse_herdr_session_detail(&noisy);
        assert_eq!(detail.status, HerdrStatus::Available);
        assert_eq!(detail.workspaces.len(), 2);
        assert!(detail.agents.is_empty());
    }

    #[test]
    fn herdr_requires_complete_list_replies_not_just_probe_markers() {
        for body in ["", "{", r#"{"id":"cli:session:list"}"#] {
            let capture = format!(
                "{HERDR_PROBE_HEADER}\n{HERDR_SESSIONS_SECTION}\n{body}\n{HERDR_SNAPSHOT_END}"
            );
            assert_eq!(
                parse_herdr_snapshot(&capture).status,
                HerdrStatus::Malformed,
                "{body:?}"
            );
        }
        let empty = format!("{HERDR_PROBE_HEADER}\n{HERDR_SESSIONS_SECTION}\n{{\"sessions\":[]}}");
        assert_eq!(parse_herdr_snapshot(&empty).status, HerdrStatus::Malformed);
        let snapshot = parse_herdr_snapshot(&format!("{empty}\n{HERDR_SNAPSHOT_END}"));
        assert_eq!(snapshot.status, HerdrStatus::Available);
        assert!(snapshot.sessions.is_empty());

        for marker in DETAIL_SECTIONS {
            let capture = detail_output_with(marker, "");
            assert_eq!(
                parse_herdr_session_detail(&capture).status,
                HerdrStatus::Malformed,
                "{marker} printed no reply"
            );
        }
    }

    #[test]
    fn herdr_parser_ignores_marker_text_inside_server_strings() {
        let envelope = r#"{"result":{"workspaces":[{"workspace_id":"wA","label":"===HERDR_PANES=== __OXIDE_HERDR_UNAVAILABLE__","number":1}]}}"#;
        let output = detail_output_with(HERDR_WORKSPACES_SECTION, envelope);
        let detail = parse_herdr_session_detail(&output);
        assert_eq!(detail.status, HerdrStatus::Available);
        assert_eq!(detail.workspaces.len(), 1);
        assert_eq!(
            detail.workspaces[0].label,
            "===HERDR_PANES=== __OXIDE_HERDR_UNAVAILABLE__"
        );
    }

    #[test]
    fn herdr_framing_is_recognized_only_where_the_probe_prints_markers() {
        // The server put a later section marker, the missing-binary marker and
        // the terminal sentinel on lines of their own, ahead of data that
        // still belongs to the open section.
        let output = format!(
            "{HERDR_PROBE_HEADER}\n{HERDR_WORKSPACES_SECTION}\n{WORKSPACE_ENVELOPE}\n{HERDR_PANES_SECTION}\n{EXTRA_WORKSPACE_ENVELOPE}\n{HERDR_UNAVAILABLE_MARKER}\n{HERDR_SNAPSHOT_END}\n{HERDR_TABS_SECTION}\n{TAB_ENVELOPE}\n{HERDR_AGENTS_SECTION}\n{AGENT_ENVELOPE_JOINED_ONLY}\n{HERDR_SNAPSHOT_END}"
        );
        let detail = parse_herdr_session_detail(&output);
        let labels: Vec<&str> = detail
            .workspaces
            .iter()
            .map(|workspace| workspace.label.as_str())
            .collect();
        assert_eq!(
            labels,
            vec!["helphub", "dbx", "extra"],
            "a payload line may not close the section it is inside"
        );
        // Only the marker the probe prints next frames a new section, so the
        // pane and agent documents never become rows of their own.
        assert!(detail.panes.is_empty());
        assert!(detail.agents.is_empty());
        assert_eq!(
            detail.tabs.len(),
            3,
            "the section in probe order is still framing"
        );
        assert_eq!(
            detail.status,
            HerdrStatus::Malformed,
            "a payload line cannot stand in for missing sections"
        );

        // Once the last section is open the sentinel is terminal, so anything
        // the server prints after it is not read at all.
        let after_sentinel = format!(
            "{HERDR_PROBE_HEADER}\n{HERDR_SESSIONS_SECTION}\n{SESSION_BARE}\n{HERDR_SNAPSHOT_END}\n{{\"sessions\":[]}}\n{HERDR_UNAVAILABLE_MARKER}\n{HERDR_SNAPSHOT_END}"
        );
        let snapshot = parse_herdr_snapshot(&after_sentinel);
        assert_eq!(snapshot.status, HerdrStatus::Available);
        assert_eq!(snapshot.sessions.len(), 2);
    }

    #[test]
    fn herdr_parser_surfaces_other_error_envelopes_bounded() {
        let long = "b".repeat(HERDR_ERROR_MESSAGE_MAX_CHARS + 40);
        let envelope = format!(
            r#"{{"id":"cli:workspace:list","error":{{"code":"invalid_params","message":"{long}\nsecond line\u001besc"}}}}"#
        );
        let detail =
            parse_herdr_session_detail(&detail_output_with(HERDR_WORKSPACES_SECTION, &envelope));
        match detail.status {
            HerdrStatus::Error { message } => {
                assert_eq!(
                    message.chars().count(),
                    HERDR_ERROR_MESSAGE_MAX_CHARS,
                    "the bound is the character cap, not a guess at it"
                );
                assert_eq!(
                    message,
                    long.chars()
                        .take(HERDR_ERROR_MESSAGE_MAX_CHARS)
                        .collect::<String>(),
                    "only the first line of a server-authored message survives"
                );
                assert!(!message.contains("second line"));
                assert!(!message.contains('\u{1b}'));
            }
            other => panic!("expected error status, got {other:?}"),
        }
    }

    #[test]
    fn herdr_parser_treats_null_error_member_as_success() {
        let envelope = r#"{"id":"cli:workspace:list","error":null,"result":{"workspaces":[{"workspace_id":"wA","label":"one","number":1}]}}"#;
        let detail =
            parse_herdr_session_detail(&detail_output_with(HERDR_WORKSPACES_SECTION, envelope));
        assert_eq!(detail.status, HerdrStatus::Available);
        assert_eq!(detail.workspaces.len(), 1);
    }

    #[test]
    fn herdr_parser_classifies_a_list_member_of_the_wrong_type() {
        let sessions = format!(
            "{HERDR_PROBE_HEADER}\n{HERDR_SESSIONS_SECTION}\n{{\"sessions\":\"none\"}}\n{HERDR_SNAPSHOT_END}"
        );
        assert!(
            matches!(
                parse_herdr_snapshot(&sessions).status,
                HerdrStatus::Malformed
            ),
            "a string where the session list belongs is not an empty list"
        );

        // A wrapper that is a string instead of an object.
        let wrapped = detail_output_with(
            HERDR_WORKSPACES_SECTION,
            r#"{"result":"no workspaces yet"}"#,
        );
        assert!(
            matches!(
                parse_herdr_session_detail(&wrapped).status,
                HerdrStatus::Malformed
            ),
            "a wrapper that is not an object is not an empty section"
        );

        // A list member that is an object instead of an array, while the
        // sections around it stay readable: the rows that did arrive are still
        // the rows the panel would have shown.
        let panes = format!(
            "{HERDR_PROBE_HEADER}\n{HERDR_WORKSPACES_SECTION}\n{WORKSPACE_ENVELOPE}\n{HERDR_TABS_SECTION}\n{TAB_ENVELOPE}\n{HERDR_PANES_SECTION}\n{{\"result\":{{\"panes\":{{\"pane_id\":\"wA:pC\"}}}}}}\n{HERDR_AGENTS_SECTION}\n{AGENT_ENVELOPE_JOINED_ONLY}\n{HERDR_SNAPSHOT_END}"
        );
        let detail = parse_herdr_session_detail(&panes);
        assert!(
            matches!(detail.status, HerdrStatus::Malformed),
            "a list member that is not a list is not an empty list"
        );
        assert!(detail.panes.is_empty());
        assert_eq!(detail.workspaces.len(), 2);
        assert_eq!(detail.tabs.len(), 3);

        // Text the probe could never have printed is a broken capture, not an
        // available server with nothing to show.
        let unframed = "bash: herdr: command not found\n";
        assert!(
            matches!(
                parse_herdr_snapshot(unframed).status,
                HerdrStatus::Malformed
            ),
            "unframed text proves nothing about herdr"
        );
    }

    #[test]
    fn herdr_parser_gives_a_silent_capture_no_verdict() {
        // Blank output is not "available with no rows"; it is no answer, so
        // the exit code or the panel has to decide instead of the capture.
        assert_eq!(parse_herdr_snapshot("").status, HerdrStatus::Unknown);
        assert_eq!(
            parse_herdr_session_detail("   \n  ").status,
            HerdrStatus::Unknown
        );
    }

    #[test]
    fn herdr_parser_sanitizes_control_characters_in_remote_display_text() {
        let output = framed_detail(&[
            (HERDR_WORKSPACES_SECTION, WORKSPACE_ENVELOPE),
            (HERDR_TABS_SECTION, TAB_ENVELOPE),
            (HERDR_PANES_SECTION, HOSTILE_LABEL_ENVELOPE),
            (HERDR_AGENTS_SECTION, HOSTILE_AGENT_ENVELOPE),
        ]);
        let detail = parse_herdr_session_detail(&output);
        assert_eq!(detail.status, HerdrStatus::Available);
        let pane = &detail.panes[0];
        assert_eq!(pane.label.as_deref(), Some("build  [31mfailed"));
        assert_eq!(pane.terminal_title.as_deref(), Some("first  second"));
        assert_eq!(pane.cwd.as_deref(), Some("/srv /api"));
        let agent = detail.agent_join().for_pane("wA:pH").unwrap();
        assert_eq!(agent.label.as_deref(), Some("clau de  [0mextra"));
        for text in [
            pane.label.as_deref(),
            pane.terminal_title.as_deref(),
            pane.cwd.as_deref(),
            agent.label.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                !text.chars().any(char::is_control),
                "{text:?} still carries a control character"
            );
        }
        // A label that is only control characters is no name at all, so the
        // pane falls back to its next preference instead of rendering a blank.
        let blank =
            r#"{"result":{"panes":[{"pane_id":"wA:pB","workspace_id":"wA","label":"\n\r\t"}]}}"#;
        let detail = parse_herdr_session_detail(&detail_output_with(HERDR_PANES_SECTION, blank));
        assert_eq!(detail.panes[0].label, None);

        // The length caps still bound sanitized values.
        let long = "a".repeat(400);
        let envelope = format!(
            r#"{{"result":{{"panes":[{{"pane_id":"wA:pC","workspace_id":"wA","cwd":"{long}"}}]}}}}"#
        );
        let detail =
            parse_herdr_session_detail(&detail_output_with(HERDR_PANES_SECTION, &envelope));
        assert_eq!(detail.panes[0].cwd.as_deref().unwrap().chars().count(), 256);
    }

    #[test]
    fn herdr_action_outcome_reads_envelopes_not_exit_codes() {
        // Server-side failures still exit zero; the envelope decides.
        assert!(!herdr_action_outcome(SERVER_DOWN_ENVELOPE, Some(0), false));
        assert!(!herdr_action_outcome(
            r#"{"id":"cli:pane:close","error":{"code":"invalid_params","message":"nope"}}"#,
            Some(0),
            false
        ));
        assert!(herdr_action_outcome(
            r#"{"id":"cli:pane:close","result":{"type":"ok"}}"#,
            Some(0),
            false
        ));
        assert!(herdr_action_outcome(
            r#"{"id":"cli:pane:close","error":null}"#,
            Some(0),
            false
        ));
        assert!(!herdr_action_outcome("", Some(1), false));
        // Stderr noise merged into stdout must not fail a clean exit.
        assert!(herdr_action_outcome(
            "warning: detached terminal",
            Some(0),
            false
        ));
    }

    #[test]
    fn herdr_action_outcome_refuses_an_undecidable_capture() {
        // A truncated capture can hold half of either envelope, so it decides
        // nothing; the same text uncut is decisive.
        let success = r#"{"id":"cli:pane:close","result":{"type":"ok"}}"#;
        let failure =
            r#"{"id":"cli:pane:close","error":{"code":"invalid_params","message":"nope"}}"#;
        assert!(herdr_action_outcome(success, Some(0), false));
        assert!(!herdr_action_outcome(&success[..24], Some(0), true));
        assert!(!herdr_action_outcome(failure, Some(0), false));
        assert!(!herdr_action_outcome(&failure[..24], Some(0), true));

        // Without a status of its own, a process that never reported an exit
        // code is not evidence the mutation worked.
        assert!(!herdr_action_outcome(
            "warning: detached terminal",
            None,
            false
        ));
        assert!(!herdr_action_outcome("", None, false));
        // A decisive envelope does not need the exit code.
        assert!(herdr_action_outcome(success, None, false));
    }
}
