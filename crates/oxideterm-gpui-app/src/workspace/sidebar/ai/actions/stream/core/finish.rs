impl WorkspaceApp {
    fn finish_ai_compaction(
        &mut self,
        conversation_id: String,
        base_ids: Vec<String>,
        plan: AiCompactionPlan,
        summary: String,
        stream_error: Option<String>,
        resume_after: Option<AiPendingChatStream>,
        silent: bool,
        cx: &mut Context<Self>,
    ) {
        self.ai_compacting_conversations.remove(&conversation_id);
        if let Some(error) = stream_error {
            if silent {
                self.clear_ai_compaction_notice_for(&conversation_id, cx);
            }
            if !silent {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        }
        let summary = summary.trim();
        if summary.is_empty() {
            if silent {
                self.clear_ai_compaction_notice_for(&conversation_id, cx);
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        }
        let now = ai_now_ms();
        let anchor_id = self.next_ai_chat_id(now);
        let Some(conversation) = self
            .ai_chat
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            if silent {
                self.clear_ai_compaction_notice_for(&conversation_id, cx);
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        };
        let latest_ids = conversation
            .messages
            .iter()
            .take(base_ids.len())
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>();
        let stale = latest_ids.len() != base_ids.len()
            || latest_ids
                .iter()
                .zip(base_ids.iter())
                .any(|(latest, expected)| *latest != expected);
        if stale {
            if silent {
                self.clear_ai_compaction_notice_for(&conversation_id, cx);
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        }
        let appended = conversation
            .messages
            .iter()
            .skip(base_ids.len())
            .cloned()
            .collect::<Vec<_>>();
        let summary_source_transcript_ref =
            ai_summary_source_transcript_ref(&plan.compact_messages, &conversation_id);
        let summary_round_id = ai_latest_summary_round_id(&plan.compact_messages);
        let compacted_until_entry_id =
            ai_transcript_boundary_id(plan.compact_messages.last(), "end");
        let total_compacted = ai_compaction_original_count(&plan.compact_messages);
        let snapshot_messages = ai_compaction_anchor_snapshot(&plan.compact_messages);
        let summary_entry_id = format!("transcript-summary-created-{anchor_id}");
        let transcript_ref = serde_json::json!({
            "conversationId": conversation_id.clone(),
            "endEntryId": summary_entry_id,
        });
        let summary_ref = serde_json::json!({
            "kind": "compaction",
            "roundId": summary_round_id.clone(),
            "transcriptRef": summary_source_transcript_ref.clone(),
        });
        let anchor = AiChatMessage {
            id: anchor_id.clone(),
            role: AiChatRole::System,
            content: summary.to_string(),
            timestamp_ms: now,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: Some(AiChatMessageMetadata {
                kind: "compaction-anchor".to_string(),
                original_count: Some(total_compacted),
                compacted_at_ms: Some(now),
                original_messages: Some(snapshot_messages),
            }),
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: Some(transcript_ref),
            summary_ref: Some(summary_ref),
            branches: None,
            suggestions: Vec::new(),
        };
        conversation.messages = std::iter::once(anchor)
            .chain(plan.keep_messages)
            .chain(appended)
            .collect();
        conversation.updated_at_ms = now;
        let metadata = conversation
            .session_metadata
            .get_or_insert_with(|| serde_json::json!({ "conversationId": conversation_id }));
        if let Some(object) = metadata.as_object_mut() {
            object.insert("conversationId".to_string(), serde_json::json!(conversation_id));
            object.insert("lastSummaryAt".to_string(), serde_json::json!(now));
            if let Some(compacted_until_entry_id) = compacted_until_entry_id.as_deref() {
                object.insert(
                    "lastCompactedUntilEntryId".to_string(),
                    serde_json::json!(compacted_until_entry_id),
                );
            }
            if let Some(summary_round_id) = summary_round_id.as_deref() {
                object.insert(
                    "lastSummaryRoundId".to_string(),
                    serde_json::json!(summary_round_id),
                );
            }
        }
        self.persist_ai_chat_state();
        self.persist_ai_summary_created(
            &conversation_id,
            &anchor_id,
            "compaction",
            summary,
            summary_round_id.clone(),
            Some(summary_source_transcript_ref),
            Some(total_compacted),
            compacted_until_entry_id,
            Some(if silent { "background" } else { "manual" }),
            now,
        );
        if silent {
            self.set_ai_compaction_notice_done(&conversation_id, total_compacted, cx);
        }
        self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
        cx.notify();
    }

    fn resume_ai_chat_after_pre_send_compaction(
        &mut self,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) {
        let pending = resume_after.or_else(|| self.ai_pending_chat_after_compaction.take());
        let Some(pending) = pending else {
            return;
        };
        self.ai_pending_chat_after_compaction = None;
        self.start_ai_chat_stream_after_budget_preflight(
            pending.conversation_id,
            pending.config,
            pending.request_content,
            pending.task_system_prompt,
            pending.rag_system_prompt,
            false,
            cx,
        );
    }

    fn finish_ai_summary(
        &mut self,
        conversation_id: String,
        base_ids: Vec<String>,
        summary: String,
        stream_error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai_compacting_conversations.remove(&conversation_id);
        self.ai_chat_loading = false;
        if let Some(error) = stream_error {
            self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            cx.notify();
            return;
        }
        let summary = summary.trim();
        if summary.is_empty() {
            cx.notify();
            return;
        }
        let now = ai_now_ms();
        let summary_id = self.next_ai_chat_id(now);
        let original_count = base_ids.len();
        let prefix = self
            .i18n
            .t("ai.context.summary_prefix")
            .replace("{{count}}", &original_count.to_string());
        let Some(conversation) = self
            .ai_chat
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            cx.notify();
            return;
        };
        let latest_ids = conversation
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>();
        let stale = latest_ids.len() != base_ids.len()
            || latest_ids
                .iter()
                .zip(base_ids.iter())
                .any(|(latest, expected)| *latest != expected);
        if stale {
            cx.notify();
            return;
        }
        conversation.messages = vec![AiChatMessage {
            id: summary_id.clone(),
            role: AiChatRole::Assistant,
            content: format!("\u{1f4cb} **{prefix}**\n\n{summary}"),
            timestamp_ms: now,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: Some(serde_json::json!({ "kind": "conversation" })),
            branches: None,
            suggestions: Vec::new(),
        }];
        let metadata = conversation
            .session_metadata
            .get_or_insert_with(|| serde_json::json!({ "conversationId": conversation_id }));
        if let Some(object) = metadata.as_object_mut() {
            object.insert("lastSummaryAt".to_string(), serde_json::json!(now));
        }
        conversation.updated_at_ms = now;
        self.ai_model_switch_warning_percentage = None;
        self.persist_ai_chat_state();
        self.persist_ai_summary_created(
            &conversation_id,
            &summary_id,
            "conversation",
            summary,
            None,
            None,
            Some(original_count),
            None,
            Some("manual"),
            now,
        );
        cx.notify();
    }

}
