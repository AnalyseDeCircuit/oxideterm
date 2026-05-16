impl WorkspaceApp {
    fn start_ai_compact_conversation(&mut self, cx: &mut Context<Self>) {
        let Some(conversation_id) = self
            .ai_chat
            .active_conversation()
            .map(|conversation| conversation.id.clone())
        else {
            return;
        };
        self.start_ai_compact_conversation_for(conversation_id, false, true, None, cx);
    }

    fn maybe_start_ai_auto_compaction(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        if self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .is_none_or(|conversation| conversation.messages.len() < 6)
        {
            return;
        }
        self.start_ai_compact_conversation_for(conversation_id.to_string(), true, false, None, cx);
    }

    fn start_ai_compact_conversation_for(
        &mut self,
        conversation_id: String,
        silent: bool,
        force: bool,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) -> bool {
        let conversation = match self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        {
            Some(conversation) if conversation.messages.len() >= 4 => conversation.clone(),
            _ => return false,
        };
        if !self
            .ai_compacting_conversations
            .insert(conversation.id.clone())
        {
            return false;
        }

        let config = match self.resolve_ai_stream_config() {
            Ok(config) => config,
            Err(error) => {
                self.ai_compacting_conversations.remove(&conversation.id);
                if !silent {
                    self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                }
                return false;
            }
        };
        let context_window = self.ai_active_model_context_window(&config);
        if silent && !force {
            let total_tokens = conversation
                .messages
                .iter()
                .map(ai_message_estimated_tokens)
                .sum::<usize>();
            let reserve = ai_response_reserve(context_window);
            let prompt_budget = compute_ai_prompt_budget(context_window, reserve, 0, None);
            let auto_compact_threshold = if prompt_budget.usable_prompt_budget > 0 {
                (context_window as f32 * AI_COMPACTION_TRIGGER_THRESHOLD)
                    / prompt_budget.usable_prompt_budget as f32
            } else {
                AI_COMPACTION_TRIGGER_THRESHOLD
            };
            let decision = determine_ai_compression_level(AiPromptBudgetInput {
                context_window,
                response_reserve: reserve,
                system_budget: 0,
                history_tokens: total_tokens,
                trimmable_history_tokens: None,
                summary_eligible_tokens: Some(total_tokens),
                can_summarize: true,
                can_lookup_transcript: false,
                in_tool_loop: false,
                auto_compact_threshold: Some(auto_compact_threshold),
                transcript_lookup_threshold: None,
                tool_loop_stop_threshold: None,
                safety_margin: None,
            });
            if decision.level < 2 {
                self.ai_compacting_conversations.remove(&conversation.id);
                return false;
            }
        }
        let Some(plan) = ai_compaction_plan(&conversation.messages, context_window) else {
            self.ai_compacting_conversations.remove(&conversation.id);
            return false;
        };
        let summary_messages = ai_compaction_summary_messages(&plan.compact_messages);
        let conversation_id = conversation.id.clone();
        let base_ids = conversation
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (ui_tx, ui_rx) = std::sync::mpsc::channel();
        if resume_after.is_some() {
            self.ai_pending_chat_after_compaction = resume_after.clone();
        }
        self.ai_compaction_rx = Some(ui_rx);
        self.forwarding_runtime
            .spawn(stream_chat_completion(config, summary_messages, tx));
        self.forwarding_runtime.spawn(async move {
            let mut summary = String::new();
            let mut stream_error = None;
            while let Some(event) = rx.recv().await {
                match event {
                    AiStreamEvent::Content(chunk) => {
                        summary.push_str(&chunk);
                    }
                    AiStreamEvent::Thinking(_)
                    | AiStreamEvent::ToolCall { .. }
                    | AiStreamEvent::ToolCallComplete { .. } => {}
                    AiStreamEvent::Done => break,
                    AiStreamEvent::Error(error) => {
                        stream_error = Some(error);
                        break;
                    }
                }
            }
            let _ = ui_tx.send(AiCompactionDelivery {
                kind: AiCompactionDeliveryKind::Compact,
                conversation_id,
                base_ids,
                plan: Some(plan),
                summary,
                stream_error,
                resume_after,
            });
        });
        self.schedule_ai_compaction_poll(cx);
        true
    }

    fn start_ai_summarize_conversation(&mut self, cx: &mut Context<Self>) {
        let conversation = match self.ai_chat.active_conversation() {
            Some(conversation) if conversation.messages.len() >= 4 => conversation.clone(),
            _ => return,
        };
        if !self
            .ai_compacting_conversations
            .insert(conversation.id.clone())
        {
            return;
        }

        let config = match self.resolve_ai_stream_config() {
            Ok(config) => config,
            Err(error) => {
                self.ai_compacting_conversations.remove(&conversation.id);
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                return;
            }
        };
        let summary_messages = ai_conversation_summary_messages(&conversation.messages);
        let conversation_id = conversation.id.clone();
        let base_ids = conversation
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (ui_tx, ui_rx) = std::sync::mpsc::channel();
        self.ai_chat_loading = true;
        self.ai_compaction_rx = Some(ui_rx);
        self.forwarding_runtime
            .spawn(stream_chat_completion(config, summary_messages, tx));
        self.forwarding_runtime.spawn(async move {
            let mut summary = String::new();
            let mut stream_error = None;
            while let Some(event) = rx.recv().await {
                match event {
                    AiStreamEvent::Content(chunk) => summary.push_str(&chunk),
                    AiStreamEvent::Thinking(_)
                    | AiStreamEvent::ToolCall { .. }
                    | AiStreamEvent::ToolCallComplete { .. } => {}
                    AiStreamEvent::Done => break,
                    AiStreamEvent::Error(error) => {
                        stream_error = Some(error);
                        break;
                    }
                }
            }
            let _ = ui_tx.send(AiCompactionDelivery {
                kind: AiCompactionDeliveryKind::Summary,
                conversation_id,
                base_ids,
                plan: None,
                summary,
                stream_error,
                resume_after: None,
            });
        });
        self.schedule_ai_compaction_poll(cx);
        cx.notify();
    }
}
