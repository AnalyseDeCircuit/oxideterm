const AI_TOOL_CALLS_PER_ROUND_SAFETY_LIMIT: usize = 16;
const AI_RUNTIME_CONTEXT_MESSAGE_ID: &str = "runtime-context-v2";

pub(in crate::workspace) async fn run_ai_chat_tool_loop(
    config: AiChatStreamConfig,
    mut history: Vec<AiChatMessage>,
    model_runtime: AiModelRuntimeState,
    services: AiModelBackendServices,
    budget_level: u8,
    generation: u64,
    tool_session_id: ToolSessionId,
    conversation_id: String,
    assistant_id: String,
    ui_tx: AiStreamDeliverySender,
) {
    let max_rounds = config
        .tool_policy
        .max_rounds
        .unwrap_or(oxideterm_settings::DEFAULT_AI_TOOL_MAX_ROUNDS)
        .clamp(
            oxideterm_settings::MIN_AI_TOOL_MAX_ROUNDS,
            oxideterm_settings::MAX_AI_TOOL_MAX_ROUNDS,
        ) as usize;
    // Keep burst protection internal so users only configure the easier to
    // understand tool-round budget. Sixteen still permits substantial parallel
    // work without accepting an unbounded tool array from a model response.
    let max_calls_per_round = AI_TOOL_CALLS_PER_ROUND_SAFETY_LIMIT;
    let mut assistant_content = String::new();
    let mut assistant_thinking = String::new();
    let response_reserve = config
        .max_response_tokens
        .and_then(|tokens| usize::try_from(tokens).ok())
        .filter(|tokens| *tokens > 0)
        .unwrap_or_else(|| ai_response_reserve(model_runtime.context_window));
    let transcript_lookup_prompt = ai_find_prompt_transcript_lookup_reference(&history)
        .map(ai_build_transcript_lookup_prompt_reference);
    let available_tool_names = config
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect::<std::collections::HashSet<_>>();
    let mut transcript_lookup_prompt_injected = history
        .iter()
        .any(|message| message.id == "transcript-lookup-reference");
    let request_text = history
        .iter()
        .rev()
        .find(|message| message.role == AiChatRole::User)
        .map(|message| message.content.clone())
        .unwrap_or_default();
    let tool_obligation = if config.tool_policy.enabled {
        ai_classify_orchestrator_obligation(&request_text)
    } else {
        AiOrchestratorObligation::auto()
    };
    let user_requested_json = ai_user_explicitly_requested_json(&request_text);
    let mut required_tool_retry_count = 0usize;
    let mut hard_deny_retry_count = 0usize;
    let mut has_required_tool_result_this_turn = false;

    let mut awaiting_summary_round_id: Option<String> = None;

    for round_index in 0..=max_rounds {
        let Some(runtime_context) = request_ai_runtime_context(
            &ui_tx,
            generation,
            &tool_session_id,
            &conversation_id,
            &assistant_id,
        )
        .await
        else {
            return;
        };
        replace_ai_runtime_context_message(&mut history, runtime_context);
        let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut provider_config = config.clone();
        if tool_obligation.mode == AiOrchestratorObligationMode::Required
            && !provider_config.tools.is_empty()
        {
            provider_config.tool_choice = oxideterm_ai::AiToolChoice::Required;
        }
        let _ = send_ai_diagnostic(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            "llm_request",
            None,
            serde_json::json!({
                "requestKind": "chat",
                "budgetLevel": budget_level,
                "logicalRound": round_index.saturating_add(1),
                "messageCount": history.len(),
                "toolDefinitionCount": provider_config.tools.len(),
                "hardDenyRetryCount": hard_deny_retry_count,
                "requiredToolRetryCount": required_tool_retry_count,
                "toolObligationMode": ai_orchestrator_obligation_mode_label(tool_obligation.mode),
                "toolObligationReason": tool_obligation.reason.clone(),
                "candidateToolNames": tool_obligation.candidate_tools.clone(),
                "toolChoice": ai_tool_choice_label(&provider_config.tool_choice),
            }),
        );
        let provider_history = oxideterm_ai::sanitize_api_messages_for_provider(history.clone());
        let _ = send_ai_prompt_usage(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            provider_history
                .iter()
                .rev()
                .find(|message| message.role == AiChatRole::User)
                .map(|message| message.id.clone()),
            provider_config
                .provider_id
                .clone()
                .unwrap_or_else(|| provider_config.provider_type.clone()),
            provider_config.model.clone(),
            ai_prompt_token_breakdown(
                &provider_history,
                &provider_config.tools,
                &provider_config.provider_type,
                response_reserve,
            ),
            model_runtime.context_window,
        );
        tokio::spawn(stream_chat_completion(
            provider_config,
            provider_history,
            stream_tx,
        ));

        let mut stream_error = None;
        let mut round_content = String::new();
        let mut round_thinking = String::new();
        let mut buffered_required_content = String::new();
        let mut buffered_required_thinking = String::new();
        let mut buffering_required_tool =
            tool_obligation.mode == AiOrchestratorObligationMode::Required;
        let mut pending_calls = BTreeMap::<String, AiToolCall>::new();
        let mut completed_calls = Vec::<AiToolCall>::new();
        let mut round_provider_parts = Vec::<serde_json::Value>::new();

        while let Some(event) = stream_rx.recv().await {
            match event {
                AiStreamEvent::Content(chunk) => {
                    if let Some(round_id) = awaiting_summary_round_id.take() {
                        let _ = send_ai_round_stateful_marker(
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            round_id,
                            None,
                        );
                    }
                    round_content.push_str(&chunk);
                    if buffering_required_tool {
                        buffered_required_content.push_str(&chunk);
                    } else {
                        assistant_content.push_str(&chunk);
                        if send_ai_stream_delivery(
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            AiStreamDeliveryEvent::Stream(AiStreamEvent::Content(chunk)),
                        )
                        .is_err()
                        {
                            return;
                        }
                    }
                }
                AiStreamEvent::Thinking(chunk) => {
                    if let Some(round_id) = awaiting_summary_round_id.take() {
                        let _ = send_ai_round_stateful_marker(
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            round_id,
                            None,
                        );
                    }
                    round_thinking.push_str(&chunk);
                    if buffering_required_tool {
                        buffered_required_thinking.push_str(&chunk);
                    } else {
                        assistant_thinking.push_str(&chunk);
                        if send_ai_stream_delivery(
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            AiStreamDeliveryEvent::Stream(AiStreamEvent::Thinking(chunk)),
                        )
                        .is_err()
                        {
                            return;
                        }
                    }
                }
                AiStreamEvent::ProviderResponsePart {
                    provider_type,
                    part,
                } => {
                    if provider_type == config.provider_type {
                        // Provider-native response parts remain inside this
                        // live tool loop and are never written to diagnostics.
                        round_provider_parts.push(part);
                    }
                }
                AiStreamEvent::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    if let Some(round_id) = awaiting_summary_round_id.take() {
                        let _ = send_ai_round_stateful_marker(
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            round_id,
                            None,
                        );
                    }
                    pending_calls.insert(
                        id.clone(),
                        AiToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: arguments.clone(),
                        },
                    );
                    if buffering_required_tool {
                        buffering_required_tool = false;
                        if flush_ai_required_tool_buffer(
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            &mut assistant_content,
                            &mut assistant_thinking,
                            &mut buffered_required_content,
                            &mut buffered_required_thinking,
                        )
                        .is_err()
                        {
                            return;
                        }
                    }
                    if send_ai_stream_delivery(
                        &ui_tx,
                        generation,
                        &conversation_id,
                        &assistant_id,
                        AiStreamDeliveryEvent::Stream(AiStreamEvent::ToolCall {
                            id,
                            name,
                            arguments,
                        }),
                    )
                    .is_err()
                    {
                        return;
                    }
                }
                AiStreamEvent::ToolCallComplete {
                    id,
                    name,
                    arguments,
                } => {
                    if let Some(round_id) = awaiting_summary_round_id.take() {
                        let _ = send_ai_round_stateful_marker(
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            round_id,
                            None,
                        );
                    }
                    let call = AiToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: arguments.clone(),
                    };
                    pending_calls.insert(id.clone(), call.clone());
                    record_completed_ai_tool_call(&mut completed_calls, call);
                    if buffering_required_tool {
                        buffering_required_tool = false;
                        if flush_ai_required_tool_buffer(
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            &mut assistant_content,
                            &mut assistant_thinking,
                            &mut buffered_required_content,
                            &mut buffered_required_thinking,
                        )
                        .is_err()
                        {
                            return;
                        }
                    }
                    if send_ai_stream_delivery(
                        &ui_tx,
                        generation,
                        &conversation_id,
                        &assistant_id,
                        AiStreamDeliveryEvent::Stream(AiStreamEvent::ToolCallComplete {
                            id,
                            name,
                            arguments,
                        }),
                    )
                    .is_err()
                    {
                        return;
                    }
                }
                AiStreamEvent::Done => {
                    if let Some(round_id) = awaiting_summary_round_id.take() {
                        let _ = send_ai_round_stateful_marker(
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            round_id,
                            None,
                        );
                    }
                    break;
                }
                AiStreamEvent::Error(error) => {
                    if let Some(round_id) = awaiting_summary_round_id.take() {
                        let _ = send_ai_round_stateful_marker(
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            round_id,
                            None,
                        );
                    }
                    stream_error = Some(error);
                    break;
                }
            }
        }

        if let Some(error) = stream_error {
            let _ = send_ai_stream_delivery(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(error)),
            );
            return;
        }

        let round_number = round_index.saturating_add(1) as i64;
        let round_id = format!("{assistant_id}-round-{round_number}");
        let _ = send_ai_assistant_round(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            round_id.clone(),
            round_number,
            round_content.len(),
            completed_calls
                .iter()
                .map(|call| call.id.clone())
                .collect::<Vec<_>>(),
            false,
            None,
            false,
        );

        if completed_calls.is_empty() {
            if !config.tool_policy.enabled
                && hard_deny_retry_count < AI_MAX_HARD_DENY_RETRIES
                && ai_should_trigger_hard_deny(&round_content, user_requested_json)
            {
                let retry_attempt = hard_deny_retry_count.saturating_add(1);
                let synthetic_round_id = format!("{assistant_id}-hard-deny-{retry_attempt}");
                let synthetic_tool_call_id = format!("{synthetic_round_id}-tool");
                let _ = send_ai_guardrail(
                    &ui_tx,
                    generation,
                    &conversation_id,
                    &assistant_id,
                    "tool-disabled-hard-deny",
                    "Tool calling is disabled, so the assistant response that looked like a tool transcript was rejected and retried.",
                    Some(round_content.clone()),
                );
                let _ = send_ai_assistant_round(
                    &ui_tx,
                    generation,
                    &conversation_id,
                    &assistant_id,
                    synthetic_round_id.clone(),
                    retry_attempt as i64,
                    round_content.len(),
                    vec![synthetic_tool_call_id.clone()],
                    true,
                    Some(retry_attempt),
                    true,
                );
                let synthetic_call = AiToolCall {
                    id: synthetic_tool_call_id.clone(),
                    name: AI_PSEUDO_TOOL_RETRY_TOOL_NAME.to_string(),
                    arguments: serde_json::json!({
                        "reason": "tool_use_disabled",
                        "retryAttempt": retry_attempt,
                    })
                    .to_string(),
                };
                let synthetic_result = rejected_ai_tool_result(
                    synthetic_tool_call_id.clone(),
                    AI_PSEUDO_TOOL_RETRY_TOOL_NAME.to_string(),
                    "tool_use_disabled",
                    "Tool use is disabled.",
                );
                send_ai_tool_status_with_payload(
                    &ui_tx,
                    generation,
                    &conversation_id,
                    &assistant_id,
                    &synthetic_call,
                    "rejected",
                    Some(synthetic_result.envelope.clone()),
                    Some("write".to_string()),
                    Some(executed_summary(&synthetic_result)),
                    true,
                    Some(round_content.clone()),
                    Some(synthetic_round_id.clone()),
                    Some(retry_attempt as i64),
                )
                .ok();
                history.push(AiChatMessage {
                    id: format!("{synthetic_round_id}-assistant"),
                    role: AiChatRole::Assistant,
                    content: String::new(),
                    timestamp_ms: ai_now_ms(),
                    model: Some(config.model.clone()),
                    context: None,
                    is_streaming: false,
                    thinking_content: None,
                    metadata: None,
                    tool_call_id: None,
                    tool_calls: vec![serde_json::json!({
                        "id": synthetic_tool_call_id,
                        "name": AI_PSEUDO_TOOL_RETRY_TOOL_NAME,
                        "arguments": serde_json::json!({
                            "reason": "tool_use_disabled",
                            "retryAttempt": retry_attempt,
                        }).to_string(),
                    })],
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    branches: None,
                    suggestions: Vec::new(),
                });
                history.push(AiChatMessage {
                    id: format!("{synthetic_round_id}-tool-result"),
                    role: AiChatRole::Tool,
                    content: serde_json::json!({
                        "kind": "tool_denied",
                        "reason": "Tool use is disabled.",
                        "detail": "Do not emit JSON that imitates a tool call or tool result. Answer conversationally without claiming app actions were performed.",
                    })
                    .to_string(),
                    timestamp_ms: ai_now_ms(),
                    model: None,
                    context: None,
                    is_streaming: false,
                    thinking_content: None,
                    metadata: None,
                    tool_call_id: Some(format!("{synthetic_round_id}-tool")),
                    tool_calls: Vec::new(),
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    branches: None,
                    suggestions: Vec::new(),
                });
                hard_deny_retry_count = retry_attempt;
                continue;
            }
            if required_tool_retry_count < AI_MAX_REQUIRED_TOOL_RETRIES
                && oxideterm_ai::ai_should_retry_required_tool_round_for_turn(
                    &tool_obligation,
                    &round_content,
                    has_required_tool_result_this_turn,
                )
            {
                let retry_attempt = required_tool_retry_count.saturating_add(1);
                let _ = send_ai_guardrail(
                    &ui_tx,
                    generation,
                    &conversation_id,
                    &assistant_id,
                    "tool-required-no-call",
                    "This request requires a real tool result before the assistant can answer. Retrying with a stricter tool-use instruction.",
                    (!round_content.trim().is_empty()).then_some(round_content.clone()),
                );
                let _ = send_ai_diagnostic(
                    &ui_tx,
                    generation,
                    &conversation_id,
                    &assistant_id,
                    "guardrail",
                    None,
                    serde_json::json!({
                        "code": "tool-required-no-call",
                        "retryAttempt": retry_attempt,
                        "candidateToolNames": tool_obligation.candidate_tools.clone(),
                    }),
                );
                let _ = send_ai_assistant_round(
                    &ui_tx,
                    generation,
                    &conversation_id,
                    &assistant_id,
                    format!("{assistant_id}-required-retry-{retry_attempt}"),
                    retry_attempt as i64,
                    round_content.len(),
                    Vec::new(),
                    true,
                    Some(retry_attempt),
                    false,
                );
                let mut retry_message = AiChatMessage {
                    id: format!("required-retry-assistant-{retry_attempt}"),
                    role: AiChatRole::Assistant,
                    content: if round_content.trim().is_empty() {
                        "(No tool call was made.)".to_string()
                    } else {
                        round_content
                    },
                    timestamp_ms: ai_now_ms(),
                    model: Some(config.model.clone()),
                    context: None,
                    is_streaming: false,
                    thinking_content: (!round_thinking.is_empty()).then_some(round_thinking),
                    metadata: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    branches: None,
                    suggestions: Vec::new(),
                };
                set_ai_provider_parts(
                    &mut retry_message,
                    &config.provider_type,
                    round_provider_parts,
                );
                history.push(retry_message);
                history.push(AiChatMessage {
                    id: format!("required-retry-user-{retry_attempt}"),
                    role: AiChatRole::User,
                    content: ai_required_tool_retry_prompt(&tool_obligation),
                    timestamp_ms: ai_now_ms(),
                    model: None,
                    context: None,
                    is_streaming: false,
                    thinking_content: None,
                    metadata: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    branches: None,
                    suggestions: Vec::new(),
                });
                required_tool_retry_count = retry_attempt;
                continue;
            }
            if flush_ai_required_tool_buffer(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                &mut assistant_content,
                &mut assistant_thinking,
                &mut buffered_required_content,
                &mut buffered_required_thinking,
            )
            .is_err()
            {
                return;
            }
            let _ = send_ai_stream_delivery(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Done),
            );
            return;
        }

        if completed_calls.len() > max_calls_per_round {
            reject_ai_tool_calls_for_protocol_guard(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                &completed_calls,
                "too_many_tool_calls",
                format!(
                    "Too many tool calls in one round (max {}).",
                    max_calls_per_round
                ),
            );
            let _ = send_ai_stream_delivery(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(format!(
                    "Too many tool calls in one round (max {}).",
                    max_calls_per_round
                ))),
            );
            return;
        }

        if round_index >= max_rounds {
            let _ = send_ai_guardrail(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                "tool-budget-limit",
                "Tool use stopped because the conversation reached the configured tool-round limit.",
                None,
            );
            reject_ai_tool_calls_for_protocol_guard(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                &completed_calls,
                "tool_budget_limit",
                "Tool use stopped because the conversation reached the configured tool-round limit.",
            );
            let _ = send_ai_stream_delivery(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(
                    "Tool execution stopped after reaching the maximum tool rounds.".to_string(),
                )),
            );
            return;
        }

        let assistant_round_id = format!("assistant-tool-round-{round_index}");
        let mut assistant_round = AiChatMessage {
            id: assistant_round_id,
            role: AiChatRole::Assistant,
            content: round_content,
            timestamp_ms: ai_now_ms(),
            model: Some(config.model.clone()),
            context: None,
            is_streaming: false,
            thinking_content: (!round_thinking.is_empty()).then_some(round_thinking),
            metadata: None,
            tool_call_id: None,
            tool_calls: completed_calls
                .iter()
                .map(ai_tool_call_message_value)
                .collect::<Vec<_>>(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };
        set_ai_provider_parts(
            &mut assistant_round,
            &config.provider_type,
            round_provider_parts,
        );
        history.push(assistant_round);

        let mut round_results = Vec::new();
        for call in completed_calls {
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
                round_results.push(AiRoundToolResultSummary {
                    tool_name: call.name.clone(),
                    success: false,
                    summary: executed_summary(&executed),
                });
                history.push(ai_tool_result_message(executed));
                has_required_tool_result_this_turn = true;
                continue;
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
                round_results.push(AiRoundToolResultSummary {
                    tool_name: call.name.clone(),
                    success: false,
                    summary: executed_summary(&executed),
                });
                history.push(ai_tool_result_message(executed));
                has_required_tool_result_this_turn = true;
                continue;
            };
            if let Some(executed) = preflight_ai_tool(
                &ui_tx,
                generation,
                &tool_session_id,
                &conversation_id,
                &assistant_id,
                call.id.clone(),
                call.name.clone(),
                parsed_args.clone(),
            )
            .await
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
                round_results.push(AiRoundToolResultSummary {
                    tool_name: call.name.clone(),
                    success: false,
                    summary: executed_summary(&executed),
                });
                history.push(ai_tool_result_message(executed));
                has_required_tool_result_this_turn = true;
                continue;
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
                    if send_ai_stream_delivery(
                        &ui_tx,
                        generation,
                        &conversation_id,
                        &assistant_id,
                        AiStreamDeliveryEvent::ToolApprovalRequested {
                            tool_call_id: call.id.clone(),
                            name: call.name.clone(),
                            arguments: sanitize_ai_tool_arguments_for_approval(
                                &call.arguments,
                            ),
                            risk: risk.clone(),
                            summary: oxideterm_ai::sanitize_for_ai(&summary),
                            sender: approval_tx,
                        },
                    )
                    .is_err()
                    {
                        return;
                    }
                    let approved = approval_rx.await.unwrap_or(false);
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
                    )
                    .await
                }
            };
            executed = resolve_ai_candidate_selection_if_needed(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                &call,
                executed,
            )
            .await;
            if executed_after_policy {
                if call.name == "run_command" {
                    annotate_ai_run_command_execution_result(
                        &mut executed,
                        &execution_summary_args,
                    );
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
            round_results.push(AiRoundToolResultSummary {
                tool_name: call.name.clone(),
                success: executed.success,
                summary: executed_summary(&executed),
            });
            history.push(ai_tool_result_message(executed));
            has_required_tool_result_this_turn = true;
        }

        if round_index >= 1 {
            condense_ai_tool_messages(&mut history);
        }
        let budget_history = oxideterm_ai::sanitize_api_messages_for_provider(history.clone());
        let prompt_breakdown = ai_prompt_token_breakdown(
            &budget_history,
            &config.tools,
            &config.provider_type,
            response_reserve,
        );
        let system_budget = prompt_breakdown
            .system_instructions
            .saturating_add(prompt_breakdown.tool_definitions);
        let regular_messages = history
            .iter()
            .filter(|message| message.role != AiChatRole::System)
            .collect::<Vec<_>>();
        let summary_eligible_tokens = ai_summary_eligible_tokens(&regular_messages);
        let tool_loop_budget = determine_ai_compression_level(AiPromptBudgetInput {
            context_window: model_runtime.context_window,
            response_reserve,
            system_budget,
            history_tokens: prompt_breakdown.history_tokens(),
            trimmable_history_tokens: None,
            summary_eligible_tokens: Some(summary_eligible_tokens),
            can_summarize: summary_eligible_tokens > 0,
            can_lookup_transcript: transcript_lookup_prompt.is_some(),
            in_tool_loop: true,
            auto_compact_threshold: None,
            transcript_lookup_threshold: None,
            tool_loop_stop_threshold: Some(ai_to_usable_budget_threshold(
                0.9,
                model_runtime.context_window,
                system_budget,
                response_reserve,
            )),
            safety_margin: None,
        });
        if !round_results.is_empty() {
            let _ = send_ai_round_summary(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                round_id.clone(),
                ai_round_summary_text(&round_results),
                serde_json::json!({
                    "source": "background",
                    "model": config.model.clone(),
                    "summarizationMode": "background",
                    "contextLengthBefore": prompt_breakdown.prompt_tokens(),
                    "numRounds": round_number,
                    "numRoundsSinceLastSummarization": 1,
                }),
            );
        }
        if tool_loop_budget.level >= 3 && !transcript_lookup_prompt_injected {
            if let Some(prompt) = transcript_lookup_prompt.clone() {
                history.push(AiChatMessage {
                    id: "transcript-lookup-reference".to_string(),
                    role: AiChatRole::System,
                    content: prompt,
                    timestamp_ms: 0,
                    model: None,
                    context: None,
                    is_streaming: false,
                    thinking_content: None,
                    metadata: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    branches: None,
                    suggestions: Vec::new(),
                });
                transcript_lookup_prompt_injected = true;
            }
        }
        if tool_loop_budget.level == 4 {
            let _ = send_ai_guardrail(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                "tool-budget-limit",
                "Tool use stopped because the conversation is approaching the current context window limit.",
                Some("Tool use stopped: approaching context window limit".to_string()),
            );
            let _ = send_ai_stream_delivery(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Done),
            );
            return;
        }
        let _ = send_ai_round_stateful_marker(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            round_id.clone(),
            Some("awaiting-summary".to_string()),
        );
        awaiting_summary_round_id = Some(round_id);
    }

    let _ = send_ai_stream_delivery(
        &ui_tx,
        generation,
        &conversation_id,
        &assistant_id,
        AiStreamDeliveryEvent::Stream(AiStreamEvent::Done),
    );
}

async fn request_ai_runtime_context(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    tool_session_id: &ToolSessionId,
    conversation_id: &str,
    assistant_id: &str,
) -> Option<String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::RuntimeContextRequested {
            tool_session_id: tool_session_id.clone(),
            sender,
        },
    )
    .ok()?;
    receiver.await.ok().flatten()
}

fn replace_ai_runtime_context_message(history: &mut Vec<AiChatMessage>, content: String) {
    let message = AiChatMessage {
        id: AI_RUNTIME_CONTEXT_MESSAGE_ID.to_string(),
        role: AiChatRole::System,
        content,
        timestamp_ms: ai_now_ms(),
        model: None,
        context: None,
        is_streaming: false,
        thinking_content: None,
        metadata: None,
        tool_call_id: None,
        tool_calls: Vec::new(),
        turn: None,
        transcript_ref: None,
        summary_ref: None,
        branches: None,
        suggestions: Vec::new(),
    };
    if let Some(existing) = history
        .iter_mut()
        .find(|entry| entry.id == AI_RUNTIME_CONTEXT_MESSAGE_ID)
    {
        *existing = message;
    } else {
        history.insert(0, message);
    }
}


pub(in crate::workspace) async fn resolve_ai_candidate_selection_if_needed(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    call: &AiToolCall,
    mut executed: AiExecutedToolResult,
) -> AiExecutedToolResult {
    let is_ambiguous = executed
        .envelope
        .pointer("/error/code")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|code| {
            matches!(
                code,
                "target_disambiguation_required" | "resource_ambiguous"
            )
        });
    let candidates = executed
        .envelope
        .get("targets")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !is_ambiguous || candidates.len() < 2 {
        return executed;
    }

    let display_candidates = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            serde_json::json!({
                "index": index,
                "label": candidate
                    .get("label")
                    .and_then(serde_json::Value::as_str)
                    .map(oxideterm_ai::sanitize_for_ai)
                    .unwrap_or_else(|| "Target".to_string()),
                "kind": candidate
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("target"),
            })
        })
        .collect::<Vec<_>>();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    if send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::ToolCandidateSelectionRequested {
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
            arguments: sanitize_ai_tool_arguments_for_persistence(&call.arguments),
            candidates: display_candidates,
            sender,
        },
    )
    .is_err()
    {
        return rejected_ai_tool_result(
            call.id.clone(),
            call.name.clone(),
            "ui_delivery_failed",
            "The target selector is no longer available.",
        );
    }

    let selected_index = receiver.await.unwrap_or(None);
    let Some(selected) = selected_index.and_then(|index| candidates.get(index).cloned()) else {
        executed.success = false;
        executed.error = Some("Target selection was cancelled.".to_string());
        executed.output = "Target selection was cancelled.".to_string();
        if let Some(envelope) = executed.envelope.as_object_mut() {
            envelope.insert("ok".to_string(), serde_json::json!(false));
            envelope.insert(
                "summary".to_string(),
                serde_json::json!("Target selection was cancelled."),
            );
            envelope.insert(
                "output".to_string(),
                serde_json::json!("Target selection was cancelled."),
            );
            envelope.insert(
                "error".to_string(),
                serde_json::json!({
                    "code": "operation_cancelled",
                    "message": "Target selection was cancelled.",
                    "recoverable": true,
                }),
            );
            envelope.insert("recoverable".to_string(), serde_json::json!(true));
        }
        return executed;
    };

    let selected_label = selected
        .get("label")
        .and_then(serde_json::Value::as_str)
        .map(oxideterm_ai::sanitize_for_ai)
        .unwrap_or_else(|| "Target".to_string());
    executed.success = true;
    executed.error = None;
    executed.output = format!("Selected {selected_label}.");
    if let Some(envelope) = executed.envelope.as_object_mut() {
        envelope.insert("ok".to_string(), serde_json::json!(true));
        envelope.insert(
            "summary".to_string(),
            serde_json::json!(format!("Selected {selected_label}.")),
        );
        envelope.insert(
            "output".to_string(),
            serde_json::json!(format!("Selected {selected_label}.")),
        );
        envelope.remove("error");
        envelope.insert("recoverable".to_string(), serde_json::json!(false));
        envelope.insert("targets".to_string(), serde_json::json!([selected.clone()]));
        envelope.insert("selectedTarget".to_string(), selected);
        envelope.remove("nextActions");
    }
    executed
}


/// Resolves the same local ACP session directory for prompts and pre-prompt discovery.
pub(in crate::workspace) fn acp_session_cwd_from_agent(
    agent: &oxideterm_settings::AcpAgentConfig,
) -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| {
        agent
            .cwd
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    })
}

pub(in crate::workspace) fn flush_ai_required_tool_buffer(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    assistant_content: &mut String,
    assistant_thinking: &mut String,
    buffered_content: &mut String,
    buffered_thinking: &mut String,
) -> Result<(), ()> {
    if !buffered_thinking.is_empty() {
        let chunk = std::mem::take(buffered_thinking);
        assistant_thinking.push_str(&chunk);
        send_ai_stream_delivery(
            ui_tx,
            generation,
            conversation_id,
            assistant_id,
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Thinking(chunk)),
        )
        .map_err(|_| ())?;
    }
    if !buffered_content.is_empty() {
        let chunk = std::mem::take(buffered_content);
        assistant_content.push_str(&chunk);
        send_ai_stream_delivery(
            ui_tx,
            generation,
            conversation_id,
            assistant_id,
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Content(chunk)),
        )
        .map_err(|_| ())?;
    }
    Ok(())
}
