impl WorkspaceApp {
    fn render_ai_message(
        &self,
        conversation: &AiConversation,
        message: &AiChatMessage,
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
                                        .child(label),
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
                                .child(message.content.clone()),
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
        let thinking_content = message
            .thinking_content
            .as_ref()
            .map(|content| content.trim())
            .filter(|content| !content.is_empty());
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
        } else if !message.content.is_empty() {
            body = body.child(self.render_ai_message_content(message));
        }
        if !message.tool_calls.is_empty() {
            body = body.child(self.render_ai_tool_calls(message, cx));
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
                            .child(format!("{}/{}", branches.active_index + 1, branches.total)),
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
        div()
            .w_full()
            .flex_none()
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.three))
            .child(header)
            .child(ai_message_body(&self.tokens, role, body))
            .into_any_element()
    }

    fn render_ai_message_content(&self, message: &AiChatMessage) -> AnyElement {
        if message.role == AiChatRole::User {
            return div()
                .w_full()
                .min_w_0()
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(rgb(self.tokens.ui.text))
                .children(message.content.split('\n').map(|line| {
                    div()
                        .w_full()
                        .min_w_0()
                        .child(line.to_string())
                        .into_any_element()
                }))
                .into_any_element();
        }

        let mut options = MarkdownOptions::from_theme(&self.tokens);
        options.base_font_size = 13.0;
        options.block_gap = 8.0;
        div()
            .w_full()
            .min_w_0()
            .text_color(rgb(self.tokens.ui.text))
            .child(markdown_with_options(&self.tokens, &message.content, &options))
            .into_any_element()
    }

    fn render_ai_tool_calls(&self, message: &AiChatMessage, cx: &mut Context<Self>) -> AnyElement {
        let mut block = ai_tool_block(&self.tokens).child(ai_tool_heading(&self.tokens, "工具调用"));
        for call in &message.tool_calls {
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
                .unwrap_or_else(|| ai_tool_status_label(status))
                .to_string();
            let result = call.get("result").filter(|value| !value.is_null());
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
                bypass_approval: false,
            };
            let mut item = ai_tool_item(&self.tokens, &view).child(ai_tool_item_header(
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
                    LucideIcon::ChevronDown,
                    12.0,
                    rgba((self.tokens.ui.text_muted << 8) | 0x80),
                ),
            ));

            let mut details = ai_tool_details(&self.tokens)
                .child(
                    div()
                        .child(ai_tool_section_label(&self.tokens, "参数", None))
                        .child(ai_tool_args_pre(
                            &self.tokens,
                            ("ai-tool-args", ai_message_element_seed(&id)),
                            pretty_tool_json_or_raw(&arguments),
                        )),
                );
            if let Some(result) = result {
                let output = result
                    .get("output")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| serde_json::to_string_pretty(result).unwrap_or_default());
                details = details.child(
                    div()
                        .child(ai_tool_section_label(
                            &self.tokens,
                            "结果",
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
                        )),
                );
            }
            item = item.child(details);

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
                        .child("需要确认后执行"),
                    ai_tool_approval_button(
                        &self.tokens,
                        "允许",
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
                        "拒绝",
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
                marked_text: self.marked_text_for_target(target),
            },
        )
        .border_0()
        .bg(rgba(0x00000000))
        .p_0()
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| {
                this.ai_editing_message_focused = true;
                this.ai_chat_input_focused = false;
                this.ai_model_selector_search_focused = false;
                this.ime_marked_text = None;
                window.focus(&this.focus_handle);
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let workspace = cx.entity();
        let input = text_input_anchor_probe(target.anchor_id(), input, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        });

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
                            .child(self.i18n.t("ai.chat.header").to_uppercase()),
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
                                    .child(title)
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
                                move |anchor, _window, cx| {
                                    let _ = workspace.update(cx, |this, cx| {
                                        this.update_select_anchor(anchor, cx);
                                    });
                                },
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
        _label: String,
        action: Option<AiHeaderAction>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let button = div()
            .p(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgba((self.tokens.ui.border << 8) | 0x1a))
                    .text_color(rgb(self.tokens.ui.text))
            })
            .child(Self::render_lucide_icon(icon, 14.0, rgb(self.tokens.ui.text_muted)))
            .on_mouse_down(
                MouseButton::Left,
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
            );

        if matches!(action, Some(AiHeaderAction::Settings)) {
            let workspace = cx.entity();
            select_anchor_probe(
                SelectAnchorId::AiChatMenu,
                button,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            )
            .into_any_element()
        } else {
            button.into_any_element()
        }
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

fn ai_tool_status_label(status: AiToolStatus) -> &'static str {
    match status {
        AiToolStatus::Pending => "等待执行",
        AiToolStatus::PendingApproval => "等待确认",
        AiToolStatus::Approved => "已允许",
        AiToolStatus::Running => "执行中",
        AiToolStatus::Completed => "已完成",
        AiToolStatus::Error => "执行失败",
        AiToolStatus::Rejected => "已拒绝",
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

fn pretty_tool_json_or_raw(value: &str) -> String {
    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|parsed| serde_json::to_string_pretty(&parsed).ok())
        .unwrap_or_else(|| value.to_string())
}
