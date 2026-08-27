use gpui::{App, Context, Entity, Window};
use oxideterm_connections::ConnectionTerminalOptions;
use oxideterm_gpui_terminal::{TerminalPane, TerminalSerialAction, TerminalTelnetAction};
use oxideterm_public_mcp::{
    DomainRequest, PublicTelnetControl, PublicToolCall, TerminalControlAction, TerminalOpenSource,
    TerminalRef, ToolEnvelope, ToolGroup,
};
use oxideterm_session_adapter::auth_method_from_saved_auth;
use oxideterm_ssh::SshConfig;
use oxideterm_terminal::{
    SerialSessionConfig, TelnetControlCommand, TelnetSessionConfig, TerminalLifecycle,
    TerminalSessionKind,
};
use serde_json::json;

use super::{
    CONNECTION_KEY_DESKTOP_PREFIX, CONNECTION_KEY_MOSH_PREFIX, CONNECTION_KEY_SERIAL_PREFIX,
    CONNECTION_KEY_SSH_PREFIX, CONNECTION_KEY_TELNET_PREFIX, PublicMcpPendingTerminalOpen,
    PublicMcpTerminalRecord, WorkspaceApp, finish_serialized, node_lease_for_client,
};
use crate::workspace::{
    TerminalSessionId,
    new_connection::{
        MoshConnectionOptions, SshConnectionIntent, mosh_options_from_profile,
        terminal_serial_flow_from_profile, terminal_serial_parity_from_profile,
    },
};

const TERMINAL_READ_OUTPUT_LIMIT_BYTES: usize = 256 * 1024;
const PUBLIC_MCP_TERMINAL_CAPACITY: usize = 128;
const PUBLIC_MCP_TERMINAL_CAPACITY_PER_CLIENT: usize = 32;
const PENDING_MOSH_OPENS_PER_CLIENT: usize = 8;

pub(in crate::workspace) enum PublicMcpTerminalWindowEffect {
    Open(DomainRequest),
    Close(DomainRequest),
    Revoke(Vec<TerminalSessionId>),
}

impl PublicMcpTerminalWindowEffect {
    pub(in crate::workspace) fn finish_without_window(self) {
        let request = match self {
            Self::Open(request) | Self::Close(request) => Some(request),
            Self::Revoke(_) => None,
        };
        if let Some(request) = request {
            request.finish(ToolEnvelope::failed(
                "A live OxideTerm window is required for terminal sessions",
            ));
        }
    }
}

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_terminal_open(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        if request.is_cancelled() {
            return;
        }
        self.enqueue_public_mcp_terminal_window_effect(
            PublicMcpTerminalWindowEffect::Open(request),
            cx,
        );
    }

    pub(super) fn handle_public_mcp_terminal_state(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::TerminalState(args) = &request.call else {
            return;
        };
        let terminal_ref = args.terminal_ref.clone();
        match self.public_mcp_terminal_projection(&request.client_ref, &terminal_ref, cx) {
            Ok(state) => finish_serialized(request, json!({ "terminal": state })),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(super) fn handle_public_mcp_terminal_read(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::ReadTerminal(args) = &request.call else {
            return;
        };
        let terminal_ref = args.terminal_ref.clone();
        let cursor = args.cursor;
        let line_limit = args.line_limit as usize;
        let tail = args.tail;
        let Ok((_, pane)) = self.public_mcp_terminal_pane(&request.client_ref, &terminal_ref, cx)
        else {
            request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
            return;
        };
        let pane = pane.read(cx);
        let snapshot = pane.ai_screen_snapshot();
        if cursor == Some(snapshot.generation) {
            finish_serialized(
                request,
                json!({
                    "terminal_ref": terminal_ref,
                    "cursor": snapshot.generation,
                    "unchanged": true,
                    "lines": [],
                    "truncated": false,
                }),
            );
            return;
        }
        let (lines, truncated) = bounded_terminal_lines(
            &pane.visible_text_snapshot(),
            line_limit,
            tail,
            TERMINAL_READ_OUTPUT_LIMIT_BYTES,
        );
        finish_serialized(
            request,
            json!({
                "terminal_ref": terminal_ref,
                "cursor": snapshot.generation,
                "unchanged": false,
                "cols": snapshot.cols,
                "rows": snapshot.rows,
                "display_offset": snapshot.display_offset,
                "alternate_buffer": pane.ai_screen_is_alternate_buffer(),
                "lines": lines,
                "truncated": truncated,
            }),
        );
    }

    pub(super) fn handle_public_mcp_terminal_find(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::FindTerminal(args) = &request.call else {
            return;
        };
        let terminal_ref = args.terminal_ref.clone();
        let query = args.query.clone();
        let limit = args.limit as usize;
        let Ok((_, pane)) = self.public_mcp_terminal_pane(&request.client_ref, &terminal_ref, cx)
        else {
            request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
            return;
        };
        let matches = pane.read(cx).shared_session().lock().search_matches(&query);
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
    }

    pub(super) fn handle_public_mcp_terminal_submit(
        &mut self,
        mut request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let (terminal_ref, accepted) = {
            let PublicToolCall::SubmitTerminal(args) = &mut request.call else {
                return;
            };
            let terminal_ref = args.terminal_ref.clone();
            let Ok((_, pane)) =
                self.public_mcp_terminal_pane(&request.client_ref, &terminal_ref, cx)
            else {
                request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
                return;
            };
            if args.append_enter {
                args.input.push(b'\r');
            }
            let accepted = if args.is_text {
                std::str::from_utf8(&args.input).is_ok_and(|text| {
                    pane.update(cx, |pane, cx| pane.send_command_sender_text_chunk(text, cx))
                })
            } else {
                pane.update(cx, |pane, cx| {
                    pane.send_command_sender_raw_bytes(&args.input, cx)
                })
            };
            (terminal_ref, accepted)
        };
        if accepted {
            finish_serialized(
                request,
                json!({ "terminal_ref": terminal_ref, "accepted": true }),
            );
        } else {
            request.finish(ToolEnvelope::failed(
                "The live terminal rejected the submitted input",
            ));
        }
    }

    pub(super) fn handle_public_mcp_terminal_resize(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::ResizeTerminal(args) = &request.call else {
            return;
        };
        let terminal_ref = args.terminal_ref.clone();
        let cols = usize::from(args.cols);
        let rows = usize::from(args.rows);
        let Ok((_, pane)) = self.public_mcp_terminal_pane(&request.client_ref, &terminal_ref, cx)
        else {
            request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
            return;
        };
        match pane.update(cx, |pane, cx| pane.resize_grid(cols, rows, cx)) {
            Ok(()) => finish_serialized(
                request,
                json!({ "terminal_ref": terminal_ref, "cols": cols, "rows": rows }),
            ),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(super) fn handle_public_mcp_terminal_control(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::ControlTerminal(args) = &request.call else {
            return;
        };
        let terminal_ref = args.terminal_ref.clone();
        let action = args.action;
        let Ok((_, pane)) = self.public_mcp_terminal_pane(&request.client_ref, &terminal_ref, cx)
        else {
            request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
            return;
        };
        let session_kind = pane.read(cx).session_kind();
        if matches!(
            action,
            TerminalControlAction::Terminate | TerminalControlAction::Kill
        ) && session_kind != TerminalSessionKind::LocalPty
        {
            // Remote transports can send an interrupt byte but cannot deliver local OS signals.
            request.finish(ToolEnvelope::failed(
                "Process terminate and kill signals are available only for local terminals",
            ));
            return;
        }
        if matches!(action, TerminalControlAction::SerialReconnect) {
            request.finish(ToolEnvelope::failed(
                "Serial reconnect must create a new terminal tab from Active Sessions",
            ));
            return;
        }
        let result = pane.update(cx, |pane, cx| match action {
            TerminalControlAction::Interrupt => pane
                .send_command_sender_raw_bytes(&[0x03], cx)
                .then_some(())
                .ok_or_else(|| "The terminal rejected the interrupt input".to_string()),
            TerminalControlAction::Terminate => pane
                .shared_session()
                .lock()
                .terminate_active_task()
                .map_err(|error| error.to_string()),
            TerminalControlAction::Kill => pane
                .shared_session()
                .lock()
                .kill_active_task()
                .map_err(|error| error.to_string()),
            TerminalControlAction::SerialBreak => {
                pane.apply_serial_action(TerminalSerialAction::SendBreak, cx)
            }
            TerminalControlAction::SerialReconnect => unreachable!("handled above"),
            TerminalControlAction::SerialDataTerminalReady { asserted } => {
                pane.apply_serial_action(TerminalSerialAction::SetDataTerminalReady(asserted), cx)
            }
            TerminalControlAction::SerialRequestToSend { asserted } => {
                pane.apply_serial_action(TerminalSerialAction::SetRequestToSend(asserted), cx)
            }
            TerminalControlAction::Telnet { command } => pane.apply_telnet_action(
                TerminalTelnetAction::SendControl(telnet_control(command)),
                cx,
            ),
        });
        match result {
            Ok(()) => finish_serialized(
                request,
                json!({ "terminal_ref": terminal_ref, "applied": true }),
            ),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(super) fn handle_public_mcp_terminal_close(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        self.enqueue_public_mcp_terminal_window_effect(
            PublicMcpTerminalWindowEffect::Close(request),
            cx,
        );
    }

    pub(in crate::workspace) fn apply_public_mcp_terminal_window_effect(
        &mut self,
        effect: PublicMcpTerminalWindowEffect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            PublicMcpTerminalWindowEffect::Open(request) => {
                self.apply_public_mcp_terminal_open(request, window, cx)
            }
            PublicMcpTerminalWindowEffect::Close(request) => {
                self.apply_public_mcp_terminal_close(request, window, cx)
            }
            PublicMcpTerminalWindowEffect::Revoke(session_ids) => {
                for session_id in session_ids {
                    self.close_terminal_session(session_id, window, cx);
                }
            }
        }
    }

    fn apply_public_mcp_terminal_open(
        &mut self,
        request: DomainRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if request.is_cancelled() {
            return;
        }
        let session_group_enabled = self
            .public_mcp
            .clients()
            .into_iter()
            .find(|client| client.client_ref == request.client_ref)
            .is_some_and(|client| {
                client.enabled && client.tool_groups.contains(&ToolGroup::TerminalSession)
            });
        if !session_group_enabled {
            request.finish(ToolEnvelope::failed(
                "The MCP client authorization changed before the terminal opened",
            ));
            return;
        }
        let retained_total = self
            .public_mcp
            .terminals
            .len()
            .saturating_add(self.public_mcp.pending_terminal_opens.len());
        let retained_for_client = self
            .public_mcp
            .terminals
            .values()
            .filter(|record| record.client_ref == request.client_ref)
            .count()
            .saturating_add(
                self.public_mcp
                    .pending_terminal_opens
                    .values()
                    .filter(|pending| pending.client_ref == request.client_ref)
                    .count(),
            );
        if retained_total >= PUBLIC_MCP_TERMINAL_CAPACITY
            || retained_for_client >= PUBLIC_MCP_TERMINAL_CAPACITY_PER_CLIENT
        {
            request.finish(ToolEnvelope::failed(
                "The retained terminal session limit has been reached",
            ));
            return;
        }
        let PublicToolCall::OpenTerminal(args) = &request.call else {
            return;
        };
        let source = args.source.clone();
        let cols = args.cols;
        let rows = args.rows;
        let requested_title = args.title.clone();
        match source {
            TerminalOpenSource::Node { node_ref } => {
                let Some(lease) = node_lease_for_client(
                    &self.public_mcp.runtime_handles,
                    &request.client_ref,
                    &node_ref,
                ) else {
                    request.finish(ToolEnvelope::failed("The node handle is unavailable"));
                    return;
                };
                let title = requested_title.unwrap_or_else(|| {
                    self.node_router
                        .node_metadata(&lease.node_id)
                        .map(|metadata| format!("{}@{}", metadata.username, metadata.host))
                        .unwrap_or_else(|| "SSH terminal".to_owned())
                });
                match self.create_ssh_terminal_tab_for_existing_node(
                    &lease.node_id,
                    None,
                    title.clone(),
                    window,
                    cx,
                ) {
                    Ok(session_id) => self.finish_public_mcp_terminal_open(
                        request,
                        TerminalRef::new(),
                        session_id,
                        "ssh",
                        title,
                        Some(node_ref),
                        cols,
                        rows,
                        cx,
                    ),
                    Err(error) => request.finish(ToolEnvelope::failed(error.to_string())),
                }
            }
            TerminalOpenSource::Local => {
                let title = requested_title.unwrap_or_else(|| self.local_terminal_tab_title());
                match self.create_local_terminal_tab_with_owned_session(
                    self.local_terminal_config(),
                    title.clone(),
                    window,
                    cx,
                ) {
                    Ok((session_id, _)) => self.finish_public_mcp_terminal_open(
                        request,
                        TerminalRef::new(),
                        session_id,
                        "local",
                        title,
                        None,
                        cols,
                        rows,
                        cx,
                    ),
                    Err(error) => request.finish(ToolEnvelope::failed(error.to_string())),
                }
            }
            TerminalOpenSource::Connection { connection_ref } => {
                self.apply_public_mcp_saved_terminal_open(
                    request,
                    connection_ref,
                    requested_title,
                    cols,
                    rows,
                    window,
                    cx,
                );
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the arguments are the frozen MCP open request"
    )]
    fn apply_public_mcp_saved_terminal_open(
        &mut self,
        request: DomainRequest,
        connection_ref: oxideterm_public_mcp::ConnectionRef,
        requested_title: Option<String>,
        cols: u16,
        rows: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(connection_key) = self.public_mcp.connection_key(
            &request.client_ref,
            &connection_ref,
            &self.connection_store,
        ) else {
            request.finish(ToolEnvelope::failed("The connection handle is unavailable"));
            return;
        };
        if connection_key.starts_with(CONNECTION_KEY_SSH_PREFIX) {
            request.finish(ToolEnvelope::failed(
                "SSH terminals require an acquired node_ref source",
            ));
            return;
        }
        if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_SERIAL_PREFIX) {
            let Some(profile) = self
                .connection_store
                .serial_profiles()
                .iter()
                .find(|profile| profile.id == profile_id)
                .cloned()
            else {
                request.finish(ToolEnvelope::failed(
                    "The saved serial profile no longer exists",
                ));
                return;
            };
            let title = requested_title.unwrap_or_else(|| profile.name.clone());
            let config = SerialSessionConfig {
                port_path: profile.port_path,
                baud_rate: profile.baud_rate,
                data_bits: profile.data_bits,
                stop_bits: profile.stop_bits,
                parity: terminal_serial_parity_from_profile(&profile.parity),
                flow_control: terminal_serial_flow_from_profile(&profile.flow_control),
            };
            match self.create_serial_terminal_tab_with_title(
                config,
                ConnectionTerminalOptions::default(),
                title.clone(),
                window,
                cx,
            ) {
                Ok(session_id) => {
                    self.register_terminal_saved_connection(
                        session_id,
                        oxideterm_terminal_triggers::SavedConnectionKind::Serial,
                        profile_id.to_string(),
                        cx,
                    );
                    let _ = self.connection_store.mark_serial_profile_used(profile_id);
                    self.finish_public_mcp_terminal_open(
                        request,
                        TerminalRef::new(),
                        session_id,
                        "serial",
                        title,
                        None,
                        cols,
                        rows,
                        cx,
                    );
                }
                Err(error) => request.finish(ToolEnvelope::failed(error.to_string())),
            }
            return;
        }
        if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_TELNET_PREFIX) {
            let Some(profile) = self
                .connection_store
                .telnet_profiles()
                .iter()
                .find(|profile| profile.id == profile_id)
                .cloned()
            else {
                request.finish(ToolEnvelope::failed(
                    "The saved Telnet profile no longer exists",
                ));
                return;
            };
            let title = requested_title.unwrap_or_else(|| profile.name.clone());
            let config = TelnetSessionConfig {
                host: profile.host,
                port: profile.port,
            };
            match self.create_telnet_terminal_tab_with_title(
                config,
                profile.terminal,
                title.clone(),
                window,
                cx,
            ) {
                Ok(session_id) => {
                    self.register_terminal_saved_connection(
                        session_id,
                        oxideterm_terminal_triggers::SavedConnectionKind::Telnet,
                        profile_id.to_string(),
                        cx,
                    );
                    let _ = self.connection_store.mark_telnet_profile_used(profile_id);
                    self.finish_public_mcp_terminal_open(
                        request,
                        TerminalRef::new(),
                        session_id,
                        "telnet",
                        title,
                        None,
                        cols,
                        rows,
                        cx,
                    );
                }
                Err(error) => request.finish(ToolEnvelope::failed(error.to_string())),
            }
            return;
        }
        if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_MOSH_PREFIX) {
            self.public_mcp
                .pending_terminal_opens
                .retain(|_, pending| !pending.request.is_cancelled());
            let pending_for_client = self
                .public_mcp
                .pending_terminal_opens
                .values()
                .filter(|pending| pending.client_ref == request.client_ref)
                .count();
            if pending_for_client >= PENDING_MOSH_OPENS_PER_CLIENT {
                request.finish(ToolEnvelope::failed(
                    "Too many Mosh terminal opens are waiting for authentication",
                ));
                return;
            }
            let Some(profile) = self.connection_store.get_mosh_profile(profile_id).cloned() else {
                request.finish(ToolEnvelope::failed(
                    "The saved Mosh profile no longer exists",
                ));
                return;
            };
            let Some(auth) = auth_method_from_saved_auth(&self.connection_store, &profile.auth)
            else {
                request.finish(ToolEnvelope::failed(
                    "The saved Mosh profile requires credentials that are not available",
                ));
                return;
            };
            let title = requested_title.unwrap_or_else(|| profile.name.clone());
            let terminal_ref = TerminalRef::new();
            let pending_token = terminal_ref.to_string();
            self.public_mcp.pending_terminal_opens.insert(
                pending_token.clone(),
                PublicMcpPendingTerminalOpen {
                    client_ref: request.client_ref.clone(),
                    terminal_ref,
                    request,
                    cols,
                    rows,
                    title: title.clone(),
                },
            );
            let config = SshConfig {
                host: profile.host.clone(),
                port: profile.ssh_port,
                username: profile.username.clone(),
                auth,
                identity_agent: profile.identity_agent.clone(),
                legacy_ssh_compatibility: profile.legacy_ssh_compatibility,
                strict_host_key_checking: true,
                ..SshConfig::default()
            };
            let mut options = mosh_options_from_profile(&profile);
            options.public_mcp_open_token = Some(pending_token);
            self.start_ssh_preflight(config, title, SshConnectionIntent::Mosh(options), cx);
            return;
        }
        if connection_key.starts_with(CONNECTION_KEY_DESKTOP_PREFIX) {
            request.finish(ToolEnvelope::failed(
                "Remote desktop profiles must be opened with desktops_open",
            ));
            return;
        }
        request.finish(ToolEnvelope::failed(
            "The saved connection does not provide a terminal transport",
        ));
    }

    fn apply_public_mcp_terminal_close(
        &mut self,
        request: DomainRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::CloseTerminal(args) = &request.call else {
            return;
        };
        let terminal_ref = args.terminal_ref.clone();
        let Some(record) = self.public_mcp.terminals.get(&terminal_ref).cloned() else {
            request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
            return;
        };
        if record.client_ref != request.client_ref {
            request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
            return;
        }
        self.stop_public_mcp_recordings_for_terminal(&request.client_ref, &terminal_ref, cx);
        self.public_mcp.terminals.remove(&terminal_ref);
        self.close_terminal_session(record.session_id, window, cx);
        finish_serialized(
            request,
            json!({ "terminal_ref": terminal_ref, "close_requested": true }),
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "registration returns the exact open result"
    )]
    fn finish_public_mcp_terminal_open(
        &mut self,
        request: DomainRequest,
        terminal_ref: TerminalRef,
        session_id: TerminalSessionId,
        transport: &'static str,
        title: String,
        node_ref: Option<oxideterm_public_mcp::NodeRef>,
        cols: u16,
        rows: u16,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.terminal_pane_for_session(session_id, cx) else {
            // A created session must not survive failure to register its public handle.
            let _ = self.enqueue_public_mcp_terminal_window_effect(
                PublicMcpTerminalWindowEffect::Revoke(vec![session_id]),
                cx,
            );
            request.finish(ToolEnvelope::failed(
                "The created terminal did not register its live pane",
            ));
            return;
        };
        let _ = pane.update(cx, |pane, cx| {
            pane.resize_grid(usize::from(cols), usize::from(rows), cx)
        });
        self.public_mcp.terminals.insert(
            terminal_ref.clone(),
            PublicMcpTerminalRecord {
                client_ref: request.client_ref.clone(),
                session_id,
                transport,
                title,
                node_ref,
            },
        );
        match self.public_mcp_terminal_projection(&request.client_ref, &terminal_ref, cx) {
            Ok(state) => finish_serialized(request, json!({ "terminal": state })),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(in crate::workspace) fn complete_public_mcp_mosh_terminal_open(
        &mut self,
        token: String,
        result: Result<TerminalSessionId, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.public_mcp.pending_terminal_opens.remove(&token) else {
            return;
        };
        match result {
            Ok(session_id) => self.finish_public_mcp_terminal_open(
                pending.request,
                pending.terminal_ref,
                session_id,
                "mosh",
                pending.title,
                None,
                pending.cols,
                pending.rows,
                cx,
            ),
            Err(error) => pending.request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(in crate::workspace) fn cancel_public_mcp_mosh_open_if_request_ended(
        &mut self,
        token: &str,
    ) -> bool {
        let cancelled = self
            .public_mcp
            .pending_terminal_opens
            .get(token)
            .is_none_or(|pending| pending.request.is_cancelled());
        if cancelled {
            self.public_mcp.pending_terminal_opens.remove(token);
        }
        cancelled
    }

    pub(in crate::workspace) fn fail_public_mcp_mosh_open_for_intent(
        &mut self,
        intent: &SshConnectionIntent,
        error: impl Into<String>,
    ) {
        if let SshConnectionIntent::Mosh(MoshConnectionOptions {
            runtime_connection_attempt_id: Some(connection_attempt_id),
            ..
        }) = intent
        {
            self.standalone_connections
                .mark_attempt_error(connection_attempt_id);
        }
        let SshConnectionIntent::Mosh(MoshConnectionOptions {
            public_mcp_open_token: Some(token),
            ..
        }) = intent
        else {
            return;
        };
        if let Some(pending) = self.public_mcp.pending_terminal_opens.remove(token) {
            pending.request.finish(ToolEnvelope::failed(error));
        }
    }

    pub(super) fn revoke_public_mcp_client_terminals(
        &mut self,
        client_ref: &oxideterm_public_mcp::ClientRef,
        cx: &mut Context<Self>,
    ) {
        let terminal_refs = self
            .public_mcp
            .terminals
            .iter()
            .filter_map(|(terminal_ref, record)| {
                (&record.client_ref == client_ref).then_some(terminal_ref.clone())
            })
            .collect::<Vec<_>>();
        for terminal_ref in &terminal_refs {
            self.stop_public_mcp_recordings_for_terminal(client_ref, terminal_ref, cx);
        }
        let session_ids = terminal_refs
            .into_iter()
            .filter_map(|terminal_ref| self.public_mcp.terminals.remove(&terminal_ref))
            .map(|record| record.session_id)
            .collect::<Vec<_>>();
        let pending_tokens = self
            .public_mcp
            .pending_terminal_opens
            .iter()
            .filter_map(|(token, pending)| {
                (&pending.client_ref == client_ref).then_some(token.clone())
            })
            .collect::<Vec<_>>();
        for token in pending_tokens {
            if let Some(pending) = self.public_mcp.pending_terminal_opens.remove(&token) {
                pending
                    .request
                    .finish(ToolEnvelope::failed("The MCP client authorization changed"));
            }
        }
        if !session_ids.is_empty() {
            let _ = self.enqueue_public_mcp_terminal_window_effect(
                PublicMcpTerminalWindowEffect::Revoke(session_ids),
                cx,
            );
        }
    }

    pub(in crate::workspace) fn release_public_mcp_terminal_for_closed_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        let terminal_refs = self
            .public_mcp
            .terminals
            .iter()
            .filter_map(|(terminal_ref, record)| {
                (record.session_id == session_id)
                    .then_some((terminal_ref.clone(), record.client_ref.clone()))
            })
            .collect::<Vec<_>>();
        for (terminal_ref, client_ref) in terminal_refs {
            // A UI-owned pane close revokes only its public handle; an SSH node
            // remains owned by NodeRouter and any other registered consumers.
            self.stop_public_mcp_recordings_for_terminal(&client_ref, &terminal_ref, cx);
            self.public_mcp.terminals.remove(&terminal_ref);
        }
    }

    pub(in crate::workspace) fn remount_public_mcp_terminal_session(
        &mut self,
        old_session_id: TerminalSessionId,
        new_session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        let terminal_refs = self
            .public_mcp
            .terminals
            .iter()
            .filter_map(|(terminal_ref, record)| {
                (record.session_id == old_session_id)
                    .then_some((terminal_ref.clone(), record.client_ref.clone()))
            })
            .collect::<Vec<_>>();
        for (terminal_ref, client_ref) in terminal_refs {
            // Recorders belong to the old pane, while the public terminal handle
            // follows the replacement session created for the same NodeRouter node.
            self.stop_public_mcp_recordings_for_terminal(&client_ref, &terminal_ref, cx);
            if let Some(record) = self.public_mcp.terminals.get_mut(&terminal_ref) {
                record.session_id = new_session_id;
            }
        }
    }

    fn public_mcp_terminal_projection(
        &self,
        client_ref: &oxideterm_public_mcp::ClientRef,
        terminal_ref: &TerminalRef,
        cx: &App,
    ) -> Result<serde_json::Value, String> {
        let (record, pane) = self.public_mcp_terminal_pane(client_ref, terminal_ref, cx)?;
        let pane = pane.read(cx);
        // Collect pane projections before holding the shared terminal lock because
        // these accessors acquire that same lock internally.
        let snapshot = pane.ai_screen_snapshot();
        let alternate_buffer = pane.ai_screen_is_alternate_buffer();
        let session_kind = pane.session_kind();
        let serial = pane.serial_status().map(|status| {
            json!({
                "port_available": status.port_available,
                "can_reconnect": status.can_reconnect,
                "data_terminal_ready": status.control_state.data_terminal_ready,
                "request_to_send": status.control_state.request_to_send,
            })
        });
        let session = pane.shared_session();
        let session = session.lock();
        let lifecycle = session.lifecycle();
        let (lifecycle_name, exit_code) = terminal_lifecycle_projection(&lifecycle);
        let interactive = session.is_interactive();
        let buffer_lines = session.buffer_line_count();
        drop(session);
        Ok(json!({
            "terminal_ref": terminal_ref,
            "transport": record.transport,
            "title": record.title,
            "lifecycle": lifecycle_name,
            "exit_code": exit_code,
            "interactive": interactive,
            "cols": snapshot.cols,
            "rows": snapshot.rows,
            "buffer_lines": buffer_lines,
            "alternate_buffer": alternate_buffer,
            "node_ref": record.node_ref,
            "serial": serial,
            "capabilities": {
                "read": true,
                "find": true,
                "input": interactive,
                "resize": true,
                "serial_control": session_kind == TerminalSessionKind::Serial,
                "telnet_control": session_kind == TerminalSessionKind::Telnet,
                "process_signals": session_kind == TerminalSessionKind::LocalPty,
                "node_backed": session_kind == TerminalSessionKind::SshPty,
            }
        }))
    }

    pub(super) fn public_mcp_terminal_pane(
        &self,
        client_ref: &oxideterm_public_mcp::ClientRef,
        terminal_ref: &TerminalRef,
        cx: &App,
    ) -> Result<(PublicMcpTerminalRecord, Entity<TerminalPane>), String> {
        let record = self
            .public_mcp
            .terminals
            .get(terminal_ref)
            .filter(|record| &record.client_ref == client_ref)
            .cloned()
            .ok_or_else(|| "The terminal handle is unavailable".to_owned())?;
        let pane = self
            .terminal_pane_for_session(record.session_id, cx)
            .ok_or_else(|| "The terminal session is no longer live".to_owned())?;
        Ok((record, pane))
    }

    fn terminal_pane_for_session(
        &self,
        session_id: TerminalSessionId,
        cx: &App,
    ) -> Option<Entity<TerminalPane>> {
        let location = self.tab_host.read(cx).terminal_location(session_id)?;
        self.tab_host
            .read(cx)
            .panes()
            .get(&location.pane_id)
            .cloned()
    }
}

fn terminal_lifecycle_projection(lifecycle: &TerminalLifecycle) -> (&'static str, Option<i32>) {
    match lifecycle {
        TerminalLifecycle::Running => ("running", None),
        TerminalLifecycle::Exited(exit_code) => ("exited", *exit_code),
        TerminalLifecycle::Closed => ("closed", None),
    }
}

fn telnet_control(control: PublicTelnetControl) -> TelnetControlCommand {
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
