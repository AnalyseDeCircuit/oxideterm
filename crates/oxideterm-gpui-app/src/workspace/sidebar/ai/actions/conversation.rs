use crate::workspace::ai_state::AiStandardConfirmKind;
use oxideterm_gpui_ai::conversation::actions::AiChatActions;
use oxideterm_gpui_ai::state::conversation_queue::AiDraftContext;

impl AiChatActions for WorkspaceApp {
    type Cx<'a> = Context<'a, Self>;

    fn chat_state(&self) -> &Entity<crate::workspace::ai_state::AiWorkspaceEntity> {
        &self.ai.state
    }
    fn chat_settings(&self) -> &oxideterm_settings::AiSettings {
        &self.settings_store.settings().ai
    }
    fn chat_i18n(&self) -> &I18n {
        &self.i18n
    }
    fn chat_skills(&self) -> &oxideterm_gpui_ai::state::skills::AiSkillCatalog {
        &self.ai.skills
    }
    fn capture_chat_context(
        &self,
        references: &[AiReferenceMatch],
        cx: &mut Context<Self>,
    ) -> AiDraftContext {
        AiDraftContext {
            sidebar: self.resolve_ai_sidebar_context_block(cx),
            terminal: self.resolve_ai_selected_terminal_context(cx),
            references: self.resolve_ai_reference_context(references, cx),
        }
    }
    fn resolve_ai_sidebar_system_prompt_segment(
        &mut self,
        cx: &mut Self::Cx<'_>,
    ) -> Option<String> {
        WorkspaceApp::resolve_ai_sidebar_system_prompt_segment(self, cx)
    }
    fn resolve_ai_stream_config(&self, cx: &App) -> Result<AiChatStreamConfig, String> {
        WorkspaceApp::resolve_ai_stream_config(self, cx)
    }
    fn ensure_ai_chat_initialized(&mut self, cx: &mut App) {
        WorkspaceApp::ensure_ai_chat_initialized(self, cx);
    }
    fn clear_chat_composition(&mut self) {
        self.ime_marked_text = None;
    }
    fn chat_notice(&mut self, message: String, error: bool, cx: &mut Self::Cx<'_>) {
        self.push_ai_settings_toast(
            message,
            if error {
                TerminalNoticeVariant::Error
            } else {
                TerminalNoticeVariant::Warning
            },
            cx,
        );
    }
    fn redraw_chat_chrome(&mut self, cx: &mut Self::Cx<'_>) {
        cx.notify();
    }
    fn start_ai_compact_conversation(&mut self, cx: &mut Self::Cx<'_>) {
        WorkspaceApp::start_ai_compact_conversation(self, cx);
    }
    fn reset_ai_chat_input_after_submit(&mut self, cx: &mut App) {
        WorkspaceApp::reset_ai_chat_input_after_submit(self, cx);
    }
    fn cancel_ai_chat_stream(&mut self, cx: &mut Self::Cx<'_>) {
        WorkspaceApp::cancel_ai_chat_stream(self, cx);
    }
    fn launch_prepared_queued_turn(&mut self, pending: AiPendingChatStream, cx: &mut Self::Cx<'_>) {
        WorkspaceApp::launch_prepared_queued_turn(self, pending, cx);
    }
    fn launch_chat_turn(&mut self, turn: AiPendingChatStream, cx: &mut Self::Cx<'_>) {
        self.start_ai_chat_stream_after_api_key_lookup(
            turn.conversation_id,
            turn.config,
            turn.request_content,
            turn.task_system_prompt,
            cx,
        );
    }
    fn chat_launcher(
        &self,
        cx: &Self::Cx<'_>,
    ) -> impl FnOnce(AiPendingChatStream, &mut App) + 'static {
        let workspace = cx.weak_entity();
        move |pending, cx| {
            let _ = workspace.update(cx, |app, cx| app.launch_chat_turn(pending, cx));
        }
    }
    fn record_ai_memory_usage(&mut self, ids: &[String], cx: &mut Self::Cx<'_>) {
        WorkspaceApp::record_ai_memory_usage(self, ids, cx);
    }
}

impl WorkspaceApp {
    fn open_chat_confirm(
        &mut self,
        kind: AiStandardConfirmKind,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) {
        self.ai
            .state
            .update(cx, |ai, _| ai.open_standard_chat_confirm(kind));
        if keyboard {
            self.reset_standard_confirm_focus();
        } else {
            self.clear_standard_confirm_focus();
        }
        cx.notify();
    }

    fn close_chat_confirm(&mut self, kind: AiStandardConfirmKind, cx: &mut Context<Self>) -> bool {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let closing = self
            .ai
            .state
            .update(cx, |ai, cx| ai.close_chat_confirm(kind, delay, cx));
        if closing {
            self.clear_standard_confirm_focus();
        }
        closing
    }

    pub(in crate::workspace) fn open_ai_safety_confirm(&mut self, cx: &mut Context<Self>) {
        self.open_chat_confirm(AiStandardConfirmKind::Safety, false, cx);
    }

    pub(in crate::workspace) fn open_ai_summarize_confirm(&mut self, cx: &mut Context<Self>) {
        self.open_chat_confirm(AiStandardConfirmKind::Summarize, true, cx);
    }

    pub(in crate::workspace) fn begin_ai_safety_confirm_exit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.close_chat_confirm(AiStandardConfirmKind::Safety, cx)
    }

    pub(in crate::workspace) fn begin_ai_summarize_confirm_exit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.close_chat_confirm(AiStandardConfirmKind::Summarize, cx)
    }

    pub(in crate::workspace) fn create_ai_sidebar_conversation(
        &mut self,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.ensure_ai_chat_initialized(cx);
        self.ai
            .state
            .update(cx, |ai, cx| ai.create_sidebar_conversation(title, cx))
    }

    fn launch_prepared_queued_turn(
        &mut self,
        pending: AiPendingChatStream,
        cx: &mut Context<Self>,
    ) {
        self.record_ai_memory_usage(&pending.config.memory_entry_ids, cx);
        self.launch_chat_turn(pending, cx);
    }

    pub(in crate::workspace) fn start_ai_chat_stream_after_api_key_lookup(
        &mut self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_ai::state::launch::{IntentReady, start_chat_launch};
        let host = cx.weak_entity();
        let notice = host.clone();
        let backend = host.clone();
        let context_window = self.ai_active_model_context_window(&config);
        let pending = AiPendingChatStream {
            launch_id: None,
            conversation_id,
            config,
            request_content,
            task_system_prompt,
            rag_system_prompt: None,
        };
        start_chat_launch(
            &self.ai.state,
            pending,
            context_window,
            self.i18n.clone(),
            move |cx| Some(backend.upgrade()?.read(cx).ai_model_backend_services(cx)),
            move |ready, cx| {
                let _ = host.update(cx, |app, cx| match ready {
                    IntentReady::Acp(turn) => app.start_acp_chat_thread(turn, cx),
                    IntentReady::Provider(turn) => {
                        app.start_ai_chat_stream_after_budget_preflight(turn, true, cx)
                    }
                });
            },
            move |error, cx| {
                let _ = notice.update(cx, |app, cx| app.chat_notice(error, true, cx));
            },
            cx,
        );
    }

    pub(in crate::workspace) fn delete_ai_message(
        &mut self,
        message_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.ai
            .state
            .update(cx, |ai, cx| ai.delete_ai_message(message_id, cx));
    }

    pub(in crate::workspace) fn set_ai_safety_mode_default(&mut self, cx: &mut Context<Self>) {
        self.set_ai_safety_mode(AiSafetyMode::Default, cx);
    }

    pub(in crate::workspace) fn set_ai_safety_mode_read_only(&mut self, cx: &mut Context<Self>) {
        self.set_ai_safety_mode(AiSafetyMode::ReadOnly, cx);
    }

    pub(in crate::workspace) fn set_ai_safety_mode_auto(&mut self, cx: &mut Context<Self>) {
        self.set_ai_safety_mode(AiSafetyMode::Auto, cx);
    }

    pub(in crate::workspace) fn confirm_ai_safety_bypass(&mut self, cx: &mut Context<Self>) {
        self.set_ai_safety_mode(AiSafetyMode::Bypass, cx);
    }

    fn set_ai_safety_mode(&mut self, mode: AiSafetyMode, cx: &mut Context<Self>) {
        self.ai
            .state
            .update(cx, |ai, cx| ai.select_chat_safety_mode(mode, cx));
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn start_edit_ai_message(
        &mut self,
        message_id: String,
        cx: &mut Context<Self>,
    ) {
        let workspace = cx.weak_entity();
        self.ai.state.update(cx, |ai, cx| {
            ai.load_message_edit(
                message_id,
                move |cx| {
                    let _ = workspace.update(cx, |app, _| app.ime_marked_text = None);
                },
                cx,
            )
        });
    }

    pub(in crate::workspace) fn cancel_edit_ai_message(&mut self, cx: &mut Context<Self>) {
        self.ai
            .state
            .update(cx, |ai, cx| ai.cancel_message_edit(cx));
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn reset_ai_chat_input_after_submit(&mut self, cx: &mut App) {
        self.ai.state.update(cx, |ai, cx| {
            ai.clear_chat_input_after_submit();
            cx.notify();
        });
        self.ime_marked_text = None;
    }
}
