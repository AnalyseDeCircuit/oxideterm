const AI_TOOL_CALLS_PER_ROUND_SAFETY_LIMIT: usize = 16;
const ACP_BASE_SYSTEM_MESSAGE_ID: &str = "base-system";
const ACP_CURRENT_CONTEXT_MESSAGE_ID: &str = "current-terminal-context";
const AI_RUNTIME_CONTEXT_MESSAGE_ID: &str = "runtime-context-v2";

pub(in crate::workspace) fn acp_current_turn_prompt(
    history: &[AiChatMessage],
) -> Option<String> {
    let request = history
        .iter()
        .rev()
        .find(|message| message.role == AiChatRole::User)
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty())?;
    let base_system = history
        .iter()
        .find(|message| {
            message.role == AiChatRole::System && message.id == ACP_BASE_SYSTEM_MESSAGE_ID
        })
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty());
    let current_context = history
        .iter()
        .find(|message| {
            message.role == AiChatRole::System && message.id == ACP_CURRENT_CONTEXT_MESSAGE_ID
        })
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty());

    // ACP owns its conversation history after a session starts. Send only the
    // freshly built runtime context for this turn so resumed sessions do not
    // receive duplicate copies of prior user and assistant messages.
    let mut sections = Vec::with_capacity(3);
    if let Some(base_system) = base_system {
        sections.push(format!("## OxideTerm Instructions\n{base_system}"));
    }
    if let Some(current_context) = current_context {
        sections.push(format!("## OxideTerm Current Context\n{current_context}"));
    }
    if sections.is_empty() {
        return Some(oxideterm_ai::sanitize_for_ai(request));
    }
    sections.push(format!("## User Request\n{request}"));
    Some(oxideterm_ai::sanitize_for_ai(&sections.join("\n\n")))
}

pub(in crate::workspace) async fn run_ai_chat_tool_loop(
    config: AiChatStreamConfig,
    mut history: Vec<AiChatMessage>,
    model_runtime: AiModelRuntimeState,
    services: AiModelBackendServices,
    acp_launch: Result<Option<AiAcpChatLaunch>, String>,
    budget_level: u8,
    generation: u64,
    tool_session_id: ToolSessionId,
    conversation_id: String,
    assistant_id: String,
    ui_tx: AiStreamDeliverySender,
) {
    if config.execution_backend == AiExecutionBackend::Acp {
        run_acp_chat_loop(
            config,
            history,
            services,
            acp_launch,
            generation,
            tool_session_id,
            conversation_id,
            assistant_id,
            ui_tx,
        )
        .await;
        return;
    }

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
                        "tool_disabled",
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
        let system_message_tokens = history
            .iter()
            .filter(|message| message.role == AiChatRole::System)
            .map(ai_message_estimated_tokens)
            .sum::<usize>();
        let total_message_tokens = history
            .iter()
            .map(ai_message_estimated_tokens)
            .sum::<usize>();
        let system_budget = system_message_tokens
            .saturating_add(ai_tool_definitions_estimated_tokens(&config.tools));
        let regular_messages = history
            .iter()
            .filter(|message| message.role != AiChatRole::System)
            .collect::<Vec<_>>();
        let summary_eligible_tokens = ai_summary_eligible_tokens(&regular_messages);
        let tool_loop_budget = determine_ai_compression_level(AiPromptBudgetInput {
            context_window: model_runtime.context_window,
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

const ACP_VISIBLE_TERMINAL_TOOL_NAMES: &[&str] = &[
    "list_targets",
    "run_command",
    "observe_terminal",
    "send_terminal_input",
];

fn acp_visible_terminal_tool_definitions(
    tool_use_enabled: bool,
) -> Vec<oxideterm_acp_host_tools::AcpHostToolDefinition> {
    if !tool_use_enabled {
        return Vec::new();
    }
    oxideterm_ai::orchestrator_tool_definitions()
        .into_iter()
        .filter(|definition| {
            ACP_VISIBLE_TERMINAL_TOOL_NAMES.contains(&definition.name.as_str())
        })
        .map(|definition| {
            oxideterm_acp_host_tools::AcpHostToolDefinition::new(
                definition.name,
                definition.description,
                definition.parameters,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn execute_acp_visible_terminal_tool_on_ui(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    tool_session_id: &ToolSessionId,
    conversation_id: &str,
    assistant_id: &str,
    tool_call_id: String,
    tool_name: String,
    args: serde_json::Value,
    post_user_approval: bool,
    dangerous_command_approved: bool,
) -> AiExecutedToolResult {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    if send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::ToolExecutionRequested {
            tool_session_id: tool_session_id.clone(),
            tool_call_id: tool_call_id.clone(),
            name: tool_name.clone(),
            args,
            post_user_approval,
            dangerous_command_approved,
            sender,
        },
    )
    .is_err()
    {
        return rejected_ai_tool_result(
            tool_call_id,
            tool_name,
            "ui_delivery_failed",
            "The native UI executor is no longer available.",
        );
    }
    receiver.await.unwrap_or_else(|_| {
        rejected_ai_tool_result(
            tool_call_id,
            tool_name,
            "ui_executor_cancelled",
            "The native UI executor cancelled the tool call.",
        )
    })
}

#[allow(clippy::too_many_arguments)]
async fn handle_acp_visible_terminal_tool_call(
    call: oxideterm_acp_host_tools::AcpHostToolCall,
    tool_policy: &oxideterm_ai::AiToolUsePolicy,
    safety_mode: oxideterm_ai::AiPolicySafetyMode,
    profile_id: Option<&str>,
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    tool_session_id: &ToolSessionId,
    conversation_id: &str,
    assistant_id: &str,
) {
    let canonical_arguments = match oxideterm_ai::canonicalize_orchestrator_tool_arguments(
        &call.name,
        call.arguments.clone(),
    ) {
        Ok(arguments) => arguments,
        Err(_) => {
            let executed = pre_execution_rejected_ai_tool_result(
                call.id.clone(),
                call.name.clone(),
                "invalid_tool_arguments",
                "The application tool arguments do not match the v2 contract.",
            );
            let model_content = serde_json::to_string(&executed.envelope)
                .map(|content| oxideterm_ai::sanitize_for_ai(&content))
                .unwrap_or_else(|_| {
                    "OxideTerm could not serialize the tool result.".to_string()
                });
            let _ = call.respond(oxideterm_acp_host_tools::AcpHostToolResponse::error(
                model_content,
            ));
            return;
        }
    };
    // Persist and display only a redacted projection while retaining the one
    // canonical argument object for policy, approval, and execution.
    let arguments = serde_json::to_string(&canonical_arguments)
        .map(|arguments| oxideterm_ai::sanitize_for_ai(&arguments))
        .unwrap_or_else(|_| "{}".to_string());
    let status_call = AiToolCall {
        id: call.id.clone(),
        name: call.name.clone(),
        arguments,
    };
    if let Some(executed) = preflight_ai_tool(
        ui_tx,
        generation,
        tool_session_id,
        conversation_id,
        assistant_id,
        call.id.clone(),
        call.name.clone(),
        canonical_arguments.clone(),
    )
    .await
    {
        send_ai_tool_status(
            ui_tx,
            generation,
            conversation_id,
            assistant_id,
            &status_call,
            "rejected",
            Some(executed.envelope.clone()),
            None,
            Some(executed_summary(&executed)),
        )
        .ok();
        let model_content = serde_json::to_string(&executed.envelope)
            .map(|content| oxideterm_ai::sanitize_for_ai(&content))
            .unwrap_or_else(|_| "OxideTerm could not serialize the tool result.".to_string());
        let _ = call.respond(oxideterm_acp_host_tools::AcpHostToolResponse::error(
            model_content,
        ));
        return;
    }
    let decision = resolve_ai_policy_decision(
        &call.name,
        Some(&canonical_arguments),
        tool_policy,
        safety_mode,
        profile_id,
    );
    let risk = ai_policy_risk_label(decision.risk).to_string();
    let policy_summary = decision.reason_code.clone();
    let execution_args = canonical_arguments;
    let mut executed_after_policy = false;

    let mut executed = match decision.decision {
        oxideterm_ai::AiPolicyDecisionKind::Deny => {
            send_ai_tool_status(
                ui_tx,
                generation,
                conversation_id,
                assistant_id,
                &status_call,
                "rejected",
                None,
                Some(risk.clone()),
                Some(policy_summary.clone()),
            )
            .ok();
            pre_execution_rejected_ai_tool_result(
                call.id.clone(),
                call.name.clone(),
                "tool_disabled",
                decision.reason_code.clone(),
            )
        }
        oxideterm_ai::AiPolicyDecisionKind::RequireApproval => {
            let (approval_tx, approval_rx) = tokio::sync::oneshot::channel();
            let delivered = send_ai_stream_delivery(
                ui_tx,
                generation,
                conversation_id,
                assistant_id,
                AiStreamDeliveryEvent::ToolApprovalRequested {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: sanitize_ai_tool_arguments_for_approval(
                        &status_call.arguments,
                    ),
                    risk: risk.clone(),
                    summary: oxideterm_ai::sanitize_for_ai(&policy_summary),
                    sender: approval_tx,
                },
            )
            .is_ok();
            let approved = delivered && approval_rx.await.unwrap_or(false);
            if !approved {
                send_ai_tool_status(
                    ui_tx,
                    generation,
                    conversation_id,
                    assistant_id,
                    &status_call,
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
                    ui_tx,
                    generation,
                    conversation_id,
                    assistant_id,
                    &status_call,
                    "approved",
                    None,
                    Some(risk.clone()),
                    Some("Approved by user.".to_string()),
                )
                .ok();
                send_ai_tool_status(
                    ui_tx,
                    generation,
                    conversation_id,
                    assistant_id,
                    &status_call,
                    "running",
                    None,
                    Some(risk.clone()),
                    Some("Approved by user.".to_string()),
                )
                .ok();
                // Keep the policy outcome out of the canonical ACP argument object.
                let dangerous_command_approved = call.name == "run_command"
                    && decision.risk == oxideterm_ai::AiActionRisk::Destructive;
                executed_after_policy = true;
                execute_acp_visible_terminal_tool_on_ui(
                    ui_tx,
                    generation,
                    tool_session_id,
                    conversation_id,
                    assistant_id,
                    call.id.clone(),
                    call.name.clone(),
                    execution_args.clone(),
                    true,
                    dangerous_command_approved,
                )
                .await
            }
        }
        oxideterm_ai::AiPolicyDecisionKind::Allow => {
            send_ai_tool_status(
                ui_tx,
                generation,
                conversation_id,
                assistant_id,
                &status_call,
                "approved",
                None,
                Some(risk.clone()),
                Some(policy_summary.clone()),
            )
            .ok();
            send_ai_tool_status(
                ui_tx,
                generation,
                conversation_id,
                assistant_id,
                &status_call,
                "running",
                None,
                Some(risk.clone()),
                Some(policy_summary.clone()),
            )
            .ok();
            // Bypass mode is prior user consent, represented as backend state only.
            let dangerous_command_approved = call.name == "run_command"
                && decision.risk == oxideterm_ai::AiActionRisk::Destructive;
            executed_after_policy = true;
            execute_acp_visible_terminal_tool_on_ui(
                ui_tx,
                generation,
                tool_session_id,
                conversation_id,
                assistant_id,
                call.id.clone(),
                call.name.clone(),
                execution_args.clone(),
                false,
                dangerous_command_approved,
            )
            .await
        }
    };

    executed = resolve_ai_candidate_selection_if_needed(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        &status_call,
        executed,
    )
    .await;
    if executed_after_policy {
        if call.name == "run_command" {
            annotate_ai_run_command_execution_result(&mut executed, &execution_args);
        }
        annotate_executed_ai_tool_result_policy(&mut executed, &decision);
    }
    send_ai_tool_status(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        &status_call,
        if executed.success { "completed" } else { "error" },
        Some(executed.envelope.clone()),
        Some(risk),
        Some(executed_summary(&executed)),
    )
    .ok();

    // MCP responses bypass provider-message sanitation, so redact at this boundary.
    let model_content = serde_json::to_string(&executed.envelope)
        .map(|content| oxideterm_ai::sanitize_for_ai(&content))
        .unwrap_or_else(|_| "OxideTerm could not serialize the tool result.".to_string());
    let response = if executed.success {
        oxideterm_acp_host_tools::AcpHostToolResponse::success(model_content)
    } else {
        oxideterm_acp_host_tools::AcpHostToolResponse::error(model_content)
    };
    let _ = call.respond(response);
}

async fn resolve_ai_candidate_selection_if_needed(
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

pub(in crate::workspace) async fn run_acp_chat_loop(
    config: AiChatStreamConfig,
    history: Vec<AiChatMessage>,
    services: AiModelBackendServices,
    acp_launch: Result<Option<AiAcpChatLaunch>, String>,
    generation: u64,
    tool_session_id: ToolSessionId,
    conversation_id: String,
    assistant_id: String,
    ui_tx: AiStreamDeliverySender,
) {
    let Some(user_prompt) = acp_current_turn_prompt(&history) else {
        let _ = send_ai_stream_delivery(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(
                "Cannot start ACP agent without a user prompt.".to_string(),
            )),
        );
        return;
    };
    let Some(runtime_context) = request_ai_runtime_context(
        &ui_tx,
        generation,
        &tool_session_id,
        &conversation_id,
        &assistant_id,
    )
    .await
    else {
        let _ = send_ai_stream_delivery(
            &ui_tx,
            generation,
            &conversation_id,
            &assistant_id,
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(
                "The current application runtime context is unavailable.".to_string(),
            )),
        );
        return;
    };
    // ACP owns its internal model loop, so provide a fresh v2 projection at the prompt
    // boundary and require subsequent live discovery through the brokered host tools.
    let prompt = format!("{runtime_context}\n\n## Current User Request\n{user_prompt}");
    let launch = match acp_launch.and_then(|launch| {
        launch.ok_or_else(|| "ACP launch configuration was not prepared.".to_string())
    }) {
        Ok(launch) => launch,
        Err(error) => {
            let _ = send_ai_stream_delivery(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(error)),
            );
            return;
        }
    };
    let launcher = match oxideterm_ai::build_acp_stdio_launcher(launch.launch_config) {
        Ok(launcher) => launcher,
        Err(error) => {
            let _ = send_ai_stream_delivery(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(error.to_string())),
            );
            return;
        }
    };
    let session_cwd = launch.session_cwd;
    let host_policy = launch.host_policy;
    let agent_id = launch.agent_id;
    let mut acp_mcp_servers = Vec::new();
    let mut host_tools_server = None;
    let mut host_tools_relay = None;
    let host_tool_definitions =
        acp_visible_terminal_tool_definitions(config.tool_policy.enabled && host_policy.terminal);
    if !host_tool_definitions.is_empty() {
        let (server, mut call_rx) =
            match oxideterm_acp_host_tools::start_acp_host_tools_server(host_tool_definitions).await {
                Ok(started) => started,
                Err(error) => {
                    let _ = send_ai_stream_delivery(
                        &ui_tx,
                        generation,
                        &conversation_id,
                        &assistant_id,
                        AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(format!(
                            "Failed to start ACP host tools bridge: {error}"
                        ))),
                    );
                    return;
                }
            };
        acp_mcp_servers.push(server.mcp_server());
        let tool_policy = config.tool_policy;
        let tool_safety_mode = config.safety_mode;
        let tool_profile_id = config.profile_id;
        let tool_ui_tx = ui_tx.clone();
        let tool_session_id = tool_session_id.clone();
        let tool_conversation_id = conversation_id.clone();
        let tool_assistant_id = assistant_id.clone();
        host_tools_relay = Some(tokio::spawn(async move {
            while let Some(call) = call_rx.recv().await {
                handle_acp_visible_terminal_tool_call(
                    call,
                    &tool_policy,
                    tool_safety_mode,
                    tool_profile_id.as_deref(),
                    &tool_ui_tx,
                    generation,
                    &tool_session_id,
                    &tool_conversation_id,
                    &tool_assistant_id,
                )
                .await;
            }
        }));
        host_tools_server = Some(server);
    }
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let bridge_ui_tx = ui_tx.clone();
    let bridge_conversation_id = conversation_id.clone();
    let bridge_assistant_id = assistant_id.clone();
    let bridge_session_cwd = session_cwd.clone();
    let bridge_host_policy = host_policy.clone();
    let bridge_terminal_registry = oxideterm_ai::AcpTerminalRegistry::new();
    let bridge = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                oxideterm_ai::AcpClientEvent::ReadTextFile {
                    request,
                    response_tx,
                } => {
                    let response = if bridge_host_policy.fs_read_text_file {
                        oxideterm_ai::resolve_acp_read_text_file_request(
                            &bridge_session_cwd,
                            &request,
                        )
                        .await
                    } else {
                        Err(oxideterm_ai::acp_method_not_found("fs/read_text_file"))
                    };
                    let _ = response_tx.send(response);
                    continue;
                }
                oxideterm_ai::AcpClientEvent::WriteTextFile {
                    request,
                    response_tx,
                } => {
                    let response = if bridge_host_policy.fs_write_text_file {
                        oxideterm_ai::resolve_acp_write_text_file_request(
                            &bridge_session_cwd,
                            &request,
                        )
                        .await
                    } else {
                        Err(oxideterm_ai::acp_method_not_found("fs/write_text_file"))
                    };
                    let _ = response_tx.send(response);
                    continue;
                }
                oxideterm_ai::AcpClientEvent::CreateTerminal {
                    request,
                    response_tx,
                } => {
                    let response = if bridge_host_policy.terminal {
                        bridge_terminal_registry
                            .create_terminal(&bridge_session_cwd, &request)
                            .await
                    } else {
                        Err(oxideterm_ai::acp_method_not_found("terminal/create"))
                    };
                    let _ = response_tx.send(response);
                    continue;
                }
                oxideterm_ai::AcpClientEvent::TerminalOutput {
                    request,
                    response_tx,
                } => {
                    let response = if bridge_host_policy.terminal {
                        bridge_terminal_registry.terminal_output(&request).await
                    } else {
                        Err(oxideterm_ai::acp_method_not_found("terminal/output"))
                    };
                    let _ = response_tx.send(response);
                    continue;
                }
                oxideterm_ai::AcpClientEvent::ReleaseTerminal {
                    request,
                    response_tx,
                } => {
                    let response = if bridge_host_policy.terminal {
                        bridge_terminal_registry.release_terminal(&request).await
                    } else {
                        Err(oxideterm_ai::acp_method_not_found("terminal/release"))
                    };
                    let _ = response_tx.send(response);
                    continue;
                }
                oxideterm_ai::AcpClientEvent::WaitForTerminalExit {
                    request,
                    response_tx,
                } => {
                    let response = if bridge_host_policy.terminal {
                        bridge_terminal_registry
                            .wait_for_terminal_exit(&request)
                            .await
                    } else {
                        Err(oxideterm_ai::acp_method_not_found("terminal/wait_for_exit"))
                    };
                    let _ = response_tx.send(response);
                    continue;
                }
                oxideterm_ai::AcpClientEvent::KillTerminal {
                    request,
                    response_tx,
                } => {
                    let response = if bridge_host_policy.terminal {
                        bridge_terminal_registry.kill_terminal(&request).await
                    } else {
                        Err(oxideterm_ai::acp_method_not_found("terminal/kill"))
                    };
                    let _ = response_tx.send(response);
                    continue;
                }
                event => {
                    if send_ai_stream_delivery(
                        &bridge_ui_tx,
                        generation,
                        &bridge_conversation_id,
                        &bridge_assistant_id,
                        AiStreamDeliveryEvent::AcpClientEvent(event),
                    )
                    .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    let result = oxideterm_ai::run_acp_prompt_session_events(
        launcher,
        env!("CARGO_PKG_VERSION").to_string(),
        host_policy,
        session_cwd,
        config.acp_session_id.clone(),
        config.acp_config_selection.clone(),
        acp_mcp_servers,
        prompt,
        event_tx,
        services.ai_acp_runtime_registry.clone(),
        conversation_id.clone(),
        generation.to_string(),
    )
    .await;
    let _ = bridge.await;
    if let Some(server) = host_tools_server {
        server.shutdown().await;
    }
    if let Some(relay) = host_tools_relay {
        // The ACP turn owns the relay; cancel any orphaned approval wait after the agent exits.
        relay.abort();
        let _ = relay.await;
    }
    match result {
        Ok(outcome) => {
            // Persist the ACP session identity before the final Done event so a
            // retry or resumed conversation can load the same agent session.
            if send_ai_stream_delivery(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::AcpSessionStarted {
                    session_id: outcome.session_id,
                    session_metadata: outcome.session_metadata,
                    session_config_options: outcome.session_config_options,
                    agent_id,
                },
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
        }
        Err(error) => {
            let _ = send_ai_stream_delivery(
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(error.to_string())),
            );
        }
    }
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
