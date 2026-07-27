#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum PendingAiStreamTextKind {
    Content,
    Thinking,
}

pub(in crate::workspace) struct PendingAiStreamText {
    generation: u64,
    conversation_id: String,
    assistant_id: String,
    kind: PendingAiStreamTextKind,
    text: String,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn schedule_ai_chat_stream_delivery_apply(
        &mut self,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        let deliveries = self
            .ai_entity
            .update(cx, |ai, _cx| ai.take_chat_stream_deliveries());
        if deliveries.is_empty() {
            return;
        }
        cx.spawn(async move |weak, cx| {
            let _ = cx.update_window(window_handle, |_, window, cx| {
                weak.update(cx, |workspace, cx| {
                    workspace.apply_ai_chat_stream_deliveries(deliveries, window, cx);
                })
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn apply_ai_chat_stream_deliveries(
        &mut self,
        deliveries: VecDeque<AiStreamDelivery>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut pending_text: Option<PendingAiStreamText> = None;
        for delivery in deliveries {
            if !self
                .ai_entity
                .read(cx)
                .is_chat_stream_generation(delivery.generation)
            {
                // Dropping a stale delivery also drops any retained approval
                // sender, matching the old generation-scoped receiver.
                continue;
            }
            match delivery.event {
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Content(chunk)) => {
                    self.merge_or_flush_pending_ai_stream_text(
                        &mut pending_text,
                        delivery.generation,
                        delivery.conversation_id,
                        delivery.assistant_id,
                        PendingAiStreamTextKind::Content,
                        chunk,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Thinking(chunk)) => {
                    self.merge_or_flush_pending_ai_stream_text(
                        &mut pending_text,
                        delivery.generation,
                        delivery.conversation_id,
                        delivery.assistant_id,
                        PendingAiStreamTextKind::Thinking,
                        chunk,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::Stream(event) => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.apply_ai_stream_event(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        event,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::AcpClientEvent(event) => {
                    // ACP session/update notifications are normalized in
                    // oxideterm-ai, then consumed by the same stream apply path
                    // as provider events so generation guards remain shared.
                    match event {
                        oxideterm_ai::AcpClientEvent::RequestPermission {
                            request,
                            response_tx,
                        } => {
                            self.flush_pending_ai_stream_text(&mut pending_text, cx);
                            if self.ai_entity.read(cx).chat_stream_generation()
                                != delivery.generation
                            {
                                let _ = response_tx
                                    .send(Ok(oxideterm_ai::acp_permission_cancelled_response()));
                                continue;
                            }
                            let projection =
                                oxideterm_ai::acp_permission_request_projection(&request);
                            let (approval_tx, approval_rx) = tokio::sync::oneshot::channel();
                            self.ai
                                .runtime
                                .pending_tool_approvals
                                .insert(projection.tool_call_id.clone(), approval_tx);
                            let forwarding_runtime = self.forwarding_runtime.clone();
                            forwarding_runtime.spawn(async move {
                                let approved = approval_rx.await.unwrap_or(false);
                                let response = oxideterm_ai::acp_permission_response_for_decision(
                                    &request, approved,
                                );
                                let _ = response_tx.send(Ok(response));
                            });
                            self.apply_ai_tool_status(
                                delivery.generation,
                                &delivery.conversation_id,
                                &delivery.assistant_id,
                                &projection.tool_call_id,
                                &projection.name,
                                &projection.arguments,
                                "pending_user_approval",
                                None,
                                Some(projection.risk),
                                Some(projection.summary),
                                false,
                                None,
                                None,
                                None,
                                cx,
                            );
                        }
                        event => {
                            for stream_event in
                                oxideterm_ai::acp_client_event_to_ai_stream_events(event)
                            {
                                match stream_event {
                                    AiStreamEvent::Content(chunk) => {
                                        self.merge_or_flush_pending_ai_stream_text(
                                            &mut pending_text,
                                            delivery.generation,
                                            delivery.conversation_id.clone(),
                                            delivery.assistant_id.clone(),
                                            PendingAiStreamTextKind::Content,
                                            chunk,
                                            cx,
                                        );
                                    }
                                    AiStreamEvent::Thinking(chunk) => {
                                        self.merge_or_flush_pending_ai_stream_text(
                                            &mut pending_text,
                                            delivery.generation,
                                            delivery.conversation_id.clone(),
                                            delivery.assistant_id.clone(),
                                            PendingAiStreamTextKind::Thinking,
                                            chunk,
                                            cx,
                                        );
                                    }
                                    event => {
                                        self.flush_pending_ai_stream_text(&mut pending_text, cx);
                                        self.apply_ai_stream_event(
                                            delivery.generation,
                                            &delivery.conversation_id,
                                            &delivery.assistant_id,
                                            event,
                                            cx,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                AiStreamDeliveryEvent::AcpSessionStarted {
                    session_id,
                    session_metadata,
                    session_config_options,
                    agent_id,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    if self.apply_ai_acp_session_started(
                        delivery.generation,
                        &delivery.conversation_id,
                        &session_id,
                        session_metadata,
                        session_config_options,
                        &agent_id,
                        cx,
                    ) {
                        cx.notify();
                    }
                }
                AiStreamDeliveryEvent::Guardrail {
                    code,
                    message,
                    raw_text,
                } => {
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
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
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
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
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
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
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
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
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.persist_ai_stream_diagnostic(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &event_type,
                        round_id,
                        data,
                        cx,
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
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
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
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
                    self.ai
                        .runtime
                        .pending_tool_approvals
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
                    self.flush_pending_ai_stream_text(&mut pending_text, cx);
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
        }
        self.flush_pending_ai_stream_text(&mut pending_text, cx);
    }

    pub(in crate::workspace) fn merge_or_flush_pending_ai_stream_text(
        &mut self,
        pending: &mut Option<PendingAiStreamText>,
        generation: u64,
        conversation_id: String,
        assistant_id: String,
        kind: PendingAiStreamTextKind,
        chunk: String,
        cx: &mut Context<Self>,
    ) {
        if chunk.is_empty() {
            return;
        }
        if let Some(existing) = pending.as_mut()
            && existing.generation == generation
            && existing.conversation_id == conversation_id
            && existing.assistant_id == assistant_id
            && existing.kind == kind
        {
            existing.text.push_str(&chunk);
            return;
        }

        self.flush_pending_ai_stream_text(pending, cx);
        *pending = Some(PendingAiStreamText {
            generation,
            conversation_id,
            assistant_id,
            kind,
            text: chunk,
        });
    }

    pub(in crate::workspace) fn flush_pending_ai_stream_text(
        &mut self,
        pending: &mut Option<PendingAiStreamText>,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = pending.take() else {
            return;
        };
        let event = match pending.kind {
            PendingAiStreamTextKind::Content => AiStreamEvent::Content(pending.text),
            PendingAiStreamTextKind::Thinking => AiStreamEvent::Thinking(pending.text),
        };
        self.apply_ai_stream_event(
            pending.generation,
            &pending.conversation_id,
            &pending.assistant_id,
            event,
            cx,
        );
    }

    pub(in crate::workspace) fn apply_ai_compaction_deliveries(
        &mut self,
        deliveries: VecDeque<AiCompactionDelivery>,
        cx: &mut Context<Self>,
    ) {
        for delivery in deliveries {
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
    }
}
