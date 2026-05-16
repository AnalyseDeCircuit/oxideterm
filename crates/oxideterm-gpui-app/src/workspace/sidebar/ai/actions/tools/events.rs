fn record_completed_ai_tool_call(completed_calls: &mut Vec<AiToolCall>, call: AiToolCall) {
    if let Some(existing) = completed_calls
        .iter_mut()
        .find(|existing| existing.id == call.id)
    {
        *existing = call;
    } else {
        completed_calls.push(call);
    }
}

fn reject_ai_tool_calls_for_protocol_guard(
    ui_tx: &std::sync::mpsc::Sender<AiStreamDelivery>,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    calls: &[AiToolCall],
    code: &str,
    message: impl Into<String>,
) {
    let message = message.into();
    for call in calls {
        let executed = rejected_ai_tool_result(
            call.id.clone(),
            call.name.clone(),
            code.to_string(),
            message.clone(),
        );
        let _ = send_ai_tool_status(
            ui_tx,
            generation,
            conversation_id,
            assistant_id,
            call,
            "rejected",
            Some(executed.envelope.clone()),
            Some("write".to_string()),
            Some(executed_summary(&executed)),
        );
    }
}

fn send_ai_guardrail(
    ui_tx: &std::sync::mpsc::Sender<AiStreamDelivery>,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    code: impl Into<String>,
    message: impl Into<String>,
    raw_text: Option<String>,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::Guardrail {
            code: code.into(),
            message: message.into(),
            raw_text,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn send_ai_assistant_round(
    ui_tx: &std::sync::mpsc::Sender<AiStreamDelivery>,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    round_id: String,
    round_number: i64,
    response_length: usize,
    tool_call_ids: Vec<String>,
    synthetic: bool,
    retry_attempt: Option<usize>,
    hard_deny_triggered: bool,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::AssistantRound {
            round_id,
            round_number,
            response_length,
            tool_call_ids,
            synthetic,
            retry_attempt,
            hard_deny_triggered,
        },
    )
}

fn send_ai_round_summary(
    ui_tx: &std::sync::mpsc::Sender<AiStreamDelivery>,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    round_id: String,
    text: String,
    metadata: serde_json::Value,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::RoundSummary {
            round_id,
            text,
            metadata,
        },
    )
}

fn send_ai_round_stateful_marker(
    ui_tx: &std::sync::mpsc::Sender<AiStreamDelivery>,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    round_id: String,
    marker: Option<String>,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::RoundStatefulMarker { round_id, marker },
    )
}

fn send_ai_diagnostic(
    ui_tx: &std::sync::mpsc::Sender<AiStreamDelivery>,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    event_type: impl Into<String>,
    round_id: Option<String>,
    data: serde_json::Value,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::Diagnostic {
            event_type: event_type.into(),
            round_id,
            data,
        },
    )
}

fn ai_orchestrator_obligation_mode_label(mode: AiOrchestratorObligationMode) -> &'static str {
    match mode {
        AiOrchestratorObligationMode::Auto => "auto",
        AiOrchestratorObligationMode::Required => "required",
    }
}

fn ai_tool_choice_label(choice: &oxideterm_ai::AiToolChoice) -> serde_json::Value {
    match choice {
        oxideterm_ai::AiToolChoice::Auto => serde_json::json!("auto"),
        oxideterm_ai::AiToolChoice::Required => serde_json::json!("required"),
        oxideterm_ai::AiToolChoice::Named(name) => serde_json::json!(name),
    }
}

#[derive(Debug)]
struct AiRoundToolResultSummary {
    tool_name: String,
    success: bool,
    summary: String,
}

fn ai_round_summary_text(results: &[AiRoundToolResultSummary]) -> String {
    results
        .iter()
        .map(|result| {
            format!(
                "{}: {} - {}",
                result.tool_name,
                if result.success { "ok" } else { "error" },
                result.summary.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn send_ai_stream_delivery(
    ui_tx: &std::sync::mpsc::Sender<AiStreamDelivery>,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    event: AiStreamDeliveryEvent,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    ui_tx.send(AiStreamDelivery {
        generation,
        conversation_id: conversation_id.to_string(),
        assistant_id: assistant_id.to_string(),
        event,
    })
}

fn send_ai_tool_status(
    ui_tx: &std::sync::mpsc::Sender<AiStreamDelivery>,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    call: &AiToolCall,
    status: &str,
    result: Option<serde_json::Value>,
    risk: Option<String>,
    summary: Option<String>,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    send_ai_tool_status_with_payload(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        call,
        status,
        result,
        risk,
        summary,
        false,
        None,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn send_ai_tool_status_with_payload(
    ui_tx: &std::sync::mpsc::Sender<AiStreamDelivery>,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    call: &AiToolCall,
    status: &str,
    result: Option<serde_json::Value>,
    risk: Option<String>,
    summary: Option<String>,
    synthetic_denied: bool,
    raw_text: Option<String>,
    round_id: Option<String>,
    round_number: Option<i64>,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::ToolStatus {
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
            status: status.to_string(),
            result,
            risk,
            summary,
            synthetic_denied,
            raw_text,
            round_id,
            round_number,
        },
    )
}

fn parse_ai_tool_args(arguments: &str) -> serde_json::Value {
    serde_json::from_str(arguments)
        .unwrap_or_else(|_| serde_json::json!({ "rawArguments": arguments }))
}

fn ai_tool_call_message_value(call: &AiToolCall) -> serde_json::Value {
    serde_json::json!({
        "id": call.id,
        "name": call.name,
        "arguments": call.arguments,
    })
}

fn ai_tool_result_message(result: AiExecutedToolResult) -> AiChatMessage {
    let fallback_tool_name = result.tool_name.clone();
    let fallback_duration_ms = result.duration_ms;
    AiChatMessage {
        id: format!("tool-result-{}", result.tool_call_id),
        role: AiChatRole::Tool,
        content: serde_json::to_string(&result.envelope).unwrap_or_else(|_| {
            serde_json::json!({
                "ok": result.success,
                "output": result.output,
                "error": result.error,
                "meta": {
                    "toolName": fallback_tool_name,
                    "durationMs": fallback_duration_ms,
                }
            })
            .to_string()
        }),
        timestamp_ms: ai_now_ms(),
        model: None,
        context: None,
        is_streaming: false,
        thinking_content: None,
        metadata: None,
        tool_call_id: Some(result.tool_call_id),
        tool_calls: Vec::new(),
        turn: None,
        transcript_ref: None,
        summary_ref: None,
        branches: None,
            suggestions: Vec::new(),
    }
}
