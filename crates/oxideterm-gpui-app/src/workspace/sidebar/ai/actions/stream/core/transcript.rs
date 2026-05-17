impl WorkspaceApp {
    fn persist_ai_assistant_turn_end(
        &self,
        conversation_id: &str,
        message_id: &str,
        status: &str,
    ) {
        let Some(message) = self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(|conversation| {
                conversation
                    .messages
                    .iter()
                    .find(|message| message.id == message_id)
            })
        else {
            return;
        };
        let parts = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        let has_parts = parts
            .as_array()
            .is_some_and(|parts| !parts.is_empty());
        let tool_round_count = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("toolRounds"))
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        let plain_text_summary =
            ai_turn_plain_text_summary(message).unwrap_or_else(|| message.content.clone());
        let now = ai_now_ms();
        let mut entries = Vec::new();
        if has_parts {
            entries.push(ai_transcript_entry(
                    format!("transcript-assistant-parts-{message_id}"),
                    conversation_id,
                    "assistant_part",
                    serde_json::json!({
                        "parts": parts,
                        "completeTurnParts": true,
                    }),
                    Some(message_id.to_string()),
                    Some(message_id.to_string()),
                    now,
                ));
        }
        entries.push(ai_transcript_entry(
            format!("transcript-assistant-end-{message_id}"),
            conversation_id,
            "assistant_turn_end",
            serde_json::json!({
                "status": status,
                "messageId": message_id,
                "plainTextSummary": plain_text_summary,
                "toolRoundCount": tool_round_count,
            }),
            Some(message_id.to_string()),
            Some(message_id.to_string()),
            now,
        ));
        self.persist_ai_transcript_entries(conversation_id.to_string(), entries);
    }

    fn persist_ai_removed_assistant_turn_end(
        &self,
        conversation_id: &str,
        message_id: &str,
        status: &str,
    ) {
        let now = ai_now_ms();
        self.persist_ai_transcript_entries(
            conversation_id.to_string(),
            vec![ai_transcript_entry(
                format!("transcript-assistant-end-{message_id}"),
                conversation_id,
                "assistant_turn_end",
                serde_json::json!({
                    "status": status,
                    "messageId": message_id,
                    "plainTextSummary": "",
                    "toolRoundCount": 0,
                }),
                Some(message_id.to_string()),
                Some(message_id.to_string()),
                now,
            )],
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_ai_assistant_round(
        &self,
        conversation_id: &str,
        message_id: &str,
        round_id: String,
        round_number: i64,
        response_length: usize,
        tool_call_ids: Vec<String>,
        synthetic: bool,
        retry_attempt: Option<usize>,
        hard_deny_triggered: bool,
    ) {
        let now = ai_now_ms();
        let mut transcript_entries = Vec::new();
        if !tool_call_ids.is_empty() || synthetic {
            transcript_entries.push(ai_transcript_entry(
                format!("transcript-assistant-round-{round_id}"),
                conversation_id,
                "assistant_round",
                serde_json::json!({
                    "round": round_number,
                    "roundId": round_id,
                    "synthetic": synthetic,
                    "retryAttempt": retry_attempt,
                    "toolCallIds": tool_call_ids,
                }),
                Some(message_id.to_string()),
                Some(message_id.to_string()),
                now,
            ));
        }
        self.persist_ai_transcript_entries(conversation_id.to_string(), transcript_entries);
        self.persist_ai_diagnostic_events(
            conversation_id.to_string(),
            vec![ai_diagnostic_event(
                format!("diagnostic-assistant-round-{round_id}"),
                conversation_id,
                "assistant_round",
                Some(message_id.to_string()),
                Some(round_id.clone()),
                now,
                self.ai_diagnostic_base(serde_json::json!({
                    "logicalRound": round_number,
                    "responseLength": response_length,
                    "toolCallCount": tool_call_ids.len(),
                    "toolRoundIds": [round_id],
                    "synthetic": synthetic,
                    "retryAttempt": retry_attempt,
                    "hardDenyTriggered": hard_deny_triggered,
                })),
            )],
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_ai_summary_created(
        &self,
        conversation_id: &str,
        message_id: &str,
        summary_kind: &str,
        summary_text: &str,
        round_id: Option<String>,
        transcript_ref: Option<serde_json::Value>,
        compacted_message_count: Option<usize>,
        compacted_until_message_id: Option<String>,
        source: Option<&str>,
        timestamp: i64,
    ) {
        let round_id_for_diagnostic = round_id.clone();
        self.persist_ai_transcript_entries(
            conversation_id.to_string(),
            vec![ai_transcript_entry(
                format!("transcript-summary-created-{message_id}"),
                conversation_id,
                "summary_created",
                serde_json::json!({
                    "messageId": message_id,
                    "roundId": round_id,
                    "summaryText": summary_text,
                    "summaryKind": summary_kind,
                    "sourceStartEntryId": transcript_ref
                        .as_ref()
                        .and_then(|value| value.get("startEntryId"))
                        .and_then(serde_json::Value::as_str),
                    "sourceEndEntryId": transcript_ref
                        .as_ref()
                        .and_then(|value| value.get("endEntryId"))
                        .and_then(serde_json::Value::as_str),
                    "source": source,
                    "summarizationMode": source,
                    "compactedMessageCount": compacted_message_count,
                    "compactedUntilMessageId": compacted_until_message_id,
                }),
                Some(message_id.to_string()),
                Some(message_id.to_string()),
                timestamp,
            )],
        );
        self.persist_ai_diagnostic_events(
            conversation_id.to_string(),
            vec![ai_diagnostic_event(
                format!("diagnostic-summary-created-{message_id}"),
                conversation_id,
                "compaction_completed",
                Some(message_id.to_string()),
                None,
                timestamp,
                self.ai_diagnostic_base(serde_json::json!({
                    "roundId": round_id_for_diagnostic,
                    "summaryKind": summary_kind,
                    "summaryLength": summary_text.len(),
                    "compactedMessageCount": compacted_message_count,
                    "source": source,
                })),
            )],
        );
    }
}
