pub fn record_completed_ai_tool_call(completed_calls: &mut Vec<AiToolCall>, call: AiToolCall) {
    if let Some(existing) = completed_calls
        .iter_mut()
        .find(|existing| existing.id == call.id)
    {
        *existing = call;
    } else {
        completed_calls.push(call);
    }
}

pub fn reject_ai_tool_calls_for_protocol_guard(
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

pub fn send_ai_guardrail(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    code: impl Into<String>,
    message: impl Into<String>,
    raw_text: Option<String>,
) -> Result<(), Box<std::sync::mpsc::SendError<AiStreamDelivery>>> {
    let raw_text = raw_text.as_deref().map(crate::sanitize_for_ai);
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

pub fn send_ai_prompt_usage(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    last_user_message_id: Option<String>,
    provider_id: String,
    model: String,
    breakdown: crate::AiPromptTokenBreakdown,
    max_tokens: usize,
) -> Result<(), Box<std::sync::mpsc::SendError<AiStreamDelivery>>> {
    send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::PromptUsage {
            last_user_message_id,
            provider_id,
            model,
            breakdown,
            max_tokens,
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub fn send_ai_assistant_round(
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
) -> Result<(), Box<std::sync::mpsc::SendError<AiStreamDelivery>>> {
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

pub fn send_ai_round_summary(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    round_id: String,
    text: String,
    metadata: serde_json::Value,
) -> Result<(), Box<std::sync::mpsc::SendError<AiStreamDelivery>>> {
    let text = crate::sanitize_for_ai(&text);
    let metadata = crate::sanitize_json_for_ai(&metadata);
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

pub fn send_ai_round_stateful_marker(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    round_id: String,
    marker: Option<String>,
) -> Result<(), Box<std::sync::mpsc::SendError<AiStreamDelivery>>> {
    send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::RoundStatefulMarker { round_id, marker },
    )
}

pub fn send_ai_diagnostic(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    event_type: impl Into<String>,
    round_id: Option<String>,
    data: serde_json::Value,
) -> Result<(), Box<std::sync::mpsc::SendError<AiStreamDelivery>>> {
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

pub fn send_ai_stream_delivery(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    event: AiStreamDeliveryEvent,
) -> Result<(), Box<std::sync::mpsc::SendError<AiStreamDelivery>>> {
    let event = match event {
        // Provider and protocol errors may contain response bodies, process
        // paths, or request metadata. The UI maps this stable category to a
        // localized message.
        AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(ref error))
            if crate::stream_error_label(error).is_some() =>
        {
            event
        }
        AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(_)) => {
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Error("stream_failed".to_string()))
        }
        other => other,
    };
    ui_tx
        .send(AiStreamDelivery {
            generation,
            conversation_id: conversation_id.to_string(),
            assistant_id: assistant_id.to_string(),
            event,
        })
        .map_err(Box::new)
}

pub fn send_ai_tool_status(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    conversation_id: &str,
    assistant_id: &str,
    call: &AiToolCall,
    status: &str,
    result: Option<serde_json::Value>,
    risk: Option<String>,
    summary: Option<String>,
) -> Result<(), Box<std::sync::mpsc::SendError<AiStreamDelivery>>> {
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
pub fn send_ai_tool_status_with_payload(
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
) -> Result<(), Box<std::sync::mpsc::SendError<AiStreamDelivery>>> {
    let arguments = sanitize_ai_tool_arguments_for_persistence(&call.arguments);
    let result = result
        .as_ref()
        .map(|result| crate::sanitize_tool_result_json_for_persistence(&call.name, result));
    let summary = summary.as_deref().map(crate::sanitize_for_persistence);
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
