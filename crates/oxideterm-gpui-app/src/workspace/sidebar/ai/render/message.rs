impl WorkspaceApp {
    fn render_ai_message(
        &self,
        conversation: &AiConversation,
        message: &AiChatMessage,
        viewport: Option<AiMessageViewport>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if message
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.kind == "compaction-anchor")
        {
            let original_count = message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.original_count)
                .unwrap_or(0);
            let label = self
                .i18n
                .t("ai.context.compacted_messages")
                .replace("{{count}}", &original_count.to_string());
            return div()
                .px(px(self.tokens.spacing.three))
                .py(px(self.tokens.spacing.two))
                .child(
                    div()
                        .overflow_hidden()
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(self.tokens.spacing.two))
                                .px(px(self.tokens.spacing.three))
                                .py(px(self.tokens.spacing.two))
                                .hover(|style| style.bg(rgba((self.tokens.ui.bg_hover << 8) | 0x4d)))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Archive,
                                    14.0,
                                    rgba((self.tokens.ui.text_muted << 8) | 0x80),
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(11.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x99))
                                        .child(self.render_display_text_with_role_and_alpha(
                                            SelectableTextRole::NonSelectable,
                                            "ai-condensed-context-label",
                                            label.clone(),
                                            label,
                                            self.tokens.ui.text_muted,
                                            0x99 as f32 / 255.0,
                                            cx,
                                        )),
                                )
                                .child(Self::render_lucide_icon(
                                    LucideIcon::ChevronRight,
                                    12.0,
                                    rgba((self.tokens.ui.text_muted << 8) | 0x66),
                                )),
                        )
                        .child(
                            div()
                                .px(px(self.tokens.spacing.three))
                                .pb(px(self.tokens.spacing.two))
                                .text_size(px(12.0))
                                .line_height(px(19.0))
                                .text_color(rgba((self.tokens.ui.text_muted << 8) | 0xb3))
                                .child(self.render_selectable_text(
                                    crate::workspace::selectable_text::selectable_text_id(
                                        "ai-compaction-anchor",
                                        &message.id,
                                    ),
                                    message.content.clone(),
                                    self.tokens.ui.text_muted,
                                    cx,
                                )),
                        ),
                )
                .into_any_element();
        }
        let role = match message.role {
            AiChatRole::User => oxideterm_gpui_ui::ai::AiMessageRole::User,
            AiChatRole::Assistant => oxideterm_gpui_ui::ai::AiMessageRole::Assistant,
            AiChatRole::System => oxideterm_gpui_ui::ai::AiMessageRole::Assistant,
            AiChatRole::Tool => oxideterm_gpui_ui::ai::AiMessageRole::Assistant,
        };
        let user = message.role == AiChatRole::User;
        let editing = user && self.ai_editing_message_id.as_deref() == Some(message.id.as_str());
        let label = match message.role {
            AiChatRole::User => self.i18n.t("ai.message.you"),
            AiChatRole::Assistant => self.i18n.t("ai.chat.title"),
            AiChatRole::System => "System".to_string(),
            AiChatRole::Tool => "Tool".to_string(),
        };
        let mut header = div()
            .mb(px(self.tokens.spacing.one / 2.0))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.one + self.tokens.spacing.one / 2.0))
            .when(user, |row| row.flex_row_reverse())
            .child(ai_message_author(&self.tokens, label));
        if let Some(model) = (!user)
            .then_some(message.model.as_ref())
            .flatten()
            .filter(|model| !model.is_empty())
        {
            let model_label = model
                .split('/')
                .filter(|part| !part.is_empty())
                .next_back()
                .unwrap_or(model)
                .to_string();
            header = header.child(ai_message_model_badge(&self.tokens, model_label));
        }
        header = header.child(ai_message_time(
            &self.tokens,
            time_label(message.timestamp_ms),
            user,
        ));
        let structured_parts = ai_turn_parts(message);
        let has_structured_parts = structured_parts.is_some_and(|parts| !parts.is_empty());
        let thinking_content = (!has_structured_parts)
            .then(|| {
                message
                    .thinking_content
                    .as_ref()
                    .map(|content| content.trim())
                    .filter(|content| !content.is_empty())
            })
            .flatten();
        let thinking_expanded = self
            .ai_thinking_expansion_state
            .get(&message.id)
            .copied()
            .unwrap_or_else(|| self.settings_store.settings().ai.thinking_default_expanded);
        let mut body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .text_size(px(13.0))
            .line_height(px(20.0))
            .text_color(rgb(self.tokens.ui.text));
        if let Some(thinking_content) = thinking_content {
            let compact = self.settings_store.settings().ai.thinking_style == AiThinkingStyle::Compact
                && !thinking_expanded;
            if compact {
                let thinking_message_id = message.id.clone();
                body = body.child(
                    ai_thinking_compact(
                        &self.tokens,
                        self.i18n.t("ai.thinking.thought"),
                        Self::render_lucide_icon(
                            LucideIcon::Brain,
                            12.0,
                            rgb(self.tokens.ui.text_muted),
                        ),
                        Self::render_lucide_icon(
                            LucideIcon::ChevronRight,
                            12.0,
                            rgb(self.tokens.ui.text_muted),
                        ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            let default_expanded =
                                this.settings_store.settings().ai.thinking_default_expanded;
                            let current = this
                                .ai_thinking_expansion_state
                                .get(&thinking_message_id)
                                .copied()
                                .unwrap_or(default_expanded);
                            this.ai_thinking_expansion_state
                                .insert(thinking_message_id.clone(), !current);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
                );
            } else {
                let chevron = if thinking_expanded {
                LucideIcon::ChevronDown
            } else {
                LucideIcon::ChevronRight
            };
                let thinking_message_id = message.id.clone();
                let thinking_header = ai_thinking_header(
                    &self.tokens,
                    self.i18n.t("ai.thinking.thought"),
                    message.is_streaming,
                    Self::render_lucide_icon(chevron, 12.0, rgb(self.tokens.ui.text_muted)),
                    Self::render_lucide_icon(
                        LucideIcon::Brain,
                        12.0,
                        rgb(self.tokens.ui.text_muted),
                    ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        let default_expanded =
                            this.settings_store.settings().ai.thinking_default_expanded;
                        let current = this
                            .ai_thinking_expansion_state
                            .get(&thinking_message_id)
                            .copied()
                            .unwrap_or(default_expanded);
                        this.ai_thinking_expansion_state
                            .insert(thinking_message_id.clone(), !current);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                );
                body = body.child(
                    ai_thinking_block(&self.tokens, thinking_expanded)
                        .child(thinking_header)
                        .when(thinking_expanded, |block| {
                            block.child(ai_thinking_content(
                                &self.tokens,
                                ("ai-thinking", ai_message_element_seed(&message.id)),
                                thinking_content.to_string(),
                            ))
                        }),
                );
            }
        }
        if editing {
            body = body.child(self.render_ai_message_edit_body(cx));
        } else if has_structured_parts {
            body = self.render_ai_turn_parts(body, message, viewport, cx);
        } else if !message.content.is_empty() {
            body = body.child(self.render_ai_message_content(message, viewport, cx));
            if !message.tool_calls.is_empty() {
                body = body.child(self.render_ai_tool_calls(message, cx));
            }
        }
        if user && !editing
            && let Some(branches) = message.branches.as_ref().filter(|branches| branches.total > 1)
        {
            let prev_disabled = branches.active_index == 0;
            let next_disabled = branches.active_index >= branches.total.saturating_sub(1);
            let prev_index = branches.active_index.saturating_sub(1);
            let next_index = (branches.active_index + 1).min(branches.total.saturating_sub(1));
            let prev_id = message.id.clone();
            let next_id = message.id.clone();
            body = body.child(
                div()
                    .mt(px(self.tokens.spacing.one))
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.one))
                    .child(
                        div()
                            .p(px(self.tokens.spacing.one / 2.0))
                            .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x66))
                            .opacity(if prev_disabled { 0.2 } else { 1.0 })
                            .cursor_pointer()
                            .hover(|style| style.text_color(rgb(self.tokens.ui.text_muted)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::ChevronLeft,
                                12.0,
                                rgba((self.tokens.ui.text_muted << 8) | 0x66),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    if !prev_disabled {
                                        this.switch_ai_message_branch(prev_id.clone(), prev_index, cx);
                                    }
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .min_w(px(28.0))
                            .text_align(gpui::TextAlign::Center)
                            .text_size(px(10.0))
                            .font_family(settings_ui_font_family(""))
                            .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x80))
                            .child(self.render_display_text_with_role_and_alpha(
                                SelectableTextRole::PlainDocument,
                                "ai-message-branches",
                                message.id.as_str(),
                                format!("{}/{}", branches.active_index + 1, branches.total),
                                self.tokens.ui.text_muted,
                                0x80 as f32 / 255.0,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .p(px(self.tokens.spacing.one / 2.0))
                            .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x66))
                            .opacity(if next_disabled { 0.2 } else { 1.0 })
                            .cursor_pointer()
                            .hover(|style| style.text_color(rgb(self.tokens.ui.text_muted)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::ChevronRight,
                                12.0,
                                rgba((self.tokens.ui.text_muted << 8) | 0x66),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    if !next_disabled {
                                        this.switch_ai_message_branch(next_id.clone(), next_index, cx);
                                    }
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            );
        }
        let last_assistant = conversation
            .messages
            .iter()
            .rev()
            .find(|candidate| candidate.role == AiChatRole::Assistant)
            .is_some_and(|candidate| candidate.id == message.id);
        if !user && !message.is_streaming && !editing {
            let content = message.content.clone();
            let delete_id = message.id.clone();
            body = body.child(
                div()
                    .mt(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(
                        ai_message_action(
                            &self.tokens,
                            self.i18n.t("ai.message.copy"),
                            Self::render_lucide_icon(
                                LucideIcon::Copy,
                                12.0,
                                rgba((self.tokens.ui.text_muted << 8) | 0x66),
                            ),
                            false,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_this, _event, _window, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(content.clone()));
                                cx.stop_propagation();
                            }),
                        ),
                    )
                    .when(last_assistant, |row| {
                        row.child(
                            ai_message_action(
                                &self.tokens,
                                self.i18n.t("ai.message.regenerate"),
                                Self::render_lucide_icon(
                                    LucideIcon::RotateCcw,
                                    12.0,
                                    rgba((self.tokens.ui.text_muted << 8) | 0x66),
                                ),
                                false,
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.regenerate_ai_last_response(cx);
                                    cx.stop_propagation();
                                }),
                            ),
                        )
                    })
                    .child(
                        ai_message_action(
                            &self.tokens,
                            "",
                            Self::render_lucide_icon(
                                LucideIcon::Trash2,
                                12.0,
                                rgba((self.tokens.ui.text_muted << 8) | 0x66),
                            ),
                            true,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.request_delete_ai_message(delete_id.clone(), cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            );
        } else if user && !message.is_streaming && !editing {
            let delete_id = message.id.clone();
            let edit_id = message.id.clone();
            let edit_content = message.content.clone();
            body = body.child(
                div()
                    .mt(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(
                        ai_message_action(
                            &self.tokens,
                            self.i18n.t("ai.message.edit"),
                            Self::render_lucide_icon(
                                LucideIcon::Pencil,
                                12.0,
                                rgba((self.tokens.ui.text_muted << 8) | 0x66),
                            ),
                            false,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, window, cx| {
                                this.start_edit_ai_message(
                                    edit_id.clone(),
                                    edit_content.clone(),
                                    cx,
                                );
                                window.focus(&this.focus_handle);
                                cx.stop_propagation();
                            }),
                        ),
                    )
                    .child(
                        ai_message_action(
                            &self.tokens,
                            self.i18n.t("ai.message.delete"),
                            Self::render_lucide_icon(
                                LucideIcon::Trash2,
                                12.0,
                                rgba((self.tokens.ui.text_muted << 8) | 0x66),
                            ),
                            true,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.request_delete_ai_message(delete_id.clone(), cx);
                                cx.stop_propagation();
                            }),
                        ),
                ),
            );
        }
        if !user
            && last_assistant
            && !message.is_streaming
            && !message.suggestions.is_empty()
            && !editing
        {
            body = body.child(self.render_ai_follow_up_suggestions(message, cx));
        }
        div()
            .w_full()
            .flex_none()
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.three))
            .child(header)
            .child(ai_message_body(&self.tokens, role, body))
            .into_any_element()
    }

    fn render_ai_message_content(
        &self,
        message: &AiChatMessage,
        viewport: Option<AiMessageViewport>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if message.role == AiChatRole::User {
            let group_id = crate::workspace::selectable_text::selectable_text_id(
                "ai-user-message",
                &message.id,
            );
            return div()
                .w_full()
                .min_w_0()
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(rgb(self.tokens.ui.text))
                .children(message.content.split('\n').enumerate().map(|(line_index, line)| {
                    div()
                        .w_full()
                        .min_w_0()
                        .child(self.render_selectable_text_in_group(
                            group_id,
                            crate::workspace::selectable_text::selectable_text_id(
                                "ai-user-message-fragment",
                                (group_id, line_index),
                            ),
                            line_index,
                            line.to_string(),
                            self.tokens.ui.text,
                            cx,
                        ))
                        .into_any_element()
                }))
                .into_any_element();
        }

        let mut options = MarkdownOptions::from_theme(&self.tokens);
        options.base_font_size = 13.0;
        options.block_gap = 8.0;
        let content = ai_visible_suggestion_content(&message.content);
        let cached = self.cached_ai_markdown_document(&content, &options, !message.is_streaming);
        let group_id = crate::workspace::selectable_text::selectable_text_id(
            "ai-markdown-message",
            &message.id,
        );
        let workspace = cx.entity();
        let code_actions = markdown_render::MarkdownCodeBlockActions {
            on_run: Some(Arc::new(move |command, window, cx| {
                let workspace = workspace.clone();
                // Markdown action callbacks are plain event callbacks, not
                // WorkspaceApp listeners. Defer the entity write so code block
                // buttons cannot nest a WorkspaceApp update inside another one.
                window.defer(cx, move |_window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.insert_ai_code_block_command(command, cx);
                    });
                });
            })),
        };
        let mut text_order = 0usize;
        let mut render_text = |key: String,
                               text: gpui::SharedString,
                               runs: Vec<gpui::TextRun>|
         -> AnyElement {
            let order = text_order;
            text_order = text_order.saturating_add(1);
            self.render_selectable_styled_text_in_group(
                group_id,
                crate::workspace::selectable_text::selectable_text_id(
                    "ai-markdown-fragment",
                    (group_id, order, &key),
                ),
                order,
                text,
                runs,
                cx,
            )
        };
        let rendered = viewport
            .filter(|_| !message.is_streaming)
            .map(|viewport| {
                markdown_render::render_document_windowed_selectable_with_code_actions(
                    &cached.document,
                    &cached.layout,
                    &self.tokens,
                    &options,
                    viewport.top,
                    viewport.height,
                    AI_MARKDOWN_WINDOW_OVERDRAW_PX,
                    Some(&code_actions),
                    &mut render_text,
                )
            })
            .unwrap_or_else(|| {
                markdown_render::render_document_selectable_with_code_actions(
                    &cached.document,
                    &self.tokens,
                    &options,
                    Some(&code_actions),
                    &mut render_text,
                )
            });
        div()
            .w_full()
            .min_w_0()
            .text_color(rgb(self.tokens.ui.text))
            .child(rendered)
            .into_any_element()
    }

    fn insert_ai_code_block_command(&mut self, command: String, cx: &mut Context<Self>) {
        if command.trim().is_empty() {
            return;
        }
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| {
                // Tauri's ai-insert-command writes programmatic terminal input without submitting it.
                pane.send_ai_input_bytes(command.as_bytes(), cx);
            });
        }
    }

    fn render_ai_follow_up_suggestions(
        &self,
        message: &AiChatMessage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .mt(px(self.tokens.spacing.two))
            .flex()
            .flex_wrap()
            .gap(px(self.tokens.spacing.one + self.tokens.spacing.one / 2.0))
            .children(message.suggestions.iter().map(|suggestion| {
                let text = suggestion.text.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.one))
                    .px(px(self.tokens.spacing.two))
                    .py(px(self.tokens.spacing.one))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
                    .text_size(px(11.0))
                    .text_color(rgba((self.tokens.ui.text_muted << 8) | 0xb3))
                    .cursor_pointer()
                    .hover(|style| {
                        style
                            .border_color(rgba((self.tokens.ui.accent << 8) | 0x66))
                            .text_color(rgb(self.tokens.ui.accent))
                            .bg(rgba((self.tokens.ui.accent << 8) | 0x0d))
                    })
                    .child(Self::render_lucide_icon(
                        LucideIcon::MessageSquare,
                        12.0,
                        rgba((self.tokens.ui.text_muted << 8) | 0xb3),
                    ))
                    .child(div().min_w_0().child(text.clone()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.send_ai_follow_up_suggestion(text.clone(), cx);
                            cx.stop_propagation();
                        }),
                    )
                    .into_any_element()
            }))
            .into_any_element()
    }

    fn render_ai_turn_parts(
        &self,
        mut body: Div,
        message: &AiChatMessage,
        viewport: Option<AiMessageViewport>,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(parts) = ai_turn_parts(message) else {
            return body;
        };
        let mut buffered_tool_parts = Vec::new();
        let mut buffered_tool_round_id: Option<String> = None;
        let mut segment_index = 0usize;

        for part in parts {
            let part_type = part
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if matches!(part_type, "tool_call" | "tool_result") {
                let next_round_id = ai_tool_part_round_id(message, part);
                if !buffered_tool_parts.is_empty()
                    && buffered_tool_round_id.as_deref() != next_round_id.as_deref()
                {
                    body = body.child(self.render_ai_tool_part_segment(
                        message,
                        segment_index,
                        &buffered_tool_parts,
                        cx,
                    ));
                    buffered_tool_parts.clear();
                    buffered_tool_round_id = None;
                    segment_index = segment_index.saturating_add(1);
                }

                buffered_tool_parts.push(part.clone());
                if buffered_tool_round_id.is_none() {
                    buffered_tool_round_id = next_round_id;
                }
                continue;
            }

            if !buffered_tool_parts.is_empty() {
                body = body.child(self.render_ai_tool_part_segment(
                    message,
                    segment_index,
                    &buffered_tool_parts,
                    cx,
                ));
                buffered_tool_parts.clear();
                buffered_tool_round_id = None;
                segment_index = segment_index.saturating_add(1);
            }

            match part_type {
                "text" => {
                    if let Some(text) = part
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .filter(|text| !text.trim().is_empty())
                    {
                        let text = ai_visible_suggestion_content(text);
                        if text.trim().is_empty() {
                            continue;
                        }
                        let mut segment = message.clone();
                        segment.id = format!("{}-text-{segment_index}", message.id);
                        segment.content = text;
                        segment.tool_calls.clear();
                        body = body.child(self.render_ai_message_content(&segment, viewport, cx));
                        segment_index = segment_index.saturating_add(1);
                    }
                }
                "thinking" => {
                    if let Some(text) = part
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .filter(|text| !text.trim().is_empty())
                    {
                        body = body.child(self.render_ai_thinking_part(
                            message,
                            segment_index,
                            text,
                            part.get("streaming")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(message.is_streaming),
                            cx,
                        ));
                        segment_index = segment_index.saturating_add(1);
                    }
                }
                "warning" | "error" => {
                    if let Some(text) = part
                        .get("message")
                        .and_then(serde_json::Value::as_str)
                        .filter(|text| !text.trim().is_empty())
                    {
                        body = body.child(
                            div()
                                .rounded(px(self.tokens.radii.md))
                                .border_1()
                                .border_color(rgba((self.tokens.ui.error << 8) | 0x66))
                                .bg(rgba((self.tokens.ui.error << 8) | 0x12))
                                .px(px(self.tokens.spacing.three))
                                .py(px(self.tokens.spacing.two))
                                .text_color(rgb(self.tokens.ui.error))
                                .child(self.render_selectable_text(
                                    crate::workspace::selectable_text::selectable_text_id(
                                        "ai-turn-warning",
                                        (&message.id, segment_index),
                                    ),
                                    text.to_string(),
                                    self.tokens.ui.error,
                                    cx,
                                )),
                        );
                        segment_index = segment_index.saturating_add(1);
                    }
                }
                "guardrail" => {
                    if let Some(text) = part
                        .get("message")
                        .and_then(serde_json::Value::as_str)
                        .filter(|text| !text.trim().is_empty())
                    {
                        body = body.child(self.render_ai_guardrail_part(
                            message,
                            segment_index,
                            part,
                            text,
                            cx,
                        ));
                        segment_index = segment_index.saturating_add(1);
                    }
                }
                _ => {}
            }
        }

        if !buffered_tool_parts.is_empty() {
            body = body.child(self.render_ai_tool_part_segment(
                message,
                segment_index,
                &buffered_tool_parts,
                cx,
            ));
        }
        body
    }

    fn render_ai_guardrail_part(
        &self,
        message: &AiChatMessage,
        segment_index: usize,
        part: &serde_json::Value,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let code = part
            .get("code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let strong = matches!(code, "tool-disabled-hard-deny" | "pseudo-tool-transcript");
        let raw_text = part
            .get("rawText")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let expanded_key = format!("{}-guardrail-{segment_index}", message.id);
        let expanded = self
            .ai_tool_call_expansion_state
            .contains(&expanded_key);
        let mut block = ai_guardrail_block(
            &self.tokens,
            text.to_string(),
            strong,
            Self::render_lucide_icon(
                LucideIcon::AlertTriangle,
                14.0,
                rgba((0xf59e0b << 8) | 0xe6),
            ),
        );
        if let Some(raw_text) = raw_text {
            let toggle_key = expanded_key.clone();
            block = block.child(
                div()
                    .mt(px(self.tokens.spacing.two))
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.one))
                    .text_size(px(11.0))
                    .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x99))
                    .cursor_pointer()
                    .hover(|style| style.text_color(rgb(self.tokens.ui.text_muted)))
                    .child(Self::render_lucide_icon(
                        if expanded {
                            LucideIcon::ChevronDown
                        } else {
                            LucideIcon::ChevronRight
                        },
                        12.0,
                        rgba((self.tokens.ui.text_muted << 8) | 0x99),
                    ))
                    .child(self.render_display_text_with_role_and_alpha(
                        SelectableTextRole::NonSelectable,
                        "ai-tool-context-action",
                        "view-original",
                        self.i18n.t("ai.context.view_original"),
                        self.tokens.ui.text_muted,
                        0x99 as f32 / 255.0,
                        cx,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            let current = this
                                .ai_tool_call_expansion_state
                                .contains(&toggle_key);
                            if current {
                                this.ai_tool_call_expansion_state.remove(&toggle_key);
                            } else {
                                this.ai_tool_call_expansion_state.insert(toggle_key.clone());
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
            if expanded {
                block = block.child(ai_raw_block(
                    &self.tokens,
                    (
                        "ai-guardrail-raw",
                        ai_message_element_seed(&expanded_key),
                    ),
                    Some(220.0),
                    raw_text.to_string(),
                ));
            }
        }
        div().w_full().child(block)
    }

    fn render_ai_thinking_part(
        &self,
        message: &AiChatMessage,
        segment_index: usize,
        text: &str,
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let thinking_key = format!("{}-thinking-{segment_index}", message.id);
        let thinking_expanded = self
            .ai_thinking_expansion_state
            .get(&thinking_key)
            .copied()
            .unwrap_or_else(|| self.settings_store.settings().ai.thinking_default_expanded);
        let compact =
            self.settings_store.settings().ai.thinking_style == AiThinkingStyle::Compact
                && !thinking_expanded;
        if compact {
            let toggle_key = thinking_key.clone();
            return ai_thinking_compact(
                &self.tokens,
                self.i18n.t("ai.thinking.thought"),
                Self::render_lucide_icon(LucideIcon::Brain, 12.0, rgb(self.tokens.ui.text_muted)),
                Self::render_lucide_icon(
                    LucideIcon::ChevronRight,
                    12.0,
                    rgb(self.tokens.ui.text_muted),
                ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    let default_expanded =
                        this.settings_store.settings().ai.thinking_default_expanded;
                    let current = this
                        .ai_thinking_expansion_state
                        .get(&toggle_key)
                        .copied()
                        .unwrap_or(default_expanded);
                    this.ai_thinking_expansion_state
                        .insert(toggle_key.clone(), !current);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element();
        }

        let toggle_key = thinking_key.clone();
        let chevron = if thinking_expanded {
            LucideIcon::ChevronDown
        } else {
            LucideIcon::ChevronRight
        };
        let header = ai_thinking_header(
            &self.tokens,
            self.i18n.t("ai.thinking.thought"),
            streaming,
            Self::render_lucide_icon(chevron, 12.0, rgb(self.tokens.ui.text_muted)),
            Self::render_lucide_icon(LucideIcon::Brain, 12.0, rgb(self.tokens.ui.text_muted)),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                let default_expanded = this.settings_store.settings().ai.thinking_default_expanded;
                let current = this
                    .ai_thinking_expansion_state
                    .get(&toggle_key)
                    .copied()
                    .unwrap_or(default_expanded);
                this.ai_thinking_expansion_state
                    .insert(toggle_key.clone(), !current);
                cx.stop_propagation();
                cx.notify();
            }),
        );
        ai_thinking_block(&self.tokens, thinking_expanded)
            .child(header)
            .when(thinking_expanded, |block| {
                block.child(ai_thinking_content(
                    &self.tokens,
                    ("ai-thinking", ai_message_element_seed(&thinking_key)),
                    text.to_string(),
                ))
            })
            .into_any_element()
    }

    fn render_ai_tool_part_segment(
        &self,
        message: &AiChatMessage,
        segment_index: usize,
        parts: &[serde_json::Value],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut values = Vec::new();
        let mut ids = Vec::<String>::new();
        for part in parts {
            let id = part
                .get("id")
                .or_else(|| part.get("toolCallId"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if id.is_empty() || ids.iter().any(|existing| existing == id) {
                continue;
            }
            ids.push(id.to_string());
        }

        for id in ids {
            if let Some(existing) = message.tool_calls.iter().find(|call| {
                call.get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|existing| existing == id)
            }) {
                values.push(existing.clone());
            } else if let Some(value) = ai_tool_call_value_from_turn_parts(&id, parts) {
                values.push(value);
            }
        }

        let mut segment = message.clone();
        segment.id = format!("{}-tools-{segment_index}", message.id);
        segment.content.clear();
        segment.tool_calls = values;
        self.render_ai_tool_calls(&segment, cx)
    }

    fn render_ai_tool_calls(&self, message: &AiChatMessage, cx: &mut Context<Self>) -> AnyElement {
        let mut block = ai_tool_block(&self.tokens).child(ai_tool_heading(
            &self.tokens,
            format!(
                "{} ({})",
                self.i18n.t("ai.tool_use.heading"),
                message.tool_calls.len()
            ),
        ));
        let should_condense = message.tool_calls.len() >= 5;
        let split_at = if should_condense {
            message.tool_calls.len().saturating_sub(3)
        } else {
            0
        };
        let condensed_key = format!("{}:condensed-tools", message.id);
        let show_condensed = self.ai_tool_call_expansion_state.contains(&condensed_key);
        if should_condense {
            let hidden_count = split_at;
            let expanded_key = condensed_key.clone();
            let label = if show_condensed {
                self.i18n.t("ai.tool_use.condensed_label")
            } else {
                self.i18n
                    .t("ai.tool_use.condensed")
                    .replace("{{count}}", &hidden_count.to_string())
            };
            block = block.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.one))
                    .rounded(px(self.tokens.radii.md))
                    .px(px(self.tokens.spacing.two))
                    .py(px(self.tokens.spacing.one))
                    .bg(rgba((self.tokens.ui.bg_hover << 8) | 0x33))
                    .hover(|style| style.bg(rgba((self.tokens.ui.bg_hover << 8) | 0x66)))
                    .text_size(px(10.0))
                    .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x66))
                    .child(Self::render_lucide_icon(
                        LucideIcon::FileArchive,
                        12.0,
                        rgba((self.tokens.ui.text_muted << 8) | 0x80),
                    ))
                    .child(div().flex_1().min_w_0().child(
                        self.render_display_text_with_role_and_alpha(
                            SelectableTextRole::NonSelectable,
                            "ai-tool-condensed-label",
                            label.clone(),
                            label,
                            self.tokens.ui.text_muted,
                            0x66 as f32 / 255.0,
                            cx,
                        ),
                    ))
                    .child(Self::render_lucide_icon(
                        if show_condensed {
                            LucideIcon::ChevronDown
                        } else {
                            LucideIcon::ChevronRight
                        },
                        12.0,
                        rgba((self.tokens.ui.text_muted << 8) | 0x80),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if !this.ai_tool_call_expansion_state.remove(&expanded_key) {
                                this.ai_tool_call_expansion_state.insert(expanded_key.clone());
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }
        let calls = message
            .tool_calls
            .iter()
            .enumerate()
            .filter(|(index, _)| !should_condense || show_condensed || *index >= split_at)
            .map(|(_, call)| call);
        for call in calls {
            let id = call
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("tool-call")
                .to_string();
            let name = call
                .get("name")
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    call.get("function")
                        .and_then(|function| function.get("name"))
                        .and_then(serde_json::Value::as_str)
                })
                .unwrap_or("tool")
                .to_string();
            let arguments = call
                .get("arguments")
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    call.get("function")
                        .and_then(|function| function.get("arguments"))
                        .and_then(serde_json::Value::as_str)
                })
                .unwrap_or_default()
                .to_string();
            let status = ai_tool_status_from_value(call.get("status"));
            let risk = ai_tool_risk_from_value(call.get("risk"), &name);
            let summary = call
                .get("summary")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| self.ai_tool_status_label(status));
            let result = call.get("result").filter(|value| !value.is_null());
            let bypass_approval = result
                .and_then(|value| value.pointer("/meta/approvalMode"))
                .and_then(serde_json::Value::as_str)
                == Some("bypass");
            let view = AiToolCallView {
                name: name.clone(),
                summary,
                status,
                risk,
                capability: result
                    .and_then(|value| value.pointer("/meta/capability"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                duration: result
                    .and_then(|value| value.pointer("/meta/durationMs"))
                    .and_then(serde_json::Value::as_u64)
                    .map(|duration| format!("{duration}ms")),
                pending_denied_command: risk == AiToolRisk::Destructive,
                bypass_approval,
            };
            let tool_mono_font = settings_mono_font_family(self.settings_store.settings());
            let expansion_key = format!("{}:{id}", message.id);
            let expanded = self.ai_tool_call_expansion_state.contains(&expansion_key);
            let header_key = expansion_key.clone();
            let mut item = ai_tool_item(&self.tokens, &view).child(
                ai_tool_item_header(
                    &self.tokens,
                    &view,
                    Self::render_lucide_icon(
                        ai_tool_status_icon(status),
                        12.0,
                        rgb(ai_tool_status_color(&self.tokens, status)),
                    ),
                    Self::render_lucide_icon(
                        LucideIcon::Wrench,
                        12.0,
                        rgba((self.tokens.ui.text_muted << 8) | 0x99),
                    ),
                    Self::render_lucide_icon(
                        if expanded {
                            LucideIcon::ChevronDown
                        } else {
                            LucideIcon::ChevronRight
                        },
                        12.0,
                        rgba((self.tokens.ui.text_muted << 8) | 0x80),
                    ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if !this.ai_tool_call_expansion_state.remove(&header_key) {
                            this.ai_tool_call_expansion_state.insert(header_key.clone());
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            );

            let mut details = ai_tool_details(&self.tokens)
                .child(
                    div()
                        .child(ai_tool_section_label(
                            &self.tokens,
                            self.i18n.t("ai.tool_use.arguments"),
                            None,
                        ))
                        .child(ai_tool_args_pre(
                            &self.tokens,
                            ("ai-tool-args", ai_message_element_seed(&id)),
                            pretty_tool_json_or_raw(&arguments),
                            tool_mono_font.clone(),
                        )),
                );
            if let Some(result) = result {
                if let Some(policy_decision) = result.pointer("/meta/policyDecision") {
                    details = details.child(
                        div()
                            .child(ai_tool_section_label(
                                &self.tokens,
                                self.i18n.t("ai.tool_use.policy"),
                                None,
                            ))
                            .child(ai_tool_output_pre(
                                &self.tokens,
                                ("ai-tool-policy", ai_message_element_seed(&id)),
                                ai_tool_policy_decision_summary(policy_decision),
                                tool_mono_font.clone(),
                            )),
                    );
                }
                let output = result
                    .get("output")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| serde_json::to_string_pretty(result).unwrap_or_default());
                details = details.child(
                    div()
                        .child(ai_tool_section_label(
                            &self.tokens,
                            self.i18n.t("ai.tool_use.output"),
                            if status == AiToolStatus::Error {
                                Some(AiTone::Red)
                            } else {
                                None
                            },
                        ))
                        .child(ai_tool_output_pre(
                            &self.tokens,
                            ("ai-tool-output", ai_message_element_seed(&id)),
                            output,
                            tool_mono_font.clone(),
                        )),
                );
            }
            if expanded {
                item = item.child(details);
            }

            if status == AiToolStatus::PendingApproval {
                let approve_id = id.clone();
                let reject_id = id.clone();
                item = item.child(ai_tool_approval_bar(
                    &self.tokens,
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(10.0))
                        .text_color(rgba((self.tokens.ui.text_muted << 8) | 0xcc))
                        .child(self.i18n.t("ai.tool_use.approval_required")),
                    ai_tool_approval_button(
                        &self.tokens,
                        self.i18n.t("ai.tool_use.approve"),
                        true,
                        Self::render_lucide_icon(LucideIcon::Check, 11.0, rgb(0x22c55e)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.resolve_ai_tool_approval(approve_id.clone(), true, cx);
                            cx.stop_propagation();
                        }),
                    ),
                    ai_tool_approval_button(
                        &self.tokens,
                        self.i18n.t("ai.tool_use.reject"),
                        false,
                        Self::render_lucide_icon(LucideIcon::X, 11.0, rgb(0xef4444)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.resolve_ai_tool_approval(reject_id.clone(), false, cx);
                            cx.stop_propagation();
                        }),
                    ),
                ));
            }
            block = block.child(item);
        }
        if ai_latest_tool_round_marker(message).as_deref() == Some("awaiting-summary") {
            block = block.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x33))
                    .bg(rgba((self.tokens.ui.bg_hover << 8) | 0x33))
                    .px(px(self.tokens.spacing.two))
                    .py(px(self.tokens.spacing.two))
                    .text_size(px(11.0))
                    .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x99))
                    .child(Self::render_lucide_icon(
                        LucideIcon::LoaderCircle,
                        14.0,
                        rgba((self.tokens.ui.accent << 8) | 0xb3),
                    ))
                    .child(self.render_display_text_with_role_and_alpha(
                        SelectableTextRole::PlainDocument,
                        "ai-tool-use",
                        "awaiting-summary",
                        self.i18n.t("ai.tool_use.awaiting_summary"),
                        self.tokens.ui.text_muted,
                        0x99 as f32 / 255.0,
                        cx,
                    )),
            );
        }
        block.into_any_element()
    }

    fn render_ai_message_edit_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::AiMessageEdit;
        let save_disabled = self.ai_editing_message_draft.trim().is_empty();
        let input = text_input(
            &self.tokens,
            TextInputView {
                value: &self.ai_editing_message_draft,
                placeholder: String::new(),
                focused: self.ai_editing_message_focused,
                caret_visible: self.new_connection_caret_visible,
                secret: false,
                selected_all: false,
                selected_range: self.ime_selected_range_for_target(target),
                marked_text: self.marked_text_for_target(target),
            },
        )
        .border_0()
        .bg(rgba(0x00000000))
        .p_0()
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                this.ai_editing_message_focused = true;
                this.ai_chat_input_focused = false;
                this.ai_model_selector_search_focused = false;
                this.ime_marked_text = None;
                window.focus(&this.focus_handle);
                this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_move(
            cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            }),
        );
        let input = text_input_anchor_probe(
            target.anchor_id(),
            input,
            Self::deferred_ai_text_input_anchor_update(cx.entity()),
        );

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.one + self.tokens.spacing.one / 2.0))
            .child(
                div()
                    .min_h(px(60.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.accent << 8) | 0x66))
                    .bg(rgba((self.tokens.ui.bg << 8) | 0x80))
                    .px(px(self.tokens.spacing.two))
                    .py(px(self.tokens.spacing.one + self.tokens.spacing.one / 2.0))
                    .child(input),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(self.tokens.spacing.one))
                    .child(
                        ai_message_action(
                            &self.tokens,
                            self.i18n.t("ai.message.cancel"),
                            Self::render_lucide_icon(
                                LucideIcon::X,
                                12.0,
                                rgb(self.tokens.ui.text_muted),
                            ),
                            false,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.cancel_edit_ai_message(cx);
                                cx.stop_propagation();
                            }),
                        ),
                    )
                    .child(
                        ai_message_action(
                            &self.tokens,
                            self.i18n.t("ai.message.save_and_resend"),
                            Self::render_lucide_icon(
                                LucideIcon::Check,
                                12.0,
                                rgb(self.tokens.ui.accent),
                            ),
                            false,
                        )
                        .opacity(if save_disabled { 0.3 } else { 1.0 })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                if !save_disabled {
                                    this.save_ai_message_edit(cx);
                                }
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            )
            .into_any_element()
    }

    fn render_ai_sidebar_chat_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let active_title = self
            .ai_chat
            .active_conversation()
            .map(|conversation| conversation.title.clone());
        div()
            .w_full()
            .min_w_0()
            .min_h(px(36.0))
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
            .bg(rgb(self.tokens.ui.bg))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_w_0()
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "ai-chat-header",
                                "label",
                                self.i18n.t("ai.chat.header").to_uppercase(),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    )
                    .when_some(active_title, |row, title| {
                        let workspace = cx.entity();
                        row.child(
                            div()
                                .flex_none()
                                .text_color(rgba((self.tokens.ui.border << 8) | 0x66))
                                .child("·"),
                        )
                        .child(
                            select_anchor_probe(
                                SelectAnchorId::AiConversationList,
                                div()
                                    .min_w_0()
                                    .cursor_pointer()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x99))
                                    .hover(|style| style.text_color(rgb(self.tokens.ui.text)))
                                    .child(self.render_display_text_with_role_and_alpha(
                                        SelectableTextRole::NonSelectable,
                                        "ai-chat-header-title",
                                        title.clone(),
                                        title,
                                        self.tokens.ui.text_muted,
                                        0x99 as f32 / 255.0,
                                        cx,
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            let next_open = !this.ai_conversation_list_open;
                                            this.close_ai_sidebar_popovers();
                                            this.ai_conversation_list_open = next_open;
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                                Self::deferred_ai_select_anchor_update(workspace),
                            ),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(self.render_ai_sidebar_header_button(
                        LucideIcon::Plus,
                        self.i18n.t("ai.chat.new_chat_tooltip"),
                        Some(AiHeaderAction::NewChat),
                        cx,
                    ))
                    .child(self.render_ai_sidebar_header_button(
                        LucideIcon::MoreVertical,
                        self.i18n.t("ai.chat.more_options"),
                        Some(AiHeaderAction::Settings),
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_ai_sidebar_header_button(
        &self,
        icon: LucideIcon,
        label: String,
        action: Option<AiHeaderAction>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = matches!(action, Some(AiHeaderAction::NewChat))
            && self.ai_chat_initialization_error.is_some();
        // Tauri AiChatPanel header buttons are title-backed icon buttons. Route
        // tooltip ownership and disabled New Chat activation through the shared
        // workspace helper so AI header actions match other toolbar buttons.
        let button = self.workspace_tooltip_icon_button(
            icon,
            14.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                disabled,
                // Tauri AI header buttons use border/10 hover instead of the
                // default toolbar hover token.
                hover_background: Some(rgba((self.tokens.ui.border << 8) | 0x1a)),
                ..IconButtonOptions::opaque_toolbar(22.0, ButtonRadius::Md)
            },
            label,
            "ai-sidebar-header-button",
            false,
            cx.listener(move |this, _event, window, cx| {
                match action {
                    Some(AiHeaderAction::NewChat) => {
                        this.create_ai_sidebar_conversation(None, cx);
                    }
                    Some(AiHeaderAction::Settings) => {
                        let next_open = !this.ai_chat_menu_open;
                        this.close_ai_sidebar_popovers();
                        this.ai_chat_menu_open = next_open;
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }
                    None => {}
                }
                cx.stop_propagation();
            }),
            cx.entity(),
        )
        .into_any_element();

        if matches!(action, Some(AiHeaderAction::Settings)) {
            let workspace = cx.entity();
            select_anchor_probe(
                SelectAnchorId::AiChatMenu,
                button,
                Self::deferred_ai_select_anchor_update(workspace),
            )
            .into_any_element()
        } else {
            button.into_any_element()
        }
    }

    fn ai_tool_status_label(&self, status: AiToolStatus) -> String {
        // Status fallback text must follow the active UI locale when the
        // streamed tool call has not provided a model-facing summary yet.
        let key = match status {
            AiToolStatus::Pending => "ai.tool_use.status.pending",
            AiToolStatus::PendingApproval => "ai.tool_use.status.pending_approval",
            AiToolStatus::Approved => "ai.tool_use.status.approved",
            AiToolStatus::Running => "ai.tool_use.status.running",
            AiToolStatus::Completed => "ai.tool_use.status.completed",
            AiToolStatus::Error => "ai.tool_use.status.error",
            AiToolStatus::Rejected => "ai.tool_use.status.rejected",
        };
        self.i18n.t(key)
    }
}

fn ai_message_element_seed(value: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&value, &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

fn ai_tool_status_from_value(value: Option<&serde_json::Value>) -> AiToolStatus {
    match value.and_then(serde_json::Value::as_str).unwrap_or("pending") {
        "pending_user_approval" | "pending_approval" => AiToolStatus::PendingApproval,
        "approved" => AiToolStatus::Approved,
        "running" => AiToolStatus::Running,
        "completed" => AiToolStatus::Completed,
        "error" | "failed" => AiToolStatus::Error,
        "rejected" => AiToolStatus::Rejected,
        _ => AiToolStatus::Pending,
    }
}

fn ai_latest_tool_round_marker(message: &AiChatMessage) -> Option<String> {
    message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("toolRounds"))
        .and_then(serde_json::Value::as_array)
        .and_then(|rounds| rounds.last())
        .and_then(|round| round.get("statefulMarker"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn ai_turn_parts(message: &AiChatMessage) -> Option<&Vec<serde_json::Value>> {
    message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("parts"))
        .and_then(serde_json::Value::as_array)
}

fn ai_tool_part_round_id(message: &AiChatMessage, part: &serde_json::Value) -> Option<String> {
    let tool_call_id = part
        .get("id")
        .or_else(|| part.get("toolCallId"))
        .and_then(serde_json::Value::as_str)?;
    let rounds = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("toolRounds"))
        .and_then(serde_json::Value::as_array)?;
    rounds.iter().find_map(|round| {
        let has_tool_call = round
            .get("toolCalls")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|tool_calls| {
                tool_calls.iter().any(|tool_call| {
                    tool_call
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|existing| existing == tool_call_id)
                })
            });
        has_tool_call
            .then(|| round.get("id").and_then(serde_json::Value::as_str))
            .flatten()
            .map(str::to_string)
    })
}

fn ai_tool_call_value_from_turn_parts(
    id: &str,
    parts: &[serde_json::Value],
) -> Option<serde_json::Value> {
    let call = parts.iter().find(|part| {
        part.get("type").and_then(serde_json::Value::as_str) == Some("tool_call")
            && part
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == id)
    })?;
    let result = parts.iter().find(|part| {
        part.get("type").and_then(serde_json::Value::as_str) == Some("tool_result")
            && part
                .get("toolCallId")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == id)
    });
    let name = call
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("tool");
    let arguments = call
        .get("argumentsText")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let mut value = serde_json::json!({
        "id": id,
        "name": name,
        "arguments": arguments,
        "status": call
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("pending"),
        "result": serde_json::Value::Null,
    });
    if let Some(result) = result
        && let Some(object) = value.as_object_mut()
    {
        let success = result
            .get("success")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        object.insert(
            "status".to_string(),
            serde_json::json!(if success { "completed" } else { "error" }),
        );
        object.insert(
            "result".to_string(),
            result
                .get("envelope")
                .cloned()
                .unwrap_or_else(|| {
                    serde_json::json!({
                        "ok": success,
                        "output": result
                            .get("output")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default(),
                    })
                }),
        );
    }
    Some(value)
}

fn ai_tool_risk_from_value(value: Option<&serde_json::Value>, tool_name: &str) -> AiToolRisk {
    match value.and_then(serde_json::Value::as_str).unwrap_or("") {
        "read" => AiToolRisk::Read,
        "write" => AiToolRisk::WriteFile,
        "execute" => AiToolRisk::ExecuteCommand,
        "interactive" => AiToolRisk::InteractiveInput,
        "destructive" => AiToolRisk::Destructive,
        "credential" => AiToolRisk::CredentialSensitive,
        _ => match tool_name {
            "run_command" => AiToolRisk::ExecuteCommand,
            "send_terminal_input" => AiToolRisk::InteractiveInput,
            "write_resource" | "transfer_resource" => AiToolRisk::WriteFile,
            "connect_target" | "open_app_surface" | "remember_preference" => {
                AiToolRisk::SettingsChange
            }
            _ => AiToolRisk::Read,
        },
    }
}

fn ai_tool_status_icon(status: AiToolStatus) -> LucideIcon {
    match status {
        AiToolStatus::Completed => LucideIcon::Check,
        AiToolStatus::Error => LucideIcon::AlertCircle,
        AiToolStatus::Rejected => LucideIcon::X,
        AiToolStatus::PendingApproval => LucideIcon::AlertTriangle,
        AiToolStatus::Running | AiToolStatus::Approved => LucideIcon::LoaderCircle,
        AiToolStatus::Pending => LucideIcon::Clock,
    }
}

fn ai_tool_status_color(tokens: &oxideterm_theme::ThemeTokens, status: AiToolStatus) -> u32 {
    match status {
        AiToolStatus::Completed => 0x22c55e,
        AiToolStatus::Error => 0xef4444,
        AiToolStatus::Rejected => tokens.ui.text_muted,
        AiToolStatus::PendingApproval => 0xf59e0b,
        AiToolStatus::Running | AiToolStatus::Approved => tokens.ui.accent,
        AiToolStatus::Pending => 0xeab308,
    }
}

fn ai_tool_policy_decision_summary(policy_decision: &serde_json::Value) -> String {
    let decision = policy_decision
        .get("decision")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");
    let reason = policy_decision
        .get("reasonCode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");
    let matched_key = policy_decision
        .get("matchedPolicyKey")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");
    format!("{decision} · {reason} · {matched_key}")
}

fn pretty_tool_json_or_raw(value: &str) -> String {
    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|parsed| serde_json::to_string_pretty(&parsed).ok())
        .unwrap_or_else(|| value.to_string())
}
