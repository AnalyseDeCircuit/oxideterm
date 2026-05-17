impl WorkspaceApp {
    pub(super) fn poll_ai_chat_stream_events(
        &mut self,
        mut window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let Some(rx) = self.ai_chat_stream_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        while let Ok(delivery) = rx.try_recv() {
            let done = matches!(
                delivery.event,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Done | AiStreamEvent::Error(_))
            );
            match delivery.event {
                AiStreamDeliveryEvent::Stream(event) => {
                    self.apply_ai_stream_event(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        event,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::Guardrail {
                    code,
                    message,
                    raw_text,
                } => {
                    self.apply_ai_guardrail(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &code,
                        &message,
                        raw_text,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::AssistantRound {
                    round_id,
                    round_number,
                    response_length,
                    tool_call_ids,
                    synthetic,
                    retry_attempt,
                    hard_deny_triggered,
                } => {
                    self.persist_ai_assistant_round(
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        round_id,
                        round_number,
                        response_length,
                        tool_call_ids,
                        synthetic,
                        retry_attempt,
                        hard_deny_triggered,
                    );
                }
                AiStreamDeliveryEvent::RoundSummary {
                    round_id,
                    text,
                    metadata,
                } => {
                    self.apply_ai_round_summary(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &round_id,
                        &text,
                        metadata,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::RoundStatefulMarker { round_id, marker } => {
                    self.apply_ai_round_stateful_marker(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &round_id,
                        marker,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::Diagnostic {
                    event_type,
                    round_id,
                    data,
                } => {
                    self.persist_ai_stream_diagnostic(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &event_type,
                        round_id,
                        data,
                    );
                }
                AiStreamDeliveryEvent::ToolStatus {
                    tool_call_id,
                    name,
                    arguments,
                    status,
                    result,
                    risk,
                    summary,
                    synthetic_denied,
                    raw_text,
                    round_id,
                    round_number,
                } => {
                    self.apply_ai_tool_status(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &tool_call_id,
                        &name,
                        &arguments,
                        &status,
                        result,
                        risk,
                        summary,
                        synthetic_denied,
                        raw_text,
                        round_id,
                        round_number,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::ToolApprovalRequested {
                    tool_call_id,
                    name,
                    arguments,
                    risk,
                    summary,
                    sender,
                } => {
                    self.ai_pending_tool_approvals
                        .insert(tool_call_id.clone(), sender);
                    self.apply_ai_tool_status(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &tool_call_id,
                        &name,
                        &arguments,
                        "pending_user_approval",
                        None,
                        Some(risk),
                        Some(summary),
                        false,
                        None,
                        None,
                        None,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::ToolExecutionRequested {
                    tool_call_id,
                    name,
                    args,
                    sender,
                } => {
                    let Some(window) = window.as_deref_mut() else {
                        self.ai_chat_stream_rx = Some(rx);
                        self.schedule_ai_chat_stream_poll(cx);
                        cx.notify();
                        return;
                    };
                    self.start_ai_ui_orchestrator_tool_execution(
                        tool_call_id,
                        name,
                        args,
                        sender,
                        window,
                        cx,
                    );
                }
            }
            if done {
                keep_rx = false;
                break;
            }
        }
        if keep_rx {
            self.ai_chat_stream_rx = Some(rx);
        }
    }

    fn schedule_ai_chat_stream_poll(&mut self, cx: &mut Context<Self>) {
        if self.ai_chat_stream_polling {
            return;
        }
        self.ai_chat_stream_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let _ = weak.update(cx, |this, cx| {
                this.ai_chat_stream_polling = false;
                if this.ai_chat_stream_rx.is_some() {
                    cx.notify();
                    this.schedule_ai_chat_stream_poll(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn poll_ai_compaction_results(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.ai_compaction_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        while let Ok(delivery) = rx.try_recv() {
            keep_rx = false;
            match delivery.kind {
                AiCompactionDeliveryKind::Compact => {
                    if let Some(plan) = delivery.plan {
                        self.finish_ai_compaction(
                            delivery.conversation_id,
                            delivery.base_ids,
                            plan,
                            delivery.summary,
                            delivery.stream_error,
                            delivery.resume_after,
                            delivery.silent,
                            cx,
                        );
                    }
                }
                AiCompactionDeliveryKind::Summary => {
                    self.finish_ai_summary(
                        delivery.conversation_id,
                        delivery.base_ids,
                        delivery.summary,
                        delivery.stream_error,
                        cx,
                    );
                }
            }
        }
        if keep_rx {
            self.ai_compaction_rx = Some(rx);
        }
    }

    fn schedule_ai_compaction_poll(&mut self, cx: &mut Context<Self>) {
        if self.ai_compaction_polling {
            return;
        }
        self.ai_compaction_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(50)).await;
            let _ = weak.update(cx, |this, cx| {
                this.ai_compaction_polling = false;
                this.poll_ai_compaction_results(cx);
                if this.ai_compaction_rx.is_some() {
                    this.schedule_ai_compaction_poll(cx);
                }
            });
        })
        .detach();
    }
}
