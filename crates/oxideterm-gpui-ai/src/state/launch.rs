use std::{rc::Rc, time::Duration};

use gpui::{App, Context};
use oxideterm_ai::{
    AiExecutionBackend, SharedAiProviderKey, provider_chat_requires_key,
    tool_loop::AiModelBackendServices,
};
use oxideterm_i18n::I18n;
use tokio_util::task::AbortOnDropHandle;

use super::{AiPendingChatStream, AiWorkspaceEntity, ai_now_ms};

pub enum AiLaunchEffect {
    Error(String),
    Ready(AiPendingChatStream),
}

pub struct AiLaunchBindings {
    pub i18n: I18n,
    pub backend_services: Rc<dyn Fn(&App) -> Option<AiModelBackendServices>>,
    pub dispatch: Rc<dyn Fn(AiLaunchEffect, &mut App)>,
}

impl AiWorkspaceEntity {
    pub fn prepare_chat_launch(
        &mut self,
        mut pending: AiPendingChatStream,
        context_window: usize,
        bindings: AiLaunchBindings,
        cx: &mut Context<Self>,
    ) {
        let target = pending.conversation_id.clone();
        pending.launch_id = Some(self.next_chat_id(ai_now_ms()));
        let launch_id = pending.launch_id.clone().expect("assigned launch");
        self.chat_launches.insert(target.clone(), launch_id);
        self.set_conversation_loading(&target, true);
        let store = self.history.store.clone();
        let branch = self
            .history
            .branches
            .get(&target)
            .cloned()
            .unwrap_or_else(|| "main".into());
        let runtime = self.task_runtime.clone();
        let key_store = self.key_store().clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.persist_chat_state();
        self.history_barrier(sender);
        let id = target.clone();
        let task = cx.spawn(async move |weak, cx| {
            if receiver.await != Ok(true) {
                return;
            }
            let Some(store) = store else {
                return;
            };
            let provider = pending.config.provider_type.clone();
            let model = pending.config.model.clone();
            let read_id = id.clone();
            let context = runtime
                .spawn_blocking(move || {
                    store.model_context(&read_id, &branch, context_window, &provider, &model)
                })
                .await;
            let installed = weak
                .update(cx, |ai, cx| {
                    if !ai.chat_launch_matches(&id, pending.launch_id.as_deref())
                        || ai.history.quitting
                    {
                        return false;
                    }
                    match context {
                        Ok(Ok(context)) => {
                            ai.history.model_contexts.insert(id.clone(), context);
                            true
                        }
                        _ => {
                            ai.history.launches.remove(&id);
                            ai.set_conversation_loading(&id, false);
                            let notify = bindings.dispatch.clone();
                            let message = bindings.i18n.t("ai.chat.load_failed_generic");
                            cx.defer(move |cx| notify(AiLaunchEffect::Error(message), cx));
                            cx.notify();
                            false
                        }
                    }
                })
                .unwrap_or(false);
            if !installed {
                return;
            }

            if pending.config.execution_backend != AiExecutionBackend::Acp {
                if let Some(provider_id) = pending.config.provider_id.clone() {
                    let requires_key = provider_chat_requires_key(&pending.config.provider_type);
                    let key_result = runtime
                        .spawn_blocking(move || key_store.get_provider_key(&provider_id))
                        .await;
                    let error = match key_result {
                        Ok(Ok(key)) if key.is_some() || !requires_key => {
                            pending.config.api_key = key.map(SharedAiProviderKey::new);
                            None
                        }
                        Ok(Ok(_)) => Some("ai.model_selector.api_key_not_found"),
                        _ if requires_key => Some("ai.model_selector.failed_to_get_api_key"),
                        _ => None,
                    };
                    if let Some(key) = error {
                        let _ = weak.update(cx, |ai, cx| {
                            if ai.chat_launch_matches(&id, pending.launch_id.as_deref()) {
                                ai.history.launches.remove(&id);
                                ai.set_conversation_loading(&id, false);
                                let notify = bindings.dispatch.clone();
                                let message = bindings.i18n.t(key);
                                cx.defer(move |cx| notify(AiLaunchEffect::Error(message), cx));
                                cx.notify();
                            }
                        });
                        return;
                    }
                }
                if let Some(query) = pending
                    .request_content
                    .as_ref()
                    .filter(|query| query.trim().chars().count() >= 4)
                    && let Some(services) = cx.update(|cx| (bindings.backend_services)(cx))
                {
                    let query = query.clone();
                    let config = pending.config;
                    let lookup = AbortOnDropHandle::new(runtime.spawn(async move {
                        let prompt = tokio::time::timeout(
                            Duration::from_millis(3000),
                            services.build_rag_system_prompt(Some(&query), &config),
                        )
                        .await
                        .ok()
                        .flatten();
                        // Return the same configuration so an extra key handle never outlives the request.
                        (config, prompt)
                    }));
                    let Ok((config, prompt)) = lookup.await else {
                        return;
                    };
                    pending.config = config;
                    pending.rag_system_prompt = prompt;
                }
            }
            let _ = weak.update(cx, |ai, cx| {
                if !ai.chat_launch_matches(&id, pending.launch_id.as_deref()) || ai.history.quitting
                {
                    return;
                }
                ai.history.launches.remove(&id);
                let dispatch = bindings.dispatch;
                cx.defer(move |cx| dispatch(AiLaunchEffect::Ready(pending), cx));
            });
        });
        self.history.launches.insert(target, task);
        cx.notify();
    }

    pub fn defer_for_unsupported_images(
        &mut self,
        pending: AiPendingChatStream,
        cx: &mut Context<Self>,
    ) -> Option<AiPendingChatStream> {
        let config = &pending.config;
        if !config.omit_history_images
            && oxideterm_ai::model_image_input(
                &config.provider_type,
                &config.model,
                config.image_input,
            ) == Some(false)
            && self
                .history
                .model_contexts
                .get(&pending.conversation_id)
                .is_some_and(|history| {
                    history
                        .messages
                        .iter()
                        .any(|m| !oxideterm_ai::message_images(m).is_empty())
                })
        {
            self.pending_image_requests
                .insert(pending.conversation_id.clone(), pending);
            cx.notify();
            None
        } else {
            Some(pending)
        }
    }
}

pub fn start_chat_launch(
    state: &gpui::Entity<AiWorkspaceEntity>,
    pending: AiPendingChatStream,
    context_window: usize,
    i18n: oxideterm_i18n::I18n,
    backend_services: impl Fn(&App) -> Option<oxideterm_ai::tool_loop::AiModelBackendServices> + 'static,
    on_ready: impl Fn(IntentReady, &mut App) + 'static,
    on_error: impl Fn(String, &mut App) + 'static,
    cx: &mut App,
) {
    let current_state = state.downgrade();
    let bindings = AiLaunchBindings {
        i18n,
        backend_services: Rc::new(backend_services),
        dispatch: Rc::new(move |effect, cx| match effect {
            AiLaunchEffect::Error(message) => on_error(message, cx),
            AiLaunchEffect::Ready(pending) => {
                let Some(state) = current_state.upgrade() else {
                    return;
                };
                if !state
                    .read(cx)
                    .chat_launch_matches(&pending.conversation_id, pending.launch_id.as_deref())
                {
                    return;
                }
                let ready =
                    if pending.config.execution_backend == oxideterm_ai::AiExecutionBackend::Acp {
                        IntentReady::Acp(pending)
                    } else {
                        IntentReady::Provider(pending)
                    };
                on_ready(ready, cx);
            }
        }),
    };
    state.update(cx, |ai, cx| {
        ai.prepare_chat_launch(pending, context_window, bindings, cx)
    });
}

pub enum IntentReady {
    Acp(AiPendingChatStream),
    Provider(AiPendingChatStream),
}
