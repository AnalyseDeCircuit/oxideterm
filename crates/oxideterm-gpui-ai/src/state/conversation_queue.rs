use gpui::{Context, FollowMode};
use oxideterm_ai::{AiChatRole, tool_loop::agent_chat_message};

use super::{AiPendingChatStream, AiWorkspaceEntity};

impl AiWorkspaceEntity {
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
