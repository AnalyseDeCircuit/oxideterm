impl WorkspaceApp {
    pub(super) fn render_ai_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let enabled = self.settings_store.settings().ai.enabled;
        let panel = if !enabled {
            ai_chat_panel(&self.tokens)
                .relative()
                .child(self.render_ai_sidebar_disabled(cx))
                .into_any_element()
        } else {
            ai_chat_panel(&self.tokens)
                .relative()
                .child(self.render_ai_sidebar_chat_header(cx))
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .child(
                            ai_chat_scroll_area(&self.tokens, "ai-sidebar-scroll")
                                .h_full()
                                .p_0()
                                .child(self.render_ai_sidebar_chat_body(cx)),
                        ),
                )
                .child(self.render_ai_context_warning_banners(cx))
                .child(self.render_ai_sidebar_model_bar(cx))
                .child(self.render_ai_sidebar_input(true, cx))
                .into_any_element()
        };

        let workspace = cx.entity();
        div()
            .w_full()
            .h_full()
            .min_w_0()
            .min_h_0()
            .child(select_anchor_probe(
                SelectAnchorId::AiPanelRoot,
                panel,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn render_ai_sidebar_chat_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(conversation) = self.ai_chat.active_conversation() else {
            return self.render_ai_sidebar_empty_chat(cx);
        };
        if conversation.messages.is_empty() {
            return self.render_ai_sidebar_empty_chat(cx);
        }
        let mut body = div().w_full().min_w_0().flex().flex_col();
        if let Some(count) = self.ai_context_trim_notice_count {
            body = body.child(self.render_ai_trim_notice(count));
        }
        for message in &conversation.messages {
            body = body.child(self.render_ai_message(conversation, message, cx));
        }
        body.child(div().h(px(16.0))).into_any_element()
    }

    fn render_ai_trim_notice(&self, count: usize) -> AnyElement {
        const AI_TRIM_NOTICE_COLOR: u32 = 0xf59e0b;
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(rgba((AI_TRIM_NOTICE_COLOR << 8) | 0x33))
            .bg(rgba((AI_TRIM_NOTICE_COLOR << 8) | 0x1a))
            .child(Self::render_lucide_icon(
                LucideIcon::Scissors,
                12.0,
                rgb(AI_TRIM_NOTICE_COLOR),
            ))
            .child(
                div()
                    .flex_1()
                    .text_size(px(10.0))
                    .text_color(rgb(0xfbbf24))
                    .child(
                        self.i18n
                            .t("ai.context.messages_trimmed")
                            .replace("{{count}}", &count.to_string()),
                    ),
            )
            .into_any_element()
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

    fn render_ai_context_warning_banners(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut banners = div().flex_none().flex().flex_col();
        if let Some(percentage) = self.ai_model_switch_warning_percentage {
            banners = banners.child(self.render_ai_context_warning_banner(
                self.i18n
                    .t("ai.context.model_switched_warning")
                    .replace("{{percentage}}", &percentage.to_string()),
                true,
                true,
                cx,
            ));
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

    fn render_ai_context_warning_banner(
        &self,
        label: String,
        dismissible: bool,
        model_switch: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Tauri banner uses bg-amber-500/10 and border-amber-500/20.
        const AI_CONTEXT_WARNING_COLOR: u32 = 0xf59e0b;
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .border_t_1()
            .border_color(rgba((AI_CONTEXT_WARNING_COLOR << 8) | 0x33))
            .bg(rgba((AI_CONTEXT_WARNING_COLOR << 8) | 0x1a))
            .child(Self::render_lucide_icon(
                LucideIcon::AlertTriangle,
                14.0,
                rgb(AI_CONTEXT_WARNING_COLOR),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(11.0))
                    .text_color(rgb(0xfbbf24))
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.render_ai_context_warning_button(
                        self.i18n.t("ai.context.compact_button"),
                        LucideIcon::Archive,
                        self.ai_chat_loading,
                        AiContextWarningAction::Compact(model_switch),
                        cx,
                    ))
                    .when(!model_switch, |actions| {
                        actions.child(self.render_ai_context_warning_button(
                            self.i18n.t("ai.context.summarize"),
                            LucideIcon::Archive,
                            self.ai_chat_loading,
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

    fn render_ai_context_warning_button(
        &self,
        label: impl Into<String>,
        icon: LucideIcon,
        disabled: bool,
        action: AiContextWarningAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = label.into();
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(2.0))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(if disabled { 0xb78322 } else { 0xfbbf24 }))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .when(!disabled, |button| {
                button.cursor_pointer().hover(|style| {
                    style
                        .bg(rgba((0xf59e0b << 8) | 0x1a))
                        .text_color(rgb(0xfcd34d))
                })
            })
            .child(Self::render_lucide_icon(icon, 12.0, rgb(0xfbbf24)))
            .when(!label.is_empty(), |button| button.child(label))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if disabled {
                        cx.stop_propagation();
                        return;
                    }
                    match action {
                        AiContextWarningAction::Compact(model_switch) => {
                            if model_switch {
                                this.ai_model_switch_warning_percentage = None;
                            }
                            this.start_ai_compact_conversation(cx);
                        }
                        AiContextWarningAction::NewChat(model_switch) => {
                            if model_switch {
                                this.ai_model_switch_warning_percentage = None;
                            }
                            this.create_ai_sidebar_conversation(None, cx);
                        }
                        AiContextWarningAction::Dismiss => {
                            this.ai_model_switch_warning_percentage = None;
                        }
                        AiContextWarningAction::Summarize => {
                            this.ai_summarize_confirm_open = true;
                        }
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn ai_context_danger_warning_active(&self) -> bool {
        let Some(conversation) = self.ai_chat.active_conversation() else {
            return false;
        };
        if conversation.messages.len() < 4 {
            return false;
        }
        let (total_tokens, max_tokens) = self.ai_context_message_usage_counts();
        ai_context_percentage(total_tokens, max_tokens) > AI_CONTEXT_DANGER_PERCENT
    }

    fn ai_context_message_usage_counts(&self) -> (usize, usize) {
        let settings = self.settings_store.settings();
        let providers = ai_provider_views(&settings.ai.providers);
        let active_provider =
            active_provider_view(&providers, settings.ai.active_provider_id.as_deref());
        let model = active_provider
            .and_then(|provider| {
                active_model_or_provider_default(settings.ai.active_model.as_deref(), provider)
            })
            .unwrap_or_default();
        let provider_id = active_provider.map(|provider| provider.id.as_str()).unwrap_or("");
        let max_tokens = ai_context_window_from_maps(
            &settings.ai.user_context_windows,
            &settings.ai.model_context_windows,
            provider_id,
            &model,
        )
        .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW);
        let total_tokens = self
            .ai_chat
            .active_conversation()
            .map(ai_conversation_message_tokens)
            .unwrap_or(0);
        (total_tokens, max_tokens)
    }

    pub(in crate::workspace) fn render_ai_summarize_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Default,
                title: div()
                    .child(self.i18n.t("ai.context.summarize_confirm"))
                    .into_any_element(),
                description: None,
                cancel_label: div().child(self.i18n.t("common.cancel")).into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("ai.context.summarize"))
                    .into_any_element(),
            },
            cx.listener(|this, _event, _window, cx| {
                this.ai_summarize_confirm_open = false;
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.ai_summarize_confirm_open = false;
                this.start_ai_summarize_conversation(cx);
                cx.stop_propagation();
            }),
        )
    }

    pub(in crate::workspace) fn render_ai_clear_all_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.i18n.t("ai.chat.clear_all_confirm"))
                    .into_any_element(),
                description: None,
                cancel_label: div().child(self.i18n.t("common.actions.cancel")).into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("common.actions.confirm"))
                    .into_any_element(),
            },
            cx.listener(|this, _event, _window, cx| {
                this.ai_clear_all_confirm_open = false;
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.clear_ai_conversations();
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    pub(in crate::workspace) fn render_ai_delete_message_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.i18n.t("ai.message.delete_confirm"))
                    .into_any_element(),
                description: None,
                cancel_label: div().child(self.i18n.t("common.actions.cancel")).into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("common.actions.confirm"))
                    .into_any_element(),
            },
            cx.listener(|this, _event, _window, cx| {
                this.ai_delete_message_confirm = None;
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                if let Some(message_id) = this.ai_delete_message_confirm.take() {
                    this.delete_ai_message(&message_id, cx);
                } else {
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
    }
}

#[derive(Clone, Copy)]
enum AiContextWarningAction {
    Compact(bool),
    NewChat(bool),
    Dismiss,
    Summarize,
}
