async fn run_ai_chat_tool_loop(
    config: AiChatStreamConfig,
    mut history: Vec<AiChatMessage>,
    snapshot: AiOrchestratorRuntimeSnapshot,
    rag_query: Option<String>,
    generation: u64,
    conversation_id: String,
    assistant_id: String,
    ui_tx: std::sync::mpsc::Sender<AiStreamDelivery>,
) {
    let max_rounds = config
        .tool_policy
        .max_rounds
        .unwrap_or(8)
        .clamp(1, AI_MAX_TOOL_ROUNDS_PER_REPLY as i64) as usize;
    let max_calls_per_round = config
        .tool_policy
        .max_calls_per_round
        .unwrap_or(oxideterm_settings::DEFAULT_AI_TOOL_MAX_CALLS_PER_ROUND)
        .clamp(
            oxideterm_settings::MIN_AI_TOOL_MAX_CALLS_PER_ROUND,
            oxideterm_settings::MAX_AI_TOOL_MAX_CALLS_PER_ROUND,
        ) as usize;
    let mut assistant_content = String::new();
    let mut assistant_thinking = String::new();
    let response_reserve = config
        .max_response_tokens
        .and_then(|tokens| usize::try_from(tokens).ok())
        .filter(|tokens| *tokens > 0)
        .unwrap_or_else(|| ai_response_reserve(snapshot.ai_context_window));
    let transcript_lookup_prompt =
        ai_find_prompt_transcript_lookup_reference(&history).map(ai_build_transcript_lookup_prompt_reference);
    let mut transcript_lookup_prompt_injected = history
        .iter()
        .any(|message| message.id == "transcript-lookup-reference");
    let request_text = rag_query
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            history
                .iter()
                .rev()
                .find(|message| message.role == AiChatRole::User)
                .map(|message| message.content.clone())
        })
        .unwrap_or_default();
    let tool_obligation = if config.tool_policy.enabled {
        ai_classify_orchestrator_obligation(&request_text)
    } else {
        AiOrchestratorObligation::auto()
    };
    if let Some(prompt) = ai_orchestrator_obligation_prompt(&tool_obligation)
        && let Some(system_message) = history
            .iter_mut()
            .find(|message| message.role == AiChatRole::System)
    {
        system_message.content.push_str("\n\n");
        system_message.content.push_str(&prompt);
    }
    let user_requested_json = ai_user_explicitly_requested_json(&request_text);
    let mut required_tool_retry_count = 0usize;
    let mut hard_deny_retry_count = 0usize;
    if let Some(rag_prompt) = snapshot
        .build_rag_system_prompt(rag_query.as_deref(), &config)
        .await
    {
        if let Some(system_message) = history
            .iter_mut()
            .find(|message| message.role == AiChatRole::System)
        {
            system_message.content.push_str("\n\n");
            system_message.content.push_str(&rag_prompt);
        }
        let trimmed_count = trim_ai_stream_history_to_budget(
            &mut history,
            snapshot.ai_context_window,
            config
                .max_response_tokens
                .and_then(|tokens| usize::try_from(tokens).ok())
                .filter(|tokens| *tokens > 0)
                .unwrap_or_else(|| ai_response_reserve(snapshot.ai_context_window)),
        );
        if trimmed_count > 0 {
            let _ = send_ai_stream_delivery(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::TrimNotice(trimmed_count),
            );
        }
    }

    let mut awaiting_summary_round_id: Option<String> = None;

    for round_index in 0..=max_rounds {
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
        tokio::spawn(stream_chat_completion(provider_config, history.clone(), stream_tx));

        let mut stream_error = None;
        let mut round_content = String::new();
        let mut round_thinking = String::new();
        let mut pending_calls = BTreeMap::<String, AiToolCall>::new();
        let mut completed_calls = Vec::<AiToolCall>::new();

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
                let synthetic_round_id =
                    format!("{assistant_id}-hard-deny-{retry_attempt}");
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
                });
                hard_deny_retry_count = retry_attempt;
                continue;
            }
            if required_tool_retry_count < AI_MAX_REQUIRED_TOOL_RETRIES
                && ai_should_retry_required_tool_round(&tool_obligation, &round_content)
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
                history.push(AiChatMessage {
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
                });
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
                });
                required_tool_retry_count = retry_attempt;
                continue;
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
        history.push(AiChatMessage {
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
        });

        let mut round_results = Vec::new();
        for call in completed_calls {
            let args = parse_ai_tool_args(&call.arguments);
            let decision = resolve_ai_policy_decision(
                &call.name,
                Some(&args),
                &config.tool_policy,
                config.safety_mode,
                config.profile_id.clone(),
            );
            let risk = ai_policy_risk_label(decision.risk).to_string();
            let summary = decision.reason_code.clone();

            let executed = match decision.decision {
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
                    rejected_ai_tool_result(
                        call.id.clone(),
                        call.name.clone(),
                        "tool_disabled",
                        decision.reason_code,
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
                            arguments: call.arguments.clone(),
                            risk: risk.clone(),
                            summary: summary.clone(),
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
                        rejected_ai_tool_result(
                            call.id.clone(),
                            call.name.clone(),
                            "user_rejected",
                            "The user rejected this tool call.",
                        )
                    } else {
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
                        execute_ai_tool(
                            &snapshot,
                            &ui_tx,
                            generation,
                            &conversation_id,
                            &assistant_id,
                            call.id.clone(),
                            call.name.clone(),
                            args,
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
                        "running",
                        None,
                        Some(risk.clone()),
                        Some(summary.clone()),
                    )
                    .ok();
                    execute_ai_tool(
                        &snapshot,
                        &ui_tx,
                        generation,
                        &conversation_id,
                        &assistant_id,
                        call.id.clone(),
                        call.name.clone(),
                        args,
                    )
                    .await
                }
            };

            let status = if executed.success { "completed" } else { "error" };
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
        }

        if round_index >= 1 {
            condense_ai_tool_messages(&mut history);
        }
        let system_message_tokens = history
            .iter()
            .filter(|message| message.role == AiChatRole::System)
            .map(ai_message_estimated_tokens)
            .sum::<usize>();
        let total_message_tokens = history.iter().map(ai_message_estimated_tokens).sum::<usize>();
        let system_budget = system_message_tokens
            .saturating_add(ai_tool_definitions_estimated_tokens(&config.tools));
        let regular_messages = history
            .iter()
            .filter(|message| message.role != AiChatRole::System)
            .collect::<Vec<_>>();
        let summary_eligible_tokens = ai_summary_eligible_tokens(&regular_messages);
        let tool_loop_budget = determine_ai_compression_level(AiPromptBudgetInput {
            context_window: snapshot.ai_context_window,
            response_reserve,
            system_budget,
            history_tokens: total_message_tokens.saturating_sub(system_message_tokens),
            trimmable_history_tokens: None,
            summary_eligible_tokens: Some(summary_eligible_tokens),
            can_summarize: summary_eligible_tokens > 0,
            can_lookup_transcript: transcript_lookup_prompt.is_some(),
            in_tool_loop: true,
            auto_compact_threshold: None,
            transcript_lookup_threshold: None,
            tool_loop_stop_threshold: Some(ai_to_usable_budget_threshold(
                0.9,
                snapshot.ai_context_window,
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
                    "contextLengthBefore": total_message_tokens,
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
