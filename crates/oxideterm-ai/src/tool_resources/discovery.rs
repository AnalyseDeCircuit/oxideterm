use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

use oxideterm_workspace::{ActiveSessionReadiness, PaneId, TabId, TabKind, TerminalSessionId};

use super::{AiOrchestratorTarget, ai_target_from_projection};
use crate::{AiIdeTargetInput, AiSftpTargetInput};
use crate::{AiRuntimeAuthority, RuntimeHandleProjection, ToolSessionId};

pub fn ai_tab_kind_label(kind: &TabKind) -> &'static str {
    match kind {
        TabKind::Workspace => "workspace",
        TabKind::LocalTerminal => "local_terminal",
        TabKind::SshTerminal => "terminal",
        TabKind::MoshTerminal => "mosh_terminal",
        TabKind::Sftp => "sftp",
        TabKind::Ide => "ide",
        TabKind::Forwards => "forwards",
        TabKind::Settings => "settings",
        TabKind::FileManager => "file_manager",
        TabKind::SessionManager => "session_manager",
        TabKind::Runtime => "runtime",
        TabKind::ConnectionPool => "runtime",
        TabKind::Topology => "topology",
        TabKind::Graphics => "graphics",
        TabKind::NotificationCenter => "notifications",
        TabKind::PluginManager => "plugin_manager",
        TabKind::Plugin { .. } => "plugin",
        TabKind::CloudSync => "cloud_sync",
        TabKind::Knowledge => "knowledge",
        TabKind::RemoteDesktop => "remote_desktop",
        TabKind::Onboarding => "onboarding",
    }
}

pub fn ai_saved_connection_target(
    id: &str,
    name: &str,
    host: &str,
    username: &str,
    port: u16,
    group: &Option<String>,
) -> AiOrchestratorTarget {
    let mut refs = BTreeMap::new();
    refs.insert("connectionId".to_string(), id.to_owned());
    let connection_label = if name.trim().is_empty() { host } else { name };
    AiOrchestratorTarget {
        id: format!("saved-connection:{}", id),
        kind: "saved-connection".to_string(),
        label: format!("{} ({}@{}:{})", connection_label, username, host, port),
        state: "available".to_string(),
        capabilities: vec!["navigation.open".to_string(), "state.list".to_string()],
        refs,
        metadata: serde_json::json!({
            "host": host,
            "port": port,
            "username": username,
            "name": name,
            "group": group,
        }),
        terminal_buffer: None,
        terminal_screen: None,
    }
}

pub fn ai_app_surface_target(
    tab_id: TabId,
    kind: &TabKind,
    title: &str,
    session_id: Option<TerminalSessionId>,
    visible: bool,
) -> AiOrchestratorTarget {
    let mut refs = BTreeMap::new();
    refs.insert("tabId".to_string(), tab_id.0.to_string());
    if let Some(session_id) = session_id {
        refs.insert("sessionId".to_string(), session_id.0.to_string());
    }
    AiOrchestratorTarget {
        id: format!("app-surface:{}:{}", ai_tab_kind_label(kind), tab_id.0),
        kind: "app-surface".to_string(),
        label: if title.is_empty() {
            ai_tab_kind_label(kind).to_string()
        } else {
            title.to_owned()
        },
        state: if visible { "connected" } else { "available" }.to_string(),
        capabilities: vec!["navigation.open".to_string(), "state.list".to_string()],
        refs,
        metadata: serde_json::json!({ "tabType": ai_tab_kind_label(kind) }),
        terminal_buffer: None,
        terminal_screen: None,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum AiTerminalTransport {
    Local,
    Serial,
    Telnet,
    Ble,
    Other,
}

pub struct AiSerialTargetInput<'a> {
    pub port_path: &'a str,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: String,
    pub flow_control: String,
}

pub struct AiTerminalTargetInput<'a> {
    pub session_id: TerminalSessionId,
    pub tab_id: TabId,
    pub pane_id: PaneId,
    pub pane_label: String,
    pub transport: AiTerminalTransport,
    pub serial: Option<AiSerialTargetInput<'a>>,
    pub terminal_buffer: String,
    pub terminal_screen: serde_json::Value,
    pub accepts_input: bool,
    pub terminal_running: bool,
}

pub fn ai_terminal_target(input: AiTerminalTargetInput<'_>) -> AiOrchestratorTarget {
    let AiTerminalTargetInput {
        session_id,
        tab_id,
        pane_id,
        pane_label,
        transport,
        serial,
        terminal_buffer,
        terminal_screen,
        accepts_input,
        terminal_running,
    } = input;
    let is_local_terminal = transport == AiTerminalTransport::Local;
    let is_serial_terminal = transport == AiTerminalTransport::Serial;
    let is_telnet_terminal = transport == AiTerminalTransport::Telnet;
    let is_ble_terminal = transport == AiTerminalTransport::Ble;
    let terminal_type = if is_ble_terminal {
        "ble"
    } else if is_serial_terminal {
        "serial"
    } else if is_telnet_terminal {
        "telnet"
    } else if is_local_terminal {
        "local_terminal"
    } else {
        "terminal"
    };
    let mut refs = BTreeMap::new();
    refs.insert("sessionId".to_string(), session_id.0.to_string());
    refs.insert("tabId".to_string(), tab_id.0.to_string());
    let label = if let Some(config) = &serial {
        format!("Serial {}", config.port_path)
    } else if is_telnet_terminal {
        format!("Telnet {} · #{}", pane_label, pane_id.0)
    } else if is_local_terminal {
        format!("Local terminal {} · #{}", pane_label, pane_id.0)
    } else {
        format!("SSH terminal {} · #{}", pane_label, pane_id.0)
    };
    let metadata = if let Some(config) = &serial {
        serde_json::json!({
            "terminalType": terminal_type,
            "terminalTransport": "serial",
            "portPath": config.port_path,
            "baudRate": config.baud_rate,
            "dataBits": config.data_bits,
            "stopBits": config.stop_bits,
            "parity": config.parity,
            "flowControl": config.flow_control,
        })
    } else if is_telnet_terminal {
        serde_json::json!({
            "paneId": pane_id.0,
            "terminalType": terminal_type,
            "terminalTransport": "telnet",
        })
    } else if is_local_terminal {
        // Tauri's local terminal store overwrites registry metadata
        // with shell-oriented metadata instead of pane internals.
        serde_json::json!({
            "terminalType": terminal_type,
            "shell": {
                "label": pane_label,
            },
        })
    } else {
        serde_json::json!({
            "paneId": pane_id.0,
            "terminalType": terminal_type,
        })
    };
    AiOrchestratorTarget {
        id: format!("terminal-session:{}", session_id.0),
        kind: "terminal-session".to_string(),
        label,
        state: if is_local_terminal {
            if terminal_running {
                "connected"
            } else {
                "stale"
            }
        } else if accepts_input {
            "connected"
        } else {
            "opening"
        }
        .to_string(),
        capabilities: {
            let mut capabilities = vec![
                "terminal.observe".to_string(),
                "terminal.send".to_string(),
                "terminal.wait".to_string(),
                "state.list".to_string(),
            ];
            if is_serial_terminal || is_telnet_terminal {
                capabilities.push("transport.state".to_string());
            }
            if is_serial_terminal {
                capabilities.push("serial.control".to_string());
            }
            capabilities
        },
        refs,
        metadata,
        terminal_buffer: Some(terminal_buffer),
        terminal_screen: Some(terminal_screen),
    }
}

/// Snapshots are collected by the runtime owners before resource policy runs.
pub enum AiTargetCapture<'a> {
    SavedConnection {
        id: &'a str,
        name: &'a str,
        host: &'a str,
        username: &'a str,
        port: u16,
        group: &'a Option<String>,
    },
    Surface {
        tab_id: TabId,
        kind: TabKind,
        title: String,
        session_id: Option<TerminalSessionId>,
        visible: bool,
    },
    SshNode {
        target: AiSshNodeTargetInput<'a>,
        sftp_session_id: Option<String>,
    },
    Sftp(AiSftpTargetInput),
    Ide {
        tab_id: TabId,
        target: AiIdeTargetInput,
    },
    Terminal(AiTerminalTargetInput<'a>),
}

impl<'a> AiTargetCapture<'a> {
    pub fn saved_connection(connection: &'a oxideterm_connections::SavedConnection) -> Self {
        Self::SavedConnection {
            id: &connection.id,
            name: &connection.name,
            host: &connection.host,
            username: &connection.username,
            port: connection.port,
            group: &connection.group,
        }
    }

    pub fn surface(tab: &oxideterm_workspace::Tab, visible: bool) -> Self {
        let session_id = tab.root_pane.as_ref().and_then(|root| root.session_id_for_pane(tab.active_pane_id?));
        Self::Surface {
            tab_id: tab.id, kind: tab.kind.clone(), title: tab.title.clone(), session_id, visible,
        }
    }


    fn append_targets(self, targets: &mut Vec<AiOrchestratorTarget>) {
        match self {
            Self::SavedConnection {
                id,
                name,
                host,
                username,
                port,
                group,
            } => {
                targets.push(ai_saved_connection_target(
                    id, name, host, username, port, group,
                ));
            }
            Self::Surface {
                tab_id,
                kind,
                title,
                session_id,
                visible,
            } => {
                targets.push(ai_app_surface_target(
                    tab_id, &kind, &title, session_id, visible,
                ));
            }
            Self::SshNode {
                target,
                sftp_session_id,
            } => {
                let sftp = sftp_session_id.as_ref().map(|session| AiSftpTargetInput {
                    node_id: target.node_id.to_owned(),
                    session_id: session.clone(),
                    connection_id: target.connection_id.map(str::to_owned),
                    host: target.host.to_owned(),
                });
                targets.extend(ai_ssh_node_target(AiSshNodeTargetInput {
                    sftp_session_id: sftp_session_id.as_deref(),
                    ..target
                }));
                if let Some(sftp) = sftp {
                    Self::Sftp(sftp).append_targets(targets);
                }
            }
            Self::Sftp(target) => {
                targets.push(ai_target_from_projection(crate::sftp_target_projection(
                    target,
                )));
            }
            Self::Ide { tab_id, target } => {
                let mut target =
                    ai_target_from_projection(crate::ide_workspace_target_projection(target));
                // Distinguish IDE surfaces for the same node without exposing the outer tab ID.
                target.id = format!("ide-surface:{}", tab_id.0);
                target
                    .refs
                    .insert("surfaceTabId".to_owned(), tab_id.0.to_string());
                targets.push(target);
            }
            Self::Terminal(terminal) => targets.push(ai_terminal_target(terminal)),
        }
    }
}

pub fn finish_ai_target_discovery<N: Clone + Eq + Hash>(
    authority: &mut AiRuntimeAuthority<N>,
    tool_session_id: Option<&ToolSessionId>,
    captures: Vec<AiTargetCapture<'_>>,
    node_id: impl Fn(&str) -> N,
) -> (
    Vec<AiOrchestratorTarget>,
    HashMap<String, RuntimeHandleProjection>,
) {
    let mut targets = Vec::with_capacity(captures.len());
    for capture in captures {
        capture.append_targets(&mut targets);
    }
    targets.push(AiOrchestratorTarget {
        id: "local-shell:default".to_string(),
        kind: "local-shell".to_string(),
        label: "Local shell".to_string(),
        state: "available".to_string(),
        capabilities: vec![
            "command.run".to_string(),
            "navigation.open".to_string(),
            "state.list".to_string(),
        ],
        refs: BTreeMap::new(),
        metadata: serde_json::json!({}),
        terminal_buffer: None,
        terminal_screen: None,
    });
    targets.push(AiOrchestratorTarget {
        id: "settings:app".to_string(),
        kind: "settings".to_string(),
        label: "Settings".to_string(),
        state: "available".to_string(),
        capabilities: vec![
            "settings.read".to_string(),
            "settings.write".to_string(),
            "navigation.open".to_string(),
            "state.list".to_string(),
        ],
        refs: BTreeMap::new(),
        metadata: serde_json::json!({}),
        terminal_buffer: None,
        terminal_screen: None,
    });
    targets.push(AiOrchestratorTarget {
        id: "rag-index:default".to_string(),
        kind: "rag-index".to_string(),
        label: "Knowledge base".to_string(),
        state: "available".to_string(),
        capabilities: vec!["state.list".to_string(), "filesystem.search".to_string()],
        refs: BTreeMap::new(),
        metadata: serde_json::json!({}),
        terminal_buffer: None,
        terminal_screen: None,
    });

    // Tauri deduplicates targets by id after discovery; keep the first
    // discovery order while replacing duplicate values with the latest
    // runtime snapshot.
    let mut target_indexes = std::collections::HashMap::<String, usize>::new();
    let mut deduped_targets = Vec::<AiOrchestratorTarget>::new();
    for target in targets {
        if let Some(index) = target_indexes.get(&target.id).copied() {
            deduped_targets[index] = target;
        } else {
            target_indexes.insert(target.id.clone(), deduped_targets.len());
            deduped_targets.push(target);
        }
    }
    let mut targets = deduped_targets;
    let runtime_handles: HashMap<String, crate::RuntimeHandleProjection> = tool_session_id
        .map(|tool_session_id| {
            targets
                .iter()
                .filter_map(|target| {
                    let ide_tab_id = (target.kind == "ide-workspace")
                        .then(|| {
                            target
                                .refs
                                .get("surfaceTabId")
                                .and_then(|value| value.parse::<u64>().ok())
                                .map(TabId)
                        })
                        .flatten();
                    match target.kind.as_str() {
                        "terminal-session" => {
                            let session_id = target
                                .refs
                                .get("sessionId")
                                .and_then(|value| value.parse::<u64>().ok())
                                .map(TerminalSessionId)?;
                            authority
                                .issue_terminal_handle(tool_session_id, session_id)
                                .ok()
                        }
                        "local-shell" => authority.issue_local_shell_handle(tool_session_id).ok(),
                        "ssh-node" => {
                            let node_id = target.refs.get("nodeId").map(|value| node_id(value))?;
                            authority.issue_node_handle(tool_session_id, &node_id).ok()
                        }
                        "sftp-session" => {
                            let node_id = target.refs.get("nodeId").map(|value| node_id(value))?;
                            authority.issue_sftp_handle(tool_session_id, &node_id).ok()
                        }
                        "ide-workspace" => ide_tab_id.and_then(|tab_id| {
                            authority.issue_ide_handle(tool_session_id, tab_id).ok()
                        }),
                        "app-surface" => {
                            let tab_id = target
                                .refs
                                .get("tabId")
                                .and_then(|value| value.parse::<u64>().ok())
                                .map(TabId)?;
                            authority
                                .issue_app_surface_handle(tool_session_id, tab_id)
                                .ok()
                        }
                        _ => None,
                    }
                    .map(|handle| (target.id.clone(), handle))
                })
                .collect()
        })
        .unwrap_or_default();

    if tool_session_id.is_some_and(|session| authority.is_agent_scoped_session(session)) {
        targets.retain(|target| runtime_handles.contains_key(&target.id));
    }
    (targets, runtime_handles)
}

pub struct AiSshNodeTargetInput<'a> {
    pub node_id: &'a str,
    pub connection_id: Option<&'a str>,
    pub readiness: ActiveSessionReadiness,
    pub host: &'a str,
    pub port: u16,
    pub username: &'a str,
    pub title: &'a str,
    pub terminal_ids: &'a [TerminalSessionId],
    pub sftp_session_id: Option<&'a str>,
}

pub fn ai_ssh_node_target(input: AiSshNodeTargetInput<'_>) -> Option<AiOrchestratorTarget> {
    if input.connection_id.is_some() || input.readiness == ActiveSessionReadiness::Ready {
        let runtime_status = match input.readiness {
            ActiveSessionReadiness::Ready => "connected",
            ActiveSessionReadiness::Connecting => "connecting",
            ActiveSessionReadiness::Error => "error",
            ActiveSessionReadiness::Disconnected => "disconnected",
        };
        let mut refs = BTreeMap::new();
        refs.insert("nodeId".to_string(), input.node_id.to_owned());
        if let Some(saved_connection_id) = input.connection_id {
            refs.insert("connectionId".to_string(), saved_connection_id.to_owned());
        }
        if let Some(session_id) = input.terminal_ids.first() {
            refs.insert("sessionId".to_string(), session_id.0.to_string());
        }
        let mut metadata = serde_json::json!({
            "host": input.host,
            "port": input.port,
            "username": input.username,
            "status": runtime_status,
            "terminalIds": input.terminal_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
            "title": input.title,
        });
        if let Some(sftp_session_id) = input.sftp_session_id
            && let Some(object) = metadata.as_object_mut()
        {
            object.insert(
                "sftpSessionId".to_string(),
                serde_json::json!(sftp_session_id),
            );
        }
        return Some(AiOrchestratorTarget {
            id: format!("ssh-node:{}", input.node_id),
            kind: "ssh-node".to_string(),
            label: format!("{}@{}:{}", input.username, input.host, input.port),
            state: match input.readiness {
                ActiveSessionReadiness::Ready => "connected",
                ActiveSessionReadiness::Connecting => "opening",
                ActiveSessionReadiness::Error => "stale",
                ActiveSessionReadiness::Disconnected => "unavailable",
            }
            .to_string(),
            capabilities: vec!["node.inspect".to_string(), "state.list".to_string()],
            refs,
            metadata,
            terminal_buffer: None,
            terminal_screen: None,
        });
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AiTransferState { Pending, Active, Paused, Completed, Cancelled, Error }

pub struct AiTransferCapture<'a> {
    pub id: &'a str,
    pub direction: &'a str,
    pub state: AiTransferState,
    pub size: u64,
    pub transferred: u64,
    pub has_error: bool,
    pub start_time: u64,
    pub end_time: Option<u64>,
}

fn ai_background_transfer_state_label(
    state: AiTransferState,
) -> &'static str {
    match state {
        AiTransferState::Pending => "pending",
        AiTransferState::Active => "active",
        AiTransferState::Paused => "paused",
        AiTransferState::Completed => "completed",
        AiTransferState::Cancelled => "cancelled",
        AiTransferState::Error => "error",
    }
}

pub fn ai_transfers_state(transfers: &[AiTransferCapture<'_>]) -> serde_json::Value {
    let count = |state| {
        transfers
            .iter()
            .filter(|transfer| transfer.state == state)
            .count()
    };
    let active_or_recent = transfers
        .iter()
        .filter(|transfer| {
            matches!(
                transfer.state,
                AiTransferState::Pending
                    | AiTransferState::Active
                    | AiTransferState::Paused
                    | AiTransferState::Error
            )
        })
        .chain(
            transfers
                .iter()
                .filter(|transfer| {
                    matches!(
                        transfer.state,
                        AiTransferState::Completed | AiTransferState::Cancelled
                    )
                })
                .rev()
                .take(5),
        )
        .take(20)
        .map(|transfer| {
            serde_json::json!({
                "id": transfer.id,
                "direction": transfer.direction,
                "state": ai_background_transfer_state_label(transfer.state),
                "size": transfer.size,
                "transferred": transfer.transferred,
                "hasError": transfer.has_error,
                "startTime": transfer.start_time,
                "endTime": transfer.end_time,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "total": transfers.len(),
        "counts": {
            "pending": count(AiTransferState::Pending),
            "active": count(AiTransferState::Active),
            "paused": count(AiTransferState::Paused),
            "completed": count(AiTransferState::Completed),
            "cancelled": count(AiTransferState::Cancelled),
            "error": count(AiTransferState::Error),
        },
        "transfers": active_or_recent,
    })
}
