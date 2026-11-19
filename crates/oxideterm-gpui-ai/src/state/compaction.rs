use std::rc::Rc;

use gpui::{App, Context};
use oxideterm_ai::{
    AiChatStreamConfig, SharedAiProviderKey, provider_chat_requires_key, scope_responses_history,
    stream_chat_completion, stream_state::*,
};
use oxideterm_i18n::I18n;

use super::{AiPendingChatStream, AiWorkspaceEntity, ai_now_ms};
use crate::stream::{AiCompactionDelivery, AiCompactionDeliveryKind};

pub struct AiCompactionRequest {
    pub conversation_id: String,
    pub summarize: bool,
    pub silent: bool,
    pub force: bool,
    pub resume_after: Option<AiPendingChatStream>,
}

pub struct AiCompactionProfile {
    pub config: AiChatStreamConfig,
    pub context_window: usize,
}

pub enum AiCompactionEffect {
    ShowError(String),
    Resume(AiPendingChatStream),
}

#[derive(Clone)]
pub struct AiCompactionBindings {
    pub i18n: I18n,
    pub current_profile: Rc<dyn Fn(bool, &App) -> Option<Result<AiCompactionProfile, String>>>,
    pub dispatch: Rc<dyn Fn(AiCompactionEffect, &mut App)>,
}

impl AiCompactionBindings {
    fn emit(&self, effect: AiCompactionEffect, cx: &mut App) {
        let dispatch = self.dispatch.clone();
        cx.defer(move |cx| dispatch(effect, cx));
    }
}

impl AiWorkspaceEntity {
    pub fn active_compaction_request(&self, summarize: bool) -> Option<AiCompactionRequest> {
        Some(AiCompactionRequest {
            conversation_id: self.conversation_state().active_conversation_id.clone()?,
            summarize,
            silent: false,
            force: true,
            resume_after: None,
        })
    }

    pub fn should_auto_compact(&self, conversation_id: &str) -> bool {
        self.conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .is_some_and(|conversation| conversation.message_count >= 6)
    }

    pub fn start_compaction_request(
        &mut self,
        request: AiCompactionRequest,
        profile: Result<AiCompactionProfile, String>,
        bindings: AiCompactionBindings,
        cx: &mut Context<Self>,
    ) -> Result<(), Option<Box<AiPendingChatStream>>> {
        let id = request.conversation_id.clone();
        if self.history.quitting
            || self.history.compaction_loads.contains_key(&id)
            || self.compaction_in_progress(&id)
        {
            return Err(request.resume_after.map(Box::new));
        }
        let Some(store) = self.history.store.clone() else {
            return Err(request.resume_after.map(Box::new));
        };
        let profile = match profile {
            Ok(profile) => profile,
            Err(error) => {
                if !request.silent {
                    bindings.emit(AiCompactionEffect::ShowError(error), cx);
                }
                return Err(request.resume_after.map(Box::new));
            }
        };
        let branch = self
            .history
            .branches
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "main".into());
        let source_branch = branch.clone();
        let target = id.clone();
        let source_id = target.clone();
        let runtime = self.task_runtime.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.persist_chat_state();
        self.history_barrier(sender);
        let task = cx.spawn(async move |weak, cx| {
            if receiver.await != Ok(true) {
                let _ = weak.update(cx, |ai, _| {
                    ai.history.compaction_loads.remove(&target);
                    ai.set_conversation_loading(&target, false);
                });
                return;
            }
            let result = runtime
                .spawn_blocking(move || {
                    store.model_context(
                        &source_id,
                        &source_branch,
                        profile.context_window.saturating_mul(2),
                        &profile.config.provider_type,
                        &profile.config.model,
                    )
                })
                .await;
            // Read preferences before entering the AI entity. The history read
            // and summary start are separate stages, as in the provider path.
            let profile = cx.update(|cx| (bindings.current_profile)(!request.summarize, cx));
            let _ = weak.update(cx, |ai, cx| {
                ai.history.compaction_loads.remove(&target);
                if ai.history.quitting
                    || ai
                        .history
                        .branches
                        .get(&target)
                        .is_some_and(|current| current != &branch)
                {
                    return;
                }
                let Ok(Ok(source)) = result else {
                    ai.set_conversation_loading(&target, false);
                    ai.history_load_failed();
                    cx.notify();
                    return;
                };
                let Some(profile) = profile else {
                    return;
                };
                ai.history
                    .compaction_sources
                    .insert(target.clone(), (branch, source));
                if let Err(resume) =
                    ai.begin_loaded_compaction(request, profile, bindings.clone(), cx)
                {
                    ai.history.compaction_sources.remove(&target);
                    if let Some(pending) = ai.valid_compaction_resume(resume) {
                        bindings.emit(AiCompactionEffect::Resume(pending), cx);
                    }
                }
            });
        });
        self.history.compaction_loads.insert(id.clone(), task);
        Ok(())
    }

    fn begin_loaded_compaction(
        &mut self,
        request: AiCompactionRequest,
        profile: Result<AiCompactionProfile, String>,
        bindings: AiCompactionBindings,
        cx: &mut Context<Self>,
    ) -> Result<(), Option<AiPendingChatStream>> {
        let id = &request.conversation_id;
        let mut messages = match self.history.compaction_sources.get(id) {
            Some((_, conversation)) if conversation.messages.len() >= 4 => {
                conversation.messages.clone()
            }
            _ => return Err(request.resume_after),
        };
        if !self.begin_compaction(id) {
            return Err(request.resume_after);
        }
        let profile = match profile {
            Ok(profile) => profile,
            Err(error) => {
                self.finish_compaction(id);
                if !request.silent {
                    bindings.emit(AiCompactionEffect::ShowError(error), cx);
                }
                return Err(request.resume_after);
            }
        };
        let config = profile.config;
        let (kind, plan, summary_messages) = if request.summarize {
            self.set_conversation_loading(id, true);
            (
                AiCompactionDeliveryKind::Summary,
                None,
                ai_conversation_summary_messages(&messages),
            )
        } else {
            scope_responses_history(&mut messages, &config);
            let context_window = profile.context_window;
            if request.silent
                && !request.force
                && !should_compact(&messages, &config, context_window)
            {
                self.finish_compaction(id);
                return Err(request.resume_after);
            }
            let Some(plan) = ai_compaction_plan_for_provider(
                &messages,
                context_window,
                request.silent,
                &config.provider_type,
            ) else {
                self.finish_compaction(id);
                return Err(request.resume_after);
            };
            if request.silent {
                self.set_compaction_notice_running(id, cx);
            }
            let summary_messages = ai_compaction_summary_messages(&plan.compact_messages);
            (
                AiCompactionDeliveryKind::Compact,
                Some(plan),
                summary_messages,
            )
        };
        let generation = self.history.compaction_runs[id].0;
        let delivery = AiCompactionDelivery {
            generation,
            kind,
            conversation_id: id.clone(),
            base_ids: messages.iter().map(|message| message.id.clone()).collect(),
            plan,
            summary: String::new(),
            failed: false,
            resume_after: request.resume_after,
            silent: request.silent,
        };
        self.start_compaction_worker(config, summary_messages, delivery, bindings, cx);
        cx.notify();
        Ok(())
    }

    fn start_compaction_worker(
        &mut self,
        mut config: AiChatStreamConfig,
        messages: Vec<oxideterm_ai::AiChatMessage>,
        mut delivery: AiCompactionDelivery,
        bindings: AiCompactionBindings,
        cx: &mut Context<Self>,
    ) {
        let target = delivery.conversation_id.clone();
        let key_store = self.key_store().clone();
        let runtime = self.task_runtime.clone();
        let requires_key = provider_chat_requires_key(&config.provider_type);
        let ui_tx = self.compaction_sender();
        let task = cx.spawn(async move |weak, cx| {
            let key_result = if let Some(provider_id) = config.provider_id.clone() {
                runtime
                    .spawn_blocking(move || key_store.get_provider_key(&provider_id))
                    .await
                    .map_err(|_| ())
                    .and_then(|result| result.map_err(|_| ()))
            } else {
                Ok(None)
            };
            let _ = weak.update(cx, |ai, cx| {
                let id = &delivery.conversation_id;
                if ai
                    .history
                    .compaction_runs
                    .get(id)
                    .is_none_or(|(current, _)| *current != delivery.generation)
                {
                    return;
                }
                ai.history.compaction_key_loads.remove(id);
                if !ai.history_ready() || ai.history.quitting {
                    ai.finish_compaction(id);
                    ai.history.compaction_sources.remove(id);
                    ai.set_conversation_loading(id, false);
                    return;
                }
                let error_key = match key_result {
                    Ok(key) if key.is_some() || !requires_key => {
                        config.api_key = key.map(SharedAiProviderKey::new);
                        None
                    }
                    Ok(_) => Some("ai.model_selector.api_key_not_found"),
                    Err(_) if requires_key => Some("ai.model_selector.failed_to_get_api_key"),
                    Err(_) => None,
                };
                if let Some(error_key) = error_key {
                    ai.finish_compaction(id);
                    ai.history.compaction_sources.remove(id);
                    ai.set_conversation_loading(id, false);
                    if delivery.silent {
                        ai.clear_compaction_notice_for(id, cx);
                    } else {
                        bindings.emit(
                            AiCompactionEffect::ShowError(bindings.i18n.t(error_key)),
                            cx,
                        );
                    }
                    if let Some(pending) = ai.valid_compaction_resume(delivery.resume_after.take())
                    {
                        bindings.emit(AiCompactionEffect::Resume(pending), cx);
                    }
                    cx.notify();
                    return;
                }
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let model = tokio_util::task::AbortOnDropHandle::new(
                    runtime.spawn(stream_chat_completion(config, messages, tx)),
                );
                let owner_id = delivery.conversation_id.clone();
                let task = runtime.spawn(async move {
                    let _model = model;
                    (delivery.summary, delivery.failed) = collect_ai_compaction_summary(rx).await;
                    let _ = ui_tx.send(delivery);
                });
                if let Some((_, owner)) = ai.history.compaction_runs.get_mut(&owner_id) {
                    *owner = Some(task);
                } else {
                    task.abort();
                }
            });
        });
        self.history.compaction_key_loads.insert(target, task);
    }

    fn valid_compaction_resume(
        &self,
        pending: Option<AiPendingChatStream>,
    ) -> Option<AiPendingChatStream> {
        pending.filter(|pending| {
            self.chat_launch_matches(&pending.conversation_id, pending.launch_id.as_deref())
        })
    }

    pub fn apply_compaction_delivery(
        &mut self,
        mut delivery: AiCompactionDelivery,
        i18n: &I18n,
        diagnostic: impl Fn(serde_json::Value) -> serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Vec<AiCompactionEffect> {
        let id = &delivery.conversation_id;
        if self
            .history
            .compaction_runs
            .get(id)
            .is_none_or(|(current, _)| *current != delivery.generation)
        {
            return Vec::new();
        }
        let manual_summary = matches!(delivery.kind, AiCompactionDeliveryKind::Summary);
        if !manual_summary && delivery.plan.is_none() {
            return Vec::new();
        }
        self.finish_compaction(id);
        if manual_summary {
            self.set_conversation_loading(id, false);
        }
        let mut effects = Vec::new();
        if delivery.failed || delivery.summary.trim().is_empty() {
            self.history.compaction_sources.remove(id);
            if delivery.silent {
                self.clear_compaction_notice_for(id, cx);
            } else if delivery.failed {
                effects.push(AiCompactionEffect::ShowError(
                    i18n.t("settings_view.ai.acp_agent_error_unknown"),
                ));
            }
        } else {
            let now = ai_now_ms();
            let message_id = self.next_chat_id(now);
            if let Some(plan) = delivery.plan.take() {
                if let Some((source, round_id, until_id, compacted)) = self.apply_compaction_result(
                    id,
                    &delivery.base_ids,
                    plan,
                    &delivery.summary,
                    &message_id,
                    now,
                ) {
                    self.persist_summary_created(
                        id,
                        &message_id,
                        "compaction",
                        &delivery.summary,
                        round_id,
                        Some(source),
                        Some(compacted),
                        until_id,
                        Some(if delivery.silent {
                            "background"
                        } else {
                            "manual"
                        }),
                        now,
                        &diagnostic,
                    );
                    if delivery.silent {
                        self.set_compaction_notice_done(id, compacted, cx);
                    }
                } else if delivery.silent {
                    self.clear_compaction_notice_for(id, cx);
                }
            } else {
                let count = delivery.base_ids.len();
                let prefix = i18n
                    .t("ai.context.summary_prefix")
                    .replace("{{count}}", &count.to_string());
                if let Some((source, round_id)) = self.apply_conversation_summary(
                    id,
                    &delivery.base_ids,
                    &message_id,
                    &delivery.summary,
                    &prefix,
                    now,
                ) {
                    self.set_model_switch_warning(None);
                    self.persist_summary_created(
                        id,
                        &message_id,
                        "conversation",
                        &delivery.summary,
                        round_id,
                        Some(source),
                        Some(count),
                        None,
                        Some("manual"),
                        now,
                        &diagnostic,
                    );
                }
            }
        }
        if let Some(pending) = self.valid_compaction_resume(delivery.resume_after) {
            effects.push(AiCompactionEffect::Resume(pending));
        }
        cx.notify();
        effects
    }
}

fn should_compact(
    messages: &[oxideterm_ai::AiChatMessage],
    config: &AiChatStreamConfig,
    context_window: usize,
) -> bool {
    let total_tokens = messages
        .iter()
        .map(|message| ai_message_payload_estimated_tokens(message, &config.provider_type))
        .sum::<usize>();
    let reserve = ai_response_reserve(context_window);
    let prompt_budget = oxideterm_ai::compute_ai_prompt_budget(context_window, reserve, 0, None);
    let threshold = if prompt_budget.usable_prompt_budget > 0 {
        (context_window as f32 * AI_COMPACTION_TRIGGER_THRESHOLD)
            / prompt_budget.usable_prompt_budget as f32
    } else {
        AI_COMPACTION_TRIGGER_THRESHOLD
    };
    determine_ai_compression_level(AiPromptBudgetInput {
        context_window,
        response_reserve: reserve,
        system_budget: 0,
        history_tokens: total_tokens,
        trimmable_history_tokens: None,
        summary_eligible_tokens: Some(total_tokens),
        can_summarize: true,
        can_lookup_transcript: false,
        in_tool_loop: false,
        auto_compact_threshold: Some(threshold),
        transcript_lookup_threshold: None,
        tool_loop_stop_threshold: None,
        safety_margin: None,
    })
    .level
        >= 2
}

impl AiCompactionRequest {
    pub fn compact(
        conversation_id: String,
        silent: bool,
        force: bool,
        resume_after: Option<AiPendingChatStream>,
    ) -> Self {
        Self {
            conversation_id,
            summarize: false,
            silent,
            force,
            resume_after,
        }
    }
}

impl AiWorkspaceEntity {
    pub fn auto_compaction_request(&self, id: &str) -> Option<AiCompactionRequest> {
        self.should_auto_compact(id)
            .then(|| AiCompactionRequest::compact(id.to_owned(), true, false, None))
    }

    pub fn can_resume_compacted_turn(&self, pending: &AiPendingChatStream) -> bool {
        self.chat_launch_matches(&pending.conversation_id, pending.launch_id.as_deref())
    }
}
