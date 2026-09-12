impl WorkspaceApp {
    pub(in crate::workspace) fn start_ai_compact_conversation(&mut self, cx: &mut Context<Self>) {
        let Some(conversation_id) = self
            .ai_entity
            .read(cx)
            .conversation_state()
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
            .ai_entity
            .read(cx)
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .is_none_or(|conversation| conversation.message_count < 6)
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
        self.load_ai_compaction_history(conversation_id, false, silent, force, resume_after, cx)
    }

    fn load_ai_compaction_history(
        &mut self,
        conversation_id: String,
        summary: bool,
        silent: bool,
        force: bool,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) -> Result<(), Option<AiPendingChatStream>> {
        let ai = self.ai_entity.read(cx);
        if ai.history.quitting
            || ai.history.compaction_loads.contains_key(&conversation_id)
            || ai.compaction_in_progress(&conversation_id)
        {
            return Err(resume_after);
        }
        let Some(store) = ai.history.store.clone() else {
            return Err(resume_after);
        };
        let branch = ai
            .history
            .branches
            .get(&conversation_id)
            .cloned()
            .unwrap_or_else(|| "main".into());
        let config = match self.resolve_ai_summary_stream_config(!summary, cx) {
            Ok(config) => config,
            Err(error) => {
                if !silent {
                    self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                }
                return Err(resume_after);
            }
        };
        let budget = self
            .ai_active_model_context_window(&config)
            .saturating_mul(2);
        let provider = config.provider_type.clone();
        let runtime = self.forwarding_runtime.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.ai_entity.update(cx, |ai, _| {
            ai.persist_chat_state();
            ai.history_barrier(sender);
        });
        let target = conversation_id.clone();
        let task = cx.spawn(async move |weak, cx| {
            if receiver.await != Ok(true) {
                let _ = weak.update(cx, |this, cx| {
                    this.ai_entity.update(cx, |ai, _| {
                        ai.history.compaction_loads.remove(&conversation_id);
                        ai.set_conversation_loading(&conversation_id, false);
                    })
                });
                return;
            }
            let id = conversation_id.clone();
            let source_branch = branch.clone();
            let result = runtime
                .spawn_blocking(move || store.model_context(&id, &source_branch, budget, &provider))
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.ai_entity.update(cx, |ai, _| {
                    ai.history.compaction_loads.remove(&conversation_id);
                });
                let ai = this.ai_entity.read(cx);
                if ai.history.quitting
                    || ai
                        .history
                        .branches
                        .get(&conversation_id)
                        .is_some_and(|current| current != &branch)
                {
                    return;
                }
                let Ok(Ok(source)) = result else {
                    this.ai_entity.update(cx, |ai, _| {
                        ai.set_conversation_loading(&conversation_id, false);
                        ai.history_load_failed();
                    });
                    cx.notify();
                    return;
                };
                this.ai_entity.update(cx, |ai, _| {
                    ai.history
                        .compaction_sources
                        .insert(conversation_id.clone(), (branch, source));
                });
                if summary {
                    if !this
                        .start_ai_summarize_conversation_from_history(conversation_id.clone(), cx)
                    {
                        this.ai_entity.update(cx, |ai, _| {
                            ai.history.compaction_sources.remove(&conversation_id);
                        });
                    }
                } else if let Err(resume) = this.start_ai_compact_conversation_from_history(
                    conversation_id.clone(),
                    silent,
                    force,
                    resume_after,
                    cx,
                ) {
                    this.ai_entity.update(cx, |ai, _| {
                        ai.history.compaction_sources.remove(&conversation_id);
                    });
                    this.resume_ai_chat_after_pre_send_compaction(resume, cx);
                }
            });
        });
        self.ai_entity.update(cx, |ai, _| {
            ai.history.compaction_loads.insert(target, task);
        });
        Ok(())
    }

    fn start_ai_compact_conversation_from_history(
        &mut self,
        conversation_id: String,
        silent: bool,
        force: bool,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) -> Result<(), Option<AiPendingChatStream>> {
        // Return an unconsumed pre-send request when compaction is skipped so
        // its zeroizing provider configuration never needs to be cloned.
        let mut messages = match self
            .ai_entity
            .read(cx)
            .history
            .compaction_sources
            .get(&conversation_id)
            .map(|(_, conversation)| conversation)
        {
            // The worker needs owned messages, but not the conversation's
            // metadata, title, session state, or branch bookkeeping.
            Some(conversation) if conversation.messages.len() >= 4 => conversation.messages.clone(),
            _ => return Err(resume_after),
        };
        if !self
            .ai_entity
            .update(cx, |ai, _cx| ai.begin_compaction(&conversation_id))
        {
            return Err(resume_after);
        }

        let config = match self.resolve_ai_summary_stream_config(true, cx) {
            Ok(config) => config,
            Err(error) => {
                self.ai_entity
                    .update(cx, |ai, _cx| ai.finish_compaction(&conversation_id));
                if !silent {
                    self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                }
                return Err(resume_after);
            }
        };
        oxideterm_ai::scope_responses_history(&mut messages, &config);
        let context_window = self.ai_active_model_context_window(&config);
        if silent && !force {
            let total_tokens = messages
                .iter()
                .map(|message| ai_message_payload_estimated_tokens(message, &config.provider_type))
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
                    .update(cx, |ai, _cx| ai.finish_compaction(&conversation_id));
                return Err(resume_after);
            }
        }
        let Some(plan) = ai_compaction_plan_for_provider(
            &messages,
            context_window,
            silent,
            &config.provider_type,
        ) else {
            self.ai_entity
                .update(cx, |ai, _cx| ai.finish_compaction(&conversation_id));
            return Err(resume_after);
        };
        if silent {
            self.ai_entity.update(cx, |ai, cx| {
                ai.set_compaction_notice_running(&conversation_id, cx);
            });
        }
        let summary_messages = ai_compaction_summary_messages(&plan.compact_messages);
        let base_ids = messages
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
        let Some(id) = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone()
        else {
            return;
        };
        let _ = self.load_ai_compaction_history(id, true, false, true, None, cx);
    }

    fn start_ai_summarize_conversation_from_history(
        &mut self,
        target: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let (conversation_id, messages) = match self
            .ai_entity
            .read(cx)
            .history
            .compaction_sources
            .get(&target)
            .map(|(_, conversation)| conversation)
        {
            // Summarization consumes message history only; keep unrelated
            // conversation metadata inside the Entity.
            Some(conversation) if conversation.messages.len() >= 4 => {
                (conversation.id.clone(), conversation.messages.clone())
            }
            _ => return false,
        };
        if !self
            .ai_entity
            .update(cx, |ai, _cx| ai.begin_compaction(&conversation_id))
        {
            return false;
        }

        let config = match self.resolve_ai_summary_stream_config(false, cx) {
            Ok(config) => config,
            Err(error) => {
                self.ai_entity
                    .update(cx, |ai, _cx| ai.finish_compaction(&conversation_id));
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                return false;
            }
        };
        let summary_messages = ai_conversation_summary_messages(&messages);
        let base_ids = messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let ui_tx = self.ai_entity.read(cx).compaction_sender();
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_conversation_loading(&conversation_id, true)
        });
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
        true
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
        let key_store = self.ai_entity.read(cx).key_store().clone();
        let runtime = self.forwarding_runtime.clone();
        let failed_to_get_key = self.i18n.t("ai.model_selector.failed_to_get_api_key");
        let api_key_not_found = self.i18n.t("ai.model_selector.api_key_not_found");
        let Some(generation) = self
            .ai_entity
            .read(cx)
            .history
            .compaction_runs
            .get(&conversation_id)
            .map(|(generation, _)| *generation)
        else {
            return;
        };
        let target = conversation_id.clone();
        let task = cx.spawn(async move |weak, cx| {
            let key_result = if let Some(provider_id) = provider_id {
                runtime
                    .spawn_blocking(move || key_store.get_provider_key(&provider_id))
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|result| result.map_err(|error| error.to_string()))
            } else {
                Ok(None)
            };
            let _ = weak.update(cx, |this, cx| {
                if this
                    .ai_entity
                    .read(cx)
                    .history
                    .compaction_runs
                    .get(&conversation_id)
                    .is_none_or(|(current, _)| *current != generation)
                {
                    return;
                }
                this.ai_entity.update(cx, |ai, _| {
                    ai.history.compaction_key_loads.remove(&conversation_id);
                });
                if !this.ai_entity.read(cx).history_ready()
                    || this.ai_entity.read(cx).history.quitting
                {
                    this.ai_entity.update(cx, |ai, _| {
                        ai.finish_compaction(&conversation_id);
                        ai.history.compaction_sources.remove(&conversation_id);
                        ai.set_conversation_loading(&conversation_id, false);
                    });
                    return;
                }
                match key_result {
                    Ok(api_key) => {
                        if requires_key && api_key.is_none() {
                            this.ai_entity.update(cx, |ai, _cx| {
                                ai.finish_compaction(&conversation_id);
                                ai.history.compaction_sources.remove(&conversation_id);
                            });
                            this.ai_entity.update(cx, |ai, _cx| {
                                ai.set_conversation_loading(&conversation_id, false)
                            });
                            if silent {
                                this.ai_entity.update(cx, |ai, cx| {
                                    ai.clear_compaction_notice_for(&conversation_id, cx);
                                });
                            }
                            if !silent {
                                this.push_ai_settings_toast(
                                    api_key_not_found,
                                    TerminalNoticeVariant::Error,
                                    cx,
                                );
                            }
                            this.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
                            cx.notify();
                            return;
                        }
                        config.api_key = api_key.map(oxideterm_ai::SharedAiProviderKey::new);
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
                            cx,
                        );
                    }
                    Err(_) if requires_key => {
                        this.ai_entity.update(cx, |ai, _cx| {
                            ai.finish_compaction(&conversation_id);
                            ai.history.compaction_sources.remove(&conversation_id);
                        });
                        this.ai_entity.update(cx, |ai, _cx| {
                            ai.set_conversation_loading(&conversation_id, false)
                        });
                        if silent {
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.clear_compaction_notice_for(&conversation_id, cx);
                            });
                        }
                        if !silent {
                            this.push_ai_settings_toast(
                                failed_to_get_key,
                                TerminalNoticeVariant::Error,
                                cx,
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
                            cx,
                        );
                    }
                }
            });
        });
        self.ai_entity.update(cx, |ai, _| {
            ai.history.compaction_key_loads.insert(target, task);
        });
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
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self
            .ai_entity
            .read(cx)
            .history
            .compaction_runs
            .get(&conversation_id)
            .map(|(generation, _)| *generation)
        else {
            return;
        };
        let target = conversation_id.clone();
        let model = tokio_util::task::AbortOnDropHandle::new(
            self.forwarding_runtime
                .spawn(stream_chat_completion(config, summary_messages, tx)),
        );
        let task = self.forwarding_runtime.spawn(async move {
            let _model = model;
            let mut summary = String::new();
            let mut failed = false;
            while let Some(event) = rx.recv().await {
                match event {
                    AiStreamEvent::Content(chunk) => {
                        summary.push_str(&chunk);
                    }
                    AiStreamEvent::Thinking(_)
                    | AiStreamEvent::Usage { .. }
                    | AiStreamEvent::ProviderResponsePart { .. }
                    | AiStreamEvent::ToolCall { .. }
                    | AiStreamEvent::ToolCallComplete { .. } => {}
                    AiStreamEvent::Done => break,
                    AiStreamEvent::Error(_error) => {
                        // Provider details can include response bodies or
                        // request metadata; only a failure category crosses
                        // the compaction worker boundary.
                        failed = true;
                        break;
                    }
                }
            }
            let _ = ui_tx.send(AiCompactionDelivery {
                generation,
                kind,
                conversation_id,
                base_ids,
                plan,
                summary,
                failed,
                resume_after,
                silent,
            });
        });
        self.ai_entity.update(cx, |ai, _| {
            if let Some((_, owner)) = ai.history.compaction_runs.get_mut(&target) {
                *owner = Some(task);
            } else {
                task.abort();
            }
        });
    }
}
