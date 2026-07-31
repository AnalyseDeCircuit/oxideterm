#[derive(Clone)]
pub(in crate::workspace) struct AcpApplicationToolTurn {
    pub(in crate::workspace) services: AiModelBackendServices,
    pub(in crate::workspace) tool_policy: oxideterm_ai::AiToolUsePolicy,
    pub(in crate::workspace) safety_mode: oxideterm_ai::AiPolicySafetyMode,
    pub(in crate::workspace) profile_id: Option<String>,
    pub(in crate::workspace) ui_tx: AiStreamDeliverySender,
    pub(in crate::workspace) generation: u64,
    pub(in crate::workspace) tool_session_id: ToolSessionId,
    pub(in crate::workspace) conversation_id: String,
    pub(in crate::workspace) assistant_id: String,
    pub(in crate::workspace) cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AcpApplicationToolTurn {
    pub(in crate::workspace) fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Executes one ACP-originated MCP call through the same application policy and
/// runtime-capability boundaries used by provider-originated tool calls.
pub(in crate::workspace) async fn handle_acp_application_tool_call(
    call: oxideterm_acp_host_tools::AcpHostToolCall,
    turn: Option<AcpApplicationToolTurn>,
) {
    let Some(turn) = turn else {
        let _ = call.respond(oxideterm_acp_host_tools::AcpHostToolResponse::error(
            "The OxideTerm ACP tool turn is no longer active.",
        ));
        return;
    };
    if turn.is_cancelled() {
        respond_to_cancelled_acp_tool(call);
        return;
    }
    let canonical_arguments =
        match oxideterm_ai::canonicalize_orchestrator_tool_arguments(&call.name, call.arguments.clone()) {
            Ok(arguments) => arguments,
            Err(_) => {
                let rejected = pre_execution_rejected_ai_tool_result(
                    call.id.clone(),
                    call.name.clone(),
                    "invalid_tool_arguments",
                    "The application tool arguments do not match the v2 contract.",
                );
                respond_to_acp_tool(
                    call,
                    rejected,
                );
                return;
            }
        };
    let arguments = serde_json::to_string(&canonical_arguments)
        .unwrap_or_else(|_| "{}".to_string());
    let status_call = AiToolCall {
        id: call.id.clone(),
        name: call.name.clone(),
        arguments,
    };

    if let Some(executed) = preflight_ai_tool(
        &turn.ui_tx,
        turn.generation,
        &turn.tool_session_id,
        &turn.conversation_id,
        &turn.assistant_id,
        call.id.clone(),
        call.name.clone(),
        canonical_arguments.clone(),
    )
    .await
    {
        let _ = send_ai_tool_status(
            &turn.ui_tx,
            turn.generation,
            &turn.conversation_id,
            &turn.assistant_id,
            &status_call,
            "rejected",
            Some(executed.envelope.clone()),
            None,
            Some(executed_summary(&executed)),
        );
        respond_to_acp_tool(call, executed);
        return;
    }

    let decision = resolve_ai_policy_decision(
        &call.name,
        Some(&canonical_arguments),
        &turn.tool_policy,
        turn.safety_mode,
        turn.profile_id.as_deref(),
    );
    let risk = ai_policy_risk_label(decision.risk).to_string();
    let mut executed_after_policy = false;
    let mut executed = match decision.decision {
        oxideterm_ai::AiPolicyDecisionKind::Deny => {
            let _ = send_ai_tool_status(
                &turn.ui_tx,
                turn.generation,
                &turn.conversation_id,
                &turn.assistant_id,
                &status_call,
                "rejected",
                None,
                Some(risk.clone()),
                Some(decision.reason_code.clone()),
            );
            pre_execution_rejected_ai_tool_result(
                call.id.clone(),
                call.name.clone(),
                decision.reason_code.clone(),
                decision.reason_code.clone(),
            )
        }
        oxideterm_ai::AiPolicyDecisionKind::RequireApproval => {
            let (approval_tx, approval_rx) = tokio::sync::oneshot::channel();
            if send_ai_stream_delivery(
                &turn.ui_tx,
                turn.generation,
                &turn.conversation_id,
                &turn.assistant_id,
                AiStreamDeliveryEvent::ToolApprovalRequested {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: sanitize_ai_tool_arguments_for_approval(&status_call.arguments),
                    risk: risk.clone(),
                    summary: oxideterm_ai::sanitize_for_ai(&decision.reason_code),
                    sender: approval_tx,
                },
            )
            .is_err()
                || !approval_rx.await.unwrap_or(false)
            {
                let _ = send_ai_tool_status(
                    &turn.ui_tx,
                    turn.generation,
                    &turn.conversation_id,
                    &turn.assistant_id,
                    &status_call,
                    "rejected",
                    None,
                    Some(risk.clone()),
                    Some("Rejected by user.".to_string()),
                );
                pre_execution_rejected_ai_tool_result(
                    call.id.clone(),
                    call.name.clone(),
                    "user_rejected",
                    "Tool call rejected by user.",
                )
            } else {
                if turn.is_cancelled() {
                    respond_to_cancelled_acp_tool(call);
                    return;
                }
                let _ = send_ai_tool_status(
                    &turn.ui_tx,
                    turn.generation,
                    &turn.conversation_id,
                    &turn.assistant_id,
                    &status_call,
                    "approved",
                    None,
                    Some(risk.clone()),
                    Some("Approved by user.".to_string()),
                );
                let _ = send_ai_tool_status(
                    &turn.ui_tx,
                    turn.generation,
                    &turn.conversation_id,
                    &turn.assistant_id,
                    &status_call,
                    "running",
                    None,
                    Some(risk.clone()),
                    Some("Approved by user.".to_string()),
                );
                executed_after_policy = true;
                execute_ai_tool(
                    &turn.services,
                    &turn.ui_tx,
                    turn.generation,
                    &turn.tool_session_id,
                    &turn.conversation_id,
                    &turn.assistant_id,
                    call.id.clone(),
                    call.name.clone(),
                    canonical_arguments.clone(),
                    true,
                    call.name == "run_command"
                        && decision.risk == oxideterm_ai::AiActionRisk::Destructive,
                )
                .await
            }
        }
        oxideterm_ai::AiPolicyDecisionKind::Allow => {
            if turn.is_cancelled() {
                respond_to_cancelled_acp_tool(call);
                return;
            }
            let _ = send_ai_tool_status(
                &turn.ui_tx,
                turn.generation,
                &turn.conversation_id,
                &turn.assistant_id,
                &status_call,
                "approved",
                None,
                Some(risk.clone()),
                Some(decision.reason_code.clone()),
            );
            let _ = send_ai_tool_status(
                &turn.ui_tx,
                turn.generation,
                &turn.conversation_id,
                &turn.assistant_id,
                &status_call,
                "running",
                None,
                Some(risk.clone()),
                Some(decision.reason_code.clone()),
            );
            executed_after_policy = true;
            execute_ai_tool(
                &turn.services,
                &turn.ui_tx,
                turn.generation,
                &turn.tool_session_id,
                &turn.conversation_id,
                &turn.assistant_id,
                call.id.clone(),
                call.name.clone(),
                canonical_arguments.clone(),
                false,
                call.name == "run_command"
                    && decision.risk == oxideterm_ai::AiActionRisk::Destructive,
            )
            .await
        }
    };
    executed = resolve_ai_candidate_selection_if_needed(
        &turn.ui_tx,
        turn.generation,
        &turn.conversation_id,
        &turn.assistant_id,
        &status_call,
        executed,
    )
    .await;
    if executed_after_policy {
        if call.name == "run_command" {
            annotate_ai_run_command_execution_result(&mut executed, &canonical_arguments);
        }
        annotate_executed_ai_tool_result_policy(&mut executed, &decision);
    }
    let status = if executed.success {
        "completed"
    } else {
        "error"
    };
    let _ = send_ai_tool_status(
        &turn.ui_tx,
        turn.generation,
        &turn.conversation_id,
        &turn.assistant_id,
        &status_call,
        status,
        Some(executed.envelope.clone()),
        Some(risk),
        Some(executed_summary(&executed)),
    );
    respond_to_acp_tool(call, executed);
}

fn respond_to_cancelled_acp_tool(call: oxideterm_acp_host_tools::AcpHostToolCall) {
    let _ = call.respond(oxideterm_acp_host_tools::AcpHostToolResponse::error(
        "The OxideTerm ACP tool turn was cancelled.",
    ));
}

fn respond_to_acp_tool(
    call: oxideterm_acp_host_tools::AcpHostToolCall,
    executed: AiExecutedToolResult,
) {
    // MCP tool results bypass provider-message sanitation, so redact here.
    let model_content =
        oxideterm_ai::sanitize_for_ai(&oxideterm_ai::ai_tool_result_model_content(&executed));
    let response = if executed.success {
        oxideterm_acp_host_tools::AcpHostToolResponse::success(model_content)
    } else {
        oxideterm_acp_host_tools::AcpHostToolResponse::error(model_content)
    };
    let _ = call.respond(response);
}
