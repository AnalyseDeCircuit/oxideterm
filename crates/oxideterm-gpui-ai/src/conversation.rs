use std::hash::Hash;

use gpui::{App, Entity};
use oxideterm_ai::AiRuntimeAuthority;

use crate::{
    acp::AcpWorkspaceEntity,
    background::AiBackgroundTaskEntity,
    state::{AiModelSelectorScope, AiStoppedAssistantTurn, AiWorkspaceEntity},
};

/// Coordinates feature-owned conversations without access to workspace tabs or panes.
pub struct AiConversationController<Node: 'static, Terminal: Clone + 'static> {
    state: Entity<AiWorkspaceEntity>,
    authority: Entity<AiRuntimeAuthority<Node>>,
    acp: Entity<AcpWorkspaceEntity<Terminal>>,
    background: Entity<AiBackgroundTaskEntity>,
}

impl<Node: Clone + Eq + Hash + 'static, Terminal: Clone + 'static> AiConversationController<Node, Terminal> {
    pub fn new(
        state: Entity<AiWorkspaceEntity>,
        authority: Entity<AiRuntimeAuthority<Node>>,
        acp: Entity<AcpWorkspaceEntity<Terminal>>,
        background: Entity<AiBackgroundTaskEntity>,
    ) -> Self {
        Self { state, authority, acp, background }
    }

    pub fn delete_ai_conversation(&mut self, id: &str, cx: &mut App) {
        let generations = self.state.read(cx).conversation_stream_generations(id);
        self.state
            .update(cx, |ai, _cx| ai.cancel_chat_stream_for(id));
        self.authority.update(cx, |runtime, _cx| {
            for generation in generations {
                runtime.finish_tool_session(
                    generation,
                    oxideterm_ai::RuntimeRevocationReason::ToolSessionCancelled,
                );
            }
        });
        self.background.update(cx, |tasks, _cx| {
            tasks.cancel_owner(id);
        });
        self.acp.update(cx, |entity, _cx| {
            entity.close_thread(id, false);
        });
        let has_conversations = self.state.update(cx, |ai, _cx| {
            let has_conversations = ai.delete_conversation(id);
            ai.persist_chat_state();
            has_conversations
        });
        self.state.update(cx, |ai, _cx| {
            ai.reset_chat_after_conversation_delete(has_conversations);
            _cx.notify();
        });
        if let Some(mut context) = oxideterm_audit::AuditContext::current_request()
            .or_else(oxideterm_audit::AuditContext::current)
        {
            if context.source == oxideterm_audit::AuditSource::Application {
                context.source = oxideterm_audit::AuditSource::User;
            }
            context.target = Some(oxideterm_audit::redact(id));
            context.observe(
                oxideterm_audit::AuditCategory::Automation,
                "ai_conversation_delete",
                None,
                oxideterm_audit::AuditOutcome::Sent,
                oxideterm_audit::AuditEvidence::Dispatch,
                oxideterm_audit::AuditAuthorization::NotRequired,
            );
        }
    }

    pub fn clear_ai_conversations(&mut self, cx: &mut App) {
        let mut ids: std::collections::HashSet<String> = match self.state
            .read(cx)
            .history
            .store
            .as_ref()
            .map(|store| store.conversation_ids())
            .transpose()
        {
            Ok(ids) => ids.unwrap_or_default().into_iter().collect(),
            Err(_) => {
                self.state.update(cx, |ai, _| ai.history_load_failed());
                return;
            }
        };
        ids.extend(
            self.state
                .read(cx)
                .conversation_state()
                .conversations
                .iter()
                .map(|conversation| conversation.id.clone()),
        );
        // Cancel the live generation before clearing its routing identifier.
        self.cancel_ai_chat_stream_without_notify(cx);
        self.state
            .update(cx, |ai, _| ai.reset_conversation_lists_after_clear());
        for id in ids {
            self.delete_ai_conversation(&id, cx);
        }
        self.acp.update(cx, |entity, _cx| {
            entity.close_all_threads(false);
        });
        self.background.update(cx, |tasks, _cx| {
            tasks.cancel_all();
        });
        self.state.update(cx, |ai, _cx| {
            ai.clear_conversations();
            ai.persist_chat_state();
        });
        self.state.update(cx, |ai, _cx| {
            ai.clear_chat_expansions();
        });
        self.state.update(cx, |state, cx| {
            state.close_chat_popovers();
            state.dismiss_model_selector();
            cx.notify();
        });
    }

    pub fn cancel_ai_chat_stream_without_notify(&mut self, cx: &mut App) {
        let cancelled_generations = self.state
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .as_deref()
            .map(|id| self.state.read(cx).conversation_stream_generations(id))
            .unwrap_or_default();
        let had_active_generation = !cancelled_generations.is_empty();
        let active_conversation_id = self.state
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone();
        if let Some(conversation_id) = active_conversation_id.as_deref() {
            // The ACP entity is the sole process/session owner. Route Stop to
            // it before invalidating the UI generation so the protocol cancel
            // reaches the still-live session.
            let _ = self.acp.read(cx).cancel_active_turn(conversation_id);
        }
        let (conversation_id, stopped_turns) = self.state.update(cx, |ai, _cx| {
            ai.cancel_chat_stream();
            ai.cancel_chat_conversation_state()
        });
        // An abort can leave a UI delivery queued behind the model task. Revoke
        // its lease before that delivery can reach a terminal or other owner.
        self.authority.update(cx, |runtime, _cx| {
            for generation in cancelled_generations {
                runtime.finish_tool_session(
                    generation,
                    oxideterm_ai::RuntimeRevocationReason::ToolSessionCancelled,
                );
            }
        });
        if let Some(conversation_id) = conversation_id.as_deref() {
            self.persist_ai_stopped_assistant_turns(conversation_id, &stopped_turns, cx);
        }
        if (had_active_generation || !stopped_turns.is_empty())
            && let Some(conversation_id) = active_conversation_id
        {
            if let Some(mut context) = oxideterm_audit::AuditContext::current_request()
                .or_else(oxideterm_audit::AuditContext::current)
            {
                if context.source == oxideterm_audit::AuditSource::Application {
                    context.source = oxideterm_audit::AuditSource::User;
                }
                context.target = Some(oxideterm_audit::redact(&conversation_id));
                context.observe(
                    oxideterm_audit::AuditCategory::Automation,
                    "ai_task_cancel",
                    None,
                    oxideterm_audit::AuditOutcome::CancelRequested,
                    oxideterm_audit::AuditEvidence::Dispatch,
                    oxideterm_audit::AuditAuthorization::NotRequired,
                );
            }
        }
    }

    pub fn persist_ai_stopped_assistant_turns(
        &self,
        conversation_id: &str,
        stopped_turns: &[AiStoppedAssistantTurn],
        cx: &App,
    ) {
        for stopped in stopped_turns {
            if stopped.retained {
                self.state.read(cx).persist_ai_assistant_turn_end(
                    conversation_id,
                    &stopped.message_id,
                    stopped.status,
                );
            } else {
                self.state.read(cx).persist_ai_removed_assistant_turn_end(
                    conversation_id,
                    &stopped.message_id,
                    stopped.status,
                );
            }
        }
    }

}

impl AiWorkspaceEntity {
    pub fn dismiss_model_selector(&mut self) {
        let restore_prompt = self.model_selector_scope() == Some(AiModelSelectorScope::TerminalInline)
            && self.terminal_inline_panel().open;
        self.close_model_selector();
        if restore_prompt {
            self.terminal_inline_panel_mut().prompt_focused = true;
        }
    }
}
