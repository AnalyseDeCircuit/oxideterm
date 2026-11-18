use super::*;
use oxideterm_public_mcp::PublicTelnetControl;
use oxideterm_terminal::{
    TelnetControlCommand, TerminalLifecycle, TerminalSession, TerminalSessionKind,
};

pub type PublicMcpTerminalSession = Arc<Mutex<TerminalSession>>;
const TERMINAL_READ_OUTPUT_LIMIT_BYTES: usize = 256 * 1024;
const PUBLIC_MCP_TERMINAL_CAPACITY: usize = 128;
const PUBLIC_MCP_TERMINAL_CAPACITY_PER_CLIENT: usize = 32;
const PENDING_MOSH_OPENS_PER_CLIENT: usize = 8;

pub enum PublicMcpSavedTerminal {
    Serial(oxideterm_connections::SerialProfile),
    Telnet(oxideterm_connections::TelnetProfile),
    Mosh {
        profile: oxideterm_connections::MoshProfile,
        config: Box<oxideterm_ssh::SshConfig>,
    },
}

impl PublicMcpHost {
    pub fn terminal(
        &self,
        client: &ClientRef,
        id: &TerminalRef,
    ) -> Option<PublicMcpTerminalRecord> {
        self.terminals
            .lock()
            .get(id)
            .filter(|record| &record.client_ref == client)
            .cloned()
    }

    pub fn register_terminal(&self, id: TerminalRef, record: PublicMcpTerminalRecord) {
        self.terminals.lock().insert(id, record);
    }

    pub fn remove_terminal(&self, id: &TerminalRef) -> Option<PublicMcpTerminalRecord> {
        let mut history = self.history.lock();
        let record = self.terminals.lock().remove(id);
        history.revoke_terminal(id, false);
        record
    }

    pub fn revoke_terminal_history(&self, id: &TerminalRef) {
        self.history.lock().revoke_terminal(id, true);
    }

    pub fn client_terminals(&self, client: &ClientRef) -> Vec<TerminalRef> {
        self.terminals
            .lock()
            .iter()
            .filter(|(_, record)| &record.client_ref == client)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn terminals_for_session(
        &self,
        session: TerminalSessionId,
    ) -> Vec<(TerminalRef, ClientRef)> {
        self.terminals
            .lock()
            .iter()
            .filter(|(_, record)| record.session_id == session)
            .map(|(id, record)| (id.clone(), record.client_ref.clone()))
            .collect()
    }

    pub fn terminal_open_rejection(&self, client: &ClientRef) -> Option<&'static str> {
        if !self.state.clients.get(client).is_some_and(|client| {
            client.enabled && client.tool_groups.contains(&ToolGroup::TerminalSession)
        }) {
            return Some("The MCP client authorization changed before the terminal opened");
        }
        let terminals = self.terminals.lock();
        let total = terminals
            .len()
            .saturating_add(self.pending_terminal_opens.len());
        let count = terminals
            .values()
            .filter(|record| &record.client_ref == client)
            .count()
            .saturating_add(
                self.pending_terminal_opens
                    .values()
                    .filter(|pending| &pending.client_ref == client)
                    .count(),
            );
        (total >= PUBLIC_MCP_TERMINAL_CAPACITY || count >= PUBLIC_MCP_TERMINAL_CAPACITY_PER_CLIENT)
            .then_some("The retained terminal session limit has been reached")
    }
}

pub fn finish_terminal_read(
    request: DomainRequest,
    generation: u64,
    (cols, rows): (usize, usize),
    display_offset: usize,
    alternate_buffer: bool,
    visible_text: impl FnOnce() -> String,
) {
    let PublicToolCall::ReadTerminal(args) = &request.call else {
        return;
    };
    let terminal_ref = args.terminal_ref.clone();
    let result = if args.cursor == Some(generation) {
        json!({ "terminal_ref": terminal_ref, "cursor": generation, "unchanged": true, "lines": [], "truncated": false })
    } else {
        let text = Zeroizing::new(visible_text());
        let (lines, truncated) = bounded_terminal_lines(
            &text,
            args.line_limit as usize,
            args.tail,
            TERMINAL_READ_OUTPUT_LIMIT_BYTES,
        );
        json!({ "terminal_ref": terminal_ref, "cursor": generation, "unchanged": false,
            "cols": cols, "rows": rows, "display_offset": display_offset,
            "alternate_buffer": alternate_buffer, "lines": lines, "truncated": truncated })
    };
    finish_serialized(request, result);
}

impl PublicMcpBackground {
    pub fn find_terminal(
        &self,
        request: DomainRequest,
        session_id: TerminalSessionId,
        session: PublicMcpTerminalSession,
    ) {
        let backend = self.clone();
        let session = Arc::downgrade(&session);
        self.forwarding_runtime.spawn_blocking(move || {
            let PublicToolCall::FindTerminal(args) = &request.call else {
                return;
            };
            if request.is_cancelled() {
                return;
            }
            let terminal_ref = args.terminal_ref.clone();
            let limit = args.limit as usize;
            let Some(session) = session.upgrade() else {
                request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
                return;
            };
            let matches = session.lock().search_matches(&args.query);
            let suspended = backend.admission.lock();
            if request.is_cancelled() {
                return;
            }
            if *suspended {
                request.finish(ToolEnvelope::failed("The OxideTerm workspace is locked"));
                return;
            }
            if !backend
                .terminals
                .lock()
                .get(&terminal_ref)
                .is_some_and(|record| {
                    record.client_ref == request.client_ref && record.session_id == session_id
                })
            {
                request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
                return;
            }
            let truncated = matches.len() > limit;
            let matches = matches
                .into_iter()
                .take(limit)
                .map(|entry| {
                    json!({
                        "line": entry.line,
                        "start_col": entry.start_col,
                        "end_col": entry.end_col,
                        "ranges": entry.ranges.into_iter().map(|range| json!({
                            "line": range.line,
                            "start_col": range.start_col,
                            "end_col": range.end_col,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>();
            finish_serialized(
                request,
                json!({
                    "terminal_ref": terminal_ref,
                    "matches": matches,
                    "truncated": truncated,
                }),
            );
        });
    }
}

pub struct PublicMcpTerminalSerialState {
    pub port_available: Option<bool>,
    pub can_reconnect: bool,
    pub data_terminal_ready: bool,
    pub request_to_send: bool,
}

pub fn terminal_projection(
    record: &PublicMcpTerminalRecord,
    terminal_ref: &TerminalRef,
    (cols, rows): (usize, usize),
    alternate_buffer: bool,
    session_kind: TerminalSessionKind,
    serial: Option<PublicMcpTerminalSerialState>,
    session: PublicMcpTerminalSession,
) -> serde_json::Value {
    let serial = serial.map(|status| json!({
        "port_available": status.port_available, "can_reconnect": status.can_reconnect,
        "data_terminal_ready": status.data_terminal_ready, "request_to_send": status.request_to_send,
    }));
    let (lifecycle_name, exit_code, interactive, buffer_lines) = {
        let session = session.lock();
        let (name, code) = terminal_lifecycle_projection(&session.lifecycle());
        (
            name,
            code,
            session.is_interactive(),
            session.buffer_line_count(),
        )
    };
    json!({
        "terminal_ref": terminal_ref,
        "transport": record.transport,
        "title": record.title,
        "lifecycle": lifecycle_name,
        "exit_code": exit_code,
        "interactive": interactive,
        "cols": cols,
        "rows": rows,
        "buffer_lines": buffer_lines,
        "alternate_buffer": alternate_buffer,
        "node_ref": record.node_ref,
        "serial": serial,
        "capabilities": {
            "read": true,
            "history_read": true,
            "find": true,
            "input": interactive,
            "resize": true,
            "serial_control": session_kind == TerminalSessionKind::Serial,
            "telnet_control": session_kind == TerminalSessionKind::Telnet,
            "process_signals": session_kind == TerminalSessionKind::LocalPty,
            "node_backed": session_kind == TerminalSessionKind::SshPty,
        }
    })
}

fn terminal_lifecycle_projection(lifecycle: &TerminalLifecycle) -> (&'static str, Option<i32>) {
    match lifecycle {
        TerminalLifecycle::Running => ("running", None),
        TerminalLifecycle::Exited(exit_code) => ("exited", *exit_code),
        TerminalLifecycle::Closed => ("closed", None),
    }
}

pub fn telnet_control(control: PublicTelnetControl) -> TelnetControlCommand {
    match control {
        PublicTelnetControl::NoOperation => TelnetControlCommand::NoOperation,
        PublicTelnetControl::Break => TelnetControlCommand::Break,
        PublicTelnetControl::InterruptProcess => TelnetControlCommand::InterruptProcess,
        PublicTelnetControl::AbortOutput => TelnetControlCommand::AbortOutput,
        PublicTelnetControl::AreYouThere => TelnetControlCommand::AreYouThere,
        PublicTelnetControl::EraseCharacter => TelnetControlCommand::EraseCharacter,
        PublicTelnetControl::EraseLine => TelnetControlCommand::EraseLine,
        PublicTelnetControl::GoAhead => TelnetControlCommand::GoAhead,
    }
}

fn bounded_terminal_lines(
    text: &str,
    line_limit: usize,
    tail: bool,
    byte_limit: usize,
) -> (Vec<String>, bool) {
    let all_lines = text.lines().collect::<Vec<_>>();
    let selected = if tail {
        &all_lines[all_lines.len().saturating_sub(line_limit)..]
    } else {
        &all_lines[..all_lines.len().min(line_limit)]
    };
    let mut lines = Vec::with_capacity(selected.len());
    let mut retained_bytes = 0usize;
    for line in selected {
        let next_bytes = line.len().saturating_add(1);
        if retained_bytes.saturating_add(next_bytes) > byte_limit {
            break;
        }
        lines.push((*line).to_owned());
        retained_bytes += next_bytes;
    }
    let truncated = selected.len() < all_lines.len() || lines.len() < selected.len();
    (lines, truncated)
}

impl PublicMcpHost {
    pub fn saved_terminal_profile(
        &mut self,
        client: &ClientRef,
        reference: &ConnectionRef,
        store: &ConnectionStore,
    ) -> Result<PublicMcpSavedTerminal, &'static str> {
        let key = self
            .connection_key(client, reference, store)
            .ok_or("The connection handle is unavailable")?;
        if key.starts_with(CONNECTION_KEY_SSH_PREFIX) {
            return Err("SSH terminals require an acquired node_ref source");
        }
        if let Some(id) = key.strip_prefix(CONNECTION_KEY_SERIAL_PREFIX) {
            return store
                .serial_profiles()
                .iter()
                .find(|profile| profile.id == id)
                .cloned()
                .map(PublicMcpSavedTerminal::Serial)
                .ok_or("The saved serial profile no longer exists");
        }
        if let Some(id) = key.strip_prefix(CONNECTION_KEY_TELNET_PREFIX) {
            return store
                .telnet_profiles()
                .iter()
                .find(|profile| profile.id == id)
                .cloned()
                .map(PublicMcpSavedTerminal::Telnet)
                .ok_or("The saved Telnet profile no longer exists");
        }
        if let Some(id) = key.strip_prefix(CONNECTION_KEY_MOSH_PREFIX) {
            self.pending_terminal_opens
                .retain(|_, pending| !pending.request.is_cancelled());
            if self
                .pending_terminal_opens
                .values()
                .filter(|pending| &pending.client_ref == client)
                .count()
                >= PENDING_MOSH_OPENS_PER_CLIENT
            {
                return Err("Too many Mosh terminal opens are waiting for authentication");
            }
            let profile = store
                .get_mosh_profile(id)
                .cloned()
                .ok_or("The saved Mosh profile no longer exists")?;
            let auth = oxideterm_session_adapter::auth_method_from_saved_auth(store, &profile.auth)
                .ok_or("The saved Mosh profile requires credentials that are not available")?;
            let config = Box::new(oxideterm_ssh::SshConfig {
                host: profile.host.clone(),
                port: profile.ssh_port,
                username: profile.username.clone(),
                auth,
                identity_agent: profile.identity_agent.clone(),
                legacy_ssh_compatibility: profile.legacy_ssh_compatibility,
                strict_host_key_checking: true,
                ..Default::default()
            });
            return Ok(PublicMcpSavedTerminal::Mosh { profile, config });
        }
        if key.starts_with(CONNECTION_KEY_DESKTOP_PREFIX) {
            return Err("Remote desktop profiles must be opened with desktops_open");
        }
        Err("The saved connection does not provide a terminal transport")
    }

    pub fn revoke_pending_terminal_opens(&mut self, client: &ClientRef) {
        for (_, pending) in self
            .pending_terminal_opens
            .extract_if(|_, pending| &pending.client_ref == client)
        {
            pending
                .request
                .finish(ToolEnvelope::failed("The MCP client authorization changed"));
        }
    }

    pub fn prepare_node_connection(
        &mut self,
        request: &DomainRequest,
        store: &ConnectionStore,
        settings: &oxideterm_settings::PersistedSettings,
    ) -> Result<(String, String, oxideterm_ssh::SshConfig), &'static str> {
        let PublicToolCall::ConnectNode(args) = &request.call else {
            unreachable!()
        };
        let id = self
            .connection_id(&request.client_ref, &args.connection_ref, store)
            .ok_or("The connection handle is unavailable")?;
        if !self.node_capacity_available(&request.client_ref) {
            return Err("The retained SSH node lease limit has been reached");
        }
        let connection = store
            .get(&id)
            .ok_or("The saved connection no longer exists")?;
        let config = oxideterm_session_adapter::ssh_config_from_saved_connection(
            store, settings, connection,
        )
        .ok_or("The saved connection requires credentials that are not available")?;
        Ok((id, connection.name.clone(), config))
    }
}
