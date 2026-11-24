use gpui::ClipboardItem;

use super::*;

impl AiWorkspaceEntity {
    pub(crate) fn copy_ai_message(
        &mut self,
        message_id: String,
        on_failure: impl FnOnce(&mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(conversation) = self.conversation_state().active_conversation() else {
            return;
        };
        let stored = self.history.store.clone().zip(
            self.history
                .pages
                .get(&conversation.id)
                .and_then(|page| page.descriptions.get(&message_id))
                .cloned(),
        );
        let Some((store, description)) = stored else {
            if let Some(message) = conversation
                .messages
                .iter()
                .find(|message| message.id == message_id)
            {
                cx.write_to_clipboard(ClipboardItem::new_string(message.content.clone()));
            }
            return;
        };
        let conversation = conversation.id.clone();
        let runtime = self.task_runtime.clone();
        let task = cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn_blocking(move || {
                    store.message_content(
                        &conversation,
                        &description.storage_id,
                        description.revision,
                    )
                })
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.history.copy_load.take();
                match result {
                    Ok(Ok(content)) => cx.write_to_clipboard(ClipboardItem::new_string(content)),
                    _ => on_failure(cx),
                }
            });
        });
        self.history.copy_load = Some(task);
    }

    pub(crate) fn switch_ai_message_branch(
        &mut self,
        message_id: String,
        branch_index: usize,
        cx: &mut Context<Self>,
    ) {
        if self.chat_is_loading() {
            return;
        }
        let reference = self
            .conversation_state()
            .active_conversation()
            .and_then(|conversation| {
                let message = conversation
                    .messages
                    .iter()
                    .find(|message| message.id == message_id)?;
                let range = message.branches.as_ref()?.refs.get(&branch_index)?.clone();
                Some((conversation.id.clone(), range))
            });
        if let Some((conversation, range)) = reference {
            let Some(store) = self.history.store.clone() else {
                return;
            };
            let launch = self.next_chat_id(ai_now_ms());
            let (sender, receiver) = tokio::sync::oneshot::channel();
            self.chat_launches
                .insert(conversation.clone(), launch.clone());
            self.set_conversation_loading(&conversation, true);
            self.history_barrier(sender);
            cx.notify();
            let runtime = self.task_runtime.clone();
            let owner = conversation.clone();
            let task = cx.spawn(async move |weak, cx| {
                if receiver.await != Ok(true) {
                    return;
                }
                let id = conversation.clone();
                let result = runtime
                    .spawn_blocking(move || {
                        let mut cursor = None;
                        let mut pages = Vec::new();
                        loop {
                            let page = store.range_messages(&id, &range, cursor.as_ref(), 50)?;
                            pages.push(page.messages);
                            cursor = page.before;
                            if cursor.is_none() {
                                break;
                            }
                        }
                        Ok::<_, anyhow::Error>(
                            pages.into_iter().rev().flatten().collect::<Vec<_>>(),
                        )
                    })
                    .await;
                let _ = weak.update(cx, |ai, cx| {
                    if !ai.chat_launch_matches(&conversation, Some(&launch)) {
                        return;
                    }
                    ai.history.launches.remove(&conversation);
                    ai.set_conversation_loading(&conversation, false);
                    cx.notify();
                    if ai.conversation_state().active_conversation_id.as_deref()
                        != Some(&conversation)
                    {
                        return;
                    }
                    if let Ok(Ok(tail)) = result {
                        if let Some(branches) = ai
                            .conversation_state_mut()
                            .active_conversation_mut()
                            .and_then(|conversation| {
                                conversation
                                    .messages
                                    .iter_mut()
                                    .find(|message| message.id == message_id)
                            })
                            .and_then(|message| message.branches.as_mut())
                        {
                            branches.tails.insert(branch_index, tail);
                        }
                        ai.switch_active_message_branch(&message_id, branch_index, ai_now_ms());
                        ai.clear_message_edit();
                    } else {
                        ai.history_load_failed();
                    }
                });
            });
            self.history.launches.insert(owner, task);
            return;
        }
        if self.switch_active_message_branch(&message_id, branch_index, ai_now_ms()) {
            self.clear_message_edit();
            cx.notify();
        }
    }

    pub(crate) fn request_delete_ai_message(&mut self, message_id: String, cx: &mut Context<Self>) {
        if !self.chat_is_loading() {
            self.open_chat_confirm(
                AiChatConfirmKind::DeleteMessage {
                    message_id: Arc::from(message_id),
                },
                cx,
            );
        }
    }

    pub fn delete_ai_message(&mut self, message_id: &str, cx: &mut Context<Self>) {
        if self.delete_active_message(message_id, ai_now_ms()) {
            self.remove_thinking_expansion(message_id);
            cx.notify();
        }
    }
}

impl AiWorkspaceEntity {
    pub fn regenerate_message(
        &mut self,
        stream_config: oxideterm_ai::AiChatStreamConfig,
        launch: impl FnOnce(AiPendingChatStream, &mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_ai::AiChatRole;
        let Some(conversation_id) = self.conversation_state().active_conversation_id.clone() else {
            return;
        };
        let Some(store) = self.history.store.clone() else {
            return;
        };
        let branch = self
            .history
            .branches
            .get(&conversation_id)
            .cloned()
            .unwrap_or_else(|| "main".into());
        let runtime = self.task_runtime.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let launch_id = self.next_chat_id(ai_now_ms());
        self.chat_launches
            .insert(conversation_id.clone(), launch_id.clone());
        self.set_conversation_loading(&conversation_id, true);
        self.persist_chat_state();
        self.history_barrier(sender);
        let target = conversation_id.clone();
        let task = cx.spawn(async move |weak, cx| {
            if receiver.await != Ok(true) {
                return;
            }
            let id = conversation_id.clone();
            let source_branch = branch.clone();
            let result = runtime
                .spawn_blocking(move || store.retry_messages(&id, &source_branch))
                .await;
            let launch_owner = weak.clone();
            let _ = weak.update(cx, |ai, cx| {
                if ai.history.quitting
                    || !ai.chat_launch_matches(&conversation_id, Some(&launch_id))
                    || ai
                        .history
                        .branches
                        .get(&conversation_id)
                        .is_some_and(|current| current != &branch)
                {
                    return;
                }
                ai.history.launches.remove(&conversation_id);
                ai.set_conversation_loading(&conversation_id, false);
                let Ok(Ok(messages)) = result else {
                    ai.history_load_failed();
                    cx.notify();
                    return;
                };
                let recovery = messages
                    .last()
                    .and_then(oxideterm_ai::agent::recoverable_checkpoint)
                    .map(|checkpoint| checkpoint.resume_prompt());
                if recovery.is_none() {
                    let Some(user) = messages
                        .into_iter()
                        .find(|message| message.role == AiChatRole::User)
                    else {
                        return;
                    };
                    ai.capture_history_before_edit(&conversation_id);
                    if let Some(conversation) = ai
                        .conversation_state_mut()
                        .conversations
                        .iter_mut()
                        .find(|conversation| conversation.id == conversation_id)
                    {
                        if let Some(index) = conversation
                            .messages
                            .iter()
                            .position(|message| message.id == user.id)
                        {
                            conversation.messages.truncate(index);
                        } else {
                            conversation.messages.clear();
                        }
                        conversation.messages.push(user);
                    }
                    ai.truncate_conversation_after_last_user(&conversation_id, ai_now_ms());
                }
                let pending = AiPendingChatStream {
                    conversation_id,
                    config: stream_config,
                    request_content: None,
                    task_system_prompt: recovery,
                    rag_system_prompt: None,
                    launch_id: Some(launch_id),
                };
                cx.defer(move |cx| {
                    let current = launch_owner.upgrade().is_some_and(|owner| {
                        owner.read(cx).chat_launch_matches(
                            &pending.conversation_id,
                            pending.launch_id.as_deref(),
                        )
                    });
                    if current {
                        launch(pending, cx);
                    }
                });
                cx.notify();
            });
        });
        self.history.launches.insert(target, task);
        cx.notify();
    }

    pub fn load_message_edit(
        &mut self,
        message_id: String,
        on_loaded: impl FnOnce(&mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_ai::AiChatRole;
        if self.chat_is_loading() {
            return;
        }
        let Some(conversation_id) = self.conversation_state().active_conversation_id.clone() else {
            return;
        };
        let Some(store) = self.history.store.clone() else {
            return;
        };
        let branch = self
            .history
            .branches
            .get(&conversation_id)
            .cloned()
            .unwrap_or_else(|| "main".into());
        let runtime = self.task_runtime.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.set_conversation_loading(&conversation_id, true);
        self.persist_chat_state();
        self.history_barrier(sender);
        let task = cx.spawn(async move |weak, cx| {
            if receiver.await != Ok(true) {
                return;
            }
            let id = conversation_id.clone();
            let source_branch = branch.clone();
            let message = message_id.clone();
            let result = runtime
                .spawn_blocking(move || store.message_by_id(&id, &source_branch, &message))
                .await;
            let _ = weak.update(cx, |ai, cx| {
                ai.history.edit_load.take();
                ai.set_conversation_loading(&conversation_id, false);
                if !ai.history.quitting
                    && ai.conversation_state().active_conversation_id.as_deref()
                        == Some(&conversation_id)
                    && ai
                        .history
                        .branches
                        .get(&conversation_id)
                        .is_none_or(|current| current == &branch)
                {
                    match result {
                        Ok(Ok(message)) if message.role == AiChatRole::User => {
                            let content = message.content.clone();
                            if let Some(slot) = ai
                                .conversation_state_mut()
                                .active_conversation_mut()
                                .and_then(|conversation| {
                                    conversation
                                        .messages
                                        .iter_mut()
                                        .find(|item| item.id == message_id)
                                })
                            {
                                *slot = message;
                            }
                            ai.begin_message_edit(message_id, content);
                            ai.set_model_selector_search_focused(false);
                        }
                        _ => ai.history_load_failed(),
                    }
                }
                cx.defer(on_loaded);
                cx.notify();
            });
        });
        self.history.edit_load = Some(task);
        cx.notify();
    }

    pub fn cancel_message_edit(&mut self, cx: &mut Context<Self>) {
        if self.history.edit_load.take().is_some() {
            if let Some(id) = self.conversation_state().active_conversation_id.clone() {
                self.set_conversation_loading(&id, false);
            }
        }
        self.clear_message_edit();
        cx.notify();
    }

    pub fn editable_message_draft(&self) -> Option<(String, String)> {
        if self.chat_is_loading() {
            return None;
        }
        let draft = self.chat_ui().editing_message_draft.trim();
        if draft.is_empty() {
            return None;
        }
        let id = self.chat_ui().editing_message_id.as_ref()?;
        let message = self
            .conversation_state()
            .active_conversation()?
            .messages
            .iter()
            .find(|message| &message.id == id)?;
        (message.role == oxideterm_ai::AiChatRole::User).then(|| (id.clone(), draft.to_owned()))
    }

    pub fn commit_message_edit(
        &mut self,
        message_id: &str,
        content: String,
        config: &oxideterm_ai::AiChatStreamConfig,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let now = ai_now_ms();
        let next = self.next_chat_id(now);
        let id = self.replace_active_user_message(
            message_id,
            next,
            content,
            config.model.clone(),
            ai_message_backend_for_stream(config),
            now,
        )?;
        self.clear_message_edit();
        cx.notify();
        Some(id)
    }

    pub fn create_sidebar_conversation(
        &mut self,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if !self.history_ready() {
            return None;
        }
        let now = ai_now_ms();
        let id = self.next_chat_id(now);
        let id = self.create_chat_conversation(id, title, now);
        self.reset_chat_for_new_conversation();
        cx.notify();
        Some(id)
    }

    pub fn close_chat_confirm(
        &mut self,
        kind: AiStandardConfirmKind,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self.begin_standard_chat_confirm_exit(kind) else {
            return false;
        };
        self.schedule_standard_confirm_exit(kind, generation, delay, cx);
        true
    }

    pub fn restore_composer_focus(&mut self) {
        self.focus_chat_input();
        self.set_model_selector_search_focused(false);
    }

    pub fn select_chat_safety_mode(
        &mut self,
        mode: oxideterm_gpui_ui::ai::AiSafetyMode,
        cx: &mut Context<Self>,
    ) {
        self.set_active_conversation_safety_mode(mode);
        self.set_chat_popover_open(super::AiChatPopover::Safety, false);
        self.restore_composer_focus();
        cx.notify();
    }

    pub fn schedule_standard_confirm_exit(
        &mut self,
        kind: AiStandardConfirmKind,
        generation: u64,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        if delay.is_zero() {
            if self.finish_standard_chat_confirm_exit(kind, generation) {
                cx.notify();
            }
            return;
        }
        cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(delay).await;
            let _ = weak.update(cx, |ai, cx| {
                if ai.finish_standard_chat_confirm_exit(kind, generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn save_conversation_rename(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.chat_ui().renaming_conversation_id.clone() else {
            return false;
        };
        let draft = self
            .chat_ui()
            .renaming_conversation_draft
            .trim()
            .to_string();
        if draft.is_empty() {
            self.clear_conversation_rename();
        } else {
            self.rename_conversation(&id, draft, ai_now_ms());
            self.persist_chat_state();
        }
        cx.notify();
        true
    }
}
