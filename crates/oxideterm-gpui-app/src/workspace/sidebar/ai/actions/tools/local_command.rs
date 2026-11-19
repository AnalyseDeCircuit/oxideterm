impl WorkspaceApp {
    #[allow(clippy::too_many_arguments)]
    fn start_ai_owned_local_command(
        &mut self,
        owner: AiToolRunContext,
        call_id: String,
        name: String,
        command: zeroize::Zeroizing<String>,
        cwd: Option<String>,
        approved: bool,
        wait_timeout: Duration,
        snapshot: AiOrchestratorRuntimeSnapshot,
        leases: Vec<oxideterm_ai::agent::AgentToolLease>,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        cx: &mut Context<Self>,
    ) {
        let resource = self.ai_owned_resource(
            &owner,
            cx,
            oxideterm_ai::agent::OwnedResourceKind::LocalProcess,
            &self.ai_tool_display_name("run_command"),
        );
        let weak = cx.weak_entity();
        let context = owner.clone();
        let status_call = call_id.clone();
        let status_name = name.clone();
        let report = move |status: &str, data, risk: &str, cx: &mut App| {
            let _ = weak.update(cx, |app, cx| app.report_ai_tool_progress(
                &context, &status_call, &status_name, status, data, risk, cx,
            ));
        };
        self.ai.state.update(cx, |state, cx| {
            state.start_ai_owned_local_command(
                owner, call_id, name, command, cwd, approved, wait_timeout,
                snapshot, leases, sender, resource, report, cx,
            );
        });
    }
}
