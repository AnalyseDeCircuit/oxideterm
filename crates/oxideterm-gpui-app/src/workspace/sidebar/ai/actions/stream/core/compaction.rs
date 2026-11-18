use oxideterm_gpui_ai::state::compaction::{
    AiCompactionBindings, AiCompactionEffect, AiCompactionProfile, AiCompactionRequest,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn start_ai_compact_conversation(&mut self, cx: &mut Context<Self>) {
        let id = self
            .ai
            .state
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone();
        if let Some(id) = id {
            let _ = self.start_ai_compact_conversation_for(id, false, true, None, cx);
        }
    }

    pub(in crate::workspace) fn maybe_start_ai_auto_compaction(
        &mut self,
        conversation_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.ai.state.read(cx).should_auto_compact(conversation_id) {
            let _ = self.start_ai_compact_conversation_for(
                conversation_id.to_owned(),
                true,
                false,
                None,
                cx,
            );
        }
    }

    pub(in crate::workspace) fn start_ai_compact_conversation_for(
        &mut self,
        conversation_id: String,
        silent: bool,
        force: bool,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) -> Result<(), Option<Box<AiPendingChatStream>>> {
        self.request_ai_compaction(
            AiCompactionRequest {
                conversation_id,
                summarize: false,
                silent,
                force,
                resume_after,
            },
            cx,
        )
    }

    pub(in crate::workspace) fn start_ai_summarize_conversation(&mut self, cx: &mut Context<Self>) {
        let id = self
            .ai
            .state
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone();
        if let Some(conversation_id) = id {
            let _ = self.request_ai_compaction(
                AiCompactionRequest {
                    conversation_id,
                    summarize: true,
                    silent: false,
                    force: true,
                    resume_after: None,
                },
                cx,
            );
        }
    }

    fn read_ai_compaction_profile(
        &self,
        compact: bool,
        cx: &App,
    ) -> Result<AiCompactionProfile, String> {
        let config = self.resolve_ai_summary_stream_config(compact, cx)?;
        let context_window = self.ai_active_model_context_window(&config);
        Ok(AiCompactionProfile {
            config,
            context_window,
        })
    }

    fn request_ai_compaction(
        &mut self,
        request: AiCompactionRequest,
        cx: &mut Context<Self>,
    ) -> Result<(), Option<Box<AiPendingChatStream>>> {
        let profile = self.read_ai_compaction_profile(!request.summarize, cx);
        let owner = cx.weak_entity();
        let effects = owner.clone();
        let bindings = AiCompactionBindings {
            i18n: self.i18n.clone(),
            current_profile: Rc::new(move |compact, cx| {
                Some(
                    owner
                        .upgrade()?
                        .read(cx)
                        .read_ai_compaction_profile(compact, cx),
                )
            }),
            dispatch: Rc::new(move |effect, cx| {
                let _ = effects.update(cx, |app, cx| app.handle_ai_compaction_effect(effect, cx));
            }),
        };
        self.ai.state.update(cx, |state, cx| {
            state.start_compaction_request(request, profile, bindings, cx)
        })
    }

    fn handle_ai_compaction_effect(&mut self, effect: AiCompactionEffect, cx: &mut Context<Self>) {
        match effect {
            AiCompactionEffect::ShowError(message) => {
                self.push_ai_settings_toast(message, TerminalNoticeVariant::Error, cx)
            }
            AiCompactionEffect::Resume(pending) => {
                if self
                    .ai
                    .state
                    .read(cx)
                    .chat_launch_matches(&pending.conversation_id, pending.launch_id.as_deref())
                {
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
            }
        }
    }

    pub(in crate::workspace) fn apply_ai_compaction_deliveries(
        &mut self,
        deliveries: VecDeque<AiCompactionDelivery>,
        cx: &mut Context<Self>,
    ) {
        for delivery in deliveries {
            let effects = self.ai.state.update(cx, |state, cx| {
                state.apply_compaction_delivery(
                    delivery,
                    &self.i18n,
                    |data| self.ai_diagnostic_base(data),
                    cx,
                )
            });
            for effect in effects {
                self.handle_ai_compaction_effect(effect, cx);
            }
        }
    }
}
