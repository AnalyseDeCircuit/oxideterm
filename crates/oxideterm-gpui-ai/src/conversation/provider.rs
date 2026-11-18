use std::hash::Hash;

use crate::{
    state::{AiPreparedPromptUsage, AiWorkspaceEntity, ai_message_backend_for_stream, ai_now_ms},
    stream::{AiStreamDeliverySender, ai_context_token_breakdown_from_prompt},
};
use gpui::{App, Context};
use oxideterm_ai::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, ToolSessionId,
    agent::{AgentExecution, AgentModel, AgentOptions},
    stream_state::{
        ai_diagnostic_event, ai_prompt_token_breakdown, ai_response_reserve, ai_transcript_entry,
    },
    tool_loop::{AiModelBackendServices, AiModelRuntimeState, run_ai_chat_tool_loop},
};

use super::AiConversationController;

pub struct AiProviderTurn {
    pub conversation_id: String,
    pub config: AiChatStreamConfig,
    pub request_content: Option<String>,
    pub history: Vec<AiChatMessage>,
    pub trimmed_count: usize,
    pub context_window: usize,
    pub budget_level: u8,
    pub budget_diagnostic_payload: serde_json::Value,
    pub services: AiModelBackendServices,
    pub agent_options: AgentOptions,
    pub agent_concurrency: usize,
}

impl<Node: Clone + Eq + Hash + 'static, Terminal: Clone + 'static>
    AiConversationController<Node, Terminal>
{
    pub fn begin_ai_chat_stream_with_authority(
        &self,
        conversation_id: &str,
        assistant_id: &str,
        cx: &mut App,
    ) -> (u64, AiStreamDeliverySender, ToolSessionId) {
        let (replaced_generation, generation, sender) = self.state.update(cx, |ai, _cx| {
            let replaced_generation = ai.conversation_stream_generations(conversation_id);
            let (generation, sender) =
                ai.begin_chat_stream(conversation_id.to_owned(), assistant_id.to_owned());
            ai.set_conversation_loading(conversation_id, true);
            (replaced_generation, generation, sender)
        });
        let tool_session = self.authority.update(cx, |runtime, _cx| {
            // The UI run owner chooses which stream was replaced. The broker
            // must not infer that starting a stream cancels every other run.
            for generation in replaced_generation {
                runtime.finish_tool_session(
                    generation,
                    oxideterm_ai::RuntimeRevocationReason::ToolSessionCancelled,
                );
            }
            runtime.begin_tool_session(generation)
        });
        (generation, sender, tool_session)
    }

    pub fn launch_provider_turn(
        &self,
        turn: AiProviderTurn,
        diagnostic: impl Fn(serde_json::Value) -> serde_json::Value,
        cx: &mut App,
    ) {
        let AiProviderTurn {
            conversation_id,
            mut config,
            request_content,
            history,
            trimmed_count,
            context_window,
            budget_level,
            budget_diagnostic_payload,
            services,
            agent_options,
            agent_concurrency,
        } = turn;
        self.state.update(cx, |ai, cx| {
            ai.history.model_contexts.remove(&conversation_id);
            if trimmed_count > 0 {
                show_trim_notice(ai, trimmed_count, cx);
            }
            record_prompt_usage(ai, &conversation_id, &config, &history, context_window);
        });
        let now = ai_now_ms();
        let assistant_id = self.state.update(cx, |ai, _| ai.next_chat_id(now));
        let request_message = self
            .state
            .read(cx)
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(|conversation| {
                conversation
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == AiChatRole::User)
                    .cloned()
            });
        let request_message_id = request_message
            .as_ref()
            .map(|message| message.id.clone())
            .unwrap_or_else(|| format!("{assistant_id}-request"));
        let backend = ai_message_backend_for_stream(&config);
        self.state.update(cx, |ai, _cx| {
            ai.begin_assistant_turn(
                &conversation_id,
                AiChatMessage {
                    id: assistant_id.clone(),
                    role: AiChatRole::Assistant,
                    content: String::new(),
                    timestamp_ms: now,
                    model: Some(config.model.clone()),
                    context: None,
                    is_streaming: true,
                    thinking_content: None,
                    metadata: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    branches: None,
                    suggestions: Vec::new(),
                },
                budget_level,
                backend,
            );
        });
        let mut transcript_entries = Vec::new();
        let mut diagnostic_events = Vec::new();
        if let Some(request_message) = request_message.as_ref() {
            transcript_entries.push(ai_transcript_entry(
                format!("transcript-user-{}", request_message.id),
                &conversation_id,
                "user_message",
                serde_json::json!({
                    "messageId": request_message.id,
                    "role": "user",
                    "content": request_content.as_deref().unwrap_or(&request_message.content),
                    "hasContext": request_message.context.as_ref().is_some_and(|context| !context.is_empty()),
                }),
                None,
                None,
                request_message.timestamp_ms,
            ));
            diagnostic_events.push(ai_diagnostic_event(
                format!("diagnostic-user-{}", request_message.id),
                &conversation_id,
                "user_message",
                None,
                None,
                request_message.timestamp_ms,
                diagnostic(serde_json::json!({
                    "messageId": request_message.id,
                    "role": "user",
                    "contentLength": request_content.as_deref().unwrap_or(&request_message.content).len(),
                    "hasContext": request_message.context.as_ref().is_some_and(|context| !context.is_empty()),
                })),
            ));
        }
        transcript_entries.push(ai_transcript_entry(
            format!("transcript-assistant-start-{assistant_id}"),
            &conversation_id,
            "assistant_turn_start",
            serde_json::json!({
                "messageId": assistant_id,
                "requestMessageId": request_message_id,
                "conversationTurnId": assistant_id,
            }),
            Some(assistant_id.clone()),
            Some(request_message_id),
            now,
        ));
        diagnostic_events.push(ai_diagnostic_event(
            format!("diagnostic-budget-{assistant_id}"),
            &conversation_id,
            "budget_level_changed",
            Some(assistant_id.clone()),
            None,
            now,
            diagnostic(budget_diagnostic_payload),
        ));
        self.state
            .read(cx)
            .persist_transcript_entries(conversation_id.clone(), transcript_entries);
        self.state
            .read(cx)
            .persist_diagnostic_events(conversation_id.clone(), diagnostic_events);
        self.state.update(cx, |ai, _cx| {
            ai.set_conversation_loading(&conversation_id, true)
        });
        // Every model turn receives a fresh authority lease. The token remains
        // transient and never enters conversation history or diagnostics.
        let (generation, ui_tx, tool_session_id) =
            self.begin_ai_chat_stream_with_authority(&conversation_id, &assistant_id, cx);

        let runtime = self.state.read(cx).task_runtime().clone();
        let execution = self.start_ai_agent_group(
            generation,
            &conversation_id,
            &assistant_id,
            &mut config,
            &services,
            &tool_session_id,
            context_window,
            &agent_options,
            agent_concurrency,
            cx,
        );
        let task = runtime.spawn(run_ai_chat_tool_loop(
            config,
            history,
            AiModelRuntimeState { context_window },
            services,
            budget_level,
            generation,
            tool_session_id,
            conversation_id,
            assistant_id,
            ui_tx,
            Some(execution),
        ));
        self.state.update(cx, |ai, cx| {
            ai.set_chat_stream_task(generation, task);
            cx.notify();
        });
    }

    fn start_ai_agent_group(
        &self,
        generation: u64,
        conversation_id: &str,
        assistant_id: &str,
        config: &mut AiChatStreamConfig,
        services: &AiModelBackendServices,
        session: &ToolSessionId,
        context_window: usize,
        options: &AgentOptions,
        concurrency: usize,
        cx: &mut App,
    ) -> AgentExecution {
        let runtime = self.state.read(cx).agents.services.runtime.clone();
        runtime.set_concurrency(concurrency);
        let scope = self.authority.read(cx).agent_scope(
            session,
            config
                .tools
                .iter()
                .filter(|tool| !config.tool_policy.disabled_tools.contains(&tool.name))
                .map(|tool| tool.name.clone())
                .collect(),
        );
        if config.tool_policy.enabled {
            config.tools.extend(
                oxideterm_ai::agent::agent_tool_definitions(false)
                    .into_iter()
                    .filter(|tool| {
                        (options.enabled || tool.name == "ask_user")
                            && !config.tool_policy.disabled_tools.contains(&tool.name)
                    }),
            );
        }
        let run = runtime.create_group(
            conversation_id.to_owned(),
            AgentModel {
                provider_id: config.provider_id.clone().unwrap_or_default(),
                model: config.model.clone(),
            },
            scope,
            config
                .tool_policy
                .max_rounds
                .unwrap_or(oxideterm_settings::DEFAULT_AI_TOOL_MAX_ROUNDS)
                .clamp(
                    oxideterm_settings::MIN_AI_TOOL_MAX_ROUNDS,
                    oxideterm_settings::MAX_AI_TOOL_MAX_ROUNDS,
                ) as usize,
        );
        self.state.update(cx, |ai, _cx| {
            ai.bind_agent_run(generation, run.clone(), false);
            ai.agents.groups.insert(
                run.group_id.clone(),
                crate::state::agents::AiAgentGroup {
                    parent: run.clone(),
                    parent_message_id: assistant_id.to_owned(),
                    config: config.clone(),
                    services: services.clone(),
                    context_window,
                },
            );
        });
        AgentExecution::new(
            runtime,
            run,
            self.state.read(cx).agents.services.resources.clone(),
        )
        .expect("new parent run is active")
    }
}
fn show_trim_notice(ai: &mut AiWorkspaceEntity, count: usize, cx: &mut Context<AiWorkspaceEntity>) {
    let sequence = ai.show_context_trim_notice(count);
    cx.spawn(async move |weak, cx| {
        gpui::Timer::after(std::time::Duration::from_secs(5)).await;
        let _ = weak.update(cx, |ai, cx| {
            if ai.clear_context_trim_notice(sequence) {
                cx.notify();
            }
        });
    })
    .detach();
}

fn record_prompt_usage(
    ai: &mut AiWorkspaceEntity,
    conversation_id: &str,
    config: &AiChatStreamConfig,
    history: &[AiChatMessage],
    max_tokens: usize,
) {
    let reserved = config
        .max_response_tokens
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| *v > 0)
        .unwrap_or_else(|| ai_response_reserve(max_tokens));
    let breakdown = ai_context_token_breakdown_from_prompt(
        ai_prompt_token_breakdown(history, &config.tools, &config.provider_type, reserved),
        max_tokens,
    );
    ai.set_prepared_prompt_usage(AiPreparedPromptUsage {
        conversation_id: conversation_id.to_owned(),
        last_user_message_id: history
            .iter()
            .rev()
            .find(|m| m.role == AiChatRole::User)
            .map(|m| m.id.clone()),
        provider_id: config
            .provider_id
            .clone()
            .unwrap_or_else(|| config.provider_type.clone()),
        model: config.model.clone(),
        breakdown,
    });
}

impl AiWorkspaceEntity {
    pub fn model_backend_services(&self, settings: &oxideterm_settings::AiSettings) -> AiModelBackendServices {
        AiModelBackendServices {
            rag_store: self.rag_store(),
            ai_mcp_registry: self.mcp_registry().clone(),
            ai_key_store: self.key_store().clone(),
            ai_providers: settings.providers.clone(),
            ai_embedding_config: settings.embedding_config.clone(),
            agent_model_limits: oxideterm_ai::provider_views(&settings.providers)
                .into_iter()
                .flat_map(|provider| {
                    provider.models.into_iter().map(move |model| {
                        let context = super::ai_model_context_window(settings, Some(&provider.id), &model);
                        let response: Option<i64> = None;
                        let reasoning = settings
                            .reasoning_model_overrides
                            .get(&provider.id)
                            .and_then(|models| models.get(&model))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("auto");
                        let reasoning = oxideterm_ai::normalize_reasoning_level_for_model(
                            &provider.provider_type,
                            &model,
                            reasoning,
                        )
                        .as_str()
                        .to_owned();
                        ((provider.id.clone(), model), (context, response, reasoning))
                    })
                })
                .collect(),
        }
    }

}
