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
        for lease in leases {
            let before = zeroize::Zeroizing::new(before.to_owned());
            let record = record.clone();
            let key = lease.lease().resource.clone();
            let node_id = node_id.clone();
            let command_id = command_id.clone();
            let resources = self.ai.state.read(cx).agents.services.resources.clone();
            let monitor_key = key.clone();
            let task = cx.spawn(async move |weak, cx| {
                loop {
                    Timer::after(Duration::from_millis(250)).await;
                    let finished = weak.update(cx, |this, cx| {
                        if !lease.is_current() {
                            // A returned tool no longer owns input. Preserve any confirmed outcome
                            // without invalidating or extending the next request's ownership.
                            if let Some(record) = &record {
                                let completed = this.ai_terminal_session(session_id, cx).and_then(|(_, pane)| {
                                    let pane = pane.read(cx);
                                    command_id.as_deref().and_then(|id| pane.ai_command_records().into_iter()
                                        .find(|entry| entry.command_id == id && entry.status == oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed))
                                        .map(|entry| (entry.exit_code, terminal_delta_output(&before, &pane.ai_buffer_snapshot())))
                                });
                                if let Some((exit_code, output)) = completed {
                                    record.outcome(&serde_json::json!({"exitCode":exit_code, "output":output}).to_string());
                                    record.finish(oxideterm_ai::agent::OwnedResourceState::Completed);
                                } else {
                                    record.finish(oxideterm_ai::agent::OwnedResourceState::OutcomeUnknown);
                                }
                            }
                            return true;
                        }
                        let recovering = node_id.as_ref().and_then(|node| this.node_router.connection_id_for_node(node))
                            .and_then(|id| this.ssh_registry.get(&id)).is_some_and(|connection|
                                matches!(connection.state(), ConnectionState::Connecting | ConnectionState::LinkDown | ConnectionState::Reconnecting | ConnectionState::Error(_)));
                        if recovering { return false; }
                        let Some((_, pane)) = this.ai_terminal_session(session_id, cx) else { resources.invalidate(&monitor_key); return true; };
                        let pane = pane.read(cx);
                        if !pane.ai_accepts_input() { resources.invalidate(&monitor_key); return true; }
                        let status = command_id.as_deref().and_then(|id| pane.ai_command_status(id));
                        if lease.response_finished() && status.is_none_or(|status| status == oxideterm_gpui_terminal::TerminalCommandFactStatus::Stale) {
                            if let Some(record) = &record { record.finish(oxideterm_ai::agent::OwnedResourceState::OutcomeUnknown); }
                            resources.invalidate(&monitor_key);
                            return true;
                        }
                        if lease.response_finished() && status == Some(oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed) {
                            lease.command_finished();
                            if let Some(record) = &record {
                                let exit_code = command_id.as_deref().and_then(|id| pane.ai_command_records().into_iter()
                                    .find(|entry| entry.command_id == id)).and_then(|entry| entry.exit_code);
                                record.outcome(&serde_json::json!({"exitCode":exit_code,
                                    "output":terminal_delta_output(&before, &pane.ai_buffer_snapshot())}).to_string());
                                record.finish(oxideterm_ai::agent::OwnedResourceState::Completed);
                            }
                            return true;
                        }
                        false
                    });
                    if finished.unwrap_or(true) { break; }
                }
                let _ = weak.update(cx, |this, cx| { this.ai.state.update(cx, |ai, _cx| { ai.agents.command_monitors.remove(&monitor_key); }); });
            });
            self.ai.state.update(cx, |ai, _cx| {
                ai.agents.command_monitors.insert(key, task);
            });
        }
    }
}
