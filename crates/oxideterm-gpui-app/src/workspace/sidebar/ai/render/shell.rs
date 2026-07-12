impl WorkspaceApp {
    pub(in crate::workspace) fn render_ai_sidebar_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_ai_chat_initialized();
        let enabled = self.settings_store.settings().ai.enabled;
        let panel = if !enabled {
            ai_chat_panel(&self.tokens)
                .relative()
                .bg(self.context_sidebar_content_background(self.tokens.ui.bg))
                .child(self.render_ai_sidebar_disabled(cx))
                .into_any_element()
        } else {
            ai_chat_panel(&self.tokens)
                .relative()
                .bg(self.context_sidebar_content_background(self.tokens.ui.bg))
                .child(self.render_ai_sidebar_chat_header(cx))
                .when_some(self.render_ai_compaction_notice(cx), |panel, notice| {
                    panel.child(notice)
                })
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .child(
                            div()
                                .id("ai-sidebar-scroll")
                                .w_full()
                                .min_w_0()
                                .h_full()
                                .min_h_0()
                                .child(self.render_ai_sidebar_chat_body(cx)),
                        ),
                )
                .child(self.render_ai_context_warning_banners(cx))
                .child(self.render_ai_sidebar_model_bar(cx))
                .child(
                    self.render_ai_sidebar_input(self.ai.chat.initialization_error.is_none(), cx),
                )
                .into_any_element()
        };

        let workspace = cx.entity();
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(select_anchor_probe(
                SelectAnchorId::AiPanelRoot,
                div()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // The probe is a custom Element used only for bounds. Keep a
                    // normal sized flex wrapper around the actual AI panel so
                    // header, body, model bar, and input inherit the sidebar width.
                    .child(panel),
                Self::deferred_ai_select_anchor_update(workspace),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_sidebar_chat_body(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(error) = self.ai.chat.initialization_error.clone() {
            return self.render_ai_sidebar_initialization_error(error, cx);
        }
        let Some((conversation_id, items, signatures)) = self
            .ai
            .chat
            .conversation_state
            .active_conversation()
            .and_then(|conversation| {
                if conversation.messages.is_empty() {
                    return None;
                }
                let mut items = Vec::new();
                if let Some(count) = self.ai.chat.context_trim_notice_count {
                    items.push(AiChatListItem::TrimNotice {
                        sequence: self.ai.chat.context_trim_notice_sequence,
                        count,
                    });
                }
                for message in &conversation.messages {
                    items.push(AiChatListItem::Message {
                        id: message.id.clone(),
                    });
                }
                items.push(AiChatListItem::BottomSpacer);
                let signatures = self.ai_chat_list_signatures(conversation, &items);
                Some((conversation.id.clone(), items, signatures))
            })
        else {
            return self.render_ai_sidebar_empty_chat(cx);
        };

        let virtual_spec = ai_chat_virtual_list_spec();
        self.sync_ai_chat_list_state(&conversation_id, &signatures, virtual_spec);

        let entity = cx.entity();
        let state = self.ai.chat.message_list_state.clone();
        let viewport = self.ai_chat_list_viewport_snapshot();
        tauri_virtual_list(state, virtual_spec, move |index, _window, cx| {
            let Some(item) = items.get(index).cloned() else {
                return div().into_any_element();
            };
            let message_viewport = Self::ai_message_viewport_for_list_item(index, viewport);
            entity.update(cx, |this, cx| {
                this.render_ai_chat_list_item(item, message_viewport, cx)
            })
        })
        .w_full()
        .h_full()
        .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_chat_list_item(
        &self,
        item: AiChatListItem,
        viewport: Option<AiMessageViewport>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match item {
            AiChatListItem::TrimNotice { count, .. } => self.render_ai_trim_notice(count, cx),
            AiChatListItem::Message { id } => {
                let Some(conversation) = self.ai.chat.conversation_state.active_conversation()
                else {
                    return div().into_any_element();
                };
                let Some(message) = conversation
                    .messages
                    .iter()
                    .find(|message| message.id == id)
                else {
                    return div().into_any_element();
                };
                self.render_ai_message(conversation, message, viewport, cx)
            }
            AiChatListItem::BottomSpacer => div().h(px(16.0)).into_any_element(),
        }
    }

    pub(in crate::workspace) fn ai_chat_list_viewport_snapshot(
        &self,
    ) -> Option<AiChatListViewportSnapshot> {
        let bounds = self.ai.chat.message_list_state.viewport_bounds();
        let height = f32::from(bounds.size.height);
        if height <= 0.0 {
            return None;
        }
        let scroll_top = self.ai.chat.message_list_state.logical_scroll_top();
        Some(AiChatListViewportSnapshot {
            item_ix: scroll_top.item_ix,
            offset_in_item: f32::from(scroll_top.offset_in_item),
            height,
        })
    }

    pub(in crate::workspace) fn ai_message_viewport_for_list_item(
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

    pub(in crate::workspace) fn ai_chat_list_signatures(
        &self,
        conversation: &AiConversation,
        items: &[AiChatListItem],
    ) -> Vec<u64> {
        items
            .iter()
            .map(|item| self.ai_chat_list_item_signature(conversation, item))
            .collect()
    }

    pub(in crate::workspace) fn ai_chat_list_item_signature(
        &self,
        conversation: &AiConversation,
        item: &AiChatListItem,
    ) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        match item {
            AiChatListItem::TrimNotice { sequence, count } => {
                std::hash::Hash::hash(&"trim", &mut hasher);
                std::hash::Hash::hash(sequence, &mut hasher);
                std::hash::Hash::hash(count, &mut hasher);
            }
            AiChatListItem::Message { id } => {
                std::hash::Hash::hash(&"message", &mut hasher);
                std::hash::Hash::hash(id, &mut hasher);
                if let Some(message) = conversation
                    .messages
                    .iter()
                    .find(|message| &message.id == id)
                {
                    let role = match message.role {
                        AiChatRole::User => 0u8,
                        AiChatRole::Assistant => 1,
                        AiChatRole::System => 2,
                        AiChatRole::Tool => 3,
                    };
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
                    std::hash::Hash::hash(
                        &self.ai.chat.thinking_expansion_state.get(&message.id),
                        &mut hasher,
                    );
                    if let Some(turn) = message.turn.as_ref() {
                        std::hash::Hash::hash(&turn.to_string(), &mut hasher);
                    }
                    if let Some(metadata) = message.metadata.as_ref() {
                        std::hash::Hash::hash(&metadata.kind, &mut hasher);
                        std::hash::Hash::hash(&metadata.original_count, &mut hasher);
                    }
                    for tool_call in &message.tool_calls {
                        std::hash::Hash::hash(&tool_call.to_string(), &mut hasher);
                    }
                }
            }
            AiChatListItem::BottomSpacer => {
                std::hash::Hash::hash(&"spacer", &mut hasher);
            }
        }
        std::hash::Hasher::finish(&hasher)
    }

    pub(in crate::workspace) fn sync_ai_chat_list_state(
        &mut self,
        conversation_id: &str,
        signatures: &[u64],
        spec: TauriVirtualListSpec,
    ) {
        let mut cache = self.ai.chat.message_list_cache.borrow_mut();
        sync_tauri_virtual_list_state_by_signatures(
            &mut self.ai.chat.message_list_state,
            &mut cache,
            conversation_id,
            signatures,
            ListAlignment::Top,
            spec,
        );
    }

    pub(in crate::workspace) fn render_ai_compaction_notice(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let active_id = self
            .ai
            .chat
            .conversation_state
            .active_conversation_id
            .as_deref()?;
        let notice = self.ai.chat.compaction_notice.as_ref()?;
        if notice.conversation_id != active_id {
            return None;
        }
        let running = notice.phase == AiCompactionNoticePhase::Running;
        let label = if running {
            self.i18n.t("ai.context.compaction_running")
        } else {
            self.i18n.t("ai.context.compaction_done").replace(
                "{{count}}",
                &notice.compacted_count.unwrap_or_default().to_string(),
            )
        };
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(if running {
                    rgba((self.tokens.ui.accent << 8) | 0x33)
                } else {
                    rgba((self.tokens.ui.border << 8) | 0x33)
                })
                .bg(if running {
                    rgba((self.tokens.ui.accent << 8) | 0x1a)
                } else {
                    rgba((self.tokens.ui.border << 8) | 0x1a)
                })
                .child(Self::render_lucide_icon(
                    LucideIcon::Archive,
                    14.0,
                    rgb(self.tokens.ui.text_muted),
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(11.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "ai-compaction-notice",
                            active_id,
                            label,
                            self.tokens.ui.text_muted,
                            cx,
                        )),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_ai_trim_notice(
        &self,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

    pub(in crate::workspace) fn render_ai_sidebar_initialization_error(
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
            .bg(self.context_sidebar_content_background(self.tokens.ui.bg))
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
                            self.i18n.t("launcher.retry"),
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

    pub(in crate::workspace) fn render_ai_sidebar_empty_chat(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

    pub(in crate::workspace) fn render_ai_quick_prompt(
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
                    this.ai.chat.draft = prompt.clone();
                    this.ai.chat.input_focused = true;
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_context_warning_banners(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut banners = div().flex_none().flex().flex_col();
        if let Some(percentage) = self.ai.chat.model_switch_warning_percentage {
            banners = banners.child(
                self.render_ai_context_warning_banner(
                    self.i18n
                        .t("ai.context.model_switched_warning")
                        .replace("{{percentage}}", &percentage.to_string()),
                    true,
                    true,
                    cx,
                ),
            );
        }
        if self.ai_context_danger_warning_active() {
            banners = banners.child(self.render_ai_context_warning_banner(
                self.i18n.t("ai.context.approaching_limit"),
                false,
                false,
                cx,
            ));
        }
        banners.into_any_element()
    }

    pub(in crate::workspace) fn render_ai_context_warning_banner(
        &self,
        label: String,
        dismissible: bool,
        model_switch: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let warning_color = self.tokens.ui.warning;
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .border_t_1()
            .border_color(rgba((warning_color << 8) | 0x33))
            .bg(rgba((warning_color << 8) | 0x1a))
            .child(Self::render_lucide_icon(
                LucideIcon::AlertTriangle,
                14.0,
                rgb(warning_color),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(11.0))
                    .text_color(rgb(warning_color))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "ai-context-warning",
                        label.clone(),
                        label,
                        warning_color,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.render_ai_context_warning_button(
                        self.i18n.t("ai.context.compact_button"),
                        LucideIcon::Archive,
                        self.ai.chat.loading,
                        AiContextWarningAction::Compact(model_switch),
                        cx,
                    ))
                    .when(!model_switch, |actions| {
                        actions.child(self.render_ai_context_warning_button(
                            self.i18n.t("ai.context.summarize"),
                            LucideIcon::Archive,
                            self.ai.chat.loading,
                            AiContextWarningAction::Summarize,
                            cx,
                        ))
                    })
                    .child(self.render_ai_context_warning_button(
                        self.i18n.t("ai.chat.new_chat_tooltip"),
                        LucideIcon::Plus,
                        false,
                        AiContextWarningAction::NewChat(model_switch),
                        cx,
                    ))
                    .when(dismissible, |actions| {
                        actions.child(self.render_ai_context_warning_button(
                            "",
                            LucideIcon::X,
                            false,
                            AiContextWarningAction::Dismiss,
                            cx,
                        ))
                    }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_context_warning_button(
        &self,
        label: impl Into<String>,
        icon: LucideIcon,
        disabled: bool,
        action: AiContextWarningAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const AI_CONTEXT_WARNING_BUTTON_H: f32 = 18.0; // Tauri inline warning actions h-[18px].
        const AI_CONTEXT_WARNING_BUTTON_PX: f32 = 8.0; // Tauri inline warning actions px-2.
        const AI_CONTEXT_WARNING_BUTTON_TEXT: f32 = 10.0; // Tauri inline warning actions text-[10px].

        let label = label.into();
        let text_color = self.tokens.ui.warning;
        let mut options = ToolbarButtonOptions::compact_text(
            ButtonVariant::Ghost,
            ButtonRadius::Md,
            AI_CONTEXT_WARNING_BUTTON_H,
            AI_CONTEXT_WARNING_BUTTON_PX,
            AI_CONTEXT_WARNING_BUTTON_TEXT,
        );
        options.button.disabled = disabled;
        options.icon_gap = Some(4.0);
        options.text_color = Some(rgb(text_color));
        options.hover_background = Some(rgba((self.tokens.ui.warning << 8) | 0x1a));
        options.hover_text_color = Some(rgb(self.tokens.ui.warning));
        // Context warning buttons are compact inline actions rather than full
        // footer buttons, but they still share disabled activation, hover, and
        // focus semantics with the workspace toolbar action primitive.
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                icon,
                12.0,
                rgb(self.tokens.ui.warning),
            )),
            options,
            cx.listener(move |this, _event, _window, cx| {
                match action {
                    AiContextWarningAction::Compact(model_switch) => {
                        if model_switch {
                            this.ai.chat.model_switch_warning_percentage = None;
                        }
                        this.start_ai_compact_conversation(cx);
                    }
                    AiContextWarningAction::NewChat(model_switch) => {
                        if model_switch {
                            this.ai.chat.model_switch_warning_percentage = None;
                        }
                        this.create_ai_sidebar_conversation(None, cx);
                    }
                    AiContextWarningAction::Dismiss => {
                        this.ai.chat.model_switch_warning_percentage = None;
                    }
                    AiContextWarningAction::Summarize => {
                        this.open_ai_summarize_confirm(cx);
                    }
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_context_danger_warning_active(&self) -> bool {
        let Some(conversation) = self.ai.chat.conversation_state.active_conversation() else {
            return false;
        };
        if conversation.messages.len() < 4 {
            return false;
        }
        let (total_tokens, max_tokens) = self.ai_context_message_usage_counts();
        ai_context_percentage(total_tokens, max_tokens) > AI_CONTEXT_DANGER_PERCENT
    }

    pub(in crate::workspace) fn ai_context_message_usage_counts(&self) -> (usize, usize) {
        let breakdown = self.ai_context_token_breakdown();
        (breakdown.messages, breakdown.max_tokens)
    }

    pub(in crate::workspace) fn render_ai_summarize_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "ai-summarize-confirm-motion",
            self.ai.chat.summarize_confirm_presence.phase(),
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Default,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "ai-summarize-confirm",
                        "title",
                        self.i18n.t("ai.context.summarize_confirm"),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-summarize-confirm",
                        "cancel",
                        self.i18n.t("common.cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-summarize-confirm",
                        "confirm",
                        self.i18n.t("ai.context.summarize"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.begin_ai_summarize_confirm_exit(cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                if this.begin_ai_summarize_confirm_exit(cx) {
                    this.start_ai_summarize_conversation(cx);
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    pub(in crate::workspace) fn render_ai_clear_all_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "ai-clear-all-confirm-motion",
            self.ai_clear_all_confirm_presence.phase(),
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "ai-clear-all-confirm",
                        "title",
                        self.i18n.t("ai.chat.clear_all_confirm"),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-clear-all-confirm",
                        "cancel",
                        self.i18n.t("common.actions.cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-clear-all-confirm",
                        "confirm",
                        self.i18n.t("common.actions.confirm"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.begin_ai_clear_all_confirm_exit(cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                if this.begin_ai_clear_all_confirm_exit(cx) {
                    this.clear_ai_conversations();
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    pub(in crate::workspace) fn render_ai_delete_message_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "ai-delete-message-confirm-motion",
            self.ai_delete_message_confirm_presence.phase(),
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "ai-delete-message-confirm",
                        "title",
                        self.i18n.t("ai.message.delete_confirm"),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-delete-message-confirm",
                        "cancel",
                        self.i18n.t("common.actions.cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-delete-message-confirm",
                        "confirm",
                        self.i18n.t("common.actions.confirm"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.begin_ai_delete_message_confirm_exit(cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                let message_id = this.ai.chat.delete_message_confirm.clone();
                if this.begin_ai_delete_message_confirm_exit(cx)
                    && let Some(message_id) = message_id
                {
                    this.delete_ai_message(&message_id, cx);
                }
                cx.stop_propagation();
            }),
        )
    }
}

#[derive(Clone, Copy)]
pub(in crate::workspace) enum AiContextWarningAction {
    Compact(bool),
    NewChat(bool),
    Dismiss,
    Summarize,
}
