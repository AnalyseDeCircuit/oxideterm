use gpui::{Context, FollowMode};
use oxideterm_ai::{
    AI_IMAGE_REQUEST_BYTES, AiChatRole, AiChatStreamConfig, AiExecutionBackend, AiImageAttachment,
    AiParsedInput, AiSlashCommand, agent_chat_message, ai_detected_intent_system_prompt,
    ai_input_system_prompt, detect_ai_intent, generate_chat_title, parse_ai_user_input,
    resolve_ai_slash_command,
};
use oxideterm_i18n::I18n;
use zeroize::Zeroizing;

use super::{
    AiPendingChatStream, AiQueuedChatTurn, AiWorkspaceEntity, ai_now_ms, skills::AiSkillCatalog,
};

pub enum AiDraftSubmission {
    Ignore,
    Warning(&'static str),
    Answered,
    Compact,
    Prepare(AiSubmittedDraft),
}

enum AiLocalChatCommand {
    Clear,
    Help,
    Compact,
    Ignore,
}

pub struct AiSubmittedDraft {
    content: Zeroizing<String>,
    images: Vec<AiImageAttachment>,
    pub input: AiParsedInput,
    slash: Option<&'static AiSlashCommand>,
}

/// Text already sampled by the window owner. Context formatting remains part
/// of the conversation request, not the terminal or IDE bridge.
pub struct AiDraftContext {
    pub terminal: Option<String>,
    pub sidebar: Option<String>,
    pub references: Option<String>,
}

impl AiSubmittedDraft {
    fn local_command(&self, busy: bool) -> Result<Option<AiLocalChatCommand>, &'static str> {
        let Some(command) = self.slash.filter(|command| command.client_only) else {
            return Ok(None);
        };
        if !self.images.is_empty() {
            return Err("ai.images.queue_only");
        }
        if busy {
            return Err("ai.queue.client_command_busy");
        }
        Ok(Some(match command.name {
            "clear" => AiLocalChatCommand::Clear,
            "help" => AiLocalChatCommand::Help,
            "compact" => AiLocalChatCommand::Compact,
            _ => AiLocalChatCommand::Ignore,
        }))
    }

    pub fn check_image_support(&self, config: &AiChatStreamConfig) -> Result<(), &'static str> {
        if !self.images.is_empty()
            && config.execution_backend != AiExecutionBackend::Acp
            && oxideterm_ai::model_image_input(
                &config.provider_type,
                &config.model,
                config.image_input,
            ) == Some(false)
        {
            return Err("ai.images.model_unavailable");
        }
        Ok(())
    }
}

fn chat_context(blocks: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    let blocks = blocks
        .into_iter()
        .flatten()
        .map(|block| block.trim().to_owned())
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>();
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}

impl AiWorkspaceEntity {
    pub fn bootstrap_chat_mcp_registry(&self, settings: &oxideterm_settings::AiSettings) {
        // Probe only when the AI surface is in use: MCP may need the OS key store.
        self.host_bridge()
            .mcp
            .bootstrap(settings.mcp_servers.clone());
    }

    pub fn prepare_draft_submission(
        &mut self,
        settings: &oxideterm_settings::AiSettings,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AiDraftSubmission {
        use AiDraftSubmission::*;
        if !self.history_ready() {
            return Ignore;
        }
        let ui = self.chat_ui();
        if ui.images.iter().map(|image| image.bytes).sum::<usize>() > AI_IMAGE_REQUEST_BYTES {
            return Warning("ai.images.request_limit");
        }
        let content = ui.draft.trim();
        if ui.image_import.is_some() || (content.is_empty() && ui.images.is_empty()) {
            return Ignore;
        }
        if !settings.enabled {
            return Warning("ai.chat.disabled_message");
        }
        let content = Zeroizing::new(content.to_owned());
        if let Some((generation, id)) = self.active_user_question() {
            if !self.chat_ui().images.is_empty() {
                return Warning("ai.images.queue_only");
            }
            return if self.resolve_user_question(generation, &id, content) {
                self.clear_chat_input_after_submit();
                cx.notify();
                Answered
            } else {
                Ignore
            };
        }
        let input = parse_ai_user_input(&content);
        let slash = input
            .slash_command
            .as_deref()
            .and_then(resolve_ai_slash_command);
        let draft = AiSubmittedDraft {
            content,
            images: self.chat_ui().images.clone(),
            input,
            slash,
        };
        self.bootstrap_chat_mcp_registry(settings);
        match draft.local_command(self.chat_is_loading()) {
            Ok(Some(AiLocalChatCommand::Clear)) => {
                self.create_sidebar_conversation(None, cx);
                self.clear_chat_input_after_submit();
                cx.notify();
                Answered
            }
            Ok(Some(AiLocalChatCommand::Help)) => {
                self.add_draft_help(draft, i18n, cx);
                Answered
            }
            Ok(Some(AiLocalChatCommand::Compact)) => Compact,
            Ok(Some(AiLocalChatCommand::Ignore)) => Ignore,
            Err(key) => Warning(key),
            Ok(None) => Prepare(draft),
        }
    }

    pub fn add_draft_help(&mut self, draft: AiSubmittedDraft, i18n: &I18n, cx: &mut Context<Self>) {
        let now = ai_now_ms();
        let title = generate_chat_title(&draft.content);
        let id = self.next_chat_id(now);
        let user = agent_chat_message(AiChatRole::User, draft.content.to_string());
        let assistant = agent_chat_message(
            AiChatRole::Assistant,
            oxideterm_ai::ai_help_markdown(|key| i18n.t(key)),
        );
        let user = oxideterm_ai::AiChatMessage {
            id: self.next_chat_id(now),
            timestamp_ms: now,
            ..user
        };
        let assistant = oxideterm_ai::AiChatMessage {
            id: self.next_chat_id(now),
            timestamp_ms: now,
            ..assistant
        };
        self.add_help_exchange(id, title, now, user, assistant);
        self.clear_chat_input_after_submit();
        cx.notify();
    }

    pub fn enqueue_submitted_draft(
        &mut self,
        draft: AiSubmittedDraft,
        context: AiDraftContext,
        runtime_prompt: Option<String>,
        config: AiChatStreamConfig,
        skills: Option<&AiSkillCatalog>,
        cx: &mut Context<Self>,
    ) -> Option<AiPendingChatStream> {
        let AiSubmittedDraft {
            content,
            images,
            input,
            slash,
        } = draft;
        #[cfg(not(target_family = "wasm"))]
        let sources = [
            context.terminal.as_ref().map(|_| "terminal"),
            context.sidebar.as_ref().map(|_| "sidebar"),
            context.references.as_ref().map(|_| "reference"),
        ];
        let context = chat_context([context.terminal, context.sidebar, context.references]);
        #[cfg(not(target_family = "wasm"))]
        if let Some(context) = &context
            && let Some(mut audit) = oxideterm_audit::AuditContext::current_request()
                .or_else(oxideterm_audit::AuditContext::current)
        {
            if audit.source == oxideterm_audit::AuditSource::Application {
                audit.source = oxideterm_audit::AuditSource::User;
            }
            audit.observe(
                oxideterm_audit::AuditCategory::Automation,
                "ai_context_provided",
                Some(&format!(
                    "provider={} model={} sources={} bytes={}",
                    config.provider_id.as_deref().unwrap_or("unknown"),
                    config.model,
                    sources.into_iter().flatten().collect::<Vec<_>>().join(","),
                    context.len()
                )),
                oxideterm_audit::AuditOutcome::Sent,
                oxideterm_audit::AuditEvidence::Dispatch,
                oxideterm_audit::AuditAuthorization::NotRequired,
            );
        }

        let mut skill = None;
        let mut skill_prompt = None;
        if slash.is_none()
            && let Some((catalog, id)) = skills.zip(input.slash_command.as_deref())
        {
            if let Some((hash, instructions)) =
                super::skills::AiSkillCatalogHost::load_explicit(catalog, id)
            {
                skill = Some((id.to_owned(), hash));
                skill_prompt = Some(format!(
                    "## Explicit Agent Skill\nThe user explicitly selected `{id}`. Follow these instructions for this request, subject to the current tool permissions and safety mode.\n\n<skill_instructions id=\"{id}\">\n{}\n</skill_instructions>",
                    oxideterm_ai::sanitize_for_ai(&instructions)
                ));
            }
        }
        let intent = detect_ai_intent(&input);
        let task_system_prompt = chat_context([
            ai_input_system_prompt(slash, &input.participants),
            skill_prompt,
            runtime_prompt,
            ai_detected_intent_system_prompt(&intent),
        ]);
        let participant = input.participants.first().map(|entry| entry.name.clone());
        let now = ai_now_ms();
        let title = generate_chat_title(&content);
        let conversation_key = self.next_chat_id(now);
        let id = self.next_chat_id(now);
        let request = if config.execution_backend == AiExecutionBackend::Acp
            && input.slash_command.is_some()
            && slash.is_none()
            && skill.is_none()
        {
            // ACP-advertised commands must keep the slash prefix on the wire.
            Zeroizing::new(content.to_string())
        } else {
            Zeroizing::new(input.clean_text)
        };
        let request_content = if request.is_empty() && skill.is_some() {
            Some(Zeroizing::new(
                "Apply the explicitly selected Agent Skill to the current task.".to_owned(),
            ))
        } else {
            (!request.is_empty()).then_some(request)
        };
        let conversation = self.ensure_conversation(conversation_key, Some(title), now, None);
        let busy = self.chat_is_loading();
        self.queued_chat_turns
            .entry(conversation.clone())
            .or_default()
            .push_back(AiQueuedChatTurn {
                images,
                id,
                content,
                context: context.map(Zeroizing::new),
                config,
                request_content,
                task_system_prompt: task_system_prompt.map(Zeroizing::new),
                participant,
                skill,
            });
        self.clear_chat_input_after_submit();
        let pending = if busy {
            None
        } else {
            self.prepare_next_queued_turn(&conversation, true)
        };
        cx.notify();
        pending
    }

    pub fn steer_chat_draft(&mut self, cx: &mut Context<Self>) -> Result<(), &'static str> {
        if !self.chat_ui().images.is_empty() {
            return Err("ai.images.queue_only");
        }
        let content = self.chat_ui().draft.trim().to_owned();
        if content.is_empty() {
            return Ok(());
        }
        if !self.can_supplement_agent() {
            return Err("ai.queue.steer_unavailable");
        }
        self.supplement_agent(&content)
            .map_err(|_| "ai.agents.message_not_sent")?;
        if let Some(id) = self.conversation_state().active_conversation_id.clone() {
            self.add_message(&id, agent_chat_message(AiChatRole::User, content));
        }
        self.set_chat_draft(String::new());
        self.persist_chat_state();
        cx.notify();
        Ok(())
    }

    pub fn prepare_next_queued_turn(
        &mut self,
        conversation_id: &str,
        enabled: bool,
    ) -> Option<AiPendingChatStream> {
        if !enabled {
            return None;
        }
        let turn = self.take_queued_chat_turn(conversation_id)?;
        let mut message = agent_chat_message(AiChatRole::User, turn.content.to_string());
        message.id = turn.id;
        message.context = turn.context.as_deref().cloned();
        message.model = Some(turn.config.model.clone());
        oxideterm_ai::set_message_images(&mut message, turn.images);
        self.begin_sidebar_user_turn(
            conversation_id.to_owned(),
            message,
            &turn.config,
            turn.participant,
        );
        self.set_conversation_loading(conversation_id, true);
        if let Some((id, hash)) = turn.skill {
            self.record_ai_loaded_skill(conversation_id, &id, &hash);
        }
        if self.conversation_state().active_conversation_id.as_deref() == Some(conversation_id) {
            self.chat_ui()
                .message_list_state
                .set_follow_mode(FollowMode::Tail);
        }
        Some(AiPendingChatStream {
            launch_id: None,
            conversation_id: conversation_id.to_owned(),
            config: turn.config,
            request_content: turn.request_content.as_deref().cloned(),
            task_system_prompt: turn.task_system_prompt.as_deref().cloned(),
            rag_system_prompt: None,
        })
    }

    pub fn apply_ai_queue_action(
        &mut self,
        conversation: &str,
        id: &str,
        action: &str,
        cx: &mut Context<Self>,
    ) -> Result<bool, &'static str> {
        if self.conversation_state().active_conversation_id.as_deref() != Some(conversation) {
            return Ok(false);
        }
        if action == "edit"
            && (!self.chat_ui().draft.is_empty() || !self.chat_ui().images.is_empty())
        {
            return Err("ai.queue.draft_not_empty");
        }
        if action == "send" && self.chat_is_loading() {
            if self
                .queued_chat_turns
                .get(conversation)
                .and_then(|turns| turns.iter().find(|turn| turn.id == id))
                .is_some_and(|turn| !turn.images.is_empty())
            {
                return Err("ai.images.queue_only");
            }
            let content = self
                .queued_chat_turns
                .get(conversation)
                .and_then(|turns| turns.iter().find(|turn| turn.id == id))
                .map(|turn| turn.content.to_string());
            let Some(content) = content else {
                return Ok(false);
            };
            let accepted = self.supplement_agent(&content).is_ok();
            if accepted {
                self.add_message(conversation, agent_chat_message(AiChatRole::User, content));
                self.persist_chat_state();
            }
            if !accepted {
                return Err("ai.queue.steer_unavailable");
            }
        }
        {
            let Some(turns) = self.queued_chat_turns.get_mut(conversation) else {
                return Ok(false);
            };
            let Some(index) = turns.iter().position(|turn| turn.id == id) else {
                return Ok(false);
            };
            match action {
                "up" => {
                    if index > 0 {
                        turns.swap(index, index - 1);
                    }
                }
                "edit" => {
                    let turn = turns.remove(index).expect("located queued message");
                    self.set_chat_draft(turn.content.to_string());
                    *self.chat_images_mut() = turn.images;
                    self.focus_chat_input();
                }
                "remove" => {
                    turns.remove(index);
                }
                "send" => {
                    let turn = turns.remove(index).expect("located queued message");
                    if !self.chat_is_loading() {
                        self.queued_chat_turns
                            .entry(conversation.to_owned())
                            .or_default()
                            .push_front(turn);
                    }
                }
                _ => {}
            }
        }
        cx.notify();
        Ok(action == "send")
    }
}
