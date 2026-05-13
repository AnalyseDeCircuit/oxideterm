use oxideterm_ai::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiConversation, AiProviderView, AiStreamEvent,
    ModelSelectorProviderProbe, active_model_or_provider_default, active_provider_view,
    ai_help_markdown as ai_help_markdown_core, apply_chat_request_overrides,
    check_model_selector_provider_online, generate_chat_title, model_selector_display_name,
    model_max_response_tokens as ai_model_max_response_tokens, model_selector_truncated_label,
    model_selector_visible_provider_groups, parse_ai_user_input,
    provider_chat_requires_key as ai_provider_chat_requires_key,
    provider_views as ai_provider_views, resolve_ai_slash_command,
    resolve_model_selector_provider_probe, select_provider_model as ai_select_provider_model,
    slash_task_system_prompt, stream_chat_completion,
};
use crate::workspace::ime::WorkspaceImeTarget;
use oxideterm_gpui_settings_view::SettingsTab;
use oxideterm_gpui_ui::{
    TextInputView,
    ai::{
        AiModelSelectorPlacement, AiModelSelectorProviderState, ai_chat_panel,
        ai_chat_input_editor, ai_chat_input_footer, ai_chat_input_frame, ai_chat_input_root,
        ai_chat_input_status, ai_chat_scroll_area, ai_message_author, ai_message_body,
        ai_message_model_badge, ai_message_time, ai_model_selector_dropdown,
        ai_model_selector_empty_search, ai_model_selector_footer, ai_model_selector_key_status,
        ai_model_selector_list, ai_model_selector_local_status, ai_model_selector_model_row,
        ai_model_selector_models_panel, ai_model_selector_no_provider_button,
        ai_model_selector_provider_header, ai_model_selector_provider_message,
        ai_model_selector_refresh_button, ai_model_selector_root, ai_model_selector_search_bar,
        ai_model_selector_trigger_compact, ai_send_button,
    },
    text_input::{text_input, text_input_anchor_probe},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AiHeaderAction {
    NewChat,
    Settings,
}

impl WorkspaceApp {
    pub(super) fn render_ai_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let enabled = self.settings_store.settings().ai.enabled;
        if !enabled {
            return ai_chat_panel(&self.tokens)
                .relative()
                .child(self.render_ai_sidebar_disabled(cx))
                .into_any_element();
        }

        ai_chat_panel(&self.tokens)
            .relative()
            .child(self.render_ai_sidebar_chat_header(cx))
            .when(self.ai_conversation_list_open, |panel| {
                panel
                    .child(self.render_ai_sidebar_overlay_backdrop(cx))
                    .child(self.render_ai_conversation_dropdown(cx))
            })
            .when(self.ai_chat_menu_open, |panel| {
                panel
                    .child(self.render_ai_sidebar_overlay_backdrop(cx))
                    .child(self.render_ai_chat_menu(cx))
            })
            .child(
                ai_chat_scroll_area(&self.tokens, "ai-sidebar-scroll")
                    .p_0()
                    .child(self.render_ai_sidebar_chat_body(cx)),
            )
            .child(self.render_ai_sidebar_model_bar(cx))
            .child(self.render_ai_sidebar_input(true, cx))
            .into_any_element()
    }

    fn render_ai_sidebar_chat_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(conversation) = self.ai_chat.active_conversation() else {
            return self.render_ai_sidebar_empty_chat(cx);
        };
        if conversation.messages.is_empty() {
            return self.render_ai_sidebar_empty_chat(cx);
        }
        let mut body = div().flex().flex_col();
        for message in &conversation.messages {
            body = body.child(self.render_ai_message(conversation, message));
        }
        body.child(div().h(px(16.0))).into_any_element()
    }

    fn render_ai_sidebar_empty_chat(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .h_full()
            .flex()
            .flex_col()
            .p(px(24.0))
            .pt(px(48.0))
            .child(
                div()
                    .mb(px(24.0))
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("ai.chat.get_started")),
            )
            .child(
                div()
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

    fn render_ai_sidebar_overlay_backdrop(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.ai_conversation_list_open = false;
                    this.ai_chat_menu_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_ai_quick_prompt(
        &self,
        icon: LucideIcon,
        label: String,
        prompt: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
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
            .child(Self::render_lucide_icon(icon, 14.0, rgb(self.tokens.ui.text_muted)))
            .child(div().truncate().child(label))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.ai_chat_draft = prompt.clone();
                    this.ai_chat_input_focused = true;
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_ai_conversation_dropdown(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .absolute()
            .left(px(8.0))
            .right(px(8.0))
            .top(px(36.0))
            .max_h(px(256.0))
            .overflow_y_scrollbar()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .shadow_lg();

        if self.ai_chat.conversations.is_empty() {
            list = list.child(
                div()
                    .p(px(16.0))
                    .text_center()
                    .text_size(px(13.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ai.chat.no_conversations")),
            );
        } else {
            for conversation in &self.ai_chat.conversations {
                list = list.child(self.render_ai_conversation_item(conversation, cx));
            }
        }
        list.into_any_element()
    }

    fn render_ai_conversation_item(
        &self,
        conversation: &AiConversation,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = conversation.id.clone();
        let delete_id = conversation.id.clone();
        let is_active = self.ai_chat.active_conversation_id.as_deref() == Some(conversation.id.as_str());
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x26))
            .bg(if is_active {
                rgba((self.tokens.ui.accent << 8) | 0x1f)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .hover(|style| style.bg(rgba((self.tokens.ui.border << 8) | 0x1a)))
            .child(Self::render_lucide_icon(
                if is_active {
                    LucideIcon::Check
                } else {
                    LucideIcon::Bot
                },
                14.0,
                if is_active {
                    rgb(self.tokens.ui.accent)
                } else {
                    rgb(self.tokens.ui.text_muted)
                },
            ))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(if is_active {
                                rgb(self.tokens.ui.text)
                            } else {
                                rgb(self.tokens.ui.text_muted)
                            })
                            .child(conversation.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x80))
                            .child(self.ai_messages_count_label(conversation.messages.len())),
                    ),
            )
            .child(
                div()
                    .size(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .hover(|style| {
                        style
                            .bg(rgba((0xef4444_u32 << 8) | 0x1a))
                            .text_color(rgb(0xef4444))
                    })
                    .child(Self::render_lucide_icon(
                        LucideIcon::Trash2,
                        13.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.delete_ai_conversation(&delete_id);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.select_ai_conversation(id.clone());
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_ai_chat_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .right(px(8.0))
            .top(px(36.0))
            .w(px(160.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .shadow_lg()
            .child(self.render_ai_chat_menu_item(
                LucideIcon::Settings,
                self.i18n.t("ai.chat.settings"),
                false,
                AiHeaderAction::Settings,
                cx,
            ))
            .child(self.render_ai_chat_menu_item(
                LucideIcon::Trash2,
                self.i18n.t("ai.chat.clear_all"),
                true,
                AiHeaderAction::NewChat,
                cx,
            ))
            .into_any_element()
    }

    fn render_ai_chat_menu_item(
        &self,
        icon: LucideIcon,
        label: String,
        destructive: bool,
        action: AiHeaderAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .mx(px(2.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(10.0))
            .py(px(7.0))
            .text_size(px(12.0))
            .text_color(if destructive {
                rgb(0xef4444)
            } else {
                rgb(self.tokens.ui.text_muted)
            })
            .cursor_pointer()
            .hover(|style| {
                style.bg(if destructive {
                    rgba((0xef4444_u32 << 8) | 0x1a)
                } else {
                    rgba((self.tokens.ui.border << 8) | 0x1a)
                })
            })
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                if destructive {
                    rgb(0xef4444)
                } else {
                    rgb(self.tokens.ui.text_muted)
                },
            ))
            .child(div().truncate().child(label))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    match action {
                        AiHeaderAction::Settings => this.open_ai_settings(window, cx),
                        AiHeaderAction::NewChat => this.clear_ai_conversations(),
                    }
                    this.ai_chat_menu_open = false;
                    this.ai_conversation_list_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_ai_message(
        &self,
        _conversation: &AiConversation,
        message: &AiChatMessage,
    ) -> AnyElement {
        let role = match message.role {
            AiChatRole::User => oxideterm_gpui_ui::ai::AiMessageRole::User,
            AiChatRole::Assistant => oxideterm_gpui_ui::ai::AiMessageRole::Assistant,
            AiChatRole::System => oxideterm_gpui_ui::ai::AiMessageRole::Assistant,
        };
        let user = message.role == AiChatRole::User;
        let label = match message.role {
            AiChatRole::User => self.i18n.t("ai.message.you"),
            AiChatRole::Assistant => self.i18n.t("ai.message.assistant"),
            AiChatRole::System => "System".to_string(),
        };
        let mut header = div()
            .mb(px(self.tokens.spacing.one / 2.0))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.one + self.tokens.spacing.one / 2.0))
            .when(user, |row| row.flex_row_reverse())
            .child(ai_message_author(&self.tokens, label))
            .child(ai_message_time(
                &self.tokens,
                time_label(message.timestamp_ms),
                user,
            ));
        if let Some(model) = message.model.as_ref().filter(|model| !model.is_empty()) {
            header = header.child(ai_message_model_badge(&self.tokens, model.clone()));
        }
        div()
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.three))
            .child(header)
            .child(ai_message_body(
                &self.tokens,
                role,
                div()
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .text_color(if user {
                        rgb(self.tokens.ui.text)
                    } else {
                        rgb(self.tokens.ui.text)
                    })
                    .child(message.content.clone()),
            ))
            .into_any_element()
    }

    fn render_ai_sidebar_chat_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let active_title = self
            .ai_chat
            .active_conversation()
            .map(|conversation| conversation.title.clone());
        div()
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
                        row.child(
                            div()
                                .flex_none()
                                .text_color(rgba((self.tokens.ui.border << 8) | 0x66))
                                .child("·"),
                        )
                        .child(
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
                                        this.ai_conversation_list_open =
                                            !this.ai_conversation_list_open;
                                        this.ai_chat_menu_open = false;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
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
                        LucideIcon::MoreHorizontal,
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
        div()
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
                            this.ai_chat_menu_open = !this.ai_chat_menu_open;
                            this.ai_conversation_list_open = false;
                            window.focus(&this.focus_handle);
                            cx.notify();
                        }
                        None => {}
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_ai_sidebar_disabled(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .px(px(16.0))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .text_center()
            .child(Self::render_lucide_icon(
                LucideIcon::Bot,
                32.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("ai.chat.title")),
            )
            .child(
                div()
                    .max_w(px(220.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .line_height(px(18.0))
                    .child(self.i18n.t("ai.chat.disabled_message")),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .rounded(px(self.tokens.radii.md))
                    .px(px(10.0))
                    .py(px(6.0))
                    .bg(rgb(self.tokens.ui.accent))
                    .text_color(rgb(self.tokens.ui.bg))
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .cursor_pointer()
                    .child(self.i18n.t("ai.chat.open_settings"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.open_ai_settings(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_ai_sidebar_model_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .min_w_0()
            .px(px(12.0))
            .py(px(6.0))
            .border_t_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x33))
            .bg(rgb(self.tokens.ui.bg))
            .child(self.render_ai_model_selector(cx))
            .into_any_element()
    }

    fn render_ai_sidebar_input(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let placeholder = if enabled {
            self.i18n.t("ai.input.placeholder")
        } else {
            self.i18n.t("ai.input.placeholder_disabled")
        };
        let target = WorkspaceImeTarget::AiChatInput;
        let focused = self.ai_chat_input_focused;
        let input = text_input(
            &self.tokens,
            TextInputView {
                value: &self.ai_chat_draft,
                placeholder,
                focused,
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
                this.ai_chat_input_focused = true;
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
        let send_disabled = !enabled || self.ai_chat_draft.trim().is_empty();
        let send = ai_send_button(
            &self.tokens,
            if self.ai_chat_loading {
                self.i18n.t("ai.input.stop")
            } else {
                self.i18n.t("ai.input.send_btn")
            },
            send_disabled && !self.ai_chat_loading,
        );
        ai_chat_input_root(&self.tokens)
            .child(
                ai_chat_input_frame(&self.tokens, focused)
                    .child(ai_chat_input_editor(&self.tokens, input)),
            )
            .child(ai_chat_input_footer(
                &self.tokens,
                ai_chat_input_status(&self.tokens, "SHIFT+ENTER", false),
                send.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if this.ai_chat_loading {
                            this.cancel_ai_chat_stream(cx);
                        } else if !send_disabled {
                            this.send_ai_chat_draft(cx);
                        }
                        cx.stop_propagation();
                    }),
                ),
            ))
            .into_any_element()
    }

    fn render_ai_model_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        let providers = ai_provider_views(&self.settings_store.settings().ai.providers);
        let enabled_providers = providers
            .iter()
            .filter(|provider| provider.enabled)
            .cloned()
            .collect::<Vec<_>>();
        if enabled_providers.is_empty() {
            return ai_model_selector_no_provider_button(
                &self.tokens,
                Self::render_lucide_icon(LucideIcon::Settings, 12.0, rgb(self.tokens.ui.accent)),
                self.i18n.t("ai.model_selector.no_provider"),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.open_ai_settings(window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element();
        }

        let active_provider = active_provider_view(
            &providers,
            self.settings_store
                .settings()
                .ai
                .active_provider_id
                .as_deref(),
        );
        let active_model = self
            .settings_store
            .settings()
            .ai
            .active_model
            .as_deref();
        let display = model_selector_display_name(active_provider, active_model);
        let ready = active_provider
            .map(|provider| {
                self.ai_model_selector_has_key(provider)
                    && self.ai_model_selector_provider_is_online(provider)
            })
            .unwrap_or(false);
        let chevron = if self.ai_model_selector_open {
            LucideIcon::ChevronDown
        } else {
            LucideIcon::ChevronRight
        };
        let trigger = ai_model_selector_trigger_compact(
            &self.tokens,
            model_selector_truncated_label(&display),
            ready,
            self.ai_model_selector_open,
            Self::render_lucide_icon(chevron, 12.0, rgb(self.tokens.ui.text_muted)),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| {
                this.toggle_ai_model_selector(window, cx);
                cx.stop_propagation();
            }),
        );

        let mut root = ai_model_selector_root().child(trigger);
        if self.ai_model_selector_open {
            root = root.child(self.render_ai_model_selector_dropdown(&providers, cx));
        }
        root.into_any_element()
    }

    fn render_ai_model_selector_dropdown(
        &self,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        ai_model_selector_dropdown(&self.tokens, AiModelSelectorPlacement::Up)
            .child(self.render_ai_model_selector_search(cx))
            .child(self.render_ai_model_selector_list(providers, cx))
            .child(
                ai_model_selector_footer(
                    &self.tokens,
                    Self::render_lucide_icon(
                        LucideIcon::Settings,
                        12.0,
                        rgb(self.tokens.ui.text_muted),
                    ),
                    self.i18n.t("ai.model_selector.manage_providers"),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        this.ai_model_selector_open = false;
                        this.ai_model_selector_search_focused = false;
                        this.ai_model_selector_search_query.clear();
                        this.open_ai_settings(window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_ai_model_selector_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::AiModelSelectorSearch;
        let focused = self.ai_model_selector_search_focused;
        let input = text_input(
            &self.tokens,
            TextInputView {
                value: &self.ai_model_selector_search_query,
                placeholder: self.i18n.t("ai.model_selector.search_placeholder"),
                focused,
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
                this.ai_model_selector_search_focused = true;
                this.ime_marked_text = None;
                window.focus(&this.focus_handle);
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let workspace = cx.entity();
        let input = text_input_anchor_probe(
            target.anchor_id(),
            input,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        );
        let clear = (!self.ai_model_selector_search_query.is_empty()).then(|| {
            div()
                .size(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.ai_model_selector_search_query.clear();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(Self::render_lucide_icon(
                    LucideIcon::X,
                    12.0,
                    rgb(self.tokens.ui.text_muted),
                ))
                .into_any_element()
        });
        ai_model_selector_search_bar(
            &self.tokens,
            Self::render_lucide_icon(LucideIcon::Search, 12.0, rgb(self.tokens.ui.text_muted)),
            input,
            clear,
        )
        .into_any_element()
    }

    fn render_ai_model_selector_list(
        &self,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let groups =
            model_selector_visible_provider_groups(providers, &self.ai_model_selector_search_query);
        let mut list = ai_model_selector_list("ai-model-selector-list");
        if groups.is_empty() {
            return list
                .child(ai_model_selector_empty_search(
                    &self.tokens,
                    self.i18n.t("ai.model_selector.no_search_results"),
                ))
                .into_any_element();
        }

        for (index, group) in groups.into_iter().enumerate() {
            let provider = group.provider;
            let has_key = self.ai_model_selector_has_key(&provider);
            let online = self.ai_model_selector_provider_is_online(&provider);
            let expanded = !self.ai_model_selector_search_query.trim().is_empty()
                || self
                    .ai_model_selector_expanded_providers
                    .contains(&provider.id);
            let active_provider = self
                .settings_store
                .settings()
                .ai
                .active_provider_id
                .as_deref()
                == Some(provider.id.as_str());
            let active_provider_model = active_provider
                .then(|| {
                    self.settings_store
                        .settings()
                        .ai
                        .active_model
                        .as_deref()
                        .and_then(|model| model.rsplit('/').next())
                        .map(str::to_string)
                })
                .flatten();
            let status = self.render_ai_model_selector_provider_status(&provider, has_key, online);
            let refresh = (has_key && online).then(|| {
                let provider_for_refresh = provider.clone();
                ai_model_selector_refresh_button(
                    &self.tokens,
                    Self::render_lucide_icon(
                        LucideIcon::RefreshCw,
                        10.0,
                        rgb(self.tokens.ui.text_muted),
                    ),
                )
                .opacity(if self.ai_model_refreshing.contains(&provider.id) {
                    0.45
                } else {
                    1.0
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.refresh_ai_provider_from_selector(provider_for_refresh.clone(), cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
            });

            let provider_id = provider.id.clone();
            let header = ai_model_selector_provider_header(
                &self.tokens,
                provider.name.clone(),
                Self::render_lucide_icon(
                    if expanded {
                        LucideIcon::ChevronDown
                    } else {
                        LucideIcon::ChevronRight
                    },
                    12.0,
                    rgb(self.tokens.ui.accent),
                ),
                active_provider_model,
                status,
                refresh,
                has_key,
                index == 0,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if !this
                        .ai_model_selector_expanded_providers
                        .remove(&provider_id)
                    {
                        this.ai_model_selector_expanded_providers
                            .insert(provider_id.clone());
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

            let mut section = div().child(header);
            if expanded {
                section = section.child(self.render_ai_model_selector_models(
                    provider,
                    group.visible_models,
                    has_key,
                    online,
                    cx,
                ));
            }
            list = list.child(section);
        }
        list.into_any_element()
    }

    fn render_ai_model_selector_provider_status(
        &self,
        provider: &AiProviderView,
        has_key: bool,
        online: bool,
    ) -> AnyElement {
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::ImplicitKey { .. } => ai_model_selector_local_status(
                &self.tokens,
                online,
                if online {
                    self.i18n.t("ai.model_selector.ok")
                } else {
                    self.i18n.t("ai.model_selector.offline")
                },
            )
            .into_any_element(),
            _ => ai_model_selector_key_status(
                &self.tokens,
                has_key,
                Self::render_lucide_icon(
                    LucideIcon::Key,
                    10.0,
                    rgb(if has_key { 0x34d399 } else { 0xfbbf24 }),
                ),
                if has_key {
                    self.i18n.t("ai.model_selector.ok")
                } else {
                    self.i18n.t("ai.model_selector.no_key")
                },
            )
            .into_any_element(),
        }
    }

    fn render_ai_model_selector_models(
        &self,
        provider: AiProviderView,
        visible_models: Vec<String>,
        has_key: bool,
        online: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut panel = ai_model_selector_models_panel(&self.tokens);
        if matches!(
            resolve_model_selector_provider_probe(&provider),
            ModelSelectorProviderProbe::ImplicitKey { .. }
        ) && !online
        {
            return panel
                .child(ai_model_selector_provider_message(
                    &self.tokens,
                    self.i18n.t("ai.model_selector.offline"),
                    AiModelSelectorProviderState::Offline,
                    false,
                ))
                .into_any_element();
        }
        if !has_key {
            return panel
                .child(
                    ai_model_selector_provider_message(
                        &self.tokens,
                        self.i18n.t("ai.model_selector.no_key_warning"),
                        AiModelSelectorProviderState::MissingKey,
                        true,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.ai_model_selector_open = false;
                            this.open_ai_settings(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
                .into_any_element();
        }
        if visible_models.is_empty() {
            return panel
                .child(ai_model_selector_provider_message(
                    &self.tokens,
                    self.i18n.t("ai.model_selector.refresh_models"),
                    AiModelSelectorProviderState::Ready,
                    false,
                ))
                .into_any_element();
        }

        for model in visible_models {
            let active = self
                .settings_store
                .settings()
                .ai
                .active_provider_id
                .as_deref()
                == Some(provider.id.as_str())
                && self.settings_store.settings().ai.active_model.as_deref()
                    == Some(model.as_str());
            let model_for_click = model.clone();
            let provider_id = provider.id.clone();
            panel = panel.child(
                ai_model_selector_model_row(
                    &self.tokens,
                    model,
                    active,
                    active.then(|| {
                        Self::render_lucide_icon(
                            LucideIcon::Check,
                            12.0,
                            rgb(self.tokens.ui.accent),
                        )
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.select_ai_model_from_selector(
                            provider_id.clone(),
                            model_for_click.clone(),
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        panel.into_any_element()
    }

    fn toggle_ai_model_selector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ai_model_selector_open = !self.ai_model_selector_open;
        if self.ai_model_selector_open {
            let providers = ai_provider_views(&self.settings_store.settings().ai.providers);
            if let Some(provider) = active_provider_view(
                &providers,
                self.settings_store
                    .settings()
                    .ai
                    .active_provider_id
                    .as_deref(),
            ) {
                self.ai_model_selector_expanded_providers
                    .insert(provider.id.clone());
            }
            self.ai_model_selector_search_focused = true;
            self.refresh_ai_model_selector_provider_statuses(cx);
            window.focus(&self.focus_handle);
        } else {
            self.ai_model_selector_search_focused = false;
            self.ai_model_selector_search_query.clear();
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    fn refresh_ai_model_selector_provider_statuses(&mut self, cx: &mut Context<Self>) {
        let providers = ai_provider_views(&self.settings_store.settings().ai.providers);
        for provider in providers {
            match resolve_model_selector_provider_probe(&provider) {
                ModelSelectorProviderProbe::Disabled => {
                    self.ai_provider_key_status.insert(provider.id.clone(), false);
                    self.ai_model_selector_provider_online
                        .insert(provider.id.clone(), false);
                }
                ModelSelectorProviderProbe::StoredKey => {
                    let has_key = self.ai_provider_has_key(&provider.id);
                    self.ai_provider_key_status.insert(provider.id.clone(), has_key);
                    self.ai_model_selector_provider_online
                        .insert(provider.id.clone(), true);
                }
                ModelSelectorProviderProbe::ImplicitKey { endpoint } => {
                    self.ai_provider_key_status.insert(provider.id.clone(), true);
                    if let Some(endpoint) = endpoint {
                        self.schedule_ai_model_selector_online_probe(provider.clone(), endpoint, cx);
                    } else {
                        self.ai_model_selector_provider_online
                            .insert(provider.id.clone(), true);
                    }
                }
            }
        }
    }

    fn schedule_ai_model_selector_online_probe(
        &mut self,
        provider: AiProviderView,
        endpoint: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.next_ai_model_selector_probe_generation =
            self.next_ai_model_selector_probe_generation.saturating_add(1);
        let generation = self.next_ai_model_selector_probe_generation;
        let provider_id = provider.id.clone();
        self.ai_model_selector_probe_generations
            .insert(provider_id.clone(), generation);
        cx.spawn(async move |weak, cx| {
            let online = check_model_selector_provider_online(&provider.base_url, endpoint).await;
            let _ = weak.update(cx, |this, cx| {
                if this.ai_model_selector_probe_generations.get(&provider_id) != Some(&generation) {
                    return;
                }
                this.ai_model_selector_provider_online
                    .insert(provider_id.clone(), online);
                cx.notify();
            });
        })
        .detach();
    }

    fn ai_model_selector_has_key(&self, provider: &AiProviderView) -> bool {
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::ImplicitKey { .. } => true,
            ModelSelectorProviderProbe::StoredKey => self.ai_provider_has_key(&provider.id),
        }
    }

    fn ai_model_selector_provider_is_online(&self, provider: &AiProviderView) -> bool {
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::StoredKey => true,
            ModelSelectorProviderProbe::ImplicitKey { .. } => self
                .ai_model_selector_provider_online
                .get(&provider.id)
                .copied()
                .unwrap_or(true),
        }
    }

    fn refresh_ai_provider_from_selector(
        &mut self,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) {
        if !self.ai_model_selector_has_key(&provider) {
            self.push_ai_settings_toast(
                self.i18n.t("ai.model_selector.no_key_warning"),
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return;
        }
        if !self.ai_model_selector_provider_is_online(&provider) {
            self.push_ai_settings_toast(
                self.i18n.t("ai.model_selector.offline"),
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return;
        }
        let Some(index) = ai_provider_views(&self.settings_store.settings().ai.providers)
            .iter()
            .position(|candidate| candidate.id == provider.id)
        else {
            return;
        };
        self.refresh_ai_provider_models(index, provider, cx);
    }

    fn select_ai_model_from_selector(
        &mut self,
        provider_id: String,
        model: String,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            |settings| {
                ai_select_provider_model(
                    &mut settings.ai.providers,
                    &mut settings.ai.active_provider_id,
                    &mut settings.ai.active_model,
                    &provider_id,
                    model.clone(),
                );
            },
            cx,
        );
        self.ai_model_selector_open = false;
        self.ai_model_selector_search_focused = false;
        self.ai_model_selector_search_query.clear();
        cx.notify();
    }

    pub(super) fn handle_ai_sidebar_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.ai_model_selector_open && self.ai_model_selector_search_focused {
            if event.keystroke.modifiers.platform {
                return false;
            }
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.ai_model_selector_open = false;
                    self.ai_model_selector_search_focused = false;
                    self.ai_model_selector_search_query.clear();
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                "backspace" => {
                    self.ai_model_selector_search_query.pop();
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                _ => true,
            }
        } else if self.ai_chat_input_focused {
            if event.keystroke.modifiers.platform {
                return false;
            }
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.ai_chat_input_focused = false;
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                "backspace" => {
                    self.ai_chat_draft.pop();
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                "enter" if !event.keystroke.modifiers.shift && !self.ai_chat_loading => {
                    self.send_ai_chat_draft(cx);
                    true
                }
                "enter" => {
                    self.ai_chat_draft.push('\n');
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                _ => true,
            }
        } else {
            false
        }
    }

    fn create_ai_sidebar_conversation(
        &mut self,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) -> String {
        let now = ai_now_ms();
        let id = self.next_ai_chat_id(now);
        let profile_id = self
            .settings_store
            .settings()
            .ai
            .execution_profiles
            .get("defaultProfileId")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let id = self
            .ai_chat
            .create_conversation(id, title, now, profile_id);
        self.persist_ai_chat_state();
        self.ai_conversation_list_open = false;
        self.ai_chat_menu_open = false;
        self.ai_chat_draft.clear();
        self.ai_chat_input_focused = true;
        cx.notify();
        id
    }

    fn send_ai_chat_draft(&mut self, cx: &mut Context<Self>) {
        let content = self.ai_chat_draft.trim().to_string();
        if content.is_empty() {
            cx.notify();
            return;
        }
        if !self.settings_store.settings().ai.enabled {
            self.push_ai_settings_toast(
                self.i18n.t("ai.chat.disabled_message"),
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return;
        }

        let parsed_input = parse_ai_user_input(&content);
        let slash_command = parsed_input
            .slash_command
            .as_deref()
            .and_then(resolve_ai_slash_command);
        if let Some(command) = slash_command.filter(|command| command.client_only) {
            match command.name {
                "clear" => {
                    self.create_ai_sidebar_conversation(None, cx);
                    self.ai_chat_draft.clear();
                    self.ime_marked_text = None;
                    cx.notify();
                    return;
                }
                "help" => {
                    self.add_ai_help_response(content, cx);
                    return;
                }
                "compact" => {
                    self.push_ai_settings_toast(
                        self.i18n.t("ai.context.compact_button"),
                        TerminalNoticeVariant::Default,
                    );
                    self.ai_chat_draft.clear();
                    self.ime_marked_text = None;
                    cx.notify();
                    return;
                }
                _ => return,
            }
        }

        let now = ai_now_ms();
        let title = generate_chat_title(&content);
        let id = self.next_ai_chat_id(now);
        let profile_id = self
            .settings_store
            .settings()
            .ai
            .execution_profiles
            .get("defaultProfileId")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let conversation_id = self
            .ai_chat
            .ensure_conversation(id, Some(title), now, profile_id);
        let providers = ai_provider_views(&self.settings_store.settings().ai.providers);
        let active_provider = active_provider_view(
            &providers,
            self.settings_store
                .settings()
                .ai
                .active_provider_id
                .as_deref(),
        );
        let active_model = active_provider.and_then(|provider| {
            active_model_or_provider_default(
                self.settings_store.settings().ai.active_model.as_deref(),
                provider,
            )
        });
        let message = AiChatMessage {
            id: self.next_ai_chat_id(now),
            role: AiChatRole::User,
            content,
            timestamp_ms: now,
            model: active_model,
            context: None,
            is_streaming: false,
        };
        self.ai_chat.add_message(&conversation_id, message);
        self.persist_ai_chat_state();
        let request_content = slash_command
            .and_then(|command| command.system_prompt_modifier.map(|_| ()))
            .and_then(|_| {
                (!parsed_input.clean_text.is_empty()).then_some(parsed_input.clean_text)
            });
        let task_system_prompt = slash_command.and_then(slash_task_system_prompt);
        self.start_ai_chat_stream(conversation_id, request_content, task_system_prompt, cx);
        self.ai_chat_draft.clear();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn add_ai_help_response(&mut self, content: String, cx: &mut Context<Self>) {
        let now = ai_now_ms();
        let title = generate_chat_title(&content);
        let id = self.next_ai_chat_id(now);
        let profile_id = self
            .settings_store
            .settings()
            .ai
            .execution_profiles
            .get("defaultProfileId")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let conversation_id = self
            .ai_chat
            .ensure_conversation(id, Some(title), now, profile_id);
        let user_message = AiChatMessage {
            id: self.next_ai_chat_id(now),
            role: AiChatRole::User,
            content,
            timestamp_ms: now,
            model: None,
            context: None,
            is_streaming: false,
        };
        let assistant_message = AiChatMessage {
            id: self.next_ai_chat_id(now),
            role: AiChatRole::Assistant,
            content: self.ai_help_markdown(),
            timestamp_ms: now,
            model: None,
            context: None,
            is_streaming: false,
        };
        self.ai_chat.add_message(&conversation_id, user_message);
        self.ai_chat.add_message(&conversation_id, assistant_message);
        self.persist_ai_chat_state();
        self.ai_chat_draft.clear();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn ai_help_markdown(&self) -> String {
        ai_help_markdown_core(|key| self.i18n.t(key))
    }

    fn start_ai_chat_stream(
        &mut self,
        conversation_id: String,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some((config, history)) =
            self.build_ai_stream_request(&conversation_id, request_content, task_system_prompt)
        else {
            return;
        };
        let now = ai_now_ms();
        let assistant_id = self.next_ai_chat_id(now);
        self.ai_chat.add_message(
            &conversation_id,
            AiChatMessage {
                id: assistant_id.clone(),
                role: AiChatRole::Assistant,
                content: String::new(),
                timestamp_ms: now,
                model: Some(config.model.clone()),
                context: None,
                is_streaming: true,
            },
        );
        self.ai_chat_loading = true;
        self.ai_chat_stream_generation = self.ai_chat_stream_generation.saturating_add(1);
        let generation = self.ai_chat_stream_generation;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        self.forwarding_runtime.spawn(stream_chat_completion(config, history, tx));
        cx.spawn(async move |weak, cx| {
            while let Some(event) = rx.recv().await {
                let done = matches!(event, AiStreamEvent::Done | AiStreamEvent::Error(_));
                let _ = weak.update(cx, |this, cx| {
                    this.apply_ai_stream_event(
                        generation,
                        &conversation_id,
                        &assistant_id,
                        event,
                        cx,
                    );
                });
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    fn build_ai_stream_request(
        &self,
        conversation_id: &str,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
    ) -> Option<(AiChatStreamConfig, Vec<AiChatMessage>)> {
        let settings = self.settings_store.settings();
        let providers = ai_provider_views(&settings.ai.providers);
        let provider =
            active_provider_view(&providers, settings.ai.active_provider_id.as_deref())?.clone();
        let model = active_model_or_provider_default(settings.ai.active_model.as_deref(), &provider)?;
        let api_key = if ai_provider_chat_requires_key(&provider.provider_type) {
            match self.ai_key_store.get_provider_key(&provider.id) {
                Ok(key) => key,
                Err(_) => None,
            }
        } else {
            None
        };
        let max_response_tokens =
            ai_model_max_response_tokens(&settings.ai.model_max_response_tokens, &provider.id, &model);
        let mut history = self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| conversation.messages.clone())?;
        apply_chat_request_overrides(&mut history, request_content, task_system_prompt);
        Some((
            AiChatStreamConfig {
                provider_type: provider.provider_type,
                base_url: provider.base_url,
                model,
                api_key,
                max_response_tokens,
            },
            history,
        ))
    }

    fn apply_ai_stream_event(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event: AiStreamEvent,
        cx: &mut Context<Self>,
    ) {
        if self.ai_chat_stream_generation != generation {
            return;
        }
        match event {
            AiStreamEvent::Content(chunk) | AiStreamEvent::Thinking(chunk) => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        message.content.push_str(&chunk);
                    });
            }
            AiStreamEvent::Done => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        message.is_streaming = false;
                    });
                self.ai_chat_loading = false;
                self.persist_ai_chat_state();
            }
            AiStreamEvent::Error(error) => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        message.is_streaming = false;
                        if message.content.is_empty() {
                            message.content = error.clone();
                        } else {
                            message.content.push_str("\n\n");
                            message.content.push_str(&error);
                        }
                    });
                self.ai_chat_loading = false;
                self.persist_ai_chat_state();
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            }
        }
        cx.notify();
    }

    fn cancel_ai_chat_stream(&mut self, cx: &mut Context<Self>) {
        self.ai_chat_stream_generation = self.ai_chat_stream_generation.saturating_add(1);
        self.ai_chat_loading = false;
        if let Some(conversation) = self.ai_chat.active_conversation_mut() {
            for message in &mut conversation.messages {
                message.is_streaming = false;
            }
        }
        self.persist_ai_chat_state();
        cx.notify();
    }

    fn select_ai_conversation(&mut self, id: String) {
        self.ai_chat.set_active_conversation(id);
        self.ai_conversation_list_open = false;
        self.ai_chat_menu_open = false;
        self.ai_chat_input_focused = true;
    }

    fn delete_ai_conversation(&mut self, id: &str) {
        self.ai_chat.delete_conversation(id);
        self.ai_conversation_list_open = !self.ai_chat.conversations.is_empty();
        self.ai_chat_menu_open = false;
        self.persist_ai_chat_state();
    }

    fn clear_ai_conversations(&mut self) {
        self.ai_chat.clear_conversations();
        self.ai_conversation_list_open = false;
        self.ai_chat_menu_open = false;
        self.cancel_ai_chat_stream_without_notify();
        self.persist_ai_chat_state();
    }

    fn cancel_ai_chat_stream_without_notify(&mut self) {
        self.ai_chat_stream_generation = self.ai_chat_stream_generation.saturating_add(1);
        self.ai_chat_loading = false;
        if let Some(conversation) = self.ai_chat.active_conversation_mut() {
            for message in &mut conversation.messages {
                message.is_streaming = false;
            }
        }
    }

    fn persist_ai_chat_state(&self) {
        let store = self.ai_chat_store.clone();
        let state = self.ai_chat.clone();
        self.forwarding_runtime.spawn_blocking(move || {
            if let Err(error) = store.save_state(&state) {
                eprintln!("[AiChatStore] Failed to persist conversation: {error}");
            }
        });
    }

    fn ai_messages_count_label(&self, count: usize) -> String {
        self.i18n
            .t("ai.chat.messages_count")
            .replace("{{count}}", &count.to_string())
    }

    fn next_ai_chat_id(&mut self, now_ms: i64) -> String {
        self.next_ai_chat_sequence = self.next_ai_chat_sequence.saturating_add(1);
        format!("chat-{now_ms}-{}", self.next_ai_chat_sequence)
    }

    fn open_ai_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_settings_tab = SettingsTab::Ai;
        self.open_settings(window, cx);
    }
}

fn ai_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn time_label(timestamp_ms: i64) -> String {
    let secs = ((timestamp_ms / 1000) % 86_400).max(0);
    let hours = secs / 3_600;
    let minutes = (secs % 3_600) / 60;
    format!("{hours:02}:{minutes:02}")
}
