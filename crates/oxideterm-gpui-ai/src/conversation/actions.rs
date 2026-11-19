use gpui::{App, Entity};
use oxideterm_ai::{AiChatStreamConfig, AiReferenceMatch};
use oxideterm_i18n::I18n;
use oxideterm_settings::AiSettings;
use std::ops::DerefMut;

use crate::state::{
    AiPendingChatStream, AiWorkspaceEntity,
    conversation_queue::{AiDraftContext, AiDraftSubmission},
    skills::AiSkillCatalog,
};

/// The conversation owns draft transitions. The window contributes the current
/// context and handles focus, persistence, and launch effects.
pub trait AiChatActions {
    type Cx<'a>: DerefMut<Target = App>;

    fn chat_state(&self) -> &Entity<AiWorkspaceEntity>;
    fn chat_settings(&self) -> &AiSettings;
    fn chat_i18n(&self) -> &I18n;
    fn chat_skills(&self) -> &AiSkillCatalog;
    fn capture_chat_context(
        &self,
        references: &[AiReferenceMatch],
        cx: &mut Self::Cx<'_>,
    ) -> AiDraftContext;
    fn resolve_ai_sidebar_system_prompt_segment(&mut self, cx: &mut Self::Cx<'_>) -> Option<String>;
    fn resolve_ai_stream_config(&self, cx: &App) -> Result<AiChatStreamConfig, String>;
    fn ensure_ai_chat_initialized(&mut self, cx: &mut App);
    fn clear_chat_composition(&mut self);
    fn chat_notice(&mut self, message: String, error: bool, cx: &mut Self::Cx<'_>);
    fn redraw_chat_chrome(&mut self, cx: &mut Self::Cx<'_>);
    fn start_ai_compact_conversation(&mut self, cx: &mut Self::Cx<'_>);
    fn reset_ai_chat_input_after_submit(&mut self, cx: &mut App);
    fn cancel_ai_chat_stream(&mut self, cx: &mut Self::Cx<'_>);
    fn launch_prepared_queued_turn(&mut self, pending: AiPendingChatStream, cx: &mut Self::Cx<'_>);
    fn launch_chat_turn(&mut self, pending: AiPendingChatStream, cx: &mut Self::Cx<'_>);
    fn chat_launcher(&self, cx: &Self::Cx<'_>) -> impl FnOnce(AiPendingChatStream, &mut App) + 'static;
    fn record_ai_memory_usage(&mut self, ids: &[String], cx: &mut Self::Cx<'_>);

    fn handle_transcript_action(
        &mut self,
        request: crate::sidebar::transcript::AiTranscriptRequest,
        cx: &mut Self::Cx<'_>,
    ) -> Option<crate::sidebar::transcript::AiTranscriptRequest> {
        use crate::sidebar::transcript::AiTranscriptRequest::*;
        match request {
            ClipboardReadFailed => {
                self.chat_notice(self.chat_i18n().t("ai.chat.load_failed_generic"), true, cx)
            }
            ImagePreviewFailed => {
                self.chat_notice(self.chat_i18n().t("ai.images.missing"), true, cx)
            }
            AgentModelLocked => {
                self.chat_notice(self.chat_i18n().t("ai.agents.model_locked"), false, cx)
            }
            Regenerate => self.regenerate_ai_last_response(cx),
            SaveEdit => self.save_ai_message_edit(cx),
            FollowUp(text) => self.send_ai_follow_up_suggestion(text, cx),
            Stop => self.cancel_ai_chat_stream(cx),
            Steer => self.steer_ai_chat_draft(cx),
            InterruptAndSend => self.interrupt_and_send_ai_chat_draft(cx),
            Send => self.send_ai_chat_draft(cx),
            Compact => self.start_ai_compact_conversation(cx),
            QueueAction(conversation, id, action) => {
                self.apply_ai_queue_action(&conversation, &id, action, cx)
            }
            other => return Some(other),
        }
        None
    }

    fn send_ai_chat_draft(&mut self, cx: &mut Self::Cx<'_>) {
        self.ensure_ai_chat_initialized(cx);
        let settings = self.chat_settings();
        let draft = match self.chat_state().update(&mut **cx, |ai, cx| {
            ai.prepare_draft_submission(settings, self.chat_i18n(), cx)
        }) {
            AiDraftSubmission::Ignore => return,
            AiDraftSubmission::Warning(key) => {
                self.chat_notice(self.chat_i18n().t(key), false, cx);
                return;
            }
            AiDraftSubmission::Answered => {
                self.clear_chat_composition();
                return;
            }
            AiDraftSubmission::Compact => {
                self.start_ai_compact_conversation(cx);
                self.reset_ai_chat_input_after_submit(cx);
                return;
            }
            AiDraftSubmission::Prepare(draft) => draft,
        };

        let context = self.capture_chat_context(&draft.input.references, cx);
        let config = match self.resolve_ai_stream_config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.chat_notice(error, true, cx);
                return;
            }
        };
        if let Err(key) = draft.check_image_support(&config) {
            self.chat_notice(self.chat_i18n().t(key), false, cx);
            return;
        }
        let runtime_prompt = self.resolve_ai_sidebar_system_prompt_segment(cx);
        let skills = self
            .chat_settings()
            .skills
            .enabled
            .then(|| self.chat_skills());
        let pending = self.chat_state().update(&mut **cx, |ai, cx| {
            ai.enqueue_submitted_draft(draft, context, runtime_prompt, config, skills, cx)
        });
        self.clear_chat_composition();
        if let Some(pending) = pending {
            self.launch_prepared_queued_turn(pending, cx);
        }
    }

    fn send_next_queued_ai_message(&mut self, conversation_id: &str, cx: &mut Self::Cx<'_>) -> bool {
        let enabled = self.chat_settings().enabled;
        let Some(pending) = self.chat_state().update(&mut **cx, |ai, _| {
            ai.prepare_next_queued_turn(conversation_id, enabled)
        }) else {
            return false;
        };
        self.launch_prepared_queued_turn(pending, cx);
        true
    }

    fn apply_ai_queue_action(
        &mut self,
        conversation: &str,
        id: &str,
        action: &str,
        cx: &mut Self::Cx<'_>,
    ) {
        match self.chat_state().update(&mut **cx, |ai, cx| {
            ai.apply_ai_queue_action(conversation, id, action, cx)
        }) {
            Ok(true) => {
                self.send_next_queued_ai_message(conversation, cx);
            }
            Ok(false) => {}
            Err(key) => self.chat_notice(self.chat_i18n().t(key), false, cx),
        }
    }

    fn interrupt_and_send_ai_chat_draft(&mut self, cx: &mut Self::Cx<'_>) {
        if self.chat_state().read(cx).chat_ui().draft.trim().is_empty()
            && self.chat_state().read(cx).chat_ui().images.is_empty()
        {
            self.cancel_ai_chat_stream(cx);
            return;
        }
        self.cancel_ai_chat_stream(cx);
        // The replacement takes precedence over queued follow-ups without deleting them.
        let id = self
            .chat_state()
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone();
        let queued = id.as_ref().and_then(|id| {
            self.chat_state()
                .update(&mut **cx, |ai, _| ai.queued_chat_turns.remove(id))
        });
        self.send_ai_chat_draft(cx);
        if let (Some(id), Some(queued)) = (id, queued) {
            self.chat_state().update(&mut **cx, |ai, _| {
                ai.queued_chat_turns.entry(id).or_default().extend(queued)
            });
        }
    }

    fn regenerate_ai_last_response(&mut self, cx: &mut Self::Cx<'_>) {
        if self.chat_state().read(cx).chat_is_loading() {
            self.redraw_chat_chrome(cx);
            return;
        }
        let stream_config = match self.resolve_ai_stream_config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.chat_notice(error, true, cx);
                self.redraw_chat_chrome(cx);
                return;
            }
        };
        self.record_ai_memory_usage(&stream_config.memory_entry_ids, cx);
        let launch = self.chat_launcher(cx);
        self.chat_state().update(&mut **cx, |ai, cx| {
            ai.regenerate_message(stream_config, launch, cx)
        });
    }

    fn save_ai_message_edit(&mut self, cx: &mut Self::Cx<'_>) {
        let Some((message_id, edited_content)) =
            self.chat_state().read(cx).editable_message_draft()
        else {
            return;
        };
        let stream_config = match self.resolve_ai_stream_config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.chat_notice(error, true, cx);
                self.redraw_chat_chrome(cx);
                return;
            }
        };
        self.record_ai_memory_usage(&stream_config.memory_entry_ids, cx);

        let request_content = Some(edited_content.clone());
        let Some(conversation_id) = self.chat_state().update(&mut **cx, |ai, cx| {
            ai.commit_message_edit(&message_id, edited_content, &stream_config, cx)
        }) else {
            return;
        };
        self.clear_chat_composition();
        self.launch_chat_turn(
            AiPendingChatStream {
                conversation_id,
                config: stream_config,
                request_content,
                task_system_prompt: None,
                launch_id: None,
                rag_system_prompt: None,
            },
            cx,
        );
        self.redraw_chat_chrome(cx);
    }

    fn send_ai_follow_up_suggestion(&mut self, text: String, cx: &mut Self::Cx<'_>) {
        self.chat_state().update(&mut **cx, |ai, _cx| {
            ai.set_chat_draft(text);
        });
        self.send_ai_chat_draft(cx);
    }

    fn steer_ai_chat_draft(&mut self, cx: &mut Self::Cx<'_>) {
        if let Err(key) = self
            .chat_state()
            .update(&mut **cx, |ai, cx| ai.steer_chat_draft(cx))
        {
            self.chat_notice(self.chat_i18n().t(key), false, cx);
        }
    }
}
