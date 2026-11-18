#[derive(Clone, Copy, Debug)]
struct AiMessageViewport {
    top: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
struct AiChatListViewportSnapshot {
    item_ix: usize,
    offset_in_item: f32,
    height: f32,
}

fn ai_chat_trim_notice_signature(sequence: u64, count: usize) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&"trim", &mut hasher);
    std::hash::Hash::hash(&sequence, &mut hasher);
    std::hash::Hash::hash(&count, &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

fn ai_chat_bottom_spacer_signature() -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&"spacer", &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

pub fn ai_chat_message_base_signature(message: &AiChatMessage) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let role = match message.role {
        AiChatRole::User => 0u8,
        AiChatRole::Assistant => 1,
        AiChatRole::System => 2,
        AiChatRole::Tool => 3,
    };
    std::hash::Hash::hash(&"message", &mut hasher);
    std::hash::Hash::hash(&message.id, &mut hasher);
    std::hash::Hash::hash(&role, &mut hasher);
    std::hash::Hash::hash(&message.content, &mut hasher);
    std::hash::Hash::hash(&message.thinking_content, &mut hasher);
    std::hash::Hash::hash(&message.is_streaming, &mut hasher);
    std::hash::Hash::hash(&message.timestamp_ms, &mut hasher);
    std::hash::Hash::hash(&message.model, &mut hasher);
    std::hash::Hash::hash(&message.context, &mut hasher);
    std::hash::Hash::hash(&message.tool_call_id, &mut hasher);
    std::hash::Hash::hash(&message.tool_calls.len(), &mut hasher);
    std::hash::Hash::hash(&message.suggestions.len(), &mut hasher);
    if let Some(branches) = message.branches.as_ref() {
        std::hash::Hash::hash(&branches.total, &mut hasher);
        std::hash::Hash::hash(&branches.active_index, &mut hasher);
    }
    if let Some(turn) = message.turn.as_ref() {
        // Hash JSON in place. Serializing tool output here would allocate a
        // secret-capable temporary string on every invalidated render row.
        ai_chat_hash_json_value(turn, &mut hasher);
    }
    if let Some(metadata) = message.metadata.as_ref() {
        std::hash::Hash::hash(&metadata.kind, &mut hasher);
        std::hash::Hash::hash(&metadata.original_count, &mut hasher);
        for image in &metadata.images {
            std::hash::Hash::hash(&image.id, &mut hasher);
            std::hash::Hash::hash(&(image.width, image.height), &mut hasher);
        }
    }
    if let Some(transcript_ref) = message.transcript_ref.as_ref() {
        ai_chat_hash_json_value(transcript_ref, &mut hasher);
    }
    if let Some(summary_ref) = message.summary_ref.as_ref() {
        ai_chat_hash_json_value(summary_ref, &mut hasher);
    }
    for tool_call in &message.tool_calls {
        ai_chat_hash_json_value(tool_call, &mut hasher);
    }
    std::hash::Hasher::finish(&hasher)
}

fn ai_chat_message_list_signature(base_signature: u64, thinking_expanded: Option<&bool>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&base_signature, &mut hasher);
    std::hash::Hash::hash(&thinking_expanded, &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

pub fn ai_chat_hash_json_value(
    value: &serde_json::Value,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) {
    match value {
        serde_json::Value::Null => std::hash::Hash::hash(&0u8, hasher),
        serde_json::Value::Bool(value) => {
            std::hash::Hash::hash(&1u8, hasher);
            std::hash::Hash::hash(value, hasher);
        }
        serde_json::Value::Number(value) => {
            std::hash::Hash::hash(&2u8, hasher);
            if let Some(value) = value.as_i64() {
                std::hash::Hash::hash(&0u8, hasher);
                std::hash::Hash::hash(&value, hasher);
            } else if let Some(value) = value.as_u64() {
                std::hash::Hash::hash(&1u8, hasher);
                std::hash::Hash::hash(&value, hasher);
            } else if let Some(value) = value.as_f64() {
                std::hash::Hash::hash(&2u8, hasher);
                std::hash::Hash::hash(&value.to_bits(), hasher);
            }
        }
        serde_json::Value::String(value) => {
            std::hash::Hash::hash(&3u8, hasher);
            std::hash::Hash::hash(value, hasher);
        }
        serde_json::Value::Array(values) => {
            std::hash::Hash::hash(&4u8, hasher);
            std::hash::Hash::hash(&values.len(), hasher);
            for value in values {
                ai_chat_hash_json_value(value, hasher);
            }
        }
        serde_json::Value::Object(values) => {
            std::hash::Hash::hash(&5u8, hasher);
            std::hash::Hash::hash(&values.len(), hasher);
            for (key, value) in values {
                std::hash::Hash::hash(key, hasher);
                ai_chat_hash_json_value(value, hasher);
            }
        }
    }
}

impl AiAssistantSidebarView {
    pub fn render_ai_sidebar_chat_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(detail) = self.render_ai_agent_detail(cx) {
            return detail;
        }
        if let Some(error) = self.state.read(cx).chat_initialization_error().copied() {
            return self.render_ai_sidebar_initialization_error(error, cx);
        }
        let Some((conversation_id, items, signatures, prompts, message_end)) = ({
            let ai = self.state.read(cx);
            let conversation = ai.conversation_state().active_conversation();
            conversation.and_then(|conversation| {
                if conversation.messages.is_empty() {
                    return None;
                }
                let chat_ui = ai.chat_ui();
                let mut items = Vec::with_capacity(conversation.messages.len().saturating_add(2));
                let mut signatures =
                    Vec::with_capacity(conversation.messages.len().saturating_add(2));
                let page = ai.history.pages.get(&conversation.id);
                if page.is_some_and(|page| page.before.is_some()) {
                    items.push(AiChatListItem::HistoryPage { older: true });
                    signatures.push(0x686973746f727901);
                }
                if let Some(count) = chat_ui.context_trim_notice_count {
                    let sequence = chat_ui.context_trim_notice_sequence;
                    items.push(AiChatListItem::TrimNotice { count });
                    signatures.push(ai_chat_trim_notice_signature(sequence, count));
                }
                let last_assistant_index = conversation
                    .messages
                    .iter()
                    .rposition(|message| message.role == AiChatRole::Assistant);
                let mut prompts = Vec::new();
                let mut signature_cache = chat_ui.message_signature_cache.borrow_mut();
                signature_cache.select_conversation(&conversation.id);
                for (index, message) in conversation.messages.iter().enumerate() {
                    let base_signature = signature_cache
                        .signature_for(&message.id, || ai_chat_message_base_signature(message));
                    let signature = ai_chat_message_list_signature(
                        base_signature
                            ^ ai.agents
                                .parent_revisions
                                .get(&message.id)
                                .copied()
                                .unwrap_or_default()
                            ^ ai.history
                                .archives
                                .get(&(conversation.id.clone(), message.id.clone()))
                                .map(|view| view.revision.rotate_left(16))
                                .unwrap_or_default()
                            ^ page
                                .and_then(|page| page.body_revisions.get(&message.id))
                                .copied()
                                .unwrap_or_default()
                                .rotate_left(24),
                        chat_ui.thinking_expansion_state.get(&message.id),
                    );
                    if message.role == AiChatRole::User {
                        prompts.push(AiStickyPrompt {
                            list_index: items.len(),
                            message_index: index,
                        });
                    }
                    items.push(AiChatListItem::Message {
                        index,
                        last_assistant: last_assistant_index == Some(index),
                    });
                    signatures
                        .push(signature ^ self.disclosure_motions.message_signature(&message.id));
                }
                let message_end = items.len();
                if page.is_some_and(|page| page.after.is_some()) {
                    items.push(AiChatListItem::HistoryPage { older: false });
                    signatures.push(0x686973746f727902);
                }
                if signature_cache.needs_prune(conversation.messages.len()) {
                    let retained_message_ids = conversation
                        .messages
                        .iter()
                        .map(|message| message.id.as_str())
                        .collect::<HashSet<_>>();
                    signature_cache.prune(&retained_message_ids);
                }
                items.push(AiChatListItem::BottomSpacer);
                signatures.push(ai_chat_bottom_spacer_signature());
                Some((
                    conversation.id.clone(),
                    items,
                    signatures,
                    prompts,
                    message_end,
                ))
            })
        }) else {
            return self.render_ai_sidebar_empty_chat(cx);
        };

        let virtual_spec = ai_chat_virtual_list_spec();
        self.sync_ai_chat_list_state(&conversation_id, &signatures, virtual_spec, cx);

        let entity = cx.entity();
        let state = self.state.read(cx).chat_ui().message_list_state.clone();
        let viewport = self.ai_chat_list_viewport_snapshot(cx);
        let scrollbar =
            oxideterm_gpui_ui::scroll::Scrollbar::for_list(&state).id("ai-chat-scrollbar");
        let list = tauri_virtual_list(state.clone(), virtual_spec, move |index, _window, cx| {
            let Some(item) = items.get(index).cloned() else {
                return div().into_any_element();
            };
            let message_viewport = Self::ai_message_viewport_for_list_item(index, viewport);
            entity.update(cx, |this, cx| {
                this.render_ai_chat_list_item(item, message_viewport, cx)
            })
        })
        .w_full()
        .h_full();
        let entity = cx.entity();
        let sticky = ai_sticky_prompt_overlay(
            list.into_any_element(),
            state,
            prompts,
            message_end,
            move |prompt, _, cx| {
                entity.update(cx, |this, cx| {
                    this.render_ai_sticky_prompt(&conversation_id, prompt, cx)
                })
            },
        );
        div()
            .relative()
            .size_full()
            .min_h_0()
            .child(sticky)
            .child(scrollbar)
            .into_any_element()
    }

    fn render_ai_chat_list_item(
        &self,
        item: AiChatListItem,
        viewport: Option<AiMessageViewport>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match item {
            AiChatListItem::HistoryPage { older } => {
                let ai = self.state.read(cx);
                let Some(id) = ai.conversation_state().active_conversation_id.clone() else {
                    return div().into_any_element();
                };
                let loading = ai.history.pages.get(&id).is_some_and(|page| page.loading);
                let label = self.i18n.t(if loading {
                    "ai.history.loading"
                } else if older {
                    "ai.history.older"
                } else {
                    "ai.history.newer"
                });
                if !loading && ai.history.pages.get(&id).is_some_and(|page| !page.failed) {
                    self.state.update(cx, |ai, cx| {
                        ai.request_visible_history_page(id.clone(), older, cx)
                    });
                }
                ai_message_action(
                    &self.tokens,
                    label,
                    Self::render_lucide_icon(
                        if older {
                            LucideIcon::ChevronLeft
                        } else {
                            LucideIcon::ChevronDown
                        },
                        14.0,
                        rgb(self.tokens.ui.text_muted),
                    ),
                    false,
                )
                .id(if older {
                    "ai-history-older"
                } else {
                    "ai-history-newer"
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state
                        .update(cx, |ai, _| ai.request_history_page(id.clone(), older));
                    cx.notify();
                }))
                .into_any_element()
            }
            AiChatListItem::TrimNotice { count, .. } => self.render_ai_trim_notice(count, cx),
            AiChatListItem::Message {
                index,
                last_assistant,
            } => {
                let live = {
                    let ai = self.state.read(cx);
                    ai.conversation_state()
                        .active_conversation()
                        .and_then(|conversation| {
                            conversation.messages.get(index).and_then(|message| {
                                let owner = crate::state::history::HistoryViewOwner::Main(
                                    conversation.id.clone(),
                                );
                                ai.live_history_view(&owner, &message.id)
                                    .map(|view| (owner, view))
                            })
                        })
                };
                if let Some((owner, view)) = live {
                    return self.render_ai_owned_message(owner, view, last_assistant, viewport, cx);
                }
                let (message, last_assistant) = {
                    let ai = self.state.read(cx);
                    let Some(conversation) = ai.conversation_state().active_conversation() else {
                        return div().into_any_element();
                    };
                    let Some(message) = conversation.messages.get(index) else {
                        return div().into_any_element();
                    };
                    // Release the Entity read guard before GPUI mutably borrows
                    // its context; only the visible message is copied.
                    (message.clone(), last_assistant)
                };
                let stored = {
                    let ai = self.state.read(cx);
                    let id = ai
                        .conversation_state()
                        .active_conversation_id
                        .clone()
                        .unwrap_or_default();
                    ai.history
                        .pages
                        .get(&id)
                        .filter(|page| page.descriptions.contains_key(&message.id))
                        .filter(|_| {
                            !message.is_streaming
                                && !ai.history_message_is_pending(&id, &message.id)
                        })
                        .map(|page| {
                            (
                                id,
                                page.bodies
                                    .get(&message.id)
                                    .and_then(std::sync::Weak::upgrade),
                                page.body_errors.contains(&message.id),
                            )
                        })
                };
                if let Some((conversation, view, failed)) = stored {
                    if let Some(view) = view {
                        return self.render_ai_stored_message(
                            &conversation,
                            view,
                            last_assistant,
                            viewport,
                            cx,
                        );
                    }
                    if !failed {
                        self.state.update(cx, |ai, _| {
                            ai.request_history_body(
                                conversation.clone(),
                                message.id.clone(),
                                None,
                                false,
                            )
                        });
                    }
                    let id = message.id.clone();
                    return div()
                        .flex()
                        .flex_col()
                        .child(self.render_ai_history_message(&message, false, viewport, true, cx))
                        .child(
                            ai_message_action(
                                &self.tokens,
                                self.i18n.t(if failed {
                                    "common.actions.retry"
                                } else {
                                    "ai.history.loading"
                                }),
                                Self::render_lucide_icon(
                                    LucideIcon::RefreshCw,
                                    12.0,
                                    rgb(self.tokens.ui.text_muted),
                                ),
                                false,
                            )
                            .id(gpui::SharedString::from(format!(
                                "ai-history-retry-{}",
                                message.id
                            )))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.state.update(cx, |ai, _| {
                                        ai.request_history_body(
                                            conversation.clone(),
                                            id.clone(),
                                            None,
                                            true,
                                        )
                                    });
                                    cx.notify();
                                },
                            )),
                        )
                        .into_any_element();
                }
                self.render_ai_message(&message, last_assistant, viewport, cx)
            }
            AiChatListItem::BottomSpacer => div().h(px(16.0)).into_any_element(),
        }
    }

    fn ai_chat_list_viewport_snapshot(&self, cx: &App) -> Option<AiChatListViewportSnapshot> {
        let bounds = self
            .state
            .read(cx)
            .chat_ui()
            .message_list_state
            .viewport_bounds();
        let height = f32::from(bounds.size.height);
        if height <= 0.0 {
            return None;
        }
        let scroll_top = self
            .state
            .read(cx)
            .chat_ui()
            .message_list_state
            .logical_scroll_top();
        Some(AiChatListViewportSnapshot {
            item_ix: scroll_top.item_ix,
            offset_in_item: f32::from(scroll_top.offset_in_item),
            height,
        })
    }

    fn ai_message_viewport_for_list_item(
        index: usize,
        viewport: Option<AiChatListViewportSnapshot>,
    ) -> Option<AiMessageViewport> {
        let viewport = viewport?;
        let top = if index < viewport.item_ix {
            f32::MAX / 4.0
        } else if index == viewport.item_ix {
            (viewport.offset_in_item - AI_MARKDOWN_CONTENT_OFFSET_PX).max(0.0)
        } else {
            0.0
        };
        Some(AiMessageViewport {
            top,
            height: viewport.height,
        })
    }

    pub fn sync_ai_chat_list_state(
        &mut self,
        conversation_id: &str,
        signatures: &[u64],
        spec: TauriVirtualListSpec,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |ai, _cx| {
            // Opening a conversation starts at its newest message. GPUI's tail
            // mode then pauses automatically while the user reads older content.
            ai.sync_chat_message_list(conversation_id, signatures, spec);
            ai.restore_history_scroll_anchor(conversation_id);
        });
    }

    pub fn render_ai_sidebar_initialization_error(
        &self,
        error: AiChatInitializationError,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let message = self.i18n.t(error.message_key);
        div()
            .w_full()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(24.0))
            .text_align(gpui::TextAlign::Center)
            .gap(px(12.0))
            .bg(self.context_sidebar_content_background(self.tokens.ui.shell_bg()))
            .child(
                div()
                    .size(px(48.0))
                    .flex()
                    .rounded(px(self.tokens.radii.md))
                    .items_center()
                    .justify_center()
                    .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                    .child(Self::render_lucide_icon(
                        LucideIcon::AlertTriangle,
                        20.0,
                        rgb(self.tokens.ui.error),
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .max_w(px(260.0))
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "ai-chat-load-failed",
                                "title",
                                self.i18n.t("ai.chat.load_failed_title"),
                                self.tokens.ui.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "ai-chat-load-failed",
                                "message",
                                message,
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    ),
            )
            .when(error.can_retry, |container| {
                container.child(
                    div()
                        .px(px(16.0))
                        .py(px(6.0))
                        .rounded(px(self.tokens.radii.md))
                        .bg(rgb(self.tokens.ui.accent))
                        .text_color(rgb(self.tokens.ui.bg))
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .cursor_pointer()
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::NonSelectable,
                            "ai-chat-load-failed",
                            "retry",
                            self.i18n.t("common.actions.retry"),
                            self.tokens.ui.bg,
                            cx,
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.retry_ai_chat_initialization(cx);
                                cx.stop_propagation();
                            }),
                        ),
                )
            })
            .into_any_element()
    }

    pub fn render_ai_sidebar_empty_chat(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .p(px(24.0))
            .pt(px(48.0))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .mb(px(24.0))
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .whitespace_nowrap()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-chat-empty",
                        "get-started",
                        self.i18n.t("ai.chat.get_started"),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(self.render_ai_quick_prompt(
                        LucideIcon::HelpCircle,
                        self.i18n.t("ai.quick_prompts.explain_command"),
                        self.i18n.t("ai.quick_prompts.explain_command_prompt"),
                        cx,
                    ))
                    .child(self.render_ai_quick_prompt(
                        LucideIcon::Terminal,
                        self.i18n.t("ai.quick_prompts.find_files"),
                        self.i18n.t("ai.quick_prompts.find_files_prompt"),
                        cx,
                    ))
                    .child(self.render_ai_quick_prompt(
                        LucideIcon::FileCode,
                        self.i18n.t("ai.quick_prompts.write_script"),
                        self.i18n.t("ai.quick_prompts.write_script_prompt"),
                        cx,
                    ))
                    .child(self.render_ai_quick_prompt(
                        LucideIcon::Zap,
                        self.i18n.t("ai.quick_prompts.optimize_command"),
                        self.i18n.t("ai.quick_prompts.optimize_command_prompt"),
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub fn render_ai_quick_prompt(
        &self,
        icon: LucideIcon,
        label: String,
        prompt: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(10.0))
            .py(px(7.0))
            .text_size(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgba((self.tokens.ui.border << 8) | 0x1a))
                    .text_color(rgb(self.tokens.ui.text))
            })
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(div().flex_1().min_w_0().truncate().child(
                // Quick prompts are clickable commands; label text must not steal the row click.
                self.render_display_text_with_role(
                    SelectableTextRole::NonSelectable,
                    "ai-quick-prompt",
                    label.clone(),
                    label,
                    self.tokens.ui.text_muted,
                    cx,
                ),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.state.update(cx, |ai, _cx| {
                        ai.set_chat_draft(prompt.clone());
                        ai.focus_chat_input();
                    });
                    (this.bindings.request)(AiTranscriptRequest::FocusComposer, Some(window), cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_ai_trim_notice(&self, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let warning_color = self.tokens.ui.warning;
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(rgba((warning_color << 8) | 0x33))
            .bg(rgba((warning_color << 8) | 0x1a))
            .child(Self::render_lucide_icon(
                LucideIcon::Scissors,
                12.0,
                rgb(warning_color),
            ))
            .child(
                div()
                    .flex_1()
                    .text_size(px(10.0))
                    .text_color(rgb(warning_color))
                    .child(
                        self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "ai-trim-notice",
                            count,
                            self.i18n
                                .t("ai.context.messages_trimmed")
                                .replace("{{count}}", &count.to_string()),
                            warning_color,
                            cx,
                        ),
                    ),
            )
            .into_any_element()
    }
}
