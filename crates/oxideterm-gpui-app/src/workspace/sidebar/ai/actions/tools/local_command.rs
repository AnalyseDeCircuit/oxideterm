struct AiOwnedCommandTask {
    process: tokio::task::JoinHandle<AiActionResultLite>,
    leases: Vec<oxideterm_ai::agent::AgentToolLease>,
}
impl AiOwnedCommandTask {
    fn new(
        process: tokio::task::JoinHandle<AiActionResultLite>,
        leases: Vec<oxideterm_ai::agent::AgentToolLease>,
    ) -> Self {
        Self { process, leases }
    }

    fn finish(&self, action: &AiActionResultLite) {
        // A failed exit still completes the operation; losing observation does not.
        if matches!(
            action.data.get("executionState").and_then(serde_json::Value::as_str),
            Some("completed" | "not_started")
        ) {
            for lease in &self.leases {
                lease.command_finished();
            }
        }
    }
}
impl Drop for AiOwnedCommandTask {
    fn drop(&mut self) {
        self.process.abort();
        for lease in &self.leases {
            lease.command_unresolved();
        }
    }
}

impl WorkspaceApp {
    fn ai_command_wait_extension(
        &mut self,
        owner: &AiToolRunContext,
        call_id: &str,
        name: &str,
        command_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> tokio::sync::oneshot::Receiver<bool> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.apply_ai_tool_status(
            owner.generation,
            &owner.conversation_id,
            &owner.assistant_id,
            call_id,
            name,
            &owner.arguments,
            "pending_user_approval",
            Some(serde_json::json!({"waitTimedOut":true, "commandId":command_id})),
            Some("read".into()),
            None,
            false,
            None,
            None,
            None,
            cx,
        );
        self.ai_entity.update(cx, |ai, _| {
            ai.register_tool_approval(owner.generation, call_id.to_owned(), sender)
        });
        receiver
    }

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
        let resource = owner.resource(
            self,
            cx,
            oxideterm_ai::agent::OwnedResourceKind::LocalProcess,
            &self.ai_tool_display_name("run_command"),
        );
        let process = self.forwarding_runtime.spawn(async move {
            run_local_ai_command(&command, cwd.as_deref(), approved, resource).await
        });
        let process = AiOwnedCommandTask::new(process, leases);
        let key = (
            owner.conversation_id.clone(),
            owner.generation,
            call_id.clone(),
        );
        let task_key = key.clone();
        let task = cx.spawn(async move |weak, cx| {
            let mut process = process;
            let mut sender = Some(sender);
            let started = std::time::Instant::now();
            loop {
                if sender.is_some() {
                    let _ = weak.update(cx, |this, cx| this.apply_ai_tool_status(owner.generation, &owner.conversation_id,
                        &owner.assistant_id, &call_id, &name, &owner.arguments, "waiting_condition",
                        Some(serde_json::json!({"waiting":true,"waitDeadline":ai_now_ms() + wait_timeout.as_millis() as i64})),
                        Some("execute".into()), None, false, None, None, None, cx));
                }
                let result = tokio::select! {
                    biased;
                    _ = async { if let Some(dispatch) = &owner.dispatch { dispatch.cancelled().await; } else { std::future::pending::<()>().await; } } => break,
                    result = &mut process.process => Some(result),
                    _ = Timer::after(wait_timeout), if sender.is_some() => None,
                    _ = async { if let Some(sender) = &mut sender { sender.closed().await; } else { std::future::pending::<()>().await; } } => break,
                    _ = async { if let Some(dispatch) = &owner.dispatch { dispatch.invalidated().await; } else { std::future::pending::<()>().await; } }, if sender.is_some() => {
                        if let Some(sender) = sender.take() {
                            let action = snapshot.ok("Local command is still running; task direction changed.", "Inspect its final outcome before repeating it.",
                                serde_json::json!({"executionState":"running", "outcomeUnknown":true}), "execute");
                            let _ = sender.send(snapshot.to_executed_tool_result(call_id.clone(), name.clone(), action, started.elapsed().as_millis()));
                        }
                        continue;
                    }
                };
                if let Some(result) = result {
                    if let Ok(action) = &result { process.finish(action); }
                    if let (Ok(action), Some(sender)) = (result, sender.take()) {
                        let _ = sender.send(snapshot.to_executed_tool_result(call_id.clone(), name.clone(), action, started.elapsed().as_millis()));
                    }
                    break;
                }
                let Ok(resume) = weak.update(cx, |this, cx| this.ai_command_wait_extension(&owner, &call_id, &name, None, cx)) else { break; };
                let continued = tokio::select! {
                    biased;
                    _ = async { if let Some(dispatch) = &owner.dispatch { dispatch.cancelled().await; } else { std::future::pending::<()>().await; } } => break,
                    value = ai_pending_dispatch(owner.dispatch.as_ref(), resume) => value,
                    _ = sender.as_mut().unwrap().closed() => break,
                };
                match continued {
                    Ok(Ok(true)) => {},
                    _ => {
                        if let Some(sender) = sender.take() {
                            let action = snapshot.fail("Observation paused; the local command may still be running.", "agent_wait_paused",
                                "Do not repeat the command without checking its outcome.", "execute");
                            let _ = sender.send(snapshot.to_executed_tool_result(call_id.clone(), name.clone(), action, started.elapsed().as_millis()));
                        }
                    }
                }
            }
            let _ = weak.update(cx, |this, cx| this.ai_entity.update(cx, |ai, _| { ai.agents.local_commands.remove(&task_key); }));
        });
        // The conversation owns both observation and the process abort handle, including after steering.
        self.ai_entity.update(cx, |ai, _| {
            ai.agents.local_commands.insert(key, task);
        });
    }
}
