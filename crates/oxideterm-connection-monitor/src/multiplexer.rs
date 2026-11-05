// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Provider-neutral model for the terminal-multiplexer Host Tool.
//!
//! The panel renders these types and never a provider's wire format. Herdr
//! delegates command construction and envelope parsing to the `herdr` module,
//! which owns the CLI grammar, the id/name validation and the error envelopes;
//! Zellij is built and parsed here because its CLI only offers one JSON verb
//! plus screen-scraped text.

use serde::Deserialize;

use crate::herdr::{
    HerdrActionKind, HerdrAttachTarget, HerdrSessionDetail, HerdrStatus,
    build_herdr_action_command, build_herdr_attach_command, build_herdr_read_command,
    build_herdr_session_detail_command, build_herdr_snapshot_command, herdr_action_outcome,
    parse_herdr_session_detail, parse_herdr_snapshot,
};
use crate::shell::{posix_shell_command, powershell_script, script_arg, windows_encoded_command};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Multiplexer {
    Zellij,
    Herdr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuxSession {
    pub name: String,
    /// A stopped session is still addressable: Herdr starts it on attach and
    /// Zellij resurrects it.
    pub running: bool,
}

/// Outcome of a probe. `Error` carries the server's own bounded message when
/// the provider produced one; the panel falls back to its localized text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MuxStatus {
    Available,
    Unavailable,
    /// The CLI is installed but its server is not running; launching the bare
    /// command in a terminal starts it.
    ServerNotRunning,
    Error(Option<String>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuxSnapshot {
    pub status: MuxStatus,
    pub sessions: Vec<MuxSession>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MuxKind {
    Workspace,
    Tab,
    Pane,
}

/// An agent detected inside a pane. Providers without agent detection never
/// build one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuxAgent {
    pub pane_id: String,
    pub label: Option<String>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuxNode {
    pub kind: MuxKind,
    pub id: String,
    /// Display label, already resolved from the provider's preference order.
    pub label: String,
    pub children: Vec<MuxNode>,
    pub focused: bool,
    /// Agent status of this row or of the agent it contains.
    pub status: Option<String>,
    pub agent: Option<MuxAgent>,
    /// OSC 0/2 terminal title, independent of the row label.
    pub title: Option<String>,
    pub cwd: Option<String>,
    /// Agent whose pane is no longer in the pane list; kept visible instead of
    /// disappearing with its pane.
    pub orphan_agent: bool,
    pub restore_failed: bool,
    /// Server-side child counts; they can exceed `children` when the provider
    /// reports more objects than it listed.
    pub tab_count: usize,
    pub pane_count: usize,
}

impl MuxNode {
    fn new(kind: MuxKind, id: String, label: String) -> Self {
        Self {
            kind,
            id,
            label,
            children: Vec::new(),
            focused: false,
            status: None,
            agent: None,
            title: None,
            cwd: None,
            orphan_agent: false,
            restore_failed: false,
            tab_count: 0,
            pane_count: 0,
        }
    }
}

/// The verbs a provider implements for one tree level. Payload-free so the
/// panel can lay out the buttons before the user supplies a label or command.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MuxVerbKind {
    Focus,
    CreateChild,
    CreateWorkspace,
    Rename,
    Run,
    Read,
    Close,
    Stop,
    Delete,
}

/// A verb with the payload the user supplied. The typed pane command is
/// zeroized because it is one line the user may have filled with a credential;
/// the derived `Debug` renders it as `[REDACTED]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MuxVerb {
    Focus,
    CreateChild,
    CreateWorkspace { label: String, cwd: Option<String> },
    Rename { label: String },
    Run { command: Zeroizing<String> },
    Read,
    Close,
    Stop,
    Delete,
}

impl Multiplexer {
    pub fn name(self) -> &'static str {
        match self {
            Self::Herdr => "Herdr",
            Self::Zellij => "Zellij",
        }
    }

    /// Command that lists sessions. Herdr answers with section markers and an
    /// explicit "not installed" marker, so its probe never trusts exit codes;
    /// Zellij has no machine-readable list, so 127 is the sentinel.
    pub fn list_command(self, windows: bool) -> String {
        match self {
            Self::Herdr => build_herdr_snapshot_command(os_type(windows)).command,
            Self::Zellij => {
                let args = "list-sessions --no-formatting";
                if windows {
                    windows_encoded_command(&format!(
                        "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); if (!(Get-Command zellij -CommandType Application -ErrorAction SilentlyContinue)) {{exit 127}}; & zellij {args}; exit $LASTEXITCODE"
                    ))
                } else {
                    posix_shell_command(&format!(
                        "command -v zellij >/dev/null || exit 127; exec zellij {args}"
                    ))
                }
            }
        }
    }

    pub fn parse_sessions(self, output: &str, exit_code: Option<i32>) -> MuxSnapshot {
        match self {
            Self::Herdr => {
                let snapshot = parse_herdr_snapshot(output);
                MuxSnapshot {
                    status: herdr_status(snapshot.status, exit_code),
                    sessions: snapshot
                        .sessions
                        .into_iter()
                        .map(|session| MuxSession {
                            name: session.name,
                            running: session.running,
                        })
                        .collect(),
                }
            }
            Self::Zellij => zellij_sessions(output, exit_code),
        }
    }

    /// Command that lists everything beneath one session.
    pub fn detail_command(self, windows: bool, session: &str) -> Option<String> {
        match self {
            Self::Herdr => Some(
                build_herdr_session_detail_command(os_type(windows), session)
                    .ok()?
                    .command,
            ),
            Self::Zellij => zellij_line(windows, Some(session), "action list-panes --json", None),
        }
    }

    /// Subtree parse. A tree is only trustworthy when the provider's own
    /// envelope says the session answered: an unavailable server, a stopped
    /// server or an error envelope yields `None`, which the panel renders as a
    /// failed row. Returning an empty tree there would cache the absence and
    /// hide the state the user has to act on.
    pub fn parse_tree(self, output: &str) -> Option<Vec<MuxNode>> {
        match self {
            Self::Herdr => {
                let detail = parse_herdr_session_detail(output);
                match detail.status {
                    HerdrStatus::Available => Some(herdr_tree(&detail)),
                    _ => None,
                }
            }
            Self::Zellij => zellij_tree(output),
        }
    }

    /// Recent scrollback of one pane. Only Herdr exposes pane buffers, so the
    /// panel asks [`Multiplexer::verbs_for`] instead of probing this.
    pub fn read_command(self, windows: bool, session: &str, node: &MuxNode) -> Option<String> {
        match self {
            Self::Herdr => Some(
                build_herdr_read_command(os_type(windows), session, &node.id)
                    .ok()?
                    .command,
            ),
            Self::Zellij => None,
        }
    }

    /// Command typed into a fresh terminal. `session` of `None` starts the
    /// provider's own default session.
    pub fn attach_command(self, windows: bool, session: Option<&str>) -> Option<String> {
        let os = os_type(windows);
        match self {
            Self::Herdr => Some(match session {
                None => build_herdr_attach_command(os, &HerdrAttachTarget::Default).ok()?,
                Some(name) => build_herdr_attach_command(
                    os,
                    &HerdrAttachTarget::Session {
                        name: name.to_string(),
                    },
                )
                .ok()?,
            }),
            Self::Zellij => match session {
                None => Some(if windows {
                    "zellij".to_string()
                } else {
                    posix_shell_command("exec zellij")
                }),
                Some(name) => zellij_attach(windows, name),
            },
        }
    }

    /// Command that creates and enters a session with a chosen name. Zellij
    /// has no non-interactive create verb, so it returns `None`.
    pub fn new_session_command(self, windows: bool, name: &str) -> Option<String> {
        match self {
            Self::Herdr => build_herdr_attach_command(
                os_type(windows),
                &HerdrAttachTarget::NewSession {
                    name: name.to_string(),
                },
            )
            .ok(),
            Self::Zellij => None,
        }
    }

    /// Command that points a terminal at the agent inside a pane.
    pub fn agent_attach_command(
        self,
        windows: bool,
        session: &str,
        node: &MuxNode,
    ) -> Option<String> {
        match self {
            Self::Herdr => build_herdr_attach_command(
                os_type(windows),
                &HerdrAttachTarget::Agent {
                    session: session.to_string(),
                    pane_id: node.id.clone(),
                },
            )
            .ok(),
            Self::Zellij => None,
        }
    }

    pub fn verbs_for(self, kind: Option<MuxKind>) -> &'static [MuxVerbKind] {
        use MuxKind::*;
        use MuxVerbKind::*;
        match (self, kind) {
            (Self::Herdr, None) => &[CreateWorkspace, Stop, Delete],
            (Self::Zellij, None) => &[CreateChild],
            (Self::Herdr, Some(Workspace)) => &[Focus, CreateChild, Rename, Close],
            (Self::Zellij, Some(Workspace)) => &[],
            (Self::Herdr, Some(Tab)) => &[Focus, Rename, Close],
            (Self::Zellij, Some(Tab)) => &[Focus, CreateChild, Close],
            (Self::Herdr, Some(Pane)) => &[CreateChild, Run, Read, Rename, Close],
            (Self::Zellij, Some(Pane)) => &[Focus, Close],
        }
    }

    /// Command for one verb. The built line embeds what the user typed into a
    /// pane, so it is returned as zeroizing text: the caller moves it into the
    /// capture and no plaintext copy is left behind when the action ends.
    pub fn verb_command(
        self,
        windows: bool,
        session: &str,
        node: Option<&MuxNode>,
        verb: &MuxVerb,
    ) -> Option<Zeroizing<String>> {
        if !self
            .verbs_for(node.map(|node| node.kind))
            .contains(&verb_kind(verb))
        {
            return None;
        }
        // An agent the server lists without its pane is not addressable: every
        // pane verb would name an id this session does not have.
        if node.is_some_and(|node| node.orphan_agent) {
            return None;
        }
        match self {
            Self::Herdr => herdr_verb_command(windows, session, node, verb),
            Self::Zellij => zellij_verb_command(windows, session, node, verb),
        }
    }

    /// Whether a mutation actually worked. Herdr answers a failed action with
    /// an error envelope under exit code zero, so the envelope decides; a
    /// truncated capture holds at most part of that envelope, so it decides
    /// nothing and is a failure.
    pub fn action_succeeded(self, output: &str, exit_code: Option<i32>, truncated: bool) -> bool {
        match self {
            Self::Herdr => herdr_action_outcome(output, exit_code, truncated),
            Self::Zellij => !truncated && exit_code.unwrap_or(1) == 0,
        }
    }
}

fn os_type(windows: bool) -> &'static str {
    if windows { "Windows" } else { "Linux" }
}

fn verb_kind(verb: &MuxVerb) -> MuxVerbKind {
    match verb {
        MuxVerb::Focus => MuxVerbKind::Focus,
        MuxVerb::CreateChild => MuxVerbKind::CreateChild,
        MuxVerb::CreateWorkspace { .. } => MuxVerbKind::CreateWorkspace,
        MuxVerb::Rename { .. } => MuxVerbKind::Rename,
        MuxVerb::Run { .. } => MuxVerbKind::Run,
        MuxVerb::Read => MuxVerbKind::Read,
        MuxVerb::Close => MuxVerbKind::Close,
        MuxVerb::Stop => MuxVerbKind::Stop,
        MuxVerb::Delete => MuxVerbKind::Delete,
    }
}

fn herdr_status(status: HerdrStatus, exit_code: Option<i32>) -> MuxStatus {
    match status {
        // The probe ends with an echo, so a non-zero exit means the shell
        // never ran it: silence is not an available server with no sessions.
        HerdrStatus::Available if exit_code.is_some_and(|code| code != 0) => MuxStatus::Error(None),
        HerdrStatus::Available => MuxStatus::Available,
        HerdrStatus::Unavailable => MuxStatus::Unavailable,
        HerdrStatus::ServerNotRunning => MuxStatus::ServerNotRunning,
        HerdrStatus::Error { message } => MuxStatus::Error(Some(message)),
        // A capture that contradicts the probe carries no text of the server's,
        // so the panel's localized failure line is the whole message.
        HerdrStatus::Malformed | HerdrStatus::Unknown => MuxStatus::Error(None),
    }
}

fn herdr_verb_command(
    windows: bool,
    session: &str,
    node: Option<&MuxNode>,
    verb: &MuxVerb,
) -> Option<Zeroizing<String>> {
    let id = node.map(|node| node.id.clone()).unwrap_or_default();
    let kind = node.map(|node| node.kind);
    let action = match verb {
        MuxVerb::CreateWorkspace { label, cwd } => HerdrActionKind::NewWorkspace {
            label: label.clone(),
            cwd: cwd.clone().unwrap_or_default(),
        },
        MuxVerb::CreateChild => match kind? {
            MuxKind::Workspace => HerdrActionKind::CreateTab { workspace_id: id },
            MuxKind::Pane => HerdrActionKind::SplitPane { pane_id: id },
            MuxKind::Tab => return None,
        },
        MuxVerb::Focus => match kind? {
            MuxKind::Workspace => HerdrActionKind::FocusWorkspace { workspace_id: id },
            MuxKind::Tab => HerdrActionKind::FocusTab { tab_id: id },
            MuxKind::Pane => return None,
        },
        MuxVerb::Rename { label } => match kind? {
            MuxKind::Workspace => HerdrActionKind::RenameWorkspace {
                workspace_id: id,
                label: label.clone(),
            },
            MuxKind::Tab => HerdrActionKind::RenameTab {
                tab_id: id,
                label: label.clone(),
            },
            MuxKind::Pane => HerdrActionKind::RenamePane {
                pane_id: id,
                label: label.clone(),
            },
        },
        MuxVerb::Close => match kind? {
            MuxKind::Workspace => HerdrActionKind::CloseWorkspace { workspace_id: id },
            MuxKind::Tab => HerdrActionKind::CloseTab { tab_id: id },
            MuxKind::Pane => HerdrActionKind::ClosePane { pane_id: id },
        },
        MuxVerb::Run { command } => HerdrActionKind::RunPane {
            pane_id: id,
            command: command.clone(),
        },
        MuxVerb::Stop => HerdrActionKind::StopSession {
            name: session.to_string(),
        },
        MuxVerb::Delete => HerdrActionKind::DeleteSession {
            name: session.to_string(),
        },
        MuxVerb::Read => {
            return build_herdr_read_command(os_type(windows), session, &id)
                .ok()
                .map(|command| Zeroizing::new(command.command));
        }
    };
    build_herdr_action_command(os_type(windows), session, &action)
        .ok()
        .map(|command| Zeroizing::new(command.command))
}

/// `zellij attach <session name>`: the name is positional, so it keeps the
/// session grammar (spaces allowed) instead of the object-id grammar.
fn zellij_attach(windows: bool, name: &str) -> Option<String> {
    // Rejecting an option-shaped name is not the defence against the shell:
    // `script_arg` keeps the value out of the parser entirely. It is zellij's
    // own argument parser that must not read the name as one of its flags.
    if name.is_empty() || name.starts_with('-') || name.chars().any(char::is_control) {
        return None;
    }
    let mut data = Vec::new();
    let session = script_arg(windows, &mut data, name);
    if windows {
        let script = format!(
            "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); & zellij attach {session}; exit $LASTEXITCODE"
        );
        Some(windows_encoded_command(&powershell_script(&data, &script)))
    } else {
        // The bare POSIX line is deliberate: `zellij` is on `PATH`, with the
        // user's environment, only inside their own interactive shell.
        Some(posix_shell_command(&format!(
            "exec zellij attach {session}"
        )))
    }
}

fn zellij_verb_command(
    windows: bool,
    session: &str,
    node: Option<&MuxNode>,
    verb: &MuxVerb,
) -> Option<Zeroizing<String>> {
    let id = node.map(|node| node.id.as_str()).unwrap_or("");
    let (args, object): (&str, Option<&str>) = match (verb, node.map(|node| node.kind)) {
        (MuxVerb::CreateChild, None) => ("action new-tab", None),
        (MuxVerb::CreateChild, Some(MuxKind::Tab)) => ("action new-pane --tab-id", Some(id)),
        (MuxVerb::Focus, Some(MuxKind::Tab)) => ("action go-to-tab-by-id", Some(id)),
        (MuxVerb::Focus, Some(MuxKind::Pane)) => ("action focus-pane-id", Some(id)),
        (MuxVerb::Close, Some(MuxKind::Tab)) => ("action close-tab-by-id", Some(id)),
        (MuxVerb::Close, Some(MuxKind::Pane)) => ("action close-pane --pane-id", Some(id)),
        _ => return None,
    };
    zellij_line(windows, Some(session), args, object).map(Zeroizing::new)
}

/// `zellij [--session S] <static argv> [object]`. The argv is a literal from
/// this module; only the session name and the object id are host data, and
/// each gets the grammar its own kind allows.
fn zellij_line(
    windows: bool,
    session: Option<&str>,
    args: &str,
    object: Option<&str>,
) -> Option<String> {
    if let Some(session) = session {
        if session.is_empty() || session.starts_with('-') || session.chars().any(char::is_control) {
            return None;
        }
    }
    if let Some(object) = object {
        // Zellij ids are numbers or `terminal_3`; anything with whitespace or
        // option syntax is not an id this panel produced.
        if object.is_empty()
            || object.starts_with('-')
            || object.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return None;
        }
    }
    let mut data = Vec::new();
    let mut words = vec!["zellij".to_string()];
    if let Some(session) = session {
        words.push("--session".to_string());
        words.push(script_arg(windows, &mut data, session));
    }
    if !args.is_empty() {
        words.push(args.to_string());
    }
    if let Some(object) = object {
        words.push(script_arg(windows, &mut data, object));
    }
    let script = words.join(" ");
    Some(if windows {
        // PowerShell closes a single-quoted string on Unicode smart quotes as
        // well as on `'`, so the argv is built from variables the script fills
        // from base64 instead of from quoted literals.
        let body = format!(
            "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); & {script}; exit $LASTEXITCODE"
        );
        windows_encoded_command(&powershell_script(&data, &body))
    } else {
        posix_shell_command(&format!("exec {script}"))
    })
}

fn zellij_sessions(output: &str, exit_code: Option<i32>) -> MuxSnapshot {
    if exit_code == Some(127) {
        return MuxSnapshot {
            status: MuxStatus::Unavailable,
            sessions: Vec::new(),
        };
    }
    if exit_code == Some(1) && output.contains("No active zellij sessions found.") {
        return MuxSnapshot {
            status: MuxStatus::Available,
            sessions: Vec::new(),
        };
    }
    let unreadable = || MuxSnapshot {
        status: MuxStatus::Error(None),
        sessions: Vec::new(),
    };
    let mut sessions = Vec::new();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((name, suffix)) = line.rsplit_once(" [Created ") else {
            return unreadable();
        };
        let Some((_, state)) = suffix.split_once(" ago]") else {
            return unreadable();
        };
        match state.trim() {
            "" | "(current)" => sessions.push(MuxSession {
                name: name.to_string(),
                running: true,
            }),
            "(EXITED - attach to resurrect)" => sessions.push(MuxSession {
                name: name.to_string(),
                running: false,
            }),
            _ => return unreadable(),
        }
    }
    if exit_code != Some(0) {
        return unreadable();
    }
    MuxSnapshot {
        status: MuxStatus::Available,
        sessions,
    }
}

fn zellij_tree(output: &str) -> Option<Vec<MuxNode>> {
    #[derive(Deserialize)]
    struct Pane {
        id: u32,
        is_plugin: bool,
        title: String,
        tab_id: u64,
        tab_name: String,
        /// Absent on the pane of a detached or headless session, where no
        /// pane holds focus.
        #[serde(default)]
        is_focused: bool,
        /// Absent for plugin panes and on builds that do not report it.
        #[serde(default)]
        pane_cwd: Option<String>,
    }
    let panes: Vec<Pane> = serde_json::from_str(output).ok()?;
    let mut tabs: Vec<MuxNode> = Vec::new();
    for pane in panes {
        let focused = pane.is_focused;
        let id = pane.tab_id.to_string();
        let index = match tabs.iter().position(|tab| tab.id == id) {
            Some(index) => index,
            None => {
                let label = if pane.tab_name.is_empty() {
                    id.clone()
                } else {
                    pane.tab_name.clone()
                };
                tabs.push(MuxNode::new(MuxKind::Tab, id.clone(), label));
                tabs.len() - 1
            }
        };
        let prefix = if pane.is_plugin { "plugin" } else { "terminal" };
        let mut node = MuxNode::new(
            MuxKind::Pane,
            format!("{prefix}_{}", pane.id),
            pane.title.clone(),
        );
        node.title = Some(pane.title);
        node.focused = focused;
        node.cwd = pane.pane_cwd;
        tabs[index].children.push(node);
    }
    for tab in &mut tabs {
        tab.pane_count = tab.children.len();
        // Zellij reports focus per pane, so a tab holds the focused marker
        // only through the pane inside it that the server marked.
        tab.focused = tab.children.iter().any(|child| child.focused);
    }
    Some(tabs)
}

/// Workspaces, tabs, panes and agents arrive as four flat lists joined by
/// parent ids; a label prefers the name a user set over anything derived.
/// Agents are indexed once because every pane row and every orphan lookup
/// reads that list.
fn herdr_tree(detail: &HerdrSessionDetail) -> Vec<MuxNode> {
    let agents = detail.agent_join();
    let mut roots: Vec<MuxNode> = detail
        .workspaces
        .iter()
        .map(|workspace| {
            let mut node = MuxNode::new(
                MuxKind::Workspace,
                workspace.workspace_id.clone(),
                workspace.label.clone(),
            );
            node.focused = workspace.focused;
            node.status = workspace.agent_status.clone();
            node.tab_count = workspace.tab_count as usize;
            node.pane_count = workspace.pane_count as usize;
            node
        })
        .collect();
    for tab in &detail.tabs {
        let mut node = MuxNode::new(
            MuxKind::Tab,
            tab.tab_id.clone(),
            tab.label
                .clone()
                .unwrap_or_else(|| format!("#{}", tab.number)),
        );
        node.focused = tab.focused;
        node.status = tab.agent_status.clone();
        node.pane_count = tab.pane_count as usize;
        match roots.iter().position(|workspace| {
            workspace.kind == MuxKind::Workspace && workspace.id == tab.workspace_id
        }) {
            Some(index) => roots[index].children.push(node),
            // A tab whose workspace is missing would vanish along with its
            // panes; keep it addressable at the top level instead.
            None => roots.push(node),
        }
    }
    for pane in &detail.panes {
        let agent = agents.for_pane(&pane.pane_id);
        let mut node = MuxNode::new(
            MuxKind::Pane,
            pane.pane_id.clone(),
            pane.label
                .clone()
                .or(pane.terminal_title.clone())
                .or_else(|| agent.and_then(|agent| agent.label.clone()))
                .or(pane.cwd.clone())
                .unwrap_or_else(|| pane.pane_id.clone()),
        );
        node.focused = pane.focused;
        node.restore_failed = pane.restore_failed;
        node.title = pane.terminal_title.clone();
        node.cwd = pane.cwd.clone();
        node.status = pane
            .agent_status
            .clone()
            .or_else(|| agent.and_then(|agent| agent.status.clone()));
        node.agent = agent.map(|agent| MuxAgent {
            pane_id: agent.pane_id.clone(),
            label: agent.label.clone(),
            status: agent.status.clone(),
        });
        // Tabs normally live under their workspace, but a tab whose workspace
        // was never listed sits at the root. A pane has to follow whichever
        // shape its own tab has, or it is detached from a parent it names.
        let parent = roots
            .iter()
            .enumerate()
            .find_map(|(workspace_index, root)| {
                root.children
                    .iter()
                    .position(|child| {
                        child.kind == MuxKind::Tab && Some(&child.id) == pane.tab_id.as_ref()
                    })
                    .map(|tab_index| (workspace_index, Some(tab_index)))
            })
            .or_else(|| {
                roots
                    .iter()
                    .position(|root| {
                        root.kind == MuxKind::Tab && Some(&root.id) == pane.tab_id.as_ref()
                    })
                    .map(|root_index| (root_index, None))
            });
        match parent {
            Some((root_index, Some(tab_index))) => {
                roots[root_index].children[tab_index].children.push(node)
            }
            Some((root_index, None)) => roots[root_index].children.push(node),
            // Panes that name no known tab still belong to their workspace,
            // and a pane with no known parent stays reachable at the top.
            None => match roots
                .iter()
                .position(|root| root.kind == MuxKind::Workspace && root.id == pane.workspace_id)
            {
                Some(index) => roots[index].children.push(node),
                None => roots.push(node),
            },
        }
    }
    for agent in agents.orphans() {
        let mut node = MuxNode::new(
            MuxKind::Pane,
            agent.pane_id.clone(),
            agent.label.clone().unwrap_or_else(|| agent.pane_id.clone()),
        );
        node.orphan_agent = true;
        node.status = agent.status.clone();
        node.agent = Some(MuxAgent {
            pane_id: agent.pane_id.clone(),
            label: agent.label.clone(),
            status: agent.status.clone(),
        });
        roots.push(node);
    }
    roots
}

/// Sidebar search runs case-insensitively over the values a row shows, so a
/// node matches exactly when one of its visible values matches. The query is
/// normalized here so callers can pass the raw input box text.
pub fn node_matches_query(node: &MuxNode, query: &str) -> bool {
    let query = normalize_query(query);
    match_query(node, &query)
}

/// A branch is visible when it matches or when anything below it does, which
/// is what lets a search reveal a pane the user never expanded.
pub fn subtree_matches_query(node: &MuxNode, query: &str) -> bool {
    let query = normalize_query(query);
    match_subtree(node, &query)
}

/// An unexpanded session has no loaded descendants, so only its name can
/// match; the panel must not fetch a subtree just to search it.
pub fn session_matches_query(session: &MuxSession, tree: Option<&[MuxNode]>, query: &str) -> bool {
    let query = normalize_query(query);
    if query.is_empty() {
        return true;
    }
    session.name.to_lowercase().contains(query.as_str())
        || tree.is_some_and(|tree| tree.iter().any(|node| match_subtree(node, &query)))
}

fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

fn match_query(node: &MuxNode, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    node.id.to_lowercase().contains(query)
        || node.label.to_lowercase().contains(query)
        || contains_query(query, node.title.as_deref())
        || contains_query(query, node.cwd.as_deref())
        || contains_query(query, node.status.as_deref())
        || node.agent.as_ref().is_some_and(|agent| {
            contains_query(query, agent.label.as_deref())
                || contains_query(query, agent.status.as_deref())
        })
}

fn match_subtree(node: &MuxNode, query: &str) -> bool {
    match_query(node, query)
        || node
            .children
            .iter()
            .any(|child| match_subtree(child, query))
}

fn contains_query(query: &str, value: Option<&str>) -> bool {
    value.is_some_and(|value| value.to_lowercase().contains(query))
}

/// Names an agent status for the panel. Unknown statuses have no chip.
pub fn status_label_key(status: &str) -> &'static str {
    match status {
        "working" => "sidebar.multiplexer.status.working",
        "blocked" => "sidebar.multiplexer.status.blocked",
        "idle" => "sidebar.multiplexer.status.idle",
        "done" => "sidebar.multiplexer.status.done",
        _ => "sidebar.multiplexer.status.unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKSPACES: &str = r#"{"id":"cli:workspace:list","result":{"workspaces":[{"agent_status":"working","focused":true,"label":"helphub","number":1,"pane_count":7,"tab_count":2,"workspace_id":"wA"},{"focused":false,"label":"dbx","number":2,"pane_count":2,"tab_count":1,"workspace_id":"wD"}]}}"#;
    const TABS: &str = r#"{"id":"cli:tab:list","result":{"tabs":[{"agent_status":"idle","focused":true,"label":"backend","number":11,"pane_count":1,"tab_id":"wA:tB","workspace_id":"wA"},{"focused":false,"number":12,"pane_count":1,"tab_id":"wA:tA","workspace_id":"wA"},{"focused":false,"label":"ci","number":13,"pane_count":1,"tab_id":"wD:t9","workspace_id":"wD"}]}}"#;
    const PANES: &str = r#"{"id":"cli:pane:list","result":{"panes":[{"agent":"omp","agent_status":"working","cwd":"C:\\Users\\me\\project\\helphub","focused":true,"pane_id":"wA:tB:p1","tab_id":"wA:tB","terminal_title":"omp","workspace_id":"wA"},{"agent_status":"unknown","cwd":"/gone/dir","label":"1324","focused":false,"pane_id":"wA:tA:p2","restore_error":"Saved directory is unavailable.","tab_id":"wA:tA","workspace_id":"wA"},{"label":"runner","focused":false,"pane_id":"wD:t9:p1","tab_id":"wD:t9","workspace_id":"wD"}]}}"#;
    const AGENTS: &str = r#"{"id":"cli:agent:list","result":{"agents":[{"label":"claude","pane_id":"wA:tB:p1","status":"working","workspace_id":"wA"},{"label":"ghost","pane_id":"wZ:tZ:pZ","status":"blocked","workspace_id":"wZ"}]}}"#;

    fn detail_output() -> String {
        format!(
            "===HERDR===\n===HERDR_WORKSPACES===\n{WORKSPACES}\n===HERDR_TABS===\n{TABS}\n===HERDR_PANES===\n{PANES}\n===HERDR_AGENTS===\n{AGENTS}\n===HERDR_END==="
        )
    }

    #[test]
    fn herdr_detail_becomes_a_workspace_tab_pane_tree_with_agents() {
        let tree = Multiplexer::Herdr.parse_tree(&detail_output()).unwrap();

        // Two workspaces plus one orphan agent: nothing in the answer is
        // dropped, even when a parent id is missing.
        assert_eq!(tree.len(), 3);
        let helphub = &tree[0];
        assert_eq!(helphub.kind, MuxKind::Workspace);
        assert!(helphub.focused);
        assert_eq!(helphub.status.as_deref(), Some("working"));
        assert_eq!(helphub.tab_count, 2);
        assert_eq!(helphub.children.len(), 2);

        let tab = &helphub.children[0];
        assert_eq!(tab.id, "wA:tB");
        assert_eq!(tab.label, "backend");
        let pane = &tab.children[0];
        assert_eq!(pane.label, "omp", "the terminal title is the display label");
        assert_eq!(pane.status.as_deref(), Some("working"));
        assert_eq!(
            pane.agent.as_ref().unwrap().label.as_deref(),
            Some("claude")
        );
        assert_eq!(pane.cwd.as_deref(), Some("C:\\Users\\me\\project\\helphub"));

        // A tab with no label falls back to its number.
        let unnamed = &helphub.children[1];
        assert_eq!(unnamed.label, "#12");
        assert_eq!(
            unnamed.children[0].label, "1324",
            "an explicit pane name wins"
        );
        assert!(unnamed.children[0].restore_failed);

        let dbx = &tree[1];
        assert_eq!(dbx.label, "dbx");
        // `dbx` owns the tab listed under it: attaching every tab to the first
        // workspace would leave this row empty and hide its pane.
        assert_eq!(dbx.children.len(), 1);
        assert_eq!(dbx.children[0].id, "wD:t9");
        assert_eq!(dbx.children[0].children[0].id, "wD:t9:p1");
        let orphan_agent = &tree[2];
        assert!(orphan_agent.orphan_agent);
        assert_eq!(orphan_agent.label, "ghost");
        assert_eq!(orphan_agent.status.as_deref(), Some("blocked"));
    }

    #[test]
    fn herdr_probe_status_comes_from_the_envelope_not_the_exit_code() {
        let missing = Multiplexer::Herdr.parse_sessions("__OXIDE_HERDR_UNAVAILABLE__", Some(0));
        assert_eq!(missing.status, MuxStatus::Unavailable);

        let down = Multiplexer::Herdr.parse_sessions(
            "===HERDR_SESSIONS===\n{\"error\":{\"code\":\"server_not_running\",\"message\":\"no herdr server is running\"}}\n===HERDR_END===",
            Some(0),
        );
        assert_eq!(down.status, MuxStatus::ServerNotRunning);
        assert!(down.sessions.is_empty());

        let listed = Multiplexer::Herdr.parse_sessions(
            "===HERDR_SESSIONS===\n{\"sessions\":[{\"name\":\"default\",\"running\":true},{\"name\":\"work space\",\"running\":false}]}\n===HERDR_END===",
            Some(0),
        );
        assert_eq!(listed.status, MuxStatus::Available);
        assert_eq!(
            listed.sessions,
            vec![
                MuxSession {
                    name: "default".to_string(),
                    running: true
                },
                MuxSession {
                    name: "work space".to_string(),
                    running: false
                }
            ]
        );

        let failed = Multiplexer::Herdr.parse_sessions(
            "===HERDR_SESSIONS===\n{\"error\":{\"code\":\"invalid_params\",\"message\":\"bad socket\"}}\n===HERDR_END===",
            Some(0),
        );
        assert_eq!(
            failed.status,
            MuxStatus::Error(Some("bad socket".to_string()))
        );

        // Silence with a failed exit code must not look like an empty list.
        assert_eq!(
            Multiplexer::Herdr.parse_sessions("", Some(1)).status,
            MuxStatus::Error(None)
        );
    }

    #[test]
    fn zellij_sessions_distinguish_resurrection_from_running() {
        let snapshot = Multiplexer::Zellij.parse_sessions(
            "dev [Created 2 hours ago] (current)\nwork space [Created 3 days ago] (EXITED - attach to resurrect)\n",
            Some(0),
        );
        assert_eq!(snapshot.status, MuxStatus::Available);
        assert_eq!(
            snapshot.sessions,
            vec![
                MuxSession {
                    name: "dev".to_string(),
                    running: true
                },
                MuxSession {
                    name: "work space".to_string(),
                    running: false
                }
            ],
            "a spaced session name keeps every character it was listed with"
        );

        assert_eq!(
            Multiplexer::Zellij.parse_sessions("", Some(127)).status,
            MuxStatus::Unavailable
        );
        let empty = Multiplexer::Zellij.parse_sessions("No active zellij sessions found.", Some(1));
        assert_eq!(empty.status, MuxStatus::Available);
        assert!(empty.sessions.is_empty());
        // An unrecognized state string is not a session.
        assert_eq!(
            Multiplexer::Zellij
                .parse_sessions("weird [Created 1 minute ago] (unknown)", Some(0))
                .status,
            MuxStatus::Error(None)
        );
    }

    /// A tab whose workspace is missing is kept at the root, and its panes
    /// belong to that tab rather than being detached beside it.
    #[test]
    fn herdr_tree_keeps_panes_under_a_root_tab() {
        let output = concat!(
            "===HERDR===\n",
            "===HERDR_WORKSPACES===\n",
            "{\"result\":{\"workspaces\":[]}}\n",
            "===HERDR_TABS===\n",
            "{\"result\":{\"tabs\":[{\"tab_id\":\"wZ:t1\",\"workspace_id\":\"wZ\",\"label\":\"orphan tab\",\"number\":4,\"pane_count\":1}]}}\n",
            "===HERDR_PANES===\n",
            "{\"result\":{\"panes\":[{\"pane_id\":\"wZ:t1:p1\",\"workspace_id\":\"wZ\",\"tab_id\":\"wZ:t1\",\"label\":\"shell\"}]}}\n",
            "===HERDR_AGENTS===\n",
            "{\"result\":{\"agents\":[]}}\n",
            "===HERDR_END===",
        );
        let tree = Multiplexer::Herdr.parse_tree(output).unwrap();
        assert_eq!(tree.len(), 1, "only the tab is addressable");
        assert_eq!(tree[0].kind, MuxKind::Tab);
        assert_eq!(
            tree[0].children[0].id, "wZ:t1:p1",
            "the pane stays under its tab"
        );
    }

    #[test]
    fn zellij_tree_groups_panes_by_tab_id() {
        // The tab ids interleave: grouping only consecutive rows would put
        // pane 6 in the wrong tab while still producing two tabs.
        let tree = Multiplexer::Zellij
            .parse_tree(
                r#"[{"id":3,"is_plugin":false,"title":"shell","tab_id":1,"tab_name":"main"},{"id":5,"is_plugin":false,"title":"build","tab_id":2,"tab_name":"work","is_focused":true,"pane_cwd":"/srv/build"},{"id":4,"is_plugin":true,"title":"status bar","tab_id":1,"tab_name":"main"},{"id":6,"is_plugin":false,"title":"tests","tab_id":2,"tab_name":"work"}]"#,
            )
            .unwrap();

        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].id, "1");
        assert_eq!(tree[0].label, "main");
        assert_eq!(tree[0].pane_count, 2);
        assert_eq!(tree[0].children[0].id, "terminal_3");
        assert_eq!(tree[0].children[1].id, "plugin_4");
        assert_eq!(tree[1].id, "2");
        assert_eq!(tree[1].pane_count, 2);
        assert_eq!(tree[1].children[0].id, "terminal_5");
        assert_eq!(tree[1].children[1].id, "terminal_6");
        // The focused pane carries its marker up to its tab, and its working
        // directory is searchable from the row.
        assert!(tree[1].focused);
        assert!(!tree[0].focused);
        assert_eq!(
            tree[1].children[0].cwd.as_deref(),
            Some("/srv/build"),
            "zellij reports the pane working directory"
        );
    }

    #[test]
    fn verbs_are_offered_only_where_the_provider_implements_them() {
        use MuxKind::*;
        use MuxVerbKind::*;
        // The whole table, provider by level: a verb the provider cannot
        // execute must never be laid out.
        assert_eq!(
            Multiplexer::Herdr.verbs_for(None),
            &[CreateWorkspace, Stop, Delete]
        );
        assert_eq!(Multiplexer::Zellij.verbs_for(None), &[CreateChild]);
        assert_eq!(
            Multiplexer::Herdr.verbs_for(Some(Workspace)),
            &[Focus, CreateChild, Rename, Close]
        );
        assert_eq!(Multiplexer::Zellij.verbs_for(Some(Workspace)), &[]);
        assert_eq!(
            Multiplexer::Herdr.verbs_for(Some(Tab)),
            &[Focus, Rename, Close]
        );
        assert_eq!(
            Multiplexer::Zellij.verbs_for(Some(Tab)),
            &[Focus, CreateChild, Close]
        );
        assert_eq!(
            Multiplexer::Herdr.verbs_for(Some(Pane)),
            &[CreateChild, Run, Read, Rename, Close]
        );
        assert_eq!(Multiplexer::Zellij.verbs_for(Some(Pane)), &[Focus, Close]);

        let pane = MuxNode::new(Pane, "wA:tB:p1".to_string(), "omp".to_string());
        // An unsupported verb never reaches the shell, even with a valid node.
        assert_eq!(
            Multiplexer::Zellij.verb_command(false, "dev", Some(&pane), &MuxVerb::Read),
            None
        );
        assert_eq!(
            Multiplexer::Herdr.verb_command(false, "dev", Some(&pane), &MuxVerb::Focus),
            None,
            "herdr focuses a pane through its window manager, not the CLI"
        );
        let session_level = Multiplexer::Zellij
            .verb_command(false, "dev", None, &MuxVerb::CreateChild)
            .unwrap();
        assert_eq!(
            session_level.as_str(),
            r#"/bin/sh -c 'exec zellij --session '"'"'dev'"'"' action new-tab'"#
        );
    }

    #[test]
    fn herdr_verbs_reach_the_cli_through_the_protocol_layer() {
        let workspace = MuxNode::new(MuxKind::Workspace, "wA".to_string(), "helphub".to_string());
        let tab = MuxNode::new(MuxKind::Tab, "wA:tB".to_string(), "backend".to_string());
        let pane = MuxNode::new(MuxKind::Pane, "wA:tB:p1".to_string(), "omp".to_string());

        assert_eq!(
            Multiplexer::Herdr
                .verb_command(
                    false,
                    "work space",
                    None,
                    &MuxVerb::CreateWorkspace {
                        label: "web".to_string(),
                        cwd: Some("/srv/web".to_string()),
                    },
                )
                .unwrap()
                .as_str(),
            "herdr --session 'work space' workspace create --label 'web' --cwd '/srv/web' 2>&1"
        );
        assert_eq!(
            Multiplexer::Herdr
                .verb_command(false, "dev", Some(&workspace), &MuxVerb::Focus)
                .unwrap()
                .as_str(),
            "herdr --session 'dev' workspace focus wA 2>&1"
        );
        assert_eq!(
            Multiplexer::Herdr
                .verb_command(false, "dev", Some(&workspace), &MuxVerb::CreateChild)
                .unwrap()
                .as_str(),
            "herdr --session 'dev' tab create --workspace wA 2>&1"
        );
        assert_eq!(
            Multiplexer::Herdr
                .verb_command(
                    false,
                    "dev",
                    Some(&tab),
                    &MuxVerb::Rename {
                        label: "it's mine".to_string()
                    },
                )
                .unwrap()
                .as_str(),
            "herdr --session 'dev' tab rename wA:tB 'it'\"'\"'s mine' 2>&1"
        );
        assert_eq!(
            Multiplexer::Herdr
                .verb_command(
                    false,
                    "dev",
                    Some(&pane),
                    &MuxVerb::Run {
                        command: Zeroizing::new("ls -la".to_string())
                    },
                )
                .unwrap()
                .as_str(),
            "herdr --session 'dev' pane run wA:tB:p1 'ls -la' 2>&1"
        );
        assert_eq!(
            Multiplexer::Herdr
                .verb_command(false, "dev", Some(&pane), &MuxVerb::CreateChild)
                .unwrap()
                .as_str(),
            "herdr --session 'dev' pane split wA:tB:p1 --direction right 2>&1"
        );
        assert_eq!(
            Multiplexer::Herdr
                .verb_command(false, "work space", None, &MuxVerb::Stop)
                .unwrap()
                .as_str(),
            "herdr session stop 'work space' 2>&1"
        );
        assert_eq!(
            Multiplexer::Herdr
                .verb_command(false, "dev", Some(&pane), &MuxVerb::Read)
                .unwrap()
                .as_str(),
            "herdr --session 'dev' pane read wA:tB:p1 --lines 500 --source recent --format text 2>&1"
        );

        // An agent listed without its pane has no addressable object, so every
        // pane verb is refused even though the kind offers them.
        let mut orphan = pane;
        orphan.orphan_agent = true;
        for verb in [
            MuxVerb::CreateChild,
            MuxVerb::Rename {
                label: "x".to_string(),
            },
            MuxVerb::Read,
            MuxVerb::Close,
        ] {
            assert_eq!(
                Multiplexer::Herdr.verb_command(false, "dev", Some(&orphan), &verb),
                None,
                "an orphan agent row must not send a pane verb at a missing id"
            );
        }
    }

    #[test]
    fn zellij_verbs_keep_their_option_boundary() {
        let tab = MuxNode::new(MuxKind::Tab, "1".to_string(), "main".to_string());
        let line = Multiplexer::Zellij
            .verb_command(false, "dev", Some(&tab), &MuxVerb::Close)
            .unwrap();
        assert_eq!(
            // The whole script is quoted for `/bin/sh -c`, so inner quotes
            // appear in their POSIX escape form.
            line.as_str(),
            r#"/bin/sh -c 'exec zellij --session '"'"'dev'"'"' action close-tab-by-id '"'"'1'"'"''"#
        );

        // A session name that looks like an option is refused, not passed on.
        assert_eq!(Multiplexer::Zellij.detail_command(false, "--force"), None);
        assert_eq!(
            Multiplexer::Zellij.attach_command(false, Some("--force")),
            None
        );
        assert_eq!(
            Multiplexer::Zellij.attach_command(false, None).unwrap(),
            "/bin/sh -c 'exec zellij'"
        );
        let attach = Multiplexer::Zellij
            .attach_command(false, Some("dev"))
            .unwrap();
        assert_eq!(
            attach, r#"/bin/sh -c 'exec zellij attach '"'"'dev'"'"''"#,
            "attach addresses the session positionally"
        );
        assert_eq!(
            Multiplexer::Zellij.new_session_command(false, "dev"),
            None,
            "zellij cannot create a session non-interactively"
        );
    }

    #[test]
    fn host_authored_ids_cannot_smuggle_a_second_command() {
        let hostile = MuxNode::new(MuxKind::Pane, "wA; rm -rf /".to_string(), "x".to_string());
        assert_eq!(
            Multiplexer::Herdr.verb_command(false, "dev", Some(&hostile), &MuxVerb::Close),
            None
        );
        assert_eq!(
            Multiplexer::Herdr.read_command(false, "dev", &hostile),
            None
        );
        assert_eq!(
            Multiplexer::Herdr.agent_attach_command(false, "dev", &hostile),
            None
        );
        assert_eq!(
            Multiplexer::Herdr.detail_command(false, "dev\nrm -rf /"),
            None
        );

        let zellij_hostile = MuxNode::new(MuxKind::Tab, "1 -x".to_string(), "main".to_string());
        assert_eq!(
            Multiplexer::Zellij.verb_command(false, "dev", Some(&zellij_hostile), &MuxVerb::Close),
            None
        );
    }

    #[test]
    fn windows_payloads_survive_both_default_shells() {
        let tab = MuxNode::new(MuxKind::Tab, "wA:tB".to_string(), "backend".to_string());
        let lines = [
            Multiplexer::Herdr
                .verb_command(
                    true,
                    "dev",
                    Some(&tab),
                    &MuxVerb::Rename {
                        label: "it's mine".to_string(),
                    },
                )
                .expect("herdr rename"),
            Multiplexer::Zellij
                .verb_command(true, "dev", Some(&tab), &MuxVerb::Close)
                .expect("zellij close"),
        ];
        for line in lines {
            assert!(line.starts_with("powershell -NoProfile"), "{line:?}");
            assert!(line.contains("-EncodedCommand "), "{line:?}");
            // Nothing for cmd.exe or PowerShell to rewrite before the payload.
            assert!(!line.contains('$'), "{line:?}");
            assert!(!line.contains('"'), "{line:?}");
        }
    }

    #[test]
    fn search_reveals_a_branch_only_through_a_matching_descendant() {
        let mut tab = MuxNode::new(MuxKind::Tab, "wA:tB".to_string(), "backend".to_string());
        let mut pane = MuxNode::new(MuxKind::Pane, "wA:tB:p1".to_string(), "omp".to_string());
        pane.cwd = Some("/srv/helphub".to_string());
        pane.agent = Some(MuxAgent {
            pane_id: "wA:tB:p1".to_string(),
            label: Some("claude".to_string()),
            status: Some("working".to_string()),
        });
        tab.children.push(pane);
        let mut root = MuxNode::new(MuxKind::Workspace, "wA".to_string(), "helphub".to_string());
        root.children.push(tab);
        let session = MuxSession {
            name: "work".to_string(),
            running: true,
        };
        let tree = [root];

        // The name does not match; only the loaded descendants can reveal it.
        assert!(!session_matches_query(&session, None, "helphub"));
        assert!(session_matches_query(&session, Some(&tree), "helphub"));
        assert!(session_matches_query(&session, Some(&tree), "WORK"));
        assert!(session_matches_query(&session, Some(&tree), "wA:tB:p1"));
        assert!(session_matches_query(&session, Some(&tree), "claude"));
        assert!(session_matches_query(&session, Some(&tree), "working"));
        assert!(!session_matches_query(&session, Some(&tree), "nomatch"));
        assert!(session_matches_query(&session, Some(&tree), ""));
    }
    #[test]
    fn status_keys_name_every_agent_state() {
        assert_eq!(
            status_label_key("working"),
            "sidebar.multiplexer.status.working"
        );
        assert_eq!(
            status_label_key("blocked"),
            "sidebar.multiplexer.status.blocked"
        );
        assert_eq!(status_label_key("idle"), "sidebar.multiplexer.status.idle");
        assert_eq!(status_label_key("done"), "sidebar.multiplexer.status.done");
        // An unrecognized status is not silently reported as working.
        assert_eq!(
            status_label_key("WORKING"),
            "sidebar.multiplexer.status.unknown"
        );
    }

    #[test]
    fn action_success_reads_the_envelope_for_herdr_and_the_exit_for_zellij() {
        assert!(Multiplexer::Herdr.action_succeeded("{\"result\":{\"ok\":true}}", Some(0), false));
        assert!(!Multiplexer::Herdr.action_succeeded(
            "{\"error\":{\"code\":\"invalid_params\",\"message\":\"no\"}}",
            Some(0),
            false
        ));
        // A truncated capture holds at most part of the envelope, so it proves
        // nothing about the action.
        assert!(
            !Multiplexer::Herdr.action_succeeded("{\"result\":{\"ok\":true}}", Some(0), true),
            "truncated output is never success"
        );
        assert!(Multiplexer::Zellij.action_succeeded("", Some(0), false));
        assert!(!Multiplexer::Zellij.action_succeeded("", Some(1), false));
        assert!(
            !Multiplexer::Zellij.action_succeeded("", None, false),
            "an unknown exit code is not success"
        );
        assert!(
            !Multiplexer::Zellij.action_succeeded("", Some(0), true),
            "truncated output is never success"
        );
    }

    /// The Zellij session name comes from the server and the line is typed into
    /// a terminal whose shell is not chosen here, so a value may only travel as
    /// data the script decodes: neither `cmd.exe` nor PowerShell may see any of
    /// it, and PowerShell would treat a U+2019 in a quoted literal as the end of
    /// that literal.
    #[test]
    fn zellij_windows_values_travel_as_data() {
        use crate::shell::{
            HOSTILE_VALUE, assert_inert_windows_line, decoded_powershell_data,
            decoded_windows_script,
        };

        let tab = MuxNode::new(MuxKind::Tab, "wA:tB".to_string(), "backend".to_string());
        let attach = Multiplexer::Zellij
            .attach_command(true, Some(HOSTILE_VALUE))
            .expect("zellij attach");
        assert_inert_windows_line(&attach);
        let script = decoded_windows_script(&attach);
        assert_eq!(decoded_powershell_data(&script), [HOSTILE_VALUE]);
        assert!(
            script.ends_with(concat!(
                "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); ",
                "& zellij attach $v0; exit $LASTEXITCODE"
            )),
            "{script}"
        );

        let close = Multiplexer::Zellij
            .verb_command(true, HOSTILE_VALUE, Some(&tab), &MuxVerb::Close)
            .expect("zellij close tab");
        assert_inert_windows_line(close.as_str());
        let script = decoded_windows_script(close.as_str());
        // Session name, then tab id: one binding each, in argument order.
        assert_eq!(decoded_powershell_data(&script), [HOSTILE_VALUE, "wA:tB"]);
        assert!(
            script.ends_with(concat!(
                "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); ",
                "& zellij --session $v0 action close-tab-by-id $v1; exit $LASTEXITCODE"
            )),
            "{script}"
        );

        // The value survives only inside the base64 literals.
        for line in [attach, close.as_str().to_string()] {
            let script = decoded_windows_script(&line);
            for fragment in [HOSTILE_VALUE, "& calc", "$(id)", "’"] {
                assert!(!script.contains(fragment), "{fragment:?} in {script}");
            }
        }
    }
}
