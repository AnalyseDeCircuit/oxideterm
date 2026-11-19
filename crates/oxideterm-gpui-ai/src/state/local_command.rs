use std::time::Duration;

use gpui::{App, Context, Timer};
use oxideterm_ai::{
    AiExecutedToolResult,
    agent::AgentResourceRecord,
    tool_loop::ai_pending_dispatch,
    tool_resources::{AiActionResultLite, AiOrchestratorRuntimeSnapshot, run_local_ai_command},
};

use super::{AiWorkspaceEntity, ai_now_ms};

#[derive(Clone)]
pub struct AiToolRunContext {
    pub arguments: zeroize::Zeroizing<String>,
    pub generation: u64,
    pub conversation_id: String,
    pub assistant_id: String,
    pub dispatch: Option<oxideterm_ai::agent::AgentDispatch>,
}

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

impl AiWorkspaceEntity {
    #[allow(clippy::too_many_arguments)]
    pub fn start_ai_owned_local_command(
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
        resource: Option<AgentResourceRecord>,
        report: impl Fn(&str, Option<serde_json::Value>, &str, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) {
        let process = self.task_runtime().spawn(async move {
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
                    cx.update(|cx| report("waiting_condition", Some(serde_json::json!({"waiting":true,"waitDeadline":ai_now_ms() + wait_timeout.as_millis() as i64})), "execute", cx));
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
                let (approval, resume) = tokio::sync::oneshot::channel();
                cx.update(|cx| report("pending_user_approval", Some(serde_json::json!({"waitTimedOut":true, "commandId":null})), "read", cx));
                if weak.update(cx, |state, _| state.register_tool_approval(owner.generation, call_id.clone(), approval)).is_err() {
                    break;
                }
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
            let _ = weak.update(cx, |state, _| { state.agents.local_commands.remove(&task_key); });
        });
        // The conversation owns both observation and the process abort handle, including after steering.
        self.agents.local_commands.insert(key, task);
    }
}

#[cfg(test)]
mod local_command_tests {
    use super::*;

    #[tokio::test]
    async fn local_command_cleanup_and_request_ownership_are_independent() {
        use oxideterm_ai::agent::{AgentModel, AgentResourceCoordinator, AgentRuntime, AgentScope, AgentToolLease};
        let runtime = AgentRuntime::new(1);
        let run = runtime.create_group("local-command".into(), AgentModel {
            provider_id: "provider".into(), model: "model".into(),
        }, AgentScope::default(), 8);
        let resources = AgentResourceCoordinator::default();
        let key = resources.workspace_resource();
        for early_response in [None, Some(true), Some(false)] {
            let lease = resources.acquire(key.clone(), run.clone(), runtime.cancellation(&run).unwrap()).await.unwrap();
            let lease = AgentToolLease::new(resources.clone(), lease);
            lease.dispatched();
            let (release, wait) = tokio::sync::oneshot::channel();
            let process = tokio::spawn(async move {
                wait.await.unwrap();
                run_local_ai_command("exit 7", None, false, None).await
            });
            let mut task = AiOwnedCommandTask::new(process, vec![lease.clone()]);
            if let Some(success) = early_response { lease.finish_response(success); }
            assert_eq!(resources.owned_by(&key, &run).is_some(), early_response.is_none(), "ownership ends with the request, independently of process outcome");
            release.send(()).unwrap();
            let action = (&mut task.process).await.unwrap();
            assert!(!action.ok);
            assert_eq!(action.data["exitCode"], 7);
            task.finish(&action);
            lease.finish_response(action.ok);
            drop(task);
            let next = tokio::time::timeout(Duration::from_secs(1), resources.acquire(key.clone(), run.clone(), runtime.cancellation(&run).unwrap())).await.unwrap().unwrap();
            resources.complete(&next);
        }

        let lease = resources.acquire(key.clone(), run.clone(), runtime.cancellation(&run).unwrap()).await.unwrap();
        let lease = AgentToolLease::new(resources.clone(), lease);
        lease.dispatched();
        let task = AiOwnedCommandTask::new(tokio::spawn(std::future::pending()), vec![lease]);
        drop(task);
        let next = resources.acquire(key, run.clone(), runtime.cancellation(&run).unwrap()).await.unwrap();
        assert!(resources.owns(&next));
        assert!(resources.complete(&next));
    }
}

impl AiWorkspaceEntity {
    pub fn apply_owned_tool_progress(
        &mut self, owner: &AiToolRunContext, call: &str, name: &str, status: &str,
        data: Option<serde_json::Value>, risk: &str,
        diagnostics: impl Fn(serde_json::Value) -> serde_json::Value, cx: &mut Context<Self>,
    ) {
        self.apply_ai_tool_status(
            owner.generation, &owner.conversation_id, &owner.assistant_id, call, name,
            &owner.arguments, status, data, Some(risk.to_owned()), None, false, None,
            None, None, diagnostics, cx,
        );
    }
}
