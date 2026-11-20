use oxideterm_gpui_ai::state::compaction::{
    AiCompactionBindings, AiCompactionEffect, AiCompactionProfile, AiCompactionRequest,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn start_ai_compact_conversation(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.ai.state.read(cx).active_compaction_request(false) {
            let _ = self.request_ai_compaction(request, cx);
        }
    }

    pub(in crate::workspace) fn maybe_start_ai_auto_compaction(
        &mut self,
        conversation_id: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(request) = self.ai.state.read(cx).auto_compaction_request(conversation_id) {
            let _ = self.request_ai_compaction(request, cx);
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
        self.request_ai_compaction(AiCompactionRequest::compact(conversation_id, silent, force, resume_after), cx)
    }

    pub(in crate::workspace) fn start_ai_summarize_conversation(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.ai.state.read(cx).active_compaction_request(true) {
            let _ = self.request_ai_compaction(request, cx);
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
        let meter = owner.clone();
        let bindings = AiCompactionBindings {
            should_compact: Rc::new(move |conversation, cx| meter.upgrade().is_some_and(|entity| {
                let app = entity.read(cx);
                app.resolve_ai_stream_config(cx).is_ok_and(|config| app.ai_conversation_prompt(cx).should_auto_compact_history(conversation, config))
            })),
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
                if self.ai.state.read(cx).can_resume_compacted_turn(&pending) {
                    self.start_ai_chat_stream_after_budget_preflight(pending, false, cx);
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
