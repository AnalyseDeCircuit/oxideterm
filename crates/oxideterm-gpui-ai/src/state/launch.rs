use std::rc::Rc;

use crate::bridge::{HostFuture, ProviderKeyError};
use gpui::{App, Context};
#[cfg(not(target_family = "wasm"))]
use oxideterm_ai::tool_loop::AiModelBackendServices;
use oxideterm_ai::{AiChatStreamConfig, AiExecutionBackend};
use oxideterm_i18n::I18n;

use super::{AiPendingChatStream, AiWorkspaceEntity, ai_now_ms};

pub enum AiLaunchEffect {
    Error(String),
    Ready(AiPendingChatStream),
}

pub type KnowledgeLookup = Box<
    dyn FnOnce(
        String,
        AiChatStreamConfig,
    ) -> HostFuture<Option<(AiChatStreamConfig, Option<String>)>>,
>;

pub struct PortableLaunchBindings {
    pub i18n: I18n,
    pub knowledge_lookup: Rc<dyn Fn(&App) -> Option<KnowledgeLookup>>,
    pub dispatch: Rc<dyn Fn(AiLaunchEffect, &mut App)>,
}

#[cfg(not(target_family = "wasm"))]
pub struct AiLaunchBindings {
    pub i18n: I18n,
    pub backend_services: Rc<dyn Fn(&App) -> Option<AiModelBackendServices>>,
    pub dispatch: Rc<dyn Fn(AiLaunchEffect, &mut App)>,
}

impl AiWorkspaceEntity {
    #[cfg(not(target_family = "wasm"))]
    pub fn prepare_chat_launch(
        &mut self,
        pending: AiPendingChatStream,
        context_window: usize,
        bindings: AiLaunchBindings,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.task_runtime().clone();
        self.prepare_hosted_chat_launch(
            pending,
            context_window,
            PortableLaunchBindings {
                i18n: bindings.i18n,
                dispatch: bindings.dispatch,
                knowledge_lookup: Rc::new(move |cx| {
                    let services = (bindings.backend_services)(cx)?;
                    let runtime = runtime.clone();
                    Some(Box::new(move |query, config| {
                        Box::pin(async move {
                            tokio_util::task::AbortOnDropHandle::new(runtime.spawn(async move {
                                let prompt = tokio::time::timeout(
                                    std::time::Duration::from_secs(3),
                                    services.build_rag_system_prompt(Some(&query), &config),
                                )
                                .await
                                .ok()
                                .flatten();
                                (config, prompt)
                            }))
                            .await
                            .ok()
                        })
                    }))
                }),
            },
            cx,
        );
    }
    pub fn prepare_hosted_chat_launch(
        &mut self,
        mut pending: AiPendingChatStream,
        context_window: usize,
        bindings: PortableLaunchBindings,
        cx: &mut Context<Self>,
    ) {
        let target = pending.conversation_id.clone();
        pending.launch_id = Some(self.next_chat_id(ai_now_ms()));
        let launch_id = pending.launch_id.clone().expect("assigned launch");
        self.chat_launches.insert(target.clone(), launch_id);
        self.set_conversation_loading(&target, true);
        let store = self.history_reader();
        let branch = self
            .history
            .branches
            .get(&target)
            .cloned()
            .unwrap_or_else(|| "main".into());
        let provider_host = self.host.provider.clone();
        self.persist_chat_state();
        let receiver = self.committed_history();
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
            let context = crate::bridge::history::read_history(|reply| {
                store.model_context(read_id, branch, context_window, provider, model, reply)
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
                        Ok(context) => {
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
                match provider_host.prepare_chat(pending.config).await {
                    Ok(config) => pending.config = config,
                    Err(error) => {
                        let key = match error {
                            ProviderKeyError::Missing => "ai.model_selector.api_key_not_found",
                            ProviderKeyError::Unavailable => {
                                "ai.model_selector.failed_to_get_api_key"
                            }
                        };
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
                    && let Some(lookup) = cx.update(|cx| (bindings.knowledge_lookup)(cx))
                {
                    let Some((config, prompt)) = lookup(query.clone(), pending.config).await else {
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

#[cfg(not(target_family = "wasm"))]
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
