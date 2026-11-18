use crate::workspace::ai_state::{AiQueuedChatTurn, AiStandardConfirmKind};

impl WorkspaceApp {
    pub(in crate::workspace) fn open_ai_safety_confirm(&mut self, cx: &mut Context<Self>) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.open_standard_chat_confirm(AiStandardConfirmKind::Safety);
        });
        // Pointer-opened confirmations do not show keyboard focus until navigation starts.
        self.clear_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn begin_ai_safety_confirm_exit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self.ai.state.update(cx, |ai, _cx| {
            ai.begin_standard_chat_confirm_exit(AiStandardConfirmKind::Safety)
        }) else {
            return false;
        };
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(&self.tokens, oxideterm_gpui_ui::motion::MotionDuration::Control);
        self.ai.state.update(cx, |ai, cx| ai.schedule_standard_confirm_exit(AiStandardConfirmKind::Safety, generation, delay, cx));
        true
    }

    pub(in crate::workspace) fn open_ai_summarize_confirm(&mut self, cx: &mut Context<Self>) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.open_standard_chat_confirm(AiStandardConfirmKind::Summarize);
        });
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn begin_ai_summarize_confirm_exit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self.ai.state.update(cx, |ai, _cx| {
            ai.begin_standard_chat_confirm_exit(AiStandardConfirmKind::Summarize)
        }) else {
            return false;
        };
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(&self.tokens, oxideterm_gpui_ui::motion::MotionDuration::Control);
        self.ai.state.update(cx, |ai, cx| ai.schedule_standard_confirm_exit(AiStandardConfirmKind::Summarize, generation, delay, cx));
        true
    }

    pub(in crate::workspace) fn create_ai_sidebar_conversation(
        &mut self,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.ensure_ai_chat_initialized(cx);
        self.ai.state.update(cx, |ai, cx| ai.create_sidebar_conversation(title, cx))
    }

    pub(in crate::workspace) fn steer_ai_chat_draft(&mut self, cx: &mut Context<Self>) {
        if let Err(key) = self.ai.state.update(cx, |ai, cx| ai.steer_chat_draft(cx)) {
            self.push_ai_settings_toast(self.i18n.t(key), TerminalNoticeVariant::Warning, cx);
        }
    }

    pub(in crate::workspace) fn send_ai_chat_draft(&mut self, cx: &mut Context<Self>) {
        self.ensure_ai_chat_initialized(cx);
        if !self.ai.state.read(cx).history_ready() {
            return;
        }
        let content = self.ai.state.read(cx).chat_ui().draft.trim().to_string();
        let images = self.ai.state.read(cx).chat_ui().images.clone();
        if images.iter().map(|image| image.bytes).sum::<usize>()
            > oxideterm_ai::AI_IMAGE_REQUEST_BYTES
        {
            self.push_ai_settings_toast(
                self.i18n.t("ai.images.request_limit"),
                TerminalNoticeVariant::Warning,
                cx,
            );
            return;
        }
        if self.ai.state.read(cx).chat_ui().image_import.is_some() {
            return;
        }
        if content.is_empty() && images.is_empty() {
            cx.notify();
            return;
        }
        if !self.settings_store.settings().ai.enabled {
            self.push_ai_settings_toast(
                self.i18n.t("ai.chat.disabled_message"),
                TerminalNoticeVariant::Warning,
                cx,
            );
            cx.notify();
            return;
        }
        let question = self.ai.state.read(cx).active_user_question();
        if !images.is_empty() && question.is_some() {
            self.push_ai_settings_toast(
                self.i18n.t("ai.images.queue_only"),
                TerminalNoticeVariant::Warning,
                cx,
            );
            return;
        }
        if let Some((generation, id)) = question {
            if self.ai.state.update(cx, |ai, _| {
                ai.resolve_user_question(generation, &id, zeroize::Zeroizing::new(content))
            }) {
                self.reset_ai_chat_input_after_submit(cx);
            }
            cx.notify();
            return;
        }
        self.bootstrap_ai_mcp_registry(cx);

        let parsed_input = parse_ai_user_input(&content);
        let detected_intent = detect_ai_intent(&parsed_input);
        let sidebar_context = self.resolve_ai_sidebar_context_block(cx);
        let selected_context = self.resolve_ai_selected_terminal_context(cx);
        let reference_context = self.resolve_ai_reference_context(&parsed_input.references, cx);
        let context_sources = [
            selected_context.as_ref().map(|_| "terminal"),
            sidebar_context.as_ref().map(|_| "sidebar"),
            reference_context.as_ref().map(|_| "reference"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        let context =
            ai_chat_message_context([selected_context, sidebar_context, reference_context]);
        let slash_command = parsed_input
            .slash_command
            .as_deref()
            .and_then(resolve_ai_slash_command);
        let explicit_skill =
            if slash_command.is_none() && self.settings_store.settings().ai.skills.enabled {
                parsed_input.slash_command.as_deref().and_then(|skill_id| {
                    let registry = self.ai.skills.read();
                    let record = registry.enabled_record(skill_id)?;
                    let instructions = registry.load(skill_id).ok()?;
                    Some((
                        skill_id.to_string(),
                        record.content_hash.clone(),
                        instructions,
                    ))
                })
            } else {
                None
            };
        if let Some(command) = slash_command.filter(|command| command.client_only) {
            if !images.is_empty() {
                self.push_ai_settings_toast(
                    self.i18n.t("ai.images.queue_only"),
                    TerminalNoticeVariant::Warning,
                    cx,
                );
                return;
            }
            if self.ai.state.read(cx).chat_is_loading() {
                self.push_ai_settings_toast(
                    self.i18n.t("ai.queue.client_command_busy"),
                    TerminalNoticeVariant::Warning,
                    cx,
                );
                return;
            }
            match command.name {
                "clear" => {
                    self.create_ai_sidebar_conversation(None, cx);
                    self.reset_ai_chat_input_after_submit(cx);
                    cx.notify();
                    return;
                }
                "help" => {
                    self.add_ai_help_response(content, cx);
                    return;
                }
                "compact" => {
                    self.start_ai_compact_conversation(cx);
                    self.reset_ai_chat_input_after_submit(cx);
                    cx.notify();
                    return;
                }
                _ => return,
            }
        }

        let stream_config = match self.resolve_ai_stream_config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                cx.notify();
                return;
            }
        };
        if !images.is_empty()
            && stream_config.execution_backend != AiExecutionBackend::Acp
            && oxideterm_ai::model_image_input(
                &stream_config.provider_type,
                &stream_config.model,
                stream_config.image_input,
            ) == Some(false)
        {
            self.push_ai_settings_toast(
                self.i18n.t("ai.images.model_unavailable"),
                TerminalNoticeVariant::Warning,
                cx,
            );
            return;
        }
        if let Some(context_block) = context.as_ref() {
            if let Some(mut audit_context) = oxideterm_audit::AuditContext::current_request()
                .or_else(oxideterm_audit::AuditContext::current)
            {
                if audit_context.source == oxideterm_audit::AuditSource::Application {
                    audit_context.source = oxideterm_audit::AuditSource::User;
                }
                audit_context.observe(
                    oxideterm_audit::AuditCategory::Automation,
                    "ai_context_provided",
                    Some(&format!(
                        "provider={} model={} sources={} bytes={}",
                        stream_config.provider_id.as_deref().unwrap_or("unknown"),
                        stream_config.model,
                        context_sources.join(","),
                        context_block.len()
                    )),
                    oxideterm_audit::AuditOutcome::Sent,
                    oxideterm_audit::AuditEvidence::Dispatch,
                    oxideterm_audit::AuditAuthorization::NotRequired,
                );
            }
        }
        let now = ai_now_ms();
        let title = generate_chat_title(&content);
        let id = self.next_ai_chat_id(now, cx);
        let message = AiChatMessage {
            id: self.next_ai_chat_id(now, cx),
            role: AiChatRole::User,
            content: content.clone(),
            timestamp_ms: now,
            model: Some(stream_config.model.clone()),
            context,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };
        let runtime_system_prompt = self.resolve_ai_sidebar_system_prompt_segment(cx);
        let explicit_skill_prompt = explicit_skill.as_ref().map(
            |(skill_id, _content_hash, instructions)| {
                format!(
                    "## Explicit Agent Skill\nThe user explicitly selected `{skill_id}`. Follow these instructions for this request, subject to the current tool permissions and safety mode.\n\n<skill_instructions id=\"{skill_id}\">\n{}\n</skill_instructions>",
                    oxideterm_ai::sanitize_for_ai(instructions)
                )
            },
        );
        let task_system_prompt = ai_chat_message_context([
            ai_input_system_prompt(slash_command, &parsed_input.participants),
            explicit_skill_prompt,
            runtime_system_prompt,
            ai_detected_intent_system_prompt(&detected_intent),
        ]);
        let active_participant = parsed_input
            .participants
            .first()
            .map(|participant| participant.name.clone());
        let request_text = if stream_config.execution_backend == AiExecutionBackend::Acp
            && parsed_input.slash_command.is_some()
            && slash_command.is_none()
            && explicit_skill.is_none()
        {
            // Agent-advertised ACP commands are protocol input, not OxideTerm
            // system prompts. Preserve the slash prefix for the agent.
            content
        } else {
            parsed_input.clean_text
        };
        let request_content = if request_text.is_empty() && explicit_skill.is_some() {
            Some("Apply the explicitly selected Agent Skill to the current task.".to_string())
        } else {
            (!request_text.is_empty()).then_some(request_text)
        };
        let conversation_id = self.ai.state.update(cx, |ai, _cx| {
            ai.ensure_conversation(id, Some(title), now, None)
        });
        let turn = AiQueuedChatTurn {
            images,
            id: message.id,
            content: zeroize::Zeroizing::new(message.content),
            context: message.context.map(zeroize::Zeroizing::new),
            config: stream_config,
            request_content: request_content.map(zeroize::Zeroizing::new),
            task_system_prompt: task_system_prompt.map(zeroize::Zeroizing::new),
            participant: active_participant,
            skill: explicit_skill.map(|(id, hash, _)| (id, hash)),
        };
        let busy = self.ai.state.read(cx).chat_is_loading();
        self.ai.state.update(cx, |ai, _| {
            ai.queued_chat_turns
                .entry(conversation_id.clone())
                .or_default()
                .push_back(turn)
        });
        self.reset_ai_chat_input_after_submit(cx);
        if !busy {
            self.send_next_queued_ai_message(&conversation_id, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn send_next_queued_ai_message(
        &mut self,
        conversation_id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let enabled = self.settings_store.settings().ai.enabled;
        let Some(pending) = self.ai.state.update(cx, |ai, _| {
            ai.prepare_next_queued_turn(conversation_id, enabled)
        }) else {
            return false;
        };
        self.launch_prepared_queued_turn(pending, cx);
        true
    }

    fn launch_prepared_queued_turn(
        &mut self,
        pending: AiPendingChatStream,
        cx: &mut Context<Self>,
    ) {
        self.record_ai_memory_usage(&pending.config.memory_entry_ids, cx);
        self.start_ai_chat_stream_after_api_key_lookup(
            pending.conversation_id,
            pending.config,
            pending.request_content,
            pending.task_system_prompt,
            cx,
        );
    }

    pub(in crate::workspace) fn apply_ai_queue_action(
        &mut self,
        conversation: &str,
        id: &str,
        action: &str,
        cx: &mut Context<Self>,
    ) {
        match self.ai.state.update(cx, |ai, cx| {
            ai.apply_ai_queue_action(conversation, id, action, cx)
        }) {
            Ok(true) => {
                self.send_next_queued_ai_message(conversation, cx);
            }
            Ok(false) => {}
            Err(key) => {
                self.push_ai_settings_toast(self.i18n.t(key), TerminalNoticeVariant::Warning, cx)
            }
        }
    }

    pub(in crate::workspace) fn interrupt_and_send_ai_chat_draft(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.ai.state.read(cx).chat_ui().draft.trim().is_empty()
            && self.ai.state.read(cx).chat_ui().images.is_empty()
        {
            self.cancel_ai_chat_stream(cx);
            return;
        }
        self.cancel_ai_chat_stream(cx);
        // The replacement takes precedence over queued follow-ups without deleting them.
        let id = self
            .ai
            .state
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone();
        let queued = id.as_ref().and_then(|id| {
            self.ai
                .state
                .update(cx, |ai, _| ai.queued_chat_turns.remove(id))
        });
        self.send_ai_chat_draft(cx);
        if let (Some(id), Some(queued)) = (id, queued) {
            self.ai.state.update(cx, |ai, _| {
                ai.queued_chat_turns.entry(id).or_default().extend(queued)
            });
        }
    }

    pub(in crate::workspace) fn start_ai_chat_stream_after_api_key_lookup(
        &mut self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_ai::state::launch::{AiLaunchBindings, AiLaunchEffect};
        let context_window = self.ai_active_model_context_window(&config);
        let owner = cx.weak_entity();
        let services_owner = owner.clone();
        let bindings = AiLaunchBindings {
            i18n: self.i18n.clone(),
            backend_services: Rc::new(move |cx| {
                Some(
                    services_owner
                        .upgrade()?
                        .read(cx)
                        .ai_model_backend_services(cx),
                )
            }),
            dispatch: Rc::new(move |effect, cx| {
                let _ = owner.update(cx, |app, cx| match effect {
                    AiLaunchEffect::Error(message) => {
                        app.push_ai_settings_toast(message, TerminalNoticeVariant::Error, cx)
                    }
                    AiLaunchEffect::Ready(pending) => {
                        if !app.ai.state.read(cx).chat_launch_matches(
                            &pending.conversation_id,
                            pending.launch_id.as_deref(),
                        ) {
                            return;
                        }
                        if pending.config.execution_backend == AiExecutionBackend::Acp {
                            app.start_acp_chat_thread(
                                pending.conversation_id,
                                pending.config,
                                pending.request_content,
                                pending.task_system_prompt,
                                cx,
                            );
                        } else {
                            app.start_ai_chat_stream_after_budget_preflight(
                                pending.conversation_id,
                                pending.config,
                                pending.request_content,
                                pending.task_system_prompt,
                                pending.rag_system_prompt,
                                true,
                                cx,
                            );
                        }
                    }
                });
            }),
        };
        self.ai.state.update(cx, |ai, cx| {
            ai.prepare_chat_launch(
                AiPendingChatStream {
                    launch_id: None,
                    conversation_id,
                    config,
                    request_content,
                    task_system_prompt,
                    rag_system_prompt: None,
                },
                context_window,
                bindings,
                cx,
            )
        });
    }

    pub(in crate::workspace) fn add_ai_help_response(
        &mut self,
        content: String,
        cx: &mut Context<Self>,
    ) {
        let now = ai_now_ms();
        let title = generate_chat_title(&content);
        let id = self.next_ai_chat_id(now, cx);
        let user_message = AiChatMessage {
            id: self.next_ai_chat_id(now, cx),
            role: AiChatRole::User,
            content,
            timestamp_ms: now,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };
        let assistant_message = AiChatMessage {
            id: self.next_ai_chat_id(now, cx),
            role: AiChatRole::Assistant,
            content: self.ai_help_markdown(),
            timestamp_ms: now,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };
        self.ai.state.update(cx, |ai, _cx| {
            ai.add_help_exchange(id, title, now, user_message, assistant_message);
        });
        self.reset_ai_chat_input_after_submit(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn send_ai_follow_up_suggestion(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.set_chat_draft(text);
        });
        self.send_ai_chat_draft(cx);
    }

    pub(in crate::workspace) fn regenerate_ai_last_response(&mut self, cx: &mut Context<Self>) {
        if self.ai.state.read(cx).chat_is_loading() {
            cx.notify();
            return;
        }
        let stream_config = match self.resolve_ai_stream_config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                cx.notify();
                return;
            }
        };
        self.record_ai_memory_usage(&stream_config.memory_entry_ids, cx);
        let workspace = cx.weak_entity();
        self.ai.state.update(cx, |ai, cx| {
            ai.regenerate_message(stream_config, move |pending, cx| {
                let _ = workspace.update(cx, |app, cx| {
                    app.start_ai_chat_stream_after_api_key_lookup(
                        pending.conversation_id, pending.config, pending.request_content, pending.task_system_prompt, cx,
                    );
                });
            }, cx)
        });
    }

    pub(in crate::workspace) fn delete_ai_message(&mut self, message_id: &str, cx: &mut Context<Self>) {
        self.ai.state.update(cx, |ai, cx| ai.delete_ai_message(message_id, cx));
    }

    pub(in crate::workspace) fn set_ai_safety_mode_default(&mut self, cx: &mut Context<Self>) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.set_active_conversation_safety_mode(AiSafetyMode::Default);
        });
        self.close_ai_safety_mode_menu(cx);
    }

    pub(in crate::workspace) fn set_ai_safety_mode_read_only(&mut self, cx: &mut Context<Self>) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.set_active_conversation_safety_mode(AiSafetyMode::ReadOnly);
        });
        self.close_ai_safety_mode_menu(cx);
    }

    fn close_ai_safety_mode_menu(&mut self, cx: &mut Context<Self>) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.set_chat_popover_open(AiChatPopover::Safety, false);
        });
        self.restore_ai_chat_input_focus_after_safety_mode_change(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn confirm_ai_safety_bypass(&mut self, cx: &mut Context<Self>) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.set_active_conversation_safety_mode(AiSafetyMode::Bypass);
        });
        self.close_ai_safety_mode_menu(cx);
    }

    pub(in crate::workspace) fn restore_ai_chat_input_focus_after_safety_mode_change(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        // Closing the safety menu returns keyboard ownership to the composer so
        // Enter/Space continue the conversation instead of falling through.
        self.ai.state.update(cx, |ai, _cx| {
            ai.focus_chat_input();
        });
        self.ai.state.update(cx, |ai, _cx| {
            ai.set_model_selector_search_focused(false);
        });
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn start_edit_ai_message(&mut self, message_id: String, cx: &mut Context<Self>) {
        let workspace = cx.weak_entity();
        self.ai.state.update(cx, |ai, cx| ai.load_message_edit(message_id, move |cx| {
            let _ = workspace.update(cx, |app, _| app.ime_marked_text = None);
        }, cx));
    }

    pub(in crate::workspace) fn cancel_edit_ai_message(&mut self, cx: &mut Context<Self>) {
        self.ai.state.update(cx, |ai, cx| ai.cancel_message_edit(cx));
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn save_ai_message_edit(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, edited_content)) = self.ai.state.read(cx).editable_message_draft() else { return };
        let stream_config = match self.resolve_ai_stream_config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                cx.notify();
                return;
            }
        };
        self.record_ai_memory_usage(&stream_config.memory_entry_ids, cx);

        let request_content = Some(edited_content.clone());
        let Some(conversation_id) = self.ai.state.update(cx, |ai, cx| ai.commit_message_edit(&message_id, edited_content, &stream_config, cx)) else { return };
        self.ime_marked_text = None;
        self.start_ai_chat_stream_after_api_key_lookup(
            conversation_id,
            stream_config,
            request_content,
            None,
            cx,
        );
        cx.notify();
    }

    pub(in crate::workspace) fn reset_ai_chat_input_after_submit(&mut self, cx: &mut App) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.clear_chat_input_after_submit();
        });
        self.ime_marked_text = None;
    }
}

pub(in crate::workspace) fn ai_chat_message_context(
    contexts: impl IntoIterator<Item = Option<String>>,
) -> Option<String> {
    let blocks = contexts
        .into_iter()
        .flatten()
        .map(|context| context.trim().to_string())
        .filter(|context| !context.is_empty())
        .collect::<Vec<_>>();
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}
