impl AiAssistantSidebarView {
    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let panel = ai_chat_panel(&self.tokens)
            .relative()
            .bg(self.frame().panel_background);
        let panel = if self.frame().composer.enabled {
            let enabled = self.state.read(cx).history_ready();
            panel
                .child(self.views.header.clone())
                .children(self.render_ai_compaction_notice(cx))
                .child(self.views.background.clone())
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
                .children(self.render_ai_history_status(cx))
                .child(self.render_ai_context_warning_banners(cx))
                .child(self.render_ai_sidebar_model_bar(cx))
                .child(self.render_ai_sidebar_input(enabled, cx))
        } else {
            panel.child(self.render_ai_sidebar_disabled(cx))
        };
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
                    .child(panel),
                self.place_select_anchor(),
            ))
            .into_any_element()
    }

    pub fn render_ai_compaction_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (active_id, running, compacted_count) = {
            let ai = self.state.read(cx);
            let active_id = ai.conversation_state().active_conversation_id.as_ref()?;
            let notice = ai.compaction_notice()?;
            if notice.conversation_id != *active_id {
                return None;
            }
            (
                active_id.clone(),
                notice.phase == AiCompactionNoticePhase::Running,
                notice.compacted_count,
            )
        };
        let label = if running {
            self.i18n.t("ai.context.compaction_running")
        } else {
            self.i18n.t("ai.context.compaction_done").replace(
                "{{count}}",
                &compacted_count.unwrap_or_default().to_string(),
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
                            &active_id,
                            label,
                            self.tokens.ui.text_muted,
                            cx,
                        )),
                )
                .into_any_element(),
        )
    }

    fn render_ai_history_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ai = self.state.read(cx);
        let failed = matches!(ai.history.status, crate::state::HistoryStatus::Failed)
            && ai.history.writer.is_some();
        let loading = !failed && ai.chat_initialization_error().is_none() && !ai.history_ready();
        if !failed && !loading {
            return None;
        }
        let mut text = self.i18n.t(if failed {
            "ai.history.unsaved"
        } else {
            "ai.history.loading"
        });
        if loading {
            if let Some((done, total)) = ai.history.progress {
                text = format!("{text} ({done}/{total})");
            }
        }
        Some(
            div()
                .w_full()
                .flex_none()
                .p(px(12.0))
                .gap(px(8.0))
                .flex()
                .flex_col()
                .text_size(px(12.0))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(text)
                .when(failed, |row| {
                    row.child(
                        div()
                            .id("ai-history-retry")
                            .cursor_pointer()
                            .text_color(rgb(self.tokens.ui.accent))
                            .child(self.i18n.t("common.actions.retry"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.update(cx, |ai, _| ai.retry_history_write());
                            })),
                    )
                })
                .into_any_element(),
        )
    }

    pub fn render_ai_context_warning_banners(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut banners = div().flex_none().flex().flex_col();
        let acp_backend_active = self.frame().composer.backend == AiActiveBackend::Acp;
        if !acp_backend_active
            && let Some(percentage) = self
                .state
                .read(cx)
                .chat_ui()
                .model_switch_warning_percentage
        {
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
        if self.ai_context_danger_warning_active(cx) {
            banners = banners.child(self.render_ai_context_warning_banner(
                self.i18n.t(if acp_backend_active {
                    "ai.context.acp_approaching_limit"
                } else {
                    "ai.context.approaching_limit"
                }),
                false,
                false,
                cx,
            ));
        }
        banners.into_any_element()
    }

    pub fn render_ai_context_warning_banner(
        &self,
        label: String,
        dismissible: bool,
        model_switch: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let warning_color = self.tokens.ui.warning;
        let provider_managed_context = self.frame().composer.backend == AiActiveBackend::Provider;
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
                    .when(provider_managed_context, |actions| {
                        actions
                            .child(self.render_ai_context_warning_button(
                                self.i18n.t("ai.context.compact_button"),
                                LucideIcon::Archive,
                                self.state.read(cx).chat_is_loading(),
                                AiContextWarningAction::Compact(model_switch),
                                cx,
                            ))
                            .when(!model_switch, |actions| {
                                actions.child(self.render_ai_context_warning_button(
                                    self.i18n.t("ai.context.summarize"),
                                    LucideIcon::Archive,
                                    self.state.read(cx).chat_is_loading(),
                                    AiContextWarningAction::Summarize,
                                    cx,
                                ))
                            })
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

    pub fn render_ai_context_warning_button(
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
                            this.state.update(cx, |ai, _cx| {
                                ai.set_model_switch_warning(None);
                            });
                        }
                        (this.bindings.request)(AiTranscriptRequest::Compact, None, cx);
                    }
                    AiContextWarningAction::NewChat(model_switch) => {
                        if model_switch {
                            this.state.update(cx, |ai, _cx| {
                                ai.set_model_switch_warning(None);
                            });
                        }
                        (this.bindings.request)(AiTranscriptRequest::NewConversation, None, cx);
                    }
                    AiContextWarningAction::Dismiss => {
                        this.state.update(cx, |ai, _cx| {
                            ai.set_model_switch_warning(None);
                        });
                    }
                    AiContextWarningAction::Summarize => {
                        (this.bindings.request)(AiTranscriptRequest::Summarize, None, cx);
                    }
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub fn ai_context_danger_warning_active(&self, cx: &App) -> bool {
        let Some(conversation) = self
            .state
            .read(cx)
            .conversation_state()
            .active_conversation()
        else {
            return false;
        };
        if self.frame().composer.backend == AiActiveBackend::Provider
            && conversation.messages.len() < 4
        {
            return false;
        }
        let Some((total_tokens, max_tokens)) = self.frame().composer.context_counts else {
            return false;
        };
        ai_context_percentage(total_tokens, max_tokens) > AI_CONTEXT_DANGER_PERCENT
    }
}

impl AiAssistantSidebarView {
    pub fn render_ai_sidebar_disabled(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .px(px(16.0))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .text_center()
            .child(
                div()
                    .size(px(48.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba((self.tokens.ui.accent << 8) | 0x0d))
                    .child(Self::render_lucide_icon(
                        LucideIcon::MessageSquare,
                        24.0,
                        rgba((self.tokens.ui.text_muted << 8) | 0x66),
                    )),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "ai-chat-disabled",
                        "title",
                        self.i18n.t("ai.chat.title"),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .child(
                div()
                    .max_w(px(220.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .line_height(px(18.0))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "ai-chat-disabled",
                        "message",
                        self.i18n.t("ai.chat.disabled_message"),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
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
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-chat-disabled",
                        "open-settings",
                        self.i18n.t("ai.chat.open_settings"),
                        self.tokens.ui.bg,
                        cx,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            (this.bindings.request)(
                                AiTranscriptRequest::OpenSettings,
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }
}

#[derive(Clone, Copy)]
pub enum AiContextWarningAction {
    Compact(bool),
    NewChat(bool),
    Dismiss,
    Summarize,
}
