#[allow(clippy::too_many_arguments)]
async fn execute_ai_round_call(
    call: AiToolCall,
    config: &AiChatStreamConfig,
    services: &AiModelBackendServices,
    available_tool_names: &std::collections::HashSet<String>,
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    tool_session_id: &ToolSessionId,
    conversation_id: &str,
    assistant_id: &str,
    execution: &mut Option<AgentExecution>,
    allow_selection: bool,
    dispatch: Option<&oxideterm_ai::agent::AgentDispatch>,
) -> Result<AiExecutedToolResult, ()> {
    let description = oxideterm_ai::agent::ToolExecutionDescription::for_call(&call);
    for attempt in 0.. {
        let mut result = execute_ai_round_call_once(
            call.clone(),
            config,
            services,
            available_tool_names,
            ui_tx,
            generation,
            tool_session_id,
            conversation_id,
            assistant_id,
            execution,
            allow_selection,
            dispatch,
        )
        .await?;
        oxideterm_ai::agent::annotate_recovery(&mut result);
        if result
            .envelope
            .pointer("/error/code")
            .and_then(serde_json::Value::as_str)
            == Some("agent_direction_changed")
        {
            let _ = send_ai_tool_status(
                ui_tx,
                generation,
                conversation_id,
                assistant_id,
                &call,
                "rejected",
                Some(result.envelope.clone()),
                None,
                None,
            );
        }
        let Some(delay) = description.retry_delay(&result, attempt) else {
            return Ok(result);
        };
        if ai_pending_dispatch(dispatch, tokio::time::sleep(delay))
            .await
            .is_err()
        {
            return Ok(ai_direction_changed_result(&call));
        }
    }
    unreachable!()
}

#[allow(clippy::too_many_arguments)]
async fn execute_ai_round_call_once(
    call: AiToolCall,
    config: &AiChatStreamConfig,
    services: &AiModelBackendServices,
    available_tool_names: &std::collections::HashSet<String>,
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    tool_session_id: &ToolSessionId,
    conversation_id: &str,
    assistant_id: &str,
    execution: &mut Option<AgentExecution>,
    allow_selection: bool,
    dispatch: Option<&oxideterm_ai::agent::AgentDispatch>,
) -> Result<AiExecutedToolResult, ()> {
    if dispatch.is_some_and(|guard| guard.check().is_err()) {
        return Ok(ai_direction_changed_result(&call));
    }
    if !available_tool_names.contains(&call.name) {
        // Tauri rejects unavailable tool names before argument parsing
        // or policy approval; keep stale/model-invented names out of
        // the executor path.
        let executed = unavailable_ai_tool_result(call.id.clone(), call.name.clone());
        send_ai_tool_status(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            &call,
            "rejected",
            Some(executed.envelope.clone()),
            None,
            Some(executed_summary(&executed)),
        )
        .ok();
        return Ok(executed);
    }
    if oxideterm_ai::agent::is_agent_tool(&call.name) {
        let executed = execute_ai_agent_coordination(
            execution,
            &ui_tx,
            generation,
            &tool_session_id,
            &conversation_id,
            &assistant_id,
            &call,
            dispatch,
        )
        .await;
        let _ = send_ai_tool_status(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            &call,
            if executed.success {
                "completed"
            } else {
                "error"
            },
            Some(executed.envelope.clone()),
            Some("read".into()),
            Some(executed_summary(&executed)),
        );
        return Ok(executed);
    }
    let Some(parsed_args) = parse_ai_tool_args(&call.name, &call.arguments) else {
        let executed = pre_execution_rejected_ai_tool_result(
            call.id.clone(),
            call.name.clone(),
            "invalid_tool_arguments",
            "The application tool arguments do not match the v2 contract.",
        );
        send_ai_tool_status(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            &call,
            "rejected",
            Some(executed.envelope.clone()),
            None,
            Some(executed_summary(&executed)),
        )
        .ok();
        return Ok(executed);
    };
    if let Some(executed) = ai_pending_dispatch(
        dispatch,
        preflight_ai_tool(
            &ui_tx,
            generation,
            &tool_session_id,
            &conversation_id,
            &assistant_id,
            call.id.clone(),
            call.name.clone(),
            parsed_args.clone(),
        ),
    )
    .await
    .unwrap_or_else(|_| Some(ai_direction_changed_result(&call)))
    {
        send_ai_tool_status(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            &call,
            "rejected",
            Some(executed.envelope.clone()),
            None,
            Some(executed_summary(&executed)),
        )
        .ok();
        return Ok(executed);
    }
    let approval_args = parsed_args.clone();
    let decision = resolve_ai_policy_decision(
        &call.name,
        Some(&approval_args),
        &config.tool_policy,
        config.safety_mode,
        config.profile_id.as_deref(),
    );
    let risk = ai_policy_risk_label(decision.risk).to_string();
    let summary = decision.reason_code.clone();
    let mut executed_after_policy = false;
    let mut execution_summary_args = serde_json::json!({});

    let mut executed = match decision.decision {
        oxideterm_ai::AiPolicyDecisionKind::Deny => {
            send_ai_tool_status(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                &call,
                "rejected",
                None,
                Some(risk.clone()),
                Some(summary.clone()),
            )
            .ok();
            pre_execution_rejected_ai_tool_result(
                call.id.clone(),
                call.name.clone(),
                decision.reason_code.clone(),
                decision.reason_code.clone(),
            )
        }
        oxideterm_ai::AiPolicyDecisionKind::RequireApproval => {
            let (approval_tx, approval_rx) = tokio::sync::oneshot::channel();
            if send_ai_loop_delivery(
                execution.is_some(),
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::ToolApprovalRequested {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: sanitize_ai_tool_arguments_for_approval(&call.arguments),
                    risk: risk.clone(),
                    summary: oxideterm_ai::sanitize_for_ai(&summary),
                    sender: approval_tx,
                },
            )
            .is_err()
            {
                return Err(());
            }
            let approved = ai_pending_dispatch(dispatch, async {
                if let Some(agent) = execution.as_mut() {
                    agent
                        .wait(AgentState::AwaitingApproval, approval_rx)
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .unwrap_or(false)
                } else {
                    approval_rx.await.unwrap_or(false)
                }
            })
            .await;
            let Ok(approved) = approved else {
                return Ok(ai_direction_changed_result(&call));
            };
            if !approved {
                send_ai_tool_status(
                    &ui_tx,
                    generation,
                    &conversation_id,
                    &assistant_id,
                    &call,
                    "rejected",
                    None,
                    Some(risk.clone()),
                    Some("Rejected by user.".to_string()),
                )
                .ok();
                pre_execution_rejected_ai_tool_result(
                    call.id.clone(),
                    call.name.clone(),
                    "user_rejected",
                    "Tool call rejected by user.",
                )
            } else {
                send_ai_tool_status(
                    &ui_tx,
                    generation,
                    &conversation_id,
                    &assistant_id,
                    &call,
                    "approved",
                    None,
                    Some(risk.clone()),
                    Some("Approved by user.".to_string()),
                )
                .ok();
                send_ai_tool_status(
                    &ui_tx,
                    generation,
                    &conversation_id,
                    &assistant_id,
                    &call,
                    "running",
                    None,
                    Some(risk.clone()),
                    Some("Approved by user.".to_string()),
                )
                .ok();
                let execution_args = parsed_args.clone();
                // Keep the policy outcome out of the canonical model argument object.
                let dangerous_command_approved = call.name == "run_command"
                    && decision.risk == oxideterm_ai::AiActionRisk::Destructive;
                execution_summary_args = execution_args.clone();
                executed_after_policy = true;
                execute_ai_tool(
                    &services,
                    &ui_tx,
                    generation,
                    &tool_session_id,
                    &conversation_id,
                    &assistant_id,
                    call.id.clone(),
                    call.name.clone(),
                    execution_args,
                    true,
                    dangerous_command_approved,
                    execution.as_mut(),
                    dispatch.cloned(),
                )
                .await
            }
        }
        oxideterm_ai::AiPolicyDecisionKind::Allow => {
            send_ai_tool_status(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                &call,
                "approved",
                None,
                Some(risk.clone()),
                Some(summary.clone()),
            )
            .ok();
            send_ai_tool_status(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                &call,
                "running",
                None,
                Some(risk.clone()),
                Some(summary.clone()),
            )
            .ok();
            let execution_args = parsed_args.clone();
            // Bypass mode is prior user consent, represented as backend state only.
            let dangerous_command_approved = call.name == "run_command"
                && decision.risk == oxideterm_ai::AiActionRisk::Destructive;
            execution_summary_args = execution_args.clone();
            executed_after_policy = true;
            execute_ai_tool(
                &services,
                &ui_tx,
                generation,
                &tool_session_id,
                &conversation_id,
                &assistant_id,
                call.id.clone(),
                call.name.clone(),
                execution_args,
                false,
                dangerous_command_approved,
                execution.as_mut(),
                dispatch.cloned(),
            )
            .await
        }
    };
    if allow_selection && !execution.as_ref().is_some_and(AgentExecution::is_child) {
        executed = resolve_ai_candidate_selection_if_needed(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            &call,
            executed,
        )
        .await;
    }
    if executed_after_policy {
        if call.name == "run_command" {
            annotate_ai_run_command_execution_result(&mut executed, &execution_summary_args);
        }
        annotate_executed_ai_tool_result_policy(&mut executed, &decision);
    }

    let status = if executed.success {
        "completed"
    } else {
        "error"
    };
    send_ai_tool_status(
        &ui_tx,
        generation,
        &conversation_id,
        &assistant_id,
        &call,
        status,
        Some(executed.envelope.clone()),
        Some(risk),
        Some(executed_summary(&executed)),
    )
    .ok();
    Ok(executed)
}

fn ai_direction_changed_result(call: &AiToolCall) -> AiExecutedToolResult {
    pre_execution_rejected_ai_tool_result(
        call.id.clone(),
        call.name.clone(),
        "agent_direction_changed",
        "Task direction changed. This call was not dispatched; reconsider it using the latest user instructions.",
    )
}

async fn ai_pending_dispatch<F: std::future::Future>(
    dispatch: Option<&oxideterm_ai::agent::AgentDispatch>,
    future: F,
) -> Result<F::Output, ()> {
    if let Some(guard) = dispatch {
        tokio::select! {
            biased;
            _ = guard.invalidated() => Err(()),
            value = future => Ok(value),
        }
    } else {
        Ok(future.await)
    }
}
