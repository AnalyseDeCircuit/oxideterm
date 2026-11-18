// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_ai_background_task_event(
        &mut self,
        _event: ai_background_tasks::AiBackgroundTaskEvent,
        cx: &mut Context<Self>,
    ) {
        let events = self.ai.background.read(cx).take_events();
        let requests = self
            .ai.background
            .read(cx)
            .take_execution_requests();
        for request in requests {
            if request.response.is_closed() {
                continue;
            }
            let context = request.execution.audit_context.clone();
            let result = oxideterm_audit::AuditContext::with_sync_request(context.as_ref(), || {
                self.execute_ai_background_read(request.execution, cx)
            });
            let _ = request.response.send(result);
        }
        let mut notifications_changed = false;
        for event in events {
            let oxideterm_ai_tasks::BackgroundTaskEvent::Changed(snapshot) = event else {
                continue;
            };
            let (severity, title_key) = match snapshot.state {
                oxideterm_ai_tasks::BackgroundTaskState::Completed => (
                    WorkspaceNotificationSeverity::Info,
                    "ai.background_tasks.completed",
                ),
                oxideterm_ai_tasks::BackgroundTaskState::Failed => (
                    WorkspaceNotificationSeverity::Error,
                    "ai.background_tasks.failed",
                ),
                _ => continue,
            };
            let body = self
                .i18n
                .t("ai.background_tasks.notification_body")
                .replace("{{count}}", &snapshot.run_count.to_string());
            notifications_changed = true;
            self.push_notification_entry(
                WorkspaceNotificationKind::Agent,
                severity,
                format!("{} · {}", self.i18n.t(title_key), snapshot.title),
                Some(body),
                WorkspaceNotificationScope::Global,
                Some(format!("ai-background-task:{}", snapshot.id.as_str())),
            );
        }
        if notifications_changed {
            cx.notify();
        }
    }

    fn execute_ai_background_read(
        &mut self,
        execution: oxideterm_ai_tasks::BackgroundTaskExecution,
        cx: &mut Context<Self>,
    ) -> Result<oxideterm_ai_tasks::BackgroundTaskExecutionResult, String> {
        use oxideterm_gpui_ai::background::{AiBackgroundRead, AiBackgroundReadPlan};
        let plan = AiBackgroundReadPlan::from_execution(execution)?;
        let snapshot = self.ai_orchestrator_snapshot(cx);
        plan.execute(&snapshot, |read, args, snapshot| {
            let session = ToolSessionId::new();
            match read {
                AiBackgroundRead::State => self.execute_ai_get_state(&session, args, cx),
                AiBackgroundRead::StableResource => self.execute_ai_read_stable_resource(args, cx),
                AiBackgroundRead::HostTools => read.result(snapshot, self.execute_ai_inspect_host_tools(&session, args, cx)),
                AiBackgroundRead::Forwards => read.result(snapshot, self.execute_ai_list_forwards(&session, args, cx)),
                AiBackgroundRead::Plugins => read.result(snapshot, Ok(self.execute_ai_list_plugins(cx))),
            }
        })
    }
}
