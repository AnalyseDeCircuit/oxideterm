impl AiWorkspaceEntity {
    fn apply_compaction_result(
        &mut self,
        conversation_id: &str,
        base_ids: &[String],
        plan: AiCompactionPlan,
        summary: &str,
        anchor_id: &str,
        now_ms: i64,
    ) -> Option<(serde_json::Value, Option<String>, Option<String>, usize)> {
        let conversation = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)?;
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
            return None;
        }
        let appended = conversation
            .messages
            .iter()
            .skip(base_ids.len())
            .cloned()
            .collect::<Vec<_>>();
        let summary_source_transcript_ref =
            ai_summary_source_transcript_ref(&plan.compact_messages, conversation_id);
        let summary_round_id = ai_latest_summary_round_id(&plan.compact_messages);
        let compacted_until_entry_id =
            ai_transcript_boundary_id(plan.compact_messages.last(), "end");
        let total_compacted = ai_compaction_original_count(&plan.compact_messages);
        let total_compacted_turns = ai_conversation_turn_count(&plan.compact_messages);
        let snapshot_messages = ai_compaction_anchor_snapshot(&plan.compact_messages);
        let summary_entry_id = format!("transcript-summary-created-{anchor_id}");
        let transcript_ref = serde_json::json!({
            "conversationId": conversation_id,
            "endEntryId": summary_entry_id,
        });
        let summary_ref = serde_json::json!({
            "kind": "compaction",
            "roundId": summary_round_id,
            "transcriptRef": summary_source_transcript_ref,
        });
        let anchor = AiChatMessage {
            id: anchor_id.to_string(),
            role: AiChatRole::System,
            content: summary.to_string(),
            timestamp_ms: now_ms,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: Some(AiChatMessageMetadata {
                kind: "compaction-anchor".to_string(),
                original_count: Some(total_compacted),
                compacted_at_ms: Some(now_ms),
                original_messages: Some(snapshot_messages),
                original_user_count: Some(total_compacted_turns),
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
        conversation.updated_at_ms = now_ms;
        conversation.message_count = conversation.messages.len();
        conversation.turn_count = ai_conversation_turn_count(&conversation.messages);
        let metadata = conversation
            .session_metadata
            .get_or_insert_with(|| serde_json::json!({ "conversationId": conversation_id }));
        if let Some(object) = metadata.as_object_mut() {
            object.insert(
                "conversationId".to_string(),
                serde_json::json!(conversation_id),
            );
            object.insert("lastSummaryAt".to_string(), serde_json::json!(now_ms));
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
        self.persist_chat_state();
        Some((
            summary_source_transcript_ref,
            summary_round_id,
            compacted_until_entry_id,
            total_compacted,
        ))
    }

    fn apply_conversation_summary(
        &mut self,
        conversation_id: &str,
        base_ids: &[String],
        summary_id: &str,
        summary: &str,
        prefix: &str,
        now_ms: i64,
    ) -> Option<(serde_json::Value, Option<String>)> {
        let conversation = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)?;
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
            return None;
        }
        let summary_source_transcript_ref =
            ai_summary_source_transcript_ref(&conversation.messages, conversation_id);
        let summary_round_id = ai_latest_summary_round_id(&conversation.messages);
        let original_user_count = ai_conversation_turn_count(&conversation.messages);
        let summary_entry_id = format!("transcript-summary-created-{summary_id}");
        let transcript_ref = serde_json::json!({
            "conversationId": conversation_id,
            "endEntryId": summary_entry_id,
        });
        // Tauri keeps a source transcript reference on manual summaries so
        // later prompt compaction can ask the model to trust the visible summary.
        let summary_ref = serde_json::json!({
            "kind": "conversation",
            "roundId": summary_round_id,
            "transcriptRef": summary_source_transcript_ref,
            "originalUserCount": original_user_count,
        });
        conversation.messages = vec![AiChatMessage {
            id: summary_id.to_string(),
            role: AiChatRole::Assistant,
            content: format!("\u{1f4cb} **{prefix}**\n\n{summary}"),
            timestamp_ms: now_ms,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: Some(transcript_ref),
            summary_ref: Some(summary_ref),
            branches: None,
            suggestions: Vec::new(),
        }];
        let metadata = conversation
            .session_metadata
            .get_or_insert_with(|| serde_json::json!({ "conversationId": conversation_id }));
        if let Some(object) = metadata.as_object_mut() {
            object.insert("lastSummaryAt".to_string(), serde_json::json!(now_ms));
            if let Some(summary_round_id) = summary_round_id.as_deref() {
                object.insert(
                    "lastSummaryRoundId".to_string(),
                    serde_json::json!(summary_round_id),
                );
            }
        }
        conversation.updated_at_ms = now_ms;
        conversation.message_count = conversation.messages.len();
        conversation.turn_count = ai_conversation_turn_count(&conversation.messages);
        self.persist_chat_state();
        Some((summary_source_transcript_ref, summary_round_id))
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn finish_ai_compaction(
        &mut self,
        conversation_id: String,
        base_ids: Vec<String>,
        plan: AiCompactionPlan,
        summary: String,
        failed: bool,
        resume_after: Option<AiPendingChatStream>,
        silent: bool,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity
            .update(cx, |ai, _cx| ai.finish_compaction(&conversation_id));
        if failed {
            if silent {
                self.ai_entity.update(cx, |ai, cx| {
                    ai.clear_compaction_notice_for(&conversation_id, cx);
                });
            }
            if !silent {
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.ai.acp_agent_error_unknown"),
                    TerminalNoticeVariant::Error,
                    cx,
                );
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        }
        if summary.trim().is_empty() {
            if silent {
                self.ai_entity.update(cx, |ai, cx| {
                    ai.clear_compaction_notice_for(&conversation_id, cx);
                });
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        }
        let now = ai_now_ms();
        let anchor_id = self.next_ai_chat_id(now, cx);
        let compaction_result = self.ai_entity.update(cx, |ai, _cx| {
            ai.apply_compaction_result(
                &conversation_id,
                &base_ids,
                plan,
                &summary,
                &anchor_id,
                now,
            )
        });
        let Some((
            summary_source_transcript_ref,
            summary_round_id,
            compacted_until_entry_id,
            total_compacted,
        )) = compaction_result
        else {
            if silent {
                self.ai_entity.update(cx, |ai, cx| {
                    ai.clear_compaction_notice_for(&conversation_id, cx);
                });
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        };
        self.persist_ai_summary_created(
            &conversation_id,
            &anchor_id,
            "compaction",
            &summary,
            summary_round_id,
            Some(summary_source_transcript_ref),
            Some(total_compacted),
            compacted_until_entry_id,
            Some(if silent { "background" } else { "manual" }),
            now,
            cx,
        );
        if silent {
            self.ai_entity.update(cx, |ai, cx| {
                ai.set_compaction_notice_done(&conversation_id, total_compacted, cx);
            });
        }
        self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn resume_ai_chat_after_pre_send_compaction(
        &mut self,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = resume_after else {
            return;
        };
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

    pub(in crate::workspace) fn finish_ai_summary(
        &mut self,
        conversation_id: String,
        base_ids: Vec<String>,
        summary: String,
        failed: bool,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity
            .update(cx, |ai, _cx| ai.finish_compaction(&conversation_id));
        self.ai_entity.update(cx, |ai, _cx| ai.set_chat_loading(false));
        if failed {
            self.push_ai_settings_toast(
                self.i18n.t("settings_view.ai.acp_agent_error_unknown"),
                TerminalNoticeVariant::Error,
                cx,
            );
            cx.notify();
            return;
        }
        if summary.trim().is_empty() {
            cx.notify();
            return;
        }
        let now = ai_now_ms();
        let summary_id = self.next_ai_chat_id(now, cx);
        let original_count = base_ids.len();
        let prefix = self
            .i18n
            .t("ai.context.summary_prefix")
            .replace("{{count}}", &original_count.to_string());
        let summary_result = self.ai_entity.update(cx, |ai, _cx| {
            ai.apply_conversation_summary(
                &conversation_id,
                &base_ids,
                &summary_id,
                &summary,
                &prefix,
                now,
            )
        });
        let Some((summary_source_transcript_ref, summary_round_id)) = summary_result else {
            cx.notify();
            return;
        };
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_model_switch_warning(None);
        });
        self.persist_ai_summary_created(
            &conversation_id,
            &summary_id,
            "conversation",
            &summary,
            summary_round_id,
            Some(summary_source_transcript_ref),
            Some(original_count),
            None,
            Some("manual"),
            now,
            cx,
        );
        cx.notify();
    }
}
