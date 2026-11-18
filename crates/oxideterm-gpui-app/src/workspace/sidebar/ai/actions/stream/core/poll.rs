use oxideterm_gpui_ai::stream::AiChatWindowDelivery;
use oxideterm_gpui_ai::conversation::acp_file_review::AcpFileReviewBindings;

impl WorkspaceApp {
    fn acp_file_review_bindings(&self, cx: &Context<Self>) -> AcpFileReviewBindings {
        let owner = cx.weak_entity();
        let attention = owner.clone();
        AcpFileReviewBindings {
            preview: oxideterm_ai::tool_resources::acp_file_write_preview,
            frame: Rc::new(move |cx| {
                let app = owner.upgrade()?.read(cx);
                Some(oxideterm_gpui_ai::stream::AiChatStreamFrame::from_settings(
                    &app.settings_store.settings().ai, app.i18n.clone(),
                ))
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
            AiStreamDeliveryEvent::ToolPreflightRequested {
                tool_session_id,
                tool_call_id,
                name,
                args,
                sender,
            } => {
                let rejection = if !self.ai.authority.read(cx).is_active_tool_session(delivery.generation, &tool_session_id) {
                    Some(oxideterm_gpui_ai::conversation::agents::inactive_tool_result(tool_call_id, name))
                } else if oxideterm_ai::tool_resources::ai_tool_preflights_runtime_owner(&name) {
                    self.validate_ai_runtime_authority(&tool_session_id, &name, &args, cx)
                        .err().map(|error| {
                            self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx)
                                .tool_preflight_rejection(tool_call_id, name, &error)
                        })
                } else { None };
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
                let owner = match self.ai.conversations.admit_tool_execution(
                    delivery.generation, delivery.conversation_id, delivery.assistant_id,
                    &tool_session_id, &tool_call_id, &name, &args, dispatch, leases, || sender.is_closed(), cx,
                ) {
                    Ok(owner) => owner,
                    Err(result) => { let _ = sender.send(result); return; }
                };
                oxideterm_audit::AuditContext::with_sync_request(audit_context.as_ref(), || {
                    self.start_ai_ui_orchestrator_tool_execution(
                        owner,
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
            _ => unreachable!("conversation-local events are consumed before dispatch to the window"),
        }
    }
}
