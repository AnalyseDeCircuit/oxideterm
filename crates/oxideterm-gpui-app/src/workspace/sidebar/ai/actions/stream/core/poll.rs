use oxideterm_gpui_ai::stream::AiChatWindowDelivery;
use oxideterm_gpui_ai::conversation::acp_file_review::AcpFileReviewBindings;

impl WorkspaceApp {
    fn acp_file_review_bindings(&self, cx: &Context<Self>) -> AcpFileReviewBindings {
        let owner = cx.weak_entity();
        let attention = owner.clone();
        AcpFileReviewBindings {
            preview: crate::workspace::acp_workspace::acp_file_write_preview,
            frame: Rc::new(move |cx| {
                let app = owner.upgrade()?.read(cx);
                let ai = &app.settings_store.settings().ai;
                Some(oxideterm_gpui_ai::stream::AiChatStreamFrame {
                    i18n: app.i18n.clone(),
                    tool_use_enabled: ai.tool_use.enabled,
                    provider_id: ai.active_provider_id.clone(),
                    model: ai.active_model.clone(),
                })
            }),
            request_attention: Rc::new(move |conversation_id, assistant_id, cx| {
                let _ = attention.update(cx, |app, cx| {
                    app.notify_ai_agent_attention(conversation_id, assistant_id, "ai.agents.approval", cx);
                });
            }),
        }
    }

    pub(in crate::workspace) fn apply_ai_chat_window_request(
        &mut self,
        delivery: AiChatWindowDelivery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delivery = match delivery {
            AiChatWindowDelivery::Host(delivery) => delivery,
            AiChatWindowDelivery::Attention { conversation_id, assistant_id, label, .. } => {
                self.notify_ai_agent_attention(&conversation_id, &assistant_id, label, cx);
                return;
            }
            AiChatWindowDelivery::FocusCandidate { .. } => {
                window.focus(&self.focus_handle, cx);
                return;
            }
        };
        match delivery.event {
            AiStreamDeliveryEvent::ToolImages(_)
            | AiStreamDeliveryEvent::HistoryBarrier(_)
            | AiStreamDeliveryEvent::Checkpoint(_)
            | AiStreamDeliveryEvent::PromptUsage { .. }
            | AiStreamDeliveryEvent::Stream(
                AiStreamEvent::Content(_) | AiStreamEvent::Thinking(_) | AiStreamEvent::Usage { .. }
                | AiStreamEvent::ProviderResponsePart { .. } | AiStreamEvent::ToolCall { .. }
                | AiStreamEvent::ToolCallComplete { .. },
            ) => {
                unreachable!("chat batch consumes local deliveries");
            }
            AiStreamDeliveryEvent::ToolResourcesRequested { tool_session_id, name, args, sender } => {
let result = self.ai.conversations.tool_resource_keys(delivery.generation, &tool_session_id, &name, &args, cx);
                let _ = sender.send(result);
            }
            AiStreamDeliveryEvent::AgentCommandRequested { dispatch, tool_session_id, call, sender } => {
                if dispatch.as_ref().is_some_and(|guard| guard.check().is_err()) {
                    let _ = sender.send(ai_direction_changed_result(&call));
                    return;
                }
                self.handle_ai_agent_command(delivery.generation, &tool_session_id, call, sender, cx);
            }
            AiStreamDeliveryEvent::Stream(event) => {
                self.apply_ai_stream_event(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    event,
                    cx,
                );
            }
            AiStreamDeliveryEvent::AcpClientEvent { agent_id, event } => {
                let event = match self.ai.conversations.handle_acp_session_request(
                    &agent_id, delivery.generation, &delivery.conversation_id, &delivery.assistant_id,
                    event,
                    |data| self.ai_diagnostic_base(data),
                    self.acp_file_review_bindings(cx),
                    cx,
                ) {
                    Ok(attention) => {
                        if attention {
                            self.notify_ai_agent_attention(&delivery.conversation_id, &delivery.assistant_id, "ai.agents.approval", cx);
                        }
                        return;
                    }
                    Err(event) => event,
                };
                self.ai.conversations.handle_acp_terminal_event(
                    agent_id, event, Self::acp_terminal_bridge(cx), window, cx,
                );
            }
            AiStreamDeliveryEvent::AcpSessionStarted { .. }
            | AiStreamDeliveryEvent::UserQuestionRequested { .. }
            | AiStreamDeliveryEvent::ToolCandidateSelectionRequested { .. } => {
                unreachable!("chat batch consumes interactive view state");
            }
            AiStreamDeliveryEvent::Guardrail { .. }
            | AiStreamDeliveryEvent::AssistantRound { .. }
            | AiStreamDeliveryEvent::RoundSummary { .. }
            | AiStreamDeliveryEvent::RoundStatefulMarker { .. }
            | AiStreamDeliveryEvent::Diagnostic { .. } => {
                unreachable!("chat batch consumes local records");
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
                self.ai.state.update(cx, |ai, _cx| {
                    ai.register_tool_approval(delivery.generation, tool_call_id, sender);
                });
            }
            AiStreamDeliveryEvent::ToolPreflightRequested {
                tool_session_id,
                tool_call_id,
                name,
                args,
                sender,
            } => {
                let session_is_active = self.ai.authority.read(cx).is_active_tool_session(
                    delivery.generation,
                    &tool_session_id,
                );
                let rejection = if !session_is_active {
                    Some(rejected_ai_tool_result(
                        tool_call_id,
                        name,
                        "operation_cancelled",
                        "The AI tool session is no longer active.",
                    ))
                } else {
                    self.preflight_ai_ui_orchestrator_tool(
                        &tool_session_id,
                        &name,
                        &args,
                        cx,
                    )
                    .err()
                    .map(|error| {
                        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(
                            Some(&tool_session_id),
                            cx,
                        );
                        snapshot.to_executed_tool_result(
                            tool_call_id,
                            name,
                            snapshot.fail(
                                "Runtime target is unavailable.",
                                error.public_code(),
                                "Rediscover the current terminal target before retrying.",
                                "interactive",
                            ),
                            0,
                        )
                    })
                };
                let _ = sender.send(rejection);
            }
            AiStreamDeliveryEvent::RuntimeContextRequested {
                tool_session_id,
                sender,
            } => {
                let session_is_active = self.ai.authority.read(cx).is_active_tool_session(
                    delivery.generation,
                    &tool_session_id,
                );
                let context = session_is_active.then(|| {
                    self.ai_runtime_context_prompt(&tool_session_id, cx)
                });
                let _ = sender.send(context);
            }
            AiStreamDeliveryEvent::ToolExecutionRequested {
                audit_context,
                dispatch,
                leases,
                tool_session_id,
                tool_call_id,
                name,
                args,
                post_user_approval,
                dangerous_command_approved,
                sender,
            } => {
                if dispatch.as_ref().is_some_and(|guard| guard.check().is_err()) {
                    let _ = sender.send(rejected_ai_tool_result(tool_call_id, name, "agent_direction_changed", "Task direction changed before dispatch."));
                    return;
                }
                let session_is_active = self.ai.authority.read(cx).is_active_tool_session(
                    delivery.generation,
                    &tool_session_id,
                );
                if !session_is_active || !self.ai.state.read(cx).run_accepts_tools(delivery.generation) || leases.iter().any(|lease| !lease.is_current()) || sender.is_closed() {
                    let _ = sender.send(rejected_ai_tool_result(
                        tool_call_id,
                        name,
                        "operation_cancelled",
                        "The AI tool session is no longer active.",
                    ));
                    return;
                }
                if name != "run_command" { for lease in &leases { lease.dispatched(); } }
                if !leases.is_empty() {
                    self.ai.state.update(cx, |ai, _cx| { ai.agents.tool_leases.insert((tool_session_id.clone(), tool_call_id.clone()), leases); });
                }
                let raw_arguments = zeroize::Zeroizing::new(args.to_string());
                oxideterm_audit::AuditContext::with_sync_request(audit_context.as_ref(), || {
                    self.start_ai_ui_orchestrator_tool_execution(
                        AiToolRunContext {
                            arguments: zeroize::Zeroizing::new(
                                sanitize_ai_tool_arguments_for_approval(&raw_arguments),
                            ),
                            generation: delivery.generation,
                            conversation_id: delivery.conversation_id.clone(),
                            assistant_id: delivery.assistant_id.clone(),
                            dispatch,
                        },
                        tool_session_id,
                        tool_call_id,
                        name,
                        args,
                        post_user_approval,
                        dangerous_command_approved,
                        sender,
                        window,
                        cx,
                    );
                });
            }
        }
    }
}
