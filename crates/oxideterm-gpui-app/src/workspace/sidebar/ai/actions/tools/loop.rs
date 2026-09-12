#[derive(Clone, Copy, PartialEq, Eq)]
enum AiAgentLoopStatus { Completed, Failed, Stalled }
struct AiAgentLoopOutcome { content: String, status: AiAgentLoopStatus }
impl AiAgentLoopOutcome {
    fn failed(&self) -> bool { self.status != AiAgentLoopStatus::Completed }
}

pub(in crate::workspace) async fn run_ai_chat_tool_loop(
    config: AiChatStreamConfig, mut history: Vec<AiChatMessage>, model_runtime: AiModelRuntimeState,
    services: AiModelBackendServices, budget_level: u8, generation: u64, tool_session_id: ToolSessionId,
    conversation_id: String, assistant_id: String, ui_tx: AiStreamDeliverySender, mut execution: Option<AgentExecution>,
) {
    let cancellation = execution.as_ref().and_then(|agent| agent.runtime.cancellation(&agent.run).ok());
    let mut work = Box::pin(execute_ai_chat_tool_loop(config, &mut history, model_runtime, services, budget_level, generation, tool_session_id,
        conversation_id.clone(), assistant_id.clone(), ui_tx.clone(), &mut execution));
    let outcome = if let Some(mut cancellation) = cancellation {
        if *cancellation.borrow() { AiAgentLoopOutcome { content: String::new(), status: AiAgentLoopStatus::Failed } }
        else { tokio::select! {
            outcome = &mut work => outcome,
            _ = cancellation.changed() => AiAgentLoopOutcome { content: String::new(), status: AiAgentLoopStatus::Failed },
        } }
    } else { work.as_mut().await };
    drop(work);
    if let Some(agent) = execution.as_mut() {
        if !agent.is_child() && outcome.failed() { stop_and_wait_for_children(agent).await; }
        let cancelled = agent.runtime.cancellation(&agent.run).is_ok_and(|cancelled| *cancelled.borrow());
        let evidence = history.iter().filter_map(|message| message.tool_call_id.as_ref()).map(|id| AgentText::new(&format!("agent:{}/run:{}/tool:{id}", agent.run.agent_id, agent.run.run_id))).collect();
        let mut actions = Vec::new();
        let mut unfinished = Vec::new();
        for message in history.iter().filter(|message| message.role == AiChatRole::Tool) {
            if let Ok(result) = serde_json::from_str::<serde_json::Value>(&message.content) {
                let summary = result.get("summary").and_then(serde_json::Value::as_str).unwrap_or_default();
                if summary.is_empty() { continue; }
                if result.get("ok").and_then(serde_json::Value::as_bool) == Some(true) { actions.push(AgentText::new(summary)); }
                else if outcome.failed() { unfinished.push(AgentText::new(summary)); }
            }
        }
        let _ = agent.runtime.save_context(&agent.run, history);
        let _ = agent.runtime.complete(&agent.run, AgentResult { summary: AgentText::new(&outcome.content), evidence,
            actions, unfinished, error_code: outcome.failed().then(|| if cancelled { "agent_cancelled" } else if outcome.status == AiAgentLoopStatus::Stalled { "agent_no_progress" } else { "agent_execution_failed" }.into()) });
        if !agent.is_child() { let _ = agent.runtime.finish_group(&agent.run); }
        let event = if outcome.status == AiAgentLoopStatus::Failed && !cancelled { AiStreamEvent::Error(outcome.content) } else { AiStreamEvent::Done };
        let _ = send_ai_stream_delivery(&ui_tx, generation, &conversation_id, &assistant_id, AiStreamDeliveryEvent::Stream(event));
    }
}

fn send_ai_loop_delivery(defer_terminal: bool, sender: &AiStreamDeliverySender, generation: u64, conversation_id: &str, assistant_id: &str, event: AiStreamDeliveryEvent) -> Result<(), ()> {
    if defer_terminal && matches!(event, AiStreamDeliveryEvent::Stream(AiStreamEvent::Done | AiStreamEvent::Error(_))) { return Ok(()); }
    send_ai_stream_delivery(sender, generation, conversation_id, assistant_id, event).map_err(|_| ())
}

const AI_TOOL_CALLS_PER_ROUND_SAFETY_LIMIT: usize = 16;
const AI_RUNTIME_CONTEXT_MESSAGE_ID: &str = "runtime-context-v2";

async fn execute_ai_chat_tool_loop(
    mut config: AiChatStreamConfig,
    mut history: &mut Vec<AiChatMessage>,
    mut model_runtime: AiModelRuntimeState,
    services: AiModelBackendServices,
    budget_level: u8,
    generation: u64,
    tool_session_id: ToolSessionId,
    conversation_id: String,
    assistant_id: String,
    ui_tx: AiStreamDeliverySender,
    execution: &mut Option<AgentExecution>,
) -> AiAgentLoopOutcome {
    if let Some(agent) = execution.as_mut().filter(|agent| agent.is_child()) {
        if let Err(error) = configure_ai_child_model(agent, &mut config, &mut model_runtime, &services).await {
            return AiAgentLoopOutcome { content: error, status: AiAgentLoopStatus::Failed };
        }
    }
    oxideterm_ai::scope_responses_history(history, &config);
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
    let task_user_index = history.iter().rposition(|message| message.role == AiChatRole::User).unwrap_or(0);
    let task_user_id = history.get(task_user_index).map(|message| message.id.clone()).unwrap_or_default();
    let mut checkpoint = history[task_user_index..].iter().rev()
        .find_map(oxideterm_ai::agent::message_checkpoint)
        .unwrap_or_else(|| oxideterm_ai::agent::AgentCheckpoint::new(&request_text));
    let mut progress_guard = oxideterm_ai::agent::ProgressGuard::default();
    let user_requested_json = ai_user_explicitly_requested_json(&request_text);
    let mut hard_deny_retry_count = 0usize;

    let mut awaiting_summary_round_id: Option<String> = None;

    for round_index in 0usize.. {
        if execution.is_none() && round_index > max_rounds { break; }
        let mut summary_only = false;
        if let Some(agent) = execution.as_mut() {
            if agent.ready().await.is_err() { return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed }; }
            let had_deferred_directions = !agent.deferred_user_messages.is_empty();
            let previous_cursor = agent.event_cursor;
            let previous_length = history.len();
            append_agent_mailbox(&mut history, agent);
            if agent.event_cursor != previous_cursor || had_deferred_directions {
                progress_guard.reset();
                checkpoint.directives.extend(history[previous_length..].iter()
                    .filter(|message| message.role == AiChatRole::User).map(|message| AgentText::new(&message.content)));
            }
            match agent.runtime.take_round(&agent.run) {
                Ok(()) => {}
                Err(oxideterm_ai::agent::AgentError::BudgetExhausted) if !agent.is_child() => {
                    if agent.runtime.take_final_summary(&agent.run).is_err() { return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed }; }
                    stop_and_wait_for_children(agent).await;
                    append_agent_mailbox(&mut history, agent);
                    loop {
                        match agent.runtime.prepare_completion(&agent.run, agent.event_cursor) {
                            Ok(()) => break,
                            Err(oxideterm_ai::agent::AgentError::PendingMessages) => append_agent_mailbox(&mut history, agent),
                            Err(_) => return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed },
                        }
                    }
                    history.push(agent_chat_message(AiChatRole::System, "The shared tool-round budget is exhausted. Summarize completed work and remaining limitations from the available evidence. Do not call tools or claim that cancelled remote operations were terminated.".into()));
                    summary_only = true;
                }
                Err(_) => { return AiAgentLoopOutcome { content: "The shared task budget is exhausted or this run was stopped.".into(), status: AiAgentLoopStatus::Failed }; }
            }
        }
        summary_only |= progress_guard.stalled();
        if progress_guard.stalled() && !history.iter().any(|message| message.id == "agent-no-progress") {
            let mut notice = agent_chat_message(AiChatRole::System,
                "Execution stopped after repeated identical failures without progress. Do not call tools. Explain what completed, what remains unresolved, and which prerequisite is needed to continue, in the user's language.".into());
            notice.id = "agent-no-progress".into();
            history.push(notice);
        }
        let Some(runtime_context) = request_ai_runtime_context(
            &ui_tx,
            generation,
            &tool_session_id,
            &conversation_id,
            &assistant_id,
        )
        .await
        else {
            return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed };
        };
        replace_ai_runtime_context_message(&mut history, runtime_context);
        if ai_prompt_token_breakdown(history, &config.tools, &config.provider_type, response_reserve).total()
            > model_runtime.context_window.saturating_mul(75) / 100 {
            if let Err(error) = compact_running_ai_task(history, &config, &mut checkpoint, &task_user_id,
                model_runtime.context_window, execution.as_ref()).await {
                let _ = send_ai_loop_delivery(execution.is_some(), &ui_tx, generation, &conversation_id, &assistant_id,
                    AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(error.clone())));
                return AiAgentLoopOutcome { content: error, status: AiAgentLoopStatus::Failed };
            }
            let _ = send_ai_loop_delivery(execution.is_some(), &ui_tx, generation, &conversation_id, &assistant_id,
                AiStreamDeliveryEvent::Checkpoint(checkpoint.clone()));
        }
        let dispatch = if let Some(agent) = execution.as_mut() {
            loop {
                let mut changes = agent.runtime.subscribe();
                append_agent_mailbox(history, agent);
                match agent.runtime.dispatch(&agent.run) {
                    Ok(guard) => break Some(guard),
                    Err(oxideterm_ai::agent::AgentError::DirectionChanged) => {
                        if agent.wait(AgentState::AwaitingParent, changes.changed()).await.is_err() {
                            return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed };
                        }
                    }
                    Err(_) => return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed },
                }
            }
        } else { None };
        let mut provider_config = config.clone();
        if summary_only { provider_config.tools.clear(); provider_config.tool_choice = oxideterm_ai::AiToolChoice::Auto; }
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
        let usage_request = execution.as_ref().and_then(|agent| agent.runtime.begin_request(&agent.run).ok());
        let mut model_request = oxideterm_ai::agent::AgentModelRequest::start(
            provider_config,
            provider_history,
        );

        let mut stream_error = None;
        let mut round_content = String::new();
        let mut round_thinking = String::new();
        let mut pending_calls = BTreeMap::<String, AiToolCall>::new();
        let mut call_ids = HashMap::<String, String>::new();
        let mut completed_calls = Vec::<AiToolCall>::new();
        let mut round_provider_parts = Vec::<serde_json::Value>::new();

        while let Some(event) = model_request.next_event().await {
            match event {
                AiStreamEvent::Usage { input_tokens, output_tokens } => {
                    if let Some((agent, request)) = execution.as_ref().zip(usage_request) { let _ = agent.runtime.record_usage(&agent.run, request, input_tokens, output_tokens); }
                }
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
                    if send_ai_loop_delivery(
                        execution.is_some(),
                        &ui_tx,
                        generation,
                        &conversation_id,
                        &assistant_id,
                        AiStreamDeliveryEvent::Stream(AiStreamEvent::Content(chunk)),
                    )
                    .is_err()
                    {
                        return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed };
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
                    if send_ai_loop_delivery(
                        execution.is_some(),
                        &ui_tx,
                        generation,
                        &conversation_id,
                        &assistant_id,
                        AiStreamDeliveryEvent::Stream(AiStreamEvent::Thinking(chunk)),
                    )
                    .is_err()
                    {
                        return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed };
                    }
                }
                AiStreamEvent::ProviderResponsePart {
                    provider_type,
                    part,
                } => {
                    if provider_type == config.provider_type
                        || (config.uses_responses() && provider_type == config.response_state_key())
                    {
                        // Keep wire metadata out of diagnostics. Responses rounds are
                        // delivered to durable history only after their tool results exist.
                        round_provider_parts.push(part);
                    }
                }
                AiStreamEvent::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    let id = call_ids.entry(id).or_insert_with(|| format!("call_{}", uuid::Uuid::new_v4().simple())).clone();
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
                    if send_ai_loop_delivery(
                        execution.is_some(),
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
                        return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed };
                    }
                }
                AiStreamEvent::ToolCallComplete {
                    id,
                    name,
                    arguments,
                } => {
                    // Provider IDs are not unique across requests. Replay the same new ID in
                    // both assistant calls and tool results so old approvals cannot target a new call.
                    let id = call_ids.entry(id).or_insert_with(|| format!("call_{}", uuid::Uuid::new_v4().simple())).clone();
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
                    if send_ai_loop_delivery(
                        execution.is_some(),
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
                        return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed };
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

        // Provider completion can precede a long approval wait. Release the
        // request and its credentials before entering any tool interaction.
        drop(model_request);

        if let Some(error) = stream_error {
            assistant_content = error.clone();
            let _ = send_ai_loop_delivery(
                        execution.is_some(),
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(error)),
            );
            return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed };
        }

        let response_round = config
            .uses_responses()
            .then(|| oxideterm_ai::responses_round_state(&round_provider_parts, &call_ids, &[]))
            .flatten();
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
            let mut final_message =
                agent_chat_message(AiChatRole::Assistant, round_content.clone());
            if let Some(round) = response_round {
                set_ai_provider_parts(
                    &mut final_message,
                    &config.response_state_key(),
                    vec![round.clone()],
                );
                let _ = send_ai_loop_delivery(
                    execution.is_some(),
                    &ui_tx,
                    generation,
                    &conversation_id,
                    &assistant_id,
                    AiStreamDeliveryEvent::Stream(AiStreamEvent::ProviderResponsePart {
                        provider_type: config.response_state_key(),
                        part: round,
                    }),
                );
            }
            history.push(final_message);
            if let Some(agent) = execution.as_mut() {
                if !summary_only { let _ = agent.runtime.refund_empty_round(&agent.run); }
                if !agent.is_child() && agent.runtime.children(&agent.run).is_ok_and(|children| children.iter().any(|child| !child.state.is_terminal())) {
                    let runtime = agent.runtime.clone();
                    let run = agent.run.clone();
                    let cursor = agent.event_cursor;
                    let _ = agent.wait(AgentState::AwaitingParent, runtime.wait_updates(&run, cursor)).await;
                    continue;
                }
                match agent.runtime.prepare_completion(&agent.run, agent.event_cursor) {
                    Ok(()) => {}
                    Err(oxideterm_ai::agent::AgentError::PendingMessages) => continue,
                    Err(_) => return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed },
                }
            }
            checkpoint.needs_continuation = progress_guard.stalled();
            let _ = send_ai_loop_delivery(execution.is_some(), &ui_tx, generation, &conversation_id, &assistant_id,
                AiStreamDeliveryEvent::Checkpoint(checkpoint.clone()));
            let _ = send_ai_loop_delivery(
                        execution.is_some(),
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Done),
            );
            return AiAgentLoopOutcome { content: assistant_content, status: if progress_guard.stalled() { AiAgentLoopStatus::Stalled } else { AiAgentLoopStatus::Completed } };
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
            let _ = send_ai_loop_delivery(
                        execution.is_some(),
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(format!(
                    "Too many tool calls in one round (max {}).",
                    max_calls_per_round
                ))),
            );
            return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed };
        }

        if summary_only || (execution.is_none() && round_index >= max_rounds) {
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
            let _ = send_ai_loop_delivery(
                        execution.is_some(),
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(
                    "Tool execution stopped after reaching the maximum tool rounds.".to_string(),
                )),
            );
            return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed };
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
        if let Some(round) = response_round {
            set_ai_provider_parts(
                &mut assistant_round,
                &config.response_state_key(),
                vec![round],
            );
        } else {
            set_ai_provider_parts(
                &mut assistant_round,
                &config.provider_type,
                round_provider_parts.clone(),
            );
        }
        history.push(assistant_round);
        let response_results_start = history.len();

        let mut pause_requested = false;
        let mut round_results = Vec::new();
        let mut dependencies = oxideterm_ai::agent::ToolDependencies::new(&completed_calls);
        let mut calls = std::collections::VecDeque::from(completed_calls);
        while let Some(call) = calls.pop_front() {
            let can_parallel = |call: &AiToolCall| {
                oxideterm_ai::agent::parallel_read_call(call)
                    && parse_ai_tool_args(&call.name, &call.arguments).is_some_and(|args| {
                        let decision = resolve_ai_policy_decision(&call.name, Some(&args), &config.tool_policy,
                            config.safety_mode, config.profile_id.as_deref());
                        decision.decision == oxideterm_ai::AiPolicyDecisionKind::Allow && decision.risk == oxideterm_ai::AiActionRisk::Read
                    })
                    && execution.as_ref().is_none_or(|agent| agent.runtime.snapshot(&agent.run)
                        .is_ok_and(|snapshot| snapshot.scope.tools.contains(&call.name)))
            };
            let mut batch = vec![call];
            if can_parallel(&batch[0]) {
                while batch.len() < oxideterm_ai::agent::MAX_PARALLEL_READS
                    && calls.front().is_some_and(&can_parallel) {
                    batch.push(calls.pop_front().unwrap());
                }
            }
            // Persist uncertainty before dispatch: cancellation can arrive after a remote side effect.
            for call in &batch { checkpoint.pending(call); }
            let _ = send_ai_loop_delivery(execution.is_some(), &ui_tx, generation, &conversation_id, &assistant_id,
                AiStreamDeliveryEvent::Checkpoint(checkpoint.clone()));
            let results = if dependencies.failed_dependency(&batch[0]) {
                batch.iter().map(|call| Ok(pre_execution_rejected_ai_tool_result(call.id.clone(), call.name.clone(),
                    "dependency_failed", "A preceding dependency failed; this call was not dispatched."))).collect()
            } else if batch.len() > 1 {
                use futures_util::StreamExt;
                // Each future borrows the loop's owner. Dropping the batch cancels all reads;
                // writes, unknown MCP actions and agent coordination never enter this path.
                futures_util::stream::iter(batch.iter().cloned().map(|call| {
                    let config = &config;
                    let services = &services;
                    let available = &available_tool_names;
                    let ui_tx = &ui_tx;
                    let session = &tool_session_id;
                    let conversation = &conversation_id;
                    let assistant = &assistant_id;
                    let dispatch = dispatch.as_ref();
                    async move {
                        execute_ai_round_call(call, config, services, available, ui_tx, generation,
                            session, conversation, assistant, &mut None, false, dispatch).await
                    }
                })).buffered(oxideterm_ai::agent::MAX_PARALLEL_READS).collect::<Vec<_>>().await
            } else {
                vec![execute_ai_round_call(batch[0].clone(), &config, &services, &available_tool_names,
                    &ui_tx, generation, &tool_session_id, &conversation_id, &assistant_id, execution, true, dispatch.as_ref()).await]
            };
            for (call, result) in batch.iter().zip(results) {
                let Ok(mut executed) = result else { return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Failed }; };
                if batch.len() > 1 && !execution.as_ref().is_some_and(AgentExecution::is_child) {
                    let selection_required = executed.envelope.pointer("/error/code")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|code| matches!(code, "target_disambiguation_required" | "resource_ambiguous"));
                    executed = resolve_ai_candidate_selection_if_needed(&ui_tx, generation, &conversation_id, &assistant_id, call, executed).await;
                    if selection_required {
                        send_ai_tool_status(&ui_tx, generation, &conversation_id, &assistant_id,
                            call, if executed.success { "completed" } else { "error" },
                            Some(executed.envelope.clone()), Some(ai_policy_risk_label(oxideterm_ai::AiActionRisk::Read).to_string()),
                            Some(executed_summary(&executed))).ok();
                    }
                }
                if executed.envelope.pointer("/error/code").and_then(serde_json::Value::as_str) == Some("dependency_failed") {
                    let _ = send_ai_tool_status(&ui_tx, generation, &conversation_id, &assistant_id, call, "rejected",
                        Some(executed.envelope.clone()), None, None);
                }
                pause_requested |= executed.envelope.pointer("/error/code").and_then(serde_json::Value::as_str) == Some("agent_wait_paused");
                oxideterm_ai::agent::annotate_recovery(&mut executed);
                dependencies.record(call, executed.success);
                checkpoint.record(call, &executed);
                if progress_guard.observe(call, &executed) {
                    history.push(agent_chat_message(AiChatRole::System,
                        "The same tool request has repeatedly produced no new evidence. Do not repeat it unchanged. Inspect the result, use a different approach, or ask the user for the missing prerequisite. Terminal polling is not subject to this warning.".into()));
                }
                let _ = send_ai_loop_delivery(execution.is_some(), &ui_tx, generation, &conversation_id, &assistant_id,
                    AiStreamDeliveryEvent::Checkpoint(checkpoint.clone()));
                round_results.push(AiRoundToolResultSummary {tool_name:call.name.clone(),success:executed.success,summary:executed_summary(&executed)});
                history.push(ai_tool_result_message(executed));
            }
        }

        if config.uses_responses()
            && let Some(round) = oxideterm_ai::responses_round_state(
                &round_provider_parts,
                &call_ids,
                &history[response_results_start..],
            )
        {
            let _ = send_ai_loop_delivery(
                execution.is_some(),
                &ui_tx,
                generation,
                &conversation_id,
                &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::ProviderResponsePart {
                    provider_type: config.response_state_key(),
                    part: round,
                }),
            );
        }
        if pause_requested {
            let _ = send_ai_loop_delivery(execution.is_some(), &ui_tx, generation, &conversation_id, &assistant_id,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Done));
            return AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Stalled };
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

    let _ = send_ai_loop_delivery(
                        execution.is_some(),
        &ui_tx,
        generation,
        &conversation_id,
        &assistant_id,
        AiStreamDeliveryEvent::Stream(AiStreamEvent::Done),
    );
    AiAgentLoopOutcome { content: assistant_content, status: AiAgentLoopStatus::Completed }
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
