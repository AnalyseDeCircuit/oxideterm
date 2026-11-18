use gpui::{App, Context, Entity, Window};
use oxideterm_connections::ConnectionTerminalOptions;
use oxideterm_gpui_terminal::{TerminalPane, TerminalSerialAction, TerminalTelnetAction};
use oxideterm_public_mcp::{
    DomainRequest, PublicToolCall, TerminalControlAction, TerminalOpenSource, TerminalRef,
    ToolEnvelope,
};
use oxideterm_public_mcp_host::{
    PublicMcpSavedTerminal, PublicMcpTerminalSerialState, finish_terminal_read, telnet_control,
    terminal_projection,
};
use oxideterm_terminal::{SerialSessionConfig, TelnetSessionConfig, TerminalSessionKind};
use serde_json::json;

use super::{
    PublicMcpPendingTerminalOpen, PublicMcpTerminalRecord, WorkspaceApp, finish_serialized,
};
use crate::workspace::{
    TerminalSessionId,
    new_connection::{
        MoshConnectionOptions, SshConnectionIntent, mosh_options_from_profile,
        terminal_serial_flow_from_profile, terminal_serial_parity_from_profile,
        terminal_serial_runtime_options_from_profile,
    },
};

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
        let Ok((_, pane)) = self.public_mcp_terminal_pane(&request.client_ref, &terminal_ref, cx)
        else {
            request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
            return;
        };
        let pane = pane.read(cx);
        let snapshot = pane.ai_screen_snapshot();
        finish_terminal_read(
            request,
            snapshot.generation,
            (snapshot.cols, snapshot.rows),
            snapshot.display_offset,
            pane.ai_screen_is_alternate_buffer(),
            || pane.visible_text_snapshot(),
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
        let Ok((record, pane)) =
            self.public_mcp_terminal_pane(&request.client_ref, &terminal_ref, cx)
        else {
            request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
            return;
        };
        self.public_mcp.background().find_terminal(
            request,
            record.session_id,
            pane.read(cx).shared_session(),
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
            Ok((cols, rows)) => finish_serialized(
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
        if let Some(error) = self.public_mcp.terminal_open_rejection(&request.client_ref) {
            request.finish(ToolEnvelope::failed(error));
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
                let Some(lease) = self.public_mcp.node_lease(&request.client_ref, &node_ref) else {
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
        let profile = match self.public_mcp.saved_terminal_profile(
            &request.client_ref,
            &connection_ref,
            &self.connection_store,
        ) {
            Ok(profile) => profile,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        match profile {
            PublicMcpSavedTerminal::Serial(profile) => {
                let profile_id = profile.id.clone();
                let title = requested_title.unwrap_or_else(|| profile.name.clone());
                let runtime_options = terminal_serial_runtime_options_from_profile(&profile);
                let config = SerialSessionConfig {
                    port_path: profile.port_path,
                    baud_rate: profile.baud_rate,
                    data_bits: profile.data_bits,
                    stop_bits: profile.stop_bits,
                    parity: terminal_serial_parity_from_profile(&profile.parity),
                    flow_control: terminal_serial_flow_from_profile(&profile.flow_control),
                    runtime_options,
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
                            profile_id.clone(),
                            cx,
                        );
                        let _ = self.connection_store.mark_serial_profile_used(&profile_id);
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
            PublicMcpSavedTerminal::Telnet(profile) => {
                let profile_id = profile.id.clone();
                let title = requested_title.unwrap_or_else(|| profile.name.clone());
                let config = TelnetSessionConfig {
                    line_endings:
                        crate::workspace::new_connection::terminal_telnet_line_endings_from_profile(
                            &profile,
                        ),
                    host: profile.host,
                    port: profile.port,
                };
                match self.create_telnet_terminal_tab_with_title(
                    config,
                    profile.upstream_proxy,
                    profile.terminal,
                    title.clone(),
                    window,
                    cx,
                ) {
                    Ok(session_id) => {
                        self.register_terminal_saved_connection(
                            session_id,
                            oxideterm_terminal_triggers::SavedConnectionKind::Telnet,
                            profile_id.clone(),
                            cx,
                        );
                        let _ = self.connection_store.mark_telnet_profile_used(&profile_id);
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
            PublicMcpSavedTerminal::Mosh { profile, config } => {
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
                let mut options = mosh_options_from_profile(&profile);
                options.public_mcp_open_token = Some(pending_token);
                self.start_ssh_preflight(*config, title, SshConnectionIntent::Mosh(options), cx);
                return;
            }
        }
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
        let Some(record) = self.public_mcp.terminal(&request.client_ref, &terminal_ref) else {
            request.finish(ToolEnvelope::failed("The terminal handle is unavailable"));
            return;
        };
        self.stop_public_mcp_recordings(&request.client_ref, Some(&terminal_ref), false, cx);
        self.public_mcp.remove_terminal(&terminal_ref);
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
        self.public_mcp.register_terminal(
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
        self.public_mcp.history.lock().revoke_client(client_ref);
        let terminal_refs = self.public_mcp.client_terminals(client_ref);
        for terminal_ref in &terminal_refs {
            self.stop_public_mcp_recordings(client_ref, Some(terminal_ref), false, cx);
        }
        let session_ids = terminal_refs
            .into_iter()
            .filter_map(|terminal_ref| self.public_mcp.remove_terminal(&terminal_ref))
            .map(|record| record.session_id)
            .collect::<Vec<_>>();
        self.public_mcp.revoke_pending_terminal_opens(client_ref);
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
        let terminal_refs = self.public_mcp.terminals_for_session(session_id);
        for (terminal_ref, client_ref) in terminal_refs {
            // A UI-owned pane close revokes only its public handle; an SSH node
            // remains owned by NodeRouter and any other registered consumers.
            self.stop_public_mcp_recordings(&client_ref, Some(&terminal_ref), false, cx);
            self.public_mcp.remove_terminal(&terminal_ref);
        }
    }

    pub(in crate::workspace) fn stop_public_mcp_recordings_for_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        let terminal_refs = self.public_mcp.terminals_for_session(session_id);
        for (terminal_ref, client_ref) in terminal_refs {
            self.public_mcp.revoke_terminal_history(&terminal_ref);
            // The public handle stays stable; recordings end at the old shell boundary.
            self.stop_public_mcp_recordings(&client_ref, Some(&terminal_ref), false, cx);
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
        let serial = pane
            .serial_status()
            .map(|status| PublicMcpTerminalSerialState {
                port_available: status.port_available,
                can_reconnect: status.can_reconnect,
                data_terminal_ready: status.control_state.data_terminal_ready,
                request_to_send: status.control_state.request_to_send,
            });
        Ok(terminal_projection(
            &record,
            terminal_ref,
            (snapshot.cols, snapshot.rows),
            alternate_buffer,
            session_kind,
            serial,
            pane.shared_session(),
        ))
    }

    pub(super) fn public_mcp_terminal_pane(
        &self,
        client_ref: &oxideterm_public_mcp::ClientRef,
        terminal_ref: &TerminalRef,
        cx: &App,
    ) -> Result<(PublicMcpTerminalRecord, Entity<TerminalPane>), String> {
        let record = self
            .public_mcp
            .terminal(client_ref, terminal_ref)
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
