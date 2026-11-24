use std::collections::HashMap;

use oxideterm_ai::{
    AiChatMessage, AiChatMessageMetadata, AiChatRole, AiChatStreamConfig, AiMessageBranches,
    ai_conversation_turn_count,
    stream_state::{AiCompactionPlan, ai_compaction_original_count, ai_latest_summary_round_id},
};

use super::{AiWorkspaceEntity, ai_message_backend_for_stream};
use crate::transcript::{ai_summary_source_transcript_ref, ai_transcript_boundary_id};

impl AiWorkspaceEntity {
    pub fn create_chat_conversation(
        &mut self,
        id: String,
        title: Option<String>,
        now_ms: i64,
    ) -> String {
        self.stash_active_chat_draft();
        let id = self.create_conversation(id, title, now_ms, None);
        self.persist_chat_state();
        #[cfg(not(target_family = "wasm"))]
        if let Some(mut context) = oxideterm_audit::AuditContext::current_request()
            .or_else(oxideterm_audit::AuditContext::current)
        {
            if context.source == oxideterm_audit::AuditSource::Application {
                context.source = oxideterm_audit::AuditSource::User;
            }
            context.target = Some(oxideterm_audit::redact(&id));
            context.observe(
                oxideterm_audit::AuditCategory::Automation,
                "ai_conversation_create",
                None,
                oxideterm_audit::AuditOutcome::Sent,
                oxideterm_audit::AuditEvidence::Dispatch,
                oxideterm_audit::AuditAuthorization::NotRequired,
            );
        }
        id
    }

    pub fn begin_sidebar_user_turn(
        &mut self,
        conversation_id: String,
        message: AiChatMessage,
        stream_config: &AiChatStreamConfig,
        active_participant: Option<String>,
    ) -> String {
        let message_id = message.id.clone();
        let backend = ai_message_backend_for_stream(stream_config);
        self.add_message(&conversation_id, message);
        if let Some(conversation) = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        {
            let metadata = conversation
                .session_metadata
                .get_or_insert_with(|| serde_json::json!({ "conversationId": conversation_id }));
            if let Some(object) = metadata.as_object_mut() {
                object.insert(
                    "conversationId".to_string(),
                    serde_json::json!(conversation_id),
                );
                object.insert("origin".to_string(), serde_json::json!("sidebar"));
                object.insert(
                    "providerId".to_string(),
                    serde_json::json!(stream_config.provider_id),
                );
                object.insert(
                    "providerModel".to_string(),
                    serde_json::json!(stream_config.model),
                );
                if let Some(participant) = active_participant {
                    object.insert(
                        "activeParticipant".to_string(),
                        serde_json::json!(participant),
                    );
                }
            }
            // Provenance is safe structural metadata used to avoid replaying
            // messages that the selected ACP session already owns.
            oxideterm_ai::store_ai_message_backend_provenance(conversation, &message_id, backend);
        }
        // Save the message and session metadata as one projection.
        self.persist_chat_state();
        conversation_id
    }

    pub fn add_help_exchange(
        &mut self,
        candidate_id: String,
        title: String,
        now_ms: i64,
        user_message: AiChatMessage,
        assistant_message: AiChatMessage,
    ) {
        let conversation_id = self.ensure_conversation(candidate_id, Some(title), now_ms, None);
        self.add_message(&conversation_id, user_message);
        self.add_message(&conversation_id, assistant_message);
        self.persist_chat_state();
    }

    pub fn truncate_conversation_after_last_user(
        &mut self,
        id: &str,
        now_ms: i64,
    ) -> Option<String> {
        self.capture_history_before_edit(id);
        let conversation = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == id)?;
        let last_user_index = conversation
            .messages
            .iter()
            .rposition(|message| message.role == AiChatRole::User)?;
        conversation.messages.truncate(last_user_index + 1);
        conversation.message_count = conversation.messages.len();
        conversation.turn_count = ai_conversation_turn_count(&conversation.messages);
        conversation.updated_at_ms = now_ms;
        let conversation_id = conversation.id.clone();
        self.replace_history_tail(&conversation_id, last_user_index + 1, None);
        self.persist_chat_state();
        Some(conversation_id)
    }

    pub fn delete_active_message(&mut self, message_id: &str, now_ms: i64) -> bool {
        let history_id = self.conversation_state().active_conversation_id.clone();
        if let Some(id) = &history_id {
            self.capture_history_before_edit(id);
        }
        let Some(conversation) = self.conversation_state_mut().active_conversation_mut() else {
            return false;
        };
        let Some(removed) = conversation
            .messages
            .iter()
            .find(|message| message.id == message_id)
        else {
            return false;
        };
        let removed_turns = ai_conversation_turn_count(std::slice::from_ref(removed));
        let original_len = conversation.messages.len();
        conversation
            .messages
            .retain(|message| message.id != message_id);
        if conversation.messages.len() == original_len {
            return false;
        }
        conversation.message_count = conversation.message_count.saturating_sub(1);
        conversation.turn_count = conversation.turn_count.saturating_sub(removed_turns);
        conversation.updated_at_ms = now_ms;
        let conversation_id = conversation.id.clone();
        self.history_message_deleted(&conversation_id, message_id);
        self.persist_chat_state();
        true
    }

    pub fn replace_active_user_message(
        &mut self,
        message_id: &str,
        new_user_id: String,
        edited_content: String,
        model: String,
        backend: oxideterm_ai::AiMessageBackendProvenance,
        now_ms: i64,
    ) -> Option<String> {
        let history_id = self.conversation_state().active_conversation_id.clone();
        if let Some(id) = &history_id {
            self.capture_history_before_edit(id);
        }
        let previous_branch = history_id
            .as_ref()
            .and_then(|id| self.history.branches.get(id))
            .cloned()
            .unwrap_or_else(|| "main".into());
        let conversation = self.conversation_state_mut().active_conversation_mut()?;
        let message_index = conversation
            .messages
            .iter()
            .position(|message| message.id == message_id)?;
        if conversation.messages.get(message_index)?.role != AiChatRole::User {
            return None;
        }
        let current_tail = oxideterm_ai::AiHistoryRange {
            branch_id: previous_branch,
            first_message_id: Some(message_id.into()),
            last_message_id: None,
        };
        let original = conversation.messages.get_mut(message_index)?;
        let branches = match original.branches.take() {
            Some(mut branches) => {
                branches.refs.insert(branches.active_index, current_tail);
                branches.total = branches.total.saturating_add(1);
                branches.active_index = branches.total.saturating_sub(1);
                branches
            }
            None => AiMessageBranches {
                refs: HashMap::from([(0, current_tail)]),
                total: 2,
                active_index: 1,
                tails: HashMap::new(),
            },
        };
        let context = original.context.take();
        let images = oxideterm_ai::message_images(original).to_vec();
        conversation.messages.truncate(message_index);
        conversation.messages.push(AiChatMessage {
            id: new_user_id.clone(),
            role: AiChatRole::User,
            content: edited_content,
            timestamp_ms: now_ms,
            model: Some(model),
            context,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: Some(branches),
            suggestions: Vec::new(),
        });
        oxideterm_ai::set_message_images(conversation.messages.last_mut()?, images);
        // Editing starts a new turn, so the replacement must carry the backend
        // owner selected for that turn instead of inheriting the removed branch.
        oxideterm_ai::store_ai_message_backend_provenance(conversation, &new_user_id, backend);
        conversation.message_count = conversation.messages.len();
        conversation.turn_count = ai_conversation_turn_count(&conversation.messages);
        conversation.updated_at_ms = now_ms;
        let conversation_id = conversation.id.clone();
        self.replace_history_tail(&conversation_id, message_index, Some(message_id));
        self.persist_chat_state();
        Some(conversation_id)
    }

    pub fn switch_active_message_branch(
        &mut self,
        message_id: &str,
        branch_index: usize,
        now_ms: i64,
    ) -> bool {
        let history_id = self.conversation_state().active_conversation_id.clone();
        if let Some(id) = &history_id {
            self.capture_history_before_edit(id);
        }
        let previous_branch = history_id
            .as_ref()
            .and_then(|id| self.history.branches.get(id))
            .cloned()
            .unwrap_or_else(|| "main".into());
        let Some(conversation) = self.conversation_state_mut().active_conversation_mut() else {
            return false;
        };
        let Some(message_index) = conversation
            .messages
            .iter()
            .position(|message| message.id == message_id)
        else {
            return false;
        };
        let Some(branches) = conversation.messages[message_index].branches.as_ref() else {
            return false;
        };
        if branch_index >= branches.total || branch_index == branches.active_index {
            return false;
        }
        if !branches
            .tails
            .get(&branch_index)
            .is_some_and(|target_tail| !target_tail.is_empty())
        {
            return false;
        }
        let live_tail = oxideterm_ai::AiHistoryRange {
            branch_id: previous_branch,
            first_message_id: Some(message_id.into()),
            last_message_id: None,
        };
        // This branch payload belongs to the message being replaced, so move it
        // instead of cloning every stored branch and message tail.
        let mut branches = conversation.messages[message_index]
            .branches
            .take()
            .expect("validated branch metadata must remain present");
        let target_tail = branches
            .tails
            .remove(&branch_index)
            .expect("validated branch tail must remain present");
        branches.refs.remove(&branch_index);
        branches.refs.insert(branches.active_index, live_tail);
        branches.active_index = branch_index;
        let mut new_messages = conversation.messages[..message_index].to_vec();
        let mut updated_branches = Some(branches);
        for (index, mut message) in target_tail.into_iter().enumerate() {
            message.branches = if index == 0 {
                updated_branches.take()
            } else {
                None
            };
            new_messages.push(message);
        }
        conversation.messages = new_messages;
        conversation.message_count = conversation.messages.len();
        conversation.turn_count = ai_conversation_turn_count(&conversation.messages);
        conversation.updated_at_ms = now_ms;
        let conversation_id = conversation.id.clone();
        self.replace_history_tail(&conversation_id, message_index, Some(message_id));
        self.persist_chat_state();
        true
    }
}

impl AiWorkspaceEntity {
    pub fn begin_assistant_turn(
        &mut self,
        conversation_id: &str,
        message: AiChatMessage,
        budget_level: u8,
        backend: oxideterm_ai::AiMessageBackendProvenance,
    ) {
        let message_id = message.id.clone();
        self.add_message(conversation_id, message);
        if let Some(conversation) = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        {
            let metadata = conversation
                .session_metadata
                .get_or_insert_with(|| serde_json::json!({ "conversationId": conversation_id }));
            if let Some(object) = metadata.as_object_mut() {
                object.insert(
                    "conversationId".to_string(),
                    serde_json::json!(conversation_id),
                );
                object.insert("origin".to_string(), serde_json::json!("sidebar"));
                object.insert(
                    "lastBudgetLevel".to_string(),
                    serde_json::json!(budget_level),
                );
            }
            // The backend owner is persisted separately from display model
            // text so mixed provider/ACP history can be synchronized exactly.
            oxideterm_ai::store_ai_message_backend_provenance(conversation, &message_id, backend);
        }
    }
}

impl AiWorkspaceEntity {
    pub fn apply_compaction_result(
        &mut self,
        conversation_id: &str,
        base_ids: &[String],
        plan: AiCompactionPlan,
        summary: &str,
        anchor_id: &str,
        now_ms: i64,
    ) -> Option<(serde_json::Value, Option<String>, Option<String>, usize)> {
        let through_message = plan.compact_messages.last()?.id.clone();
        let (source_branch, source) = self.history.compaction_sources.remove(conversation_id)?;
        if self
            .history
            .branches
            .get(conversation_id)
            .is_some_and(|branch| branch != &source_branch)
            || !source
                .messages
                .iter()
                .map(|message| &message.id)
                .eq(base_ids.iter())
        {
            return None;
        }
        self.capture_history_before_edit(conversation_id);
        let conversation = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)?;
        let last_source = base_ids.last()?;
        let suffix_start = conversation
            .messages
            .iter()
            .position(|message| &message.id == last_source)
            .map(|index| index + 1)
            .unwrap_or(conversation.messages.len());
        let appended = conversation
            .messages
            .iter()
            .skip(suffix_start)
            .cloned()
            .collect::<Vec<_>>();
        let summary_source_transcript_ref =
            ai_summary_source_transcript_ref(&plan.compact_messages, conversation_id);
        let summary_round_id = ai_latest_summary_round_id(&plan.compact_messages);
        let compacted_until_entry_id =
            ai_transcript_boundary_id(plan.compact_messages.last(), "end");
        let total_compacted = ai_compaction_original_count(&plan.compact_messages);
        let total_compacted_turns = ai_conversation_turn_count(&plan.compact_messages);
        let original_ref = oxideterm_ai::AiHistoryRange {
            branch_id: source_branch,
            first_message_id: plan
                .compact_messages
                .first()
                .map(|message| message.id.clone()),
            last_message_id: plan
                .compact_messages
                .last()
                .map(|message| message.id.clone()),
        };
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
                original_ref: Some(original_ref),
                original_messages: None,
                original_user_count: Some(total_compacted_turns),
                images: Vec::new(),
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
        if let Some(conversation) = self
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        {
            self.history
                .model_contexts
                .insert(conversation_id.to_owned(), conversation.clone());
        }
        self.compact_history(conversation_id, through_message);
        self.persist_chat_state();
        Some((
            summary_source_transcript_ref,
            summary_round_id,
            compacted_until_entry_id,
            total_compacted,
        ))
    }

    pub fn apply_conversation_summary(
        &mut self,
        conversation_id: &str,
        base_ids: &[String],
        summary_id: &str,
        summary: &str,
        prefix: &str,
        now_ms: i64,
    ) -> Option<(serde_json::Value, Option<String>)> {
        let through_message = base_ids.last()?.clone();
        let (source_branch, source) = self.history.compaction_sources.remove(conversation_id)?;
        if self
            .history
            .branches
            .get(conversation_id)
            .is_some_and(|branch| branch != &source_branch)
            || !source
                .messages
                .iter()
                .map(|message| &message.id)
                .eq(base_ids.iter())
        {
            return None;
        }
        self.capture_history_before_edit(conversation_id);
        let conversation = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)?;
        let summary_source_transcript_ref =
            ai_summary_source_transcript_ref(&source.messages, conversation_id);
        let summary_round_id = ai_latest_summary_round_id(&source.messages);
        let original_user_count = ai_conversation_turn_count(&source.messages);
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
        if let Some(conversation) = self
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        {
            self.history
                .model_contexts
                .insert(conversation_id.to_owned(), conversation.clone());
        }
        self.compact_history(conversation_id, through_message);
        self.persist_chat_state();
        Some((summary_source_transcript_ref, summary_round_id))
    }
}
