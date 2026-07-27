impl WorkspaceApp {
    pub(in crate::workspace) fn start_ai_compact_conversation(&mut self, cx: &mut Context<Self>) {
        let Some(conversation_id) = self
            .ai
            .chat
            .conversation_state
            .active_conversation()
            .map(|conversation| conversation.id.clone())
        else {
            return;
        };
        let _ = self.start_ai_compact_conversation_for(conversation_id, false, true, None, cx);
    }

    pub(in crate::workspace) fn maybe_start_ai_auto_compaction(
        &mut self,
        conversation_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self
            .ai
            .chat
            .conversation_state
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .is_none_or(|conversation| conversation.messages.len() < 6)
        {
            return;
        }
        let _ = self.start_ai_compact_conversation_for(
            conversation_id.to_string(),
            true,
            false,
            None,
            cx,
        );
    }

    pub(in crate::workspace) fn start_ai_compact_conversation_for(
        &mut self,
        conversation_id: String,
        silent: bool,
        force: bool,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) -> Result<(), Option<AiPendingChatStream>> {
        // Return an unconsumed pre-send request when compaction is skipped so
        // its zeroizing provider configuration never needs to be cloned.
        let conversation = match self
            .ai
            .chat
            .conversation_state
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        {
            Some(conversation) if conversation.messages.len() >= 4 => conversation.clone(),
            _ => return Err(resume_after),
        };
        if !self
            .ai_entity
            .update(cx, |ai, _cx| ai.begin_compaction(&conversation.id))
        {
            return Err(resume_after);
        }

        let config = match self.resolve_ai_summary_stream_config(true) {
            Ok(config) => config,
            Err(error) => {
                self.ai_entity
                    .update(cx, |ai, _cx| ai.finish_compaction(&conversation.id));
                if !silent {
                    self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                }
                return Err(resume_after);
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
                self.ai_entity
                    .update(cx, |ai, _cx| ai.finish_compaction(&conversation.id));
                return Err(resume_after);
            }
        }
        let Some(plan) = ai_compaction_plan(&conversation.messages, context_window, silent) else {
            self.ai_entity
                .update(cx, |ai, _cx| ai.finish_compaction(&conversation.id));
            return Err(resume_after);
        };
        if silent {
            self.ai_entity.update(cx, |ai, cx| {
                ai.set_compaction_notice_running(&conversation.id, cx);
            });
        }
        let summary_messages = ai_compaction_summary_messages(&plan.compact_messages);
        let conversation_id = conversation.id.clone();
        let base_ids = conversation
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let ui_tx = self.ai_entity.read(cx).compaction_sender();
        self.start_ai_compaction_stream_after_api_key_lookup(
            config,
            AiCompactionDeliveryKind::Compact,
            conversation_id,
            base_ids,
            Some(plan),
            summary_messages,
            resume_after,
            silent,
            tx,
            rx,
            ui_tx,
            cx,
        );
        Ok(())
    }

    pub(in crate::workspace) fn start_ai_summarize_conversation(&mut self, cx: &mut Context<Self>) {
        let conversation = match self.ai.chat.conversation_state.active_conversation() {
            Some(conversation) if conversation.messages.len() >= 4 => conversation.clone(),
            _ => return,
        };
        if !self
            .ai_entity
            .update(cx, |ai, _cx| ai.begin_compaction(&conversation.id))
        {
            return;
        }

        let config = match self.resolve_ai_summary_stream_config(false) {
            Ok(config) => config,
            Err(error) => {
                self.ai_entity
                    .update(cx, |ai, _cx| ai.finish_compaction(&conversation.id));
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
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let ui_tx = self.ai_entity.read(cx).compaction_sender();
        self.ai.chat.loading = true;
        self.start_ai_compaction_stream_after_api_key_lookup(
            config,
            AiCompactionDeliveryKind::Summary,
            conversation_id,
            base_ids,
            None,
            summary_messages,
            None,
            false,
            tx,
            rx,
            ui_tx,
            cx,
        );
        cx.notify();
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::workspace) fn start_ai_compaction_stream_after_api_key_lookup(
        &mut self,
        mut config: AiChatStreamConfig,
        kind: AiCompactionDeliveryKind,
        conversation_id: String,
        base_ids: Vec<String>,
        plan: Option<AiCompactionPlan>,
        summary_messages: Vec<AiChatMessage>,
        resume_after: Option<AiPendingChatStream>,
        silent: bool,
        tx: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
        rx: tokio::sync::mpsc::UnboundedReceiver<AiStreamEvent>,
        ui_tx: AiCompactionDeliverySender,
        cx: &mut Context<Self>,
    ) {
        let requires_key = ai_provider_chat_requires_key(&config.provider_type);
        let provider_id = config.provider_id.clone();
        let key_store = self.ai.models.key_store.clone();
        let runtime = self.forwarding_runtime.clone();
        let failed_to_get_key = self.i18n.t("ai.model_selector.failed_to_get_api_key");
        let api_key_not_found = self.i18n.t("ai.model_selector.api_key_not_found");
        cx.spawn(async move |weak, cx| {
            let key_result = if let Some(provider_id) = provider_id {
                runtime
                    .spawn_blocking(move || key_store.get_provider_key(&provider_id))
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|result| result.map_err(|error| error.to_string()))
            } else {
                Ok(None)
            };
            let _ = weak.update(cx, |this, cx| match key_result {
                Ok(api_key) => {
                    if requires_key && api_key.is_none() {
                        this.ai_entity
                            .update(cx, |ai, _cx| ai.finish_compaction(&conversation_id));
                        this.ai.chat.loading = false;
                        if silent {
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.clear_compaction_notice_for(&conversation_id, cx);
                            });
                        }
                        if !silent {
                            this.push_ai_settings_toast(
                                api_key_not_found,
                                TerminalNoticeVariant::Error,
                            );
                        }
                        this.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
                        cx.notify();
                        return;
                    }
                    config.api_key = api_key;
                    this.start_ai_compaction_stream_with_config(
                        config,
                        kind,
                        conversation_id,
                        base_ids,
                        plan,
                        summary_messages,
                        resume_after,
                        silent,
                        tx,
                        rx,
                        ui_tx,
                    );
                }
                Err(_) if requires_key => {
                    this.ai_entity
                        .update(cx, |ai, _cx| ai.finish_compaction(&conversation_id));
                    this.ai.chat.loading = false;
                    if silent {
                        this.ai_entity.update(cx, |ai, cx| {
                            ai.clear_compaction_notice_for(&conversation_id, cx);
                        });
                    }
                    if !silent {
                        this.push_ai_settings_toast(
                            failed_to_get_key,
                            TerminalNoticeVariant::Error,
                        );
                    }
                    this.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
                    cx.notify();
                }
                Err(_) => {
                    config.api_key = None;
                    this.start_ai_compaction_stream_with_config(
                        config,
                        kind,
                        conversation_id,
                        base_ids,
                        plan,
                        summary_messages,
                        resume_after,
                        silent,
                        tx,
                        rx,
                        ui_tx,
                    );
                }
            });
        })
        .detach();
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::workspace) fn start_ai_compaction_stream_with_config(
        &mut self,
        config: AiChatStreamConfig,
        kind: AiCompactionDeliveryKind,
        conversation_id: String,
        base_ids: Vec<String>,
        plan: Option<AiCompactionPlan>,
        summary_messages: Vec<AiChatMessage>,
        resume_after: Option<AiPendingChatStream>,
        silent: bool,
        tx: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<AiStreamEvent>,
        ui_tx: AiCompactionDeliverySender,
    ) {
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
                kind,
                conversation_id,
                base_ids,
                plan,
                summary,
                stream_error,
                resume_after,
                silent,
            });
        });
    }
}
