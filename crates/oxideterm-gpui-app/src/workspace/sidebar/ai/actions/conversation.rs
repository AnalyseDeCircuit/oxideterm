use crate::workspace::ai_state::AiStandardConfirmKind;
use oxideterm_gpui_ai::state::conversation_queue::{AiDraftContext, AiDraftSubmission, AiLocalChatCommand};

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
        let enabled = self.settings_store.settings().ai.enabled;
        let draft = match self.ai.state.update(cx, |ai, cx| ai.prepare_draft_submission(enabled, cx)) {
            AiDraftSubmission::Ignore => return,
            AiDraftSubmission::Warning(key) => {
                self.push_ai_settings_toast(self.i18n.t(key), TerminalNoticeVariant::Warning, cx);
                return;
            }
            AiDraftSubmission::Answered => {
                self.ime_marked_text = None;
                return;
            }
            AiDraftSubmission::Prepare(draft) => draft,
        };
        self.bootstrap_ai_mcp_registry(cx);
        match draft.local_command(self.ai.state.read(cx).chat_is_loading()) {
            Ok(Some(AiLocalChatCommand::Clear)) => {
                self.create_ai_sidebar_conversation(None, cx);
                self.reset_ai_chat_input_after_submit(cx);
                return;
            }
            Ok(Some(AiLocalChatCommand::Help)) => {
                self.ai.state.update(cx, |ai, cx| ai.add_draft_help(draft, &self.i18n, cx));
                self.ime_marked_text = None;
                return;
            }
            Ok(Some(AiLocalChatCommand::Compact)) => {
                self.start_ai_compact_conversation(cx);
                self.reset_ai_chat_input_after_submit(cx);
                return;
            }
            Ok(Some(AiLocalChatCommand::Ignore)) => return,
            Err(key) => {
                self.push_ai_settings_toast(self.i18n.t(key), TerminalNoticeVariant::Warning, cx);
                return;
            }
            Ok(None) => {}
        }

        let sidebar = self.resolve_ai_sidebar_context_block(cx);
        let terminal = self.resolve_ai_selected_terminal_context(cx);
        let references = self.resolve_ai_reference_context(&draft.input.references, cx);
        let config = match self.resolve_ai_stream_config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                return;
            }
        };
        if let Err(key) = draft.check_image_support(&config) {
            self.push_ai_settings_toast(self.i18n.t(key), TerminalNoticeVariant::Warning, cx);
            return;
        }
        let runtime_prompt = self.resolve_ai_sidebar_system_prompt_segment(cx);
        let skills = self.settings_store.settings().ai.skills.enabled.then_some(&self.ai.skills);
        let pending = self.ai.state.update(cx, |ai, cx| {
            ai.enqueue_submitted_draft(draft, AiDraftContext { terminal, sidebar, references }, runtime_prompt, config, skills, cx)
        });
        self.ime_marked_text = None;
        if let Some(pending) = pending {
            self.launch_prepared_queued_turn(pending, cx);
        }
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
        self.ai.state.update(cx, |ai, cx| {
            ai.clear_chat_input_after_submit();
            cx.notify();
        });
        self.ime_marked_text = None;
    }
}
