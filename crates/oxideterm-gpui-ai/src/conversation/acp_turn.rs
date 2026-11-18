use std::hash::Hash;

use gpui::App;
use oxideterm_ai::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiStreamEvent, tool_loop::AiModelBackendServices,
};

use super::AiConversationController;
use crate::{
    acp::AcpApplicationToolTurn,
    state::{ai_acp_session_state, ai_message_backend_for_stream, ai_now_ms},
    stream::{AiStreamDelivery, AiStreamDeliveryEvent},
};

pub struct AiAcpChatLaunch {
    pub launch_config: oxideterm_ai::AcpLaunchConfig,
    pub session_cwd: std::path::PathBuf,
    pub host_policy: oxideterm_ai::AcpHostCapabilityPolicy,
}

impl<Node: Clone + Eq + Hash + 'static, Terminal: Clone + 'static>
    AiConversationController<Node, Terminal>
{
    pub fn start_acp_turn(
        &self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        launch: AiAcpChatLaunch,
        skill_catalog: Option<String>,
        application_tool_services: AiModelBackendServices,
        cx: &mut App,
    ) -> bool {
        let request_message = self
            .state
            .read(cx)
            .history
            .model_contexts
            .get(&conversation_id)
            .and_then(|conversation| {
                conversation
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == AiChatRole::User)
                    .cloned()
            });
        let mut images = request_message
            .as_ref()
            .map(|m| oxideterm_ai::message_images(m).to_vec())
            .unwrap_or_default();
        let user_request = request_content
            .or_else(|| {
                request_message
                    .as_ref()
                    .map(|message| message.content.clone())
            })
            .filter(|request| !request.trim().is_empty() || !images.is_empty());
        let Some(user_request) = user_request else {
            return false;
        };
        let acp_agent_id = launch.launch_config.id.clone();
        let pending_skill_catalog = skill_catalog.and_then(|catalog| {
            use sha2::Digest as _;

            let catalog_hash = format!("{:x}", sha2::Sha256::digest(catalog.as_bytes()));
            let already_known = self
                .state
                .read(cx)
                .conversation_state()
                .conversations
                .iter()
                .find(|conversation| conversation.id == conversation_id)
                .and_then(|conversation| conversation.session_metadata.as_ref())
                .and_then(|metadata| metadata.get("acpSkillCatalogs"))
                .and_then(|catalogs| catalogs.get(&acp_agent_id))
                .and_then(serde_json::Value::as_str)
                == Some(catalog_hash.as_str());
            (!already_known).then_some((catalog, catalog_hash))
        });

        let handoff = self
            .state
            .read(cx)
            .history
            .model_contexts
            .get(&conversation_id)
            .and_then(|conversation| {
                let cursor = ai_acp_session_state(conversation)
                    .filter(|state| state.agent_id == launch.launch_config.id)
                    .and_then(|state| state.handoff_cursor);
                oxideterm_ai::build_acp_conversation_handoff(
                    conversation,
                    request_message
                        .as_ref()
                        .map(|message| message.id.as_str())?,
                    cursor.as_ref(),
                )
            });
        let mut prompt = zeroize::Zeroizing::new(String::new());
        if let Some((handoff, historical_images)) = handoff {
            prompt.push_str(handoff.as_str());
            images.splice(0..0, historical_images);
        }
        self.state.update(cx, |ai, _| {
            ai.history.model_contexts.remove(&conversation_id);
        });
        {
            let mut append_prompt_section = |heading: &str, value: &str| {
                let safe_value = zeroize::Zeroizing::new(oxideterm_ai::sanitize_for_ai(value));
                if !prompt.is_empty() {
                    prompt.push_str("\n\n");
                }
                prompt.push_str("## ");
                prompt.push_str(heading);
                prompt.push('\n');
                prompt.push_str(safe_value.as_str());
            };
            if let Some(instructions) = task_system_prompt.filter(|value| !value.trim().is_empty())
            {
                append_prompt_section("OxideTerm Instructions", &instructions);
            }
            if let Some(memory) = config
                .memory_context
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                append_prompt_section("OxideTerm Scoped Memory", memory);
            }
            if let Some((skill_catalog, _catalog_hash)) = pending_skill_catalog.as_ref() {
                append_prompt_section("OxideTerm Agent Skills", &skill_catalog);
            }
            if let Some(context) = request_message
                .as_ref()
                .and_then(|message| message.context.as_deref())
                .filter(|value| !value.trim().is_empty())
            {
                append_prompt_section("OxideTerm Current Context", context);
            }
            append_prompt_section("User Request", &user_request);
        }

        let now = ai_now_ms();
        let assistant_id = self.state.update(cx, |ai, _| ai.next_chat_id(now));
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
                0,
                backend,
            );
            ai.set_conversation_loading(&conversation_id, true);
        });
        let (generation, ui_tx, tool_session_id) =
            self.begin_ai_chat_stream_with_authority(&conversation_id, &assistant_id, cx);
        let turn_id = format!("{conversation_id}:{generation}");
        let mut config_selections = self
            .state
            .read(cx)
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(ai_acp_session_state)
            .map(|state| state.config_selections)
            .unwrap_or_default();
        if config_selections.is_empty()
            && let Some(model_selection) = config.acp_config_selection
        {
            config_selections.push(model_selection);
        }
        let mode_id = self
            .state
            .read(cx)
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(ai_acp_session_state)
            .and_then(|state| state.current_mode_id);
        let started = self.acp.update(cx, |entity, _cx| {
            entity.start_turn(crate::acp::AcpThreadStart {
                route: crate::acp::AcpTurnRoute {
                    generation,
                    conversation_id: conversation_id.clone(),
                    assistant_id: assistant_id.clone(),
                },
                launch_config: launch.launch_config,
                host_policy: launch.host_policy,
                application_tool_definitions: config.tools.clone(),
                application_tool_turn: AcpApplicationToolTurn {
                    services: application_tool_services,
                    tool_policy: config.tool_policy.clone(),
                    safety_mode: config.safety_mode,
                    profile_id: config.profile_id.clone(),
                    ui_tx,
                    generation,
                    tool_session_id,
                    conversation_id: conversation_id.clone(),
                    assistant_id: assistant_id.clone(),
                    cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                },
                request: oxideterm_ai::AcpManagedPromptRequest {
                    images,
                    thread_id: conversation_id.clone(),
                    turn_id,
                    existing_session_id: config.acp_session_id,
                    cwd: launch.session_cwd,
                    config_selections,
                    mode_id,
                    mcp_servers: Vec::new(),
                    prompt,
                },
            })
        });
        if started && let Some((skill_catalog, catalog_hash)) = pending_skill_catalog {
            if let Some(mut context) = oxideterm_audit::AuditContext::current_request()
                .or_else(oxideterm_audit::AuditContext::current)
            {
                context.source = oxideterm_audit::AuditSource::Ai;
                context.agent_id = Some(oxideterm_audit::redact(&conversation_id));
                context.observe(
                    oxideterm_audit::AuditCategory::Automation,
                    "ai_context_provided",
                    Some(&format!("provider=skills bytes={}", skill_catalog.len())),
                    oxideterm_audit::AuditOutcome::Sent,
                    oxideterm_audit::AuditEvidence::Dispatch,
                    oxideterm_audit::AuditAuthorization::NotRequired,
                );
            }
            self.state.update(cx, |ai, _cx| {
                let Some(conversation) = ai
                    .conversation_state_mut()
                    .conversations
                    .iter_mut()
                    .find(|conversation| conversation.id == conversation_id)
                else {
                    return;
                };
                let metadata = conversation
                    .session_metadata
                    .get_or_insert_with(|| serde_json::json!({}));
                let Some(metadata) = metadata.as_object_mut() else {
                    return;
                };
                let catalogs = metadata
                    .entry("acpSkillCatalogs")
                    .or_insert_with(|| serde_json::json!({}));
                let Some(catalogs) = catalogs.as_object_mut() else {
                    return;
                };
                catalogs.insert(acp_agent_id, serde_json::Value::String(catalog_hash));
            });
        }
        if !started {
            self.state.update(cx, |ai, _cx| {
                ai.enqueue_chat_stream_delivery(AiStreamDelivery {
                    generation,
                    conversation_id,
                    assistant_id,
                    event: AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(
                        "stream_failed".to_string(),
                    )),
                });
            });
        }
        self.state.update(cx, |_, cx| cx.notify());
        true
    }
}

impl<Node: Clone + Eq + std::hash::Hash + 'static, Terminal: Clone + 'static>
    AiConversationController<Node, Terminal>
{
    pub fn select_acp_picker_config(
        &self, agent_id: &str, config_id: String, value_id: String, model: bool, cx: &mut App,
    ) -> bool {
        let state = self.state.read(cx);
        let conversation = state.conversation_state().active_conversation().map(|c| c.id.clone());
        if !model && conversation.is_none() { return false; }
        let options = if model { state.picker_acp_options(agent_id) }
            else { state.active_ai_acp_session_state(agent_id).map(|state| state.config_options)
                    .or_else(|| state.picker_acp_options(agent_id)) };
        let Some(options) = options else { return false; };
        let sent = conversation.as_deref().is_some_and(|conversation| {
            self.acp.update(cx, |acp, _| acp.set_config_selection(conversation, oxideterm_ai::AcpSessionConfigSelection {
                config_id: config_id.clone(), value_id: value_id.clone(),
            }))
        });
        self.state.update(cx, |ai, cx| {
            let accepted = sent || if model {
                ai.set_active_acp_model_selection(agent_id, options, &config_id, &value_id)
            } else {
                ai.set_active_acp_config_selection(agent_id, options, &config_id, &value_id)
            };
            if accepted { cx.notify(); }
            accepted
        })
    }

    pub fn select_acp_picker_mode(&self, agent_id: &str, mode_id: String, cx: &mut App) {
        let Some(conversation) = self.state.read(cx).conversation_state().active_conversation().map(|c| c.id.clone()) else {
            return;
        };
        let sent = self.acp.update(cx, |acp, _| acp.set_mode(&conversation, mode_id.clone()));
        self.state.update(cx, |ai, cx| {
            if sent || ai.set_active_acp_mode(agent_id, &mode_id) { cx.notify(); }
        });
    }
}

impl<Node: Clone + Eq + std::hash::Hash + 'static, Terminal: Clone + 'static>
    AiConversationController<Node, Terminal>
{
    /// The controller owns ACP session policy and permission responses. Only
    /// the global attention notification needs the host window.
    pub fn handle_acp_session_request(
        &self, agent_id: &str, generation: u64, conversation_id: &str, assistant_id: &str,
        event: oxideterm_ai::AcpClientEvent, diagnostic: impl Fn(serde_json::Value) -> serde_json::Value,
        file_review: super::acp_file_review::AcpFileReviewBindings,
        cx: &mut App,
    ) -> Result<bool, oxideterm_ai::AcpClientEvent> {
        match event {
                    oxideterm_ai::AcpClientEvent::RequestPermission {
                        request,
                        response_tx,
                    } => {
                        if !self.state.read(cx).is_chat_stream_generation(generation)
                        {
                            let _ = response_tx
                                .send(Ok(oxideterm_ai::acp_permission_cancelled_response()));
                            return Ok(false);
                        }
                        let projection =
                            oxideterm_ai::acp_permission_request_projection(&request);
                        let permission_options = projection
                            .options
                            .iter()
                            .map(|option| {
                                serde_json::json!({
                                    "id": option.option_id,
                                    "name": option.name,
                                    "kind": option.kind,
                                })
                            })
                            .collect::<Vec<_>>();
                        let (choice_tx, choice_rx) = tokio::sync::oneshot::channel();
                        self.state.update(cx, |ai, cx| {
                            ai.apply_ai_tool_status(generation, conversation_id, assistant_id,
                                &projection.tool_call_id, &projection.name, &projection.arguments, "pending_user_approval",
                                Some(serde_json::json!({ "acpPermissionOptions": permission_options })),
                                Some(projection.risk), Some(projection.summary), false,
                                None, None, None, diagnostic, cx);
                            ai.register_acp_permission_choice(generation, projection.tool_call_id, choice_tx);
                        });
                        let runtime = self.state.read(cx).task_runtime().clone();
                        runtime.spawn(async move {
                            let selected_option_id = choice_rx.await.ok().flatten();
                            let response = oxideterm_ai::acp_permission_response_for_option(
                                &request,
                                selected_option_id.as_deref(),
                            );
                            let _ = response_tx.send(Ok(response));
                        });
                        Ok(true)
                    }
                    oxideterm_ai::AcpClientEvent::ReadTextFile {
                        request,
                        response_tx,
                    } => {
                        let owner_session_id = request.session_id.to_string();
                        let Some((workspace_root, policy)) =
                            self.acp
                                .read(cx)
                                .session_context(agent_id, &owner_session_id)
                        else {
                            let _ = response_tx.send(Err(oxideterm_ai::acp_internal_error(
                                "ACP session context is unavailable",
                            )));
                            return Ok(false);
                        };
                        if !policy.fs_read_text_file {
                            let _ = response_tx.send(Err(oxideterm_ai::acp_method_not_found(
                                "fs/read_text_file",
                            )));
                            return Ok(false);
                        }
                        self.state.read(cx).task_runtime().spawn(async move {
                            let response =
                                oxideterm_ai::resolve_acp_read_text_file_request(
                                    &workspace_root,
                                    &request,
                                )
                                .await;
                            let _ = response_tx.send(response);
                        });
                        Ok(false)
                    }

            event @ oxideterm_ai::AcpClientEvent::WriteTextFile { .. } => {
                self.review_acp_file_write(agent_id, generation, conversation_id, assistant_id, event, file_review, cx);
                Ok(false)
            }
            event => Err(event),
        }
    }
}
