impl WorkspaceApp {
    fn monitor_ai_terminal_command(
        &mut self,
        session_id: TerminalSessionId,
        node_id: Option<NodeId>,
        command_id: Option<String>,
        before: &str,
        leases: Vec<oxideterm_ai::agent::AgentToolLease>,
        record: Option<oxideterm_ai::agent::AgentResourceRecord>,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_ai::conversation::terminal_watch::{AiCommandEvidence, AiCommandMonitorSample};
        use oxideterm_ai::tool_resources::AiTerminalCommandStatus;
        use oxideterm_gpui_terminal::TerminalCommandFactStatus;
        let host = cx.weak_entity();
        let sample_host = host.clone();
        let sample_id = command_id.clone();
        let sample = move |cx: &mut App| {
            let Some(app) = sample_host.upgrade() else { return AiCommandMonitorSample::Unavailable };
            let app = app.read(cx);
            let recovering = node_id.as_ref().and_then(|node| app.node_router.connection_id_for_node(node))
                .and_then(|id| app.ssh_registry.get(&id)).is_some_and(|connection|
                    matches!(connection.state(), ConnectionState::Connecting | ConnectionState::LinkDown | ConnectionState::Reconnecting | ConnectionState::Error(_)));
            if recovering { return AiCommandMonitorSample::Recovering; }
            let Some(pane) = app.ai_terminal_pane_for_session(session_id, cx) else { return AiCommandMonitorSample::Unavailable };
            let pane = pane.read(cx);
            if !pane.ai_accepts_input() { return AiCommandMonitorSample::Unavailable; }
            let status = sample_id.as_deref().and_then(|id| pane.ai_command_status(id));
            AiCommandMonitorSample::Current(status.map(|status| match status {
                TerminalCommandFactStatus::Open => AiTerminalCommandStatus::Running,
                TerminalCommandFactStatus::Closed => AiTerminalCommandStatus::Completed,
                TerminalCommandFactStatus::Stale => AiTerminalCommandStatus::Stale,
            }))
        };
        let evidence = move |require_closed: bool, cx: &mut App| {
            let app = host.upgrade()?;
            let pane = app.read(cx).ai_terminal_pane_for_session(session_id, cx)?;
            let pane = pane.read(cx);
            let completed = command_id.as_deref().and_then(|id| pane.ai_command_records().into_iter().find(|entry| {
                entry.command_id == id && (!require_closed || entry.status == TerminalCommandFactStatus::Closed)
            }));
            if require_closed && completed.is_none() { return None; }
            Some(AiCommandEvidence {
                buffer: zeroize::Zeroizing::new(pane.ai_buffer_snapshot()),
                exit_code: completed.and_then(|entry| entry.exit_code),
            })
        };
        self.ai.state.update(cx, |ai, cx| {
            ai.monitor_terminal_command(before, leases, record, sample, evidence, cx);
        });
    }
}
