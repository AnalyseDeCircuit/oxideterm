pub(in crate::workspace) fn record_completed_ai_tool_call(
    completed_calls: &mut Vec<AiToolCall>,
    call: AiToolCall,
) {
    if let Some(existing) = completed_calls
        .iter_mut()
        .find(|existing| existing.id == call.id)
    {
        *existing = call;
    } else {
        completed_calls.push(call);
    }
}

pub(in crate::workspace) fn reject_ai_tool_calls_for_protocol_guard(
    ui_tx: &AiStreamDeliverySender,
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

pub(in crate::workspace) fn send_ai_guardrail(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    code: impl Into<String>,
    message: impl Into<String>,
    raw_text: Option<String>,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    let raw_text = raw_text
        .as_deref()
        .map(oxideterm_ai::sanitize_for_ai);
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
pub(in crate::workspace) fn send_ai_assistant_round(
    ui_tx: &AiStreamDeliverySender,
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

pub(in crate::workspace) fn send_ai_round_summary(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    round_id: String,
    text: String,
    metadata: serde_json::Value,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    let text = oxideterm_ai::sanitize_for_ai(&text);
    let metadata = oxideterm_ai::sanitize_json_for_ai(&metadata);
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

pub(in crate::workspace) fn send_ai_round_stateful_marker(
    ui_tx: &AiStreamDeliverySender,
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

pub(in crate::workspace) fn send_ai_diagnostic(
    ui_tx: &AiStreamDeliverySender,
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

pub(in crate::workspace) fn ai_orchestrator_obligation_mode_label(
    mode: AiOrchestratorObligationMode,
) -> &'static str {
    match mode {
        AiOrchestratorObligationMode::Auto => "auto",
        AiOrchestratorObligationMode::Required => "required",
    }
}

pub(in crate::workspace) fn ai_tool_choice_label(
    choice: &oxideterm_ai::AiToolChoice,
) -> serde_json::Value {
    match choice {
        oxideterm_ai::AiToolChoice::Auto => serde_json::json!("auto"),
        oxideterm_ai::AiToolChoice::Required => serde_json::json!("required"),
        oxideterm_ai::AiToolChoice::Named(name) => serde_json::json!(name),
    }
}

#[derive(Debug)]
pub(in crate::workspace) struct AiRoundToolResultSummary {
    pub(in crate::workspace) tool_name: String,
    pub(in crate::workspace) success: bool,
    pub(in crate::workspace) summary: String,
}

pub(in crate::workspace) fn ai_round_summary_text(results: &[AiRoundToolResultSummary]) -> String {
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

pub(in crate::workspace) fn send_ai_stream_delivery(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    event: AiStreamDeliveryEvent,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    let event = match event {
        // Provider and protocol errors may contain response bodies, process
        // paths, or request metadata. The UI maps this stable category to a
        // localized message.
        AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(_)) => {
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Error("stream_failed".to_string()))
        }
        other => other,
    };
    ui_tx.send(AiStreamDelivery {
        generation,
        conversation_id: conversation_id.to_string(),
        assistant_id: assistant_id.to_string(),
        event,
    })
}

pub(in crate::workspace) fn send_ai_tool_status(
    ui_tx: &AiStreamDeliverySender,
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
pub(in crate::workspace) fn send_ai_tool_status_with_payload(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    call: &AiToolCall,
    status: &str,
    result: Option<serde_json::Value>,
    risk: Option<String>,
    summary: Option<String>,
    synthetic_denied: bool,
    _raw_text: Option<String>,
    round_id: Option<String>,
    round_number: Option<i64>,
) -> Result<(), std::sync::mpsc::SendError<AiStreamDelivery>> {
    let arguments = sanitize_ai_tool_arguments_for_persistence(&call.arguments);
    let result = result
        .as_ref()
        .map(oxideterm_ai::sanitize_json_for_ai);
    let summary = summary
        .as_deref()
        .map(oxideterm_ai::sanitize_for_ai);
    send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::ToolStatus {
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
            arguments,
            status: status.to_string(),
            result,
            risk,
            summary,
            synthetic_denied,
            raw_text: None,
            round_id,
            round_number,
        },
    )
}

pub(in crate::workspace) fn parse_ai_tool_args(arguments: &str) -> Option<serde_json::Value> {
    let parsed = serde_json::from_str::<serde_json::Value>(arguments).ok()?;
    if parsed.is_object() {
        Some(parsed)
    } else {
        None
    }
}

pub(in crate::workspace) fn ai_tool_call_message_value(call: &AiToolCall) -> serde_json::Value {
    serde_json::json!({
        "id": call.id,
        "name": call.name,
        "arguments": call.arguments,
    })
}

pub(in crate::workspace) fn ai_tool_result_message(result: AiExecutedToolResult) -> AiChatMessage {
    let content = ai_tool_result_model_content(&result);
    AiChatMessage {
        id: format!("tool-result-{}", result.tool_call_id),
        role: AiChatRole::Tool,
        content,
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
