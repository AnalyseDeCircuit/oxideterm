impl WorkspaceApp {
    fn active_ai_safety_mode(&self) -> AiSafetyMode {
        self.ai_chat
            .active_conversation_id
            .as_ref()
            .filter(|id| self.ai_safety_bypass_conversations.contains(*id))
            .map(|_| AiSafetyMode::Bypass)
            .unwrap_or(AiSafetyMode::Default)
    }

    fn render_ai_sidebar_model_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .relative()
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
            .when_some(self.render_ai_profile_indicator(cx), |bar, indicator| {
                bar.child(indicator)
            })
            .child(self.render_ai_safety_indicator(cx))
            .child(self.render_ai_tool_indicator(cx))
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
        let autocomplete_items = self.ai_chat_autocomplete_items();
        let marked_text = self.marked_text_for_target(target).unwrap_or_default();
        let showing_placeholder = self.ai_chat_draft.is_empty() && marked_text.is_empty();
        let input_text = if showing_placeholder {
            placeholder
        } else {
            self.ai_chat_draft.clone()
        };
        let mut input = div()
            .w_full()
            .min_h(px(20.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_size(px(13.0))
            .line_height(px(20.0))
            .text_color(if showing_placeholder {
                rgba((self.tokens.ui.text_muted << 8) | 0x4d)
            } else {
                rgb(self.tokens.ui.text)
            })
            .opacity(if enabled { 1.0 } else { 0.5 })
            .cursor(CursorStyle::IBeam);
        let lines: Vec<&str> = input_text.split('\n').collect();
        for (index, line) in lines.iter().enumerate() {
            let is_last_line = index + 1 == lines.len();
            input = input.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .when(focused && showing_placeholder && index == 0, |line| {
                        line.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                    })
                    .child(div().truncate().child((*line).to_string()))
                    .when(focused && is_last_line && !showing_placeholder, |line| {
                        line.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                    }),
            );
        }
        if focused && !marked_text.is_empty() {
            input = input.child(
                div()
                    .underline()
                    .text_color(rgb(self.tokens.ui.text))
                    .child(marked_text.to_string()),
            );
        }
        let input = input.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                this.ai_chat_input_focused = true;
                this.ai_model_selector_search_focused = false;
                this.ime_marked_text = None;
                window.focus(&this.focus_handle);
                this.begin_ime_selection(target, event.position, event.modifiers.shift, window, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_move(
            cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            }),
        );
        let workspace = cx.entity();
        let input = text_input_anchor_probe(target.anchor_id(), input, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        });
        let send_disabled = !enabled || self.ai_chat_draft.trim().is_empty();
        let action = if self.ai_chat_loading {
            ai_stop_button(
                &self.tokens,
                self.i18n.t("ai.input.stop"),
                Self::render_lucide_icon(LucideIcon::StopCircle, 12.0, rgb(0xef4444)),
            )
        } else {
            ai_send_button(&self.tokens, self.i18n.t("ai.input.send_btn"), send_disabled)
        };
        let frame = ai_chat_input_frame(&self.tokens, focused)
            .when(!autocomplete_items.is_empty(), |frame| {
                frame.child(self.render_ai_autocomplete_popup(&autocomplete_items, cx))
            })
            .child(ai_chat_input_editor(&self.tokens, input));
        let footer_leading = if self.ai_chat_loading {
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(4.0))
                .text_size(px(9.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(rgb(self.tokens.ui.accent))
                .child(Self::render_lucide_icon(
                    LucideIcon::Sparkles,
                    12.0,
                    rgb(self.tokens.ui.accent),
                ))
                .child(div().truncate().child(self.i18n.t("ai.input.thinking")))
                .into_any_element()
        } else {
            self.render_ai_context_usage_indicator(cx).into_any_element()
        };
        let footer_trailing = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(!self.ai_chat_loading, |row| {
                row.child(
                    div()
                        .text_size(px(9.0))
                        .font_family(settings_ui_font_family(""))
                        .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x33))
                        .child("SHIFT+ENTER"),
                )
            })
            .child(action.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.ai_chat_loading {
                        this.cancel_ai_chat_stream(cx);
                    } else if !send_disabled {
                        this.send_ai_chat_draft(cx);
                    }
                    cx.stop_propagation();
                }),
            ));
        let frame = frame.child(ai_chat_input_footer(
            &self.tokens,
            footer_leading,
            footer_trailing,
        ));
        ai_chat_input_root(&self.tokens)
            .relative()
            .when(self.ai_should_show_context_chips(cx), |root| {
                root.child(self.render_ai_context_chips(cx))
            })
            .child(frame)
            .into_any_element()
    }

    fn render_ai_profile_indicator(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let conversation = self.ai_chat.active_conversation()?;
        let profiles = self
            .settings_store
            .settings()
            .ai
            .execution_profiles
            .get("profiles")
            .and_then(|value| value.as_array())?;
        if profiles.len() <= 1 {
            return None;
        }
        let conversation_id = conversation.id.clone();
        let workspace = cx.entity();
        Some(
            div()
                .relative()
                .flex_none()
                .child(
                    select_anchor_probe(
                        SelectAnchorId::AiProfileSelector,
                        ai_profile_button(
                            &self.tokens,
                            Self::render_lucide_icon(
                                LucideIcon::Layers,
                                14.0,
                                rgb(self.tokens.ui.accent),
                            ),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                if this.ai_chat.active_conversation_id.as_deref()
                                    != Some(conversation_id.as_str())
                                {
                                    return;
                                }
                                let next_open = !this.ai_profile_selector_open;
                                this.close_ai_sidebar_popovers();
                                this.ai_profile_selector_open = next_open;
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
            .into_any_element(),
        )
    }

    fn render_ai_safety_indicator(&self, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.active_ai_safety_mode();
        let icon = match mode {
            AiSafetyMode::Default => LucideIcon::ShieldCheck,
            AiSafetyMode::Bypass => LucideIcon::ShieldAlert,
        };
        div()
            .relative()
            .flex_none()
            .child(
                select_anchor_probe(
                    SelectAnchorId::AiSafetyMenu,
                    ai_safety_indicator(
                        &self.tokens,
                        mode,
                        if mode == AiSafetyMode::Bypass {
                            self.i18n.t("ai.safety_mode.bypass_label")
                        } else {
                            self.i18n.t("ai.safety_mode.default_label")
                        },
                        Self::render_lucide_icon(
                            icon,
                            10.0,
                            rgb(if mode == AiSafetyMode::Bypass {
                                0xfcd34d
                            } else {
                                self.tokens.ui.accent
                            }),
                        ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            let next_open = !this.ai_safety_menu_open;
                            this.close_ai_sidebar_popovers();
                            this.ai_safety_menu_open = next_open;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
                    {
                        let workspace = cx.entity();
                        move |anchor, _window, cx| {
                            let _ = workspace.update(cx, |this, cx| {
                                this.update_select_anchor(anchor, cx);
                            });
                        }
                    },
                ),
            )
            .into_any_element()
    }

    fn render_ai_profile_dropdown(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(conversation) = self.ai_chat.active_conversation() else {
            return div().into_any_element();
        };
        let profiles = self
            .settings_store
            .settings()
            .ai
            .execution_profiles
            .get("profiles")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let default_profile_id = self
            .settings_store
            .settings()
            .ai
            .execution_profiles
            .get("defaultProfileId")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .or_else(|| {
                profiles
                    .first()
                    .and_then(|profile| profile.get("id"))
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            });
        let active_profile_id = conversation
            .profile_id
            .clone()
            .or_else(|| {
                conversation
                    .session_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("profileId"))
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
            .or(default_profile_id.clone())
            .unwrap_or_else(|| "default".to_string());
        let conversation_id = conversation.id.clone();

        let mut panel = div()
            .w(px(220.0))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .shadow_lg()
            .py(px(4.0));

        for profile in profiles {
            let profile_id = profile
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or("default")
                .to_string();
            let profile_name = profile
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("Default")
                .to_string();
            let is_selected = profile_id == active_profile_id;
            let is_default = default_profile_id.as_deref() == Some(profile_id.as_str());
            let row_profile_id = profile_id.clone();
            let row_conversation_id = conversation_id.clone();
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.two))
                    .cursor_pointer()
                    .bg(if is_selected {
                        rgba((self.tokens.ui.accent << 8) | 0x1a)
                    } else {
                        rgba(0x00000000)
                    })
                    .hover(|row| row.bg(rgba((self.tokens.ui.bg_hover << 8) | 0x99)))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(if is_selected {
                                rgb(self.tokens.ui.accent)
                            } else {
                                rgb(self.tokens.ui.text)
                            })
                            .child(profile_name),
                    )
                    .when(is_default, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .text_size(px(10.0))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("ai.profile.default_badge")),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_ai_conversation_profile(
                                &row_conversation_id,
                                row_profile_id.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
            );
        }

        panel.into_any_element()
    }

    fn render_ai_safety_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        // Tauri DropdownMenuContent uses w-64 and opens upward from the compact status bar.
        div()
            .w(px(256.0))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .shadow_lg()
            .py(px(self.tokens.spacing.one))
            .child(
                div()
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.one))
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ai.safety_mode.menu_title")),
            )
            .child(self.render_ai_safety_menu_item(
                AiSafetyMode::Default,
                self.i18n.t("ai.safety_mode.default_mode"),
                self.i18n.t("ai.safety_mode.default_desc"),
                cx,
            ))
            .child(self.render_ai_safety_menu_item(
                AiSafetyMode::Bypass,
                self.i18n.t("ai.safety_mode.bypass_mode"),
                self.i18n.t("ai.safety_mode.bypass_desc"),
                cx,
            ))
            .child(
                div()
                    .my(px(self.tokens.spacing.one))
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x66)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.two))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .hover(|row| row.bg(rgba((self.tokens.ui.bg_hover << 8) | 0x99)))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Settings,
                        14.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .child(self.i18n.t("ai.safety_mode.open_settings"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.ai_safety_menu_open = false;
                            this.open_ai_settings(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_ai_safety_menu_item(
        &self,
        mode: AiSafetyMode,
        title: String,
        description: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bypass = mode == AiSafetyMode::Bypass;
        let icon = if bypass {
            LucideIcon::ShieldAlert
        } else {
            LucideIcon::ShieldCheck
        };
        div()
            .flex()
            .items_start()
            .gap(px(self.tokens.spacing.two))
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.two))
            .cursor_pointer()
            .hover(|row| row.bg(rgba((self.tokens.ui.bg_hover << 8) | 0x99)))
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(if bypass {
                    0xfcd34d
                } else {
                    self.tokens.ui.accent
                }),
            ))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one / 2.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(if bypass {
                                0xfcd34d
                            } else {
                                self.tokens.ui.text
                            }))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .line_height(px(15.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(description),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    match mode {
                        AiSafetyMode::Default => this.set_ai_safety_mode_default(cx),
                        AiSafetyMode::Bypass => {
                            if this.active_ai_safety_mode() != AiSafetyMode::Bypass {
                                this.ai_safety_confirm_open = true;
                            }
                            this.ai_safety_menu_open = false;
                            cx.notify();
                        }
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_safety_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.i18n.t("ai.safety_mode.confirm_title"))
                    .into_any_element(),
                description: Some(
                    div()
                        .child(self.i18n.t("ai.safety_mode.confirm_description"))
                        .into_any_element(),
                ),
                cancel_label: div()
                    .child(self.i18n.t("ai.safety_mode.confirm_cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("ai.safety_mode.confirm_enable"))
                    .into_any_element(),
            },
            cx.listener(|this, _event, _window, cx| {
                this.ai_safety_confirm_open = false;
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.confirm_ai_safety_bypass(cx);
                cx.stop_propagation();
            }),
        )
    }

    fn render_ai_tool_indicator(&self, cx: &mut Context<Self>) -> AnyElement {
        let tool_use = &self.settings_store.settings().ai.tool_use;
        let enabled = tool_use.enabled;
        let max_rounds = tool_use.max_rounds.unwrap_or(10);
        let label = if enabled {
            self.i18n
                .t("ai.tool_status.rounds_short")
                .replace("{{count}}", &max_rounds.to_string())
        } else {
            self.i18n.t("ai.tool_status.disabled_short")
        };
        ai_status_indicator(
            &self.tokens,
            label,
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(Self::render_lucide_icon(
                    LucideIcon::Wrench,
                    10.0,
                    rgb(if enabled {
                        self.tokens.ui.accent
                    } else {
                        self.tokens.ui.text_muted
                    }),
                ))
                .child(Self::render_lucide_icon(
                    LucideIcon::Settings,
                    10.0,
                    rgba((self.tokens.ui.text_muted << 8) | 0xb3),
                )),
            enabled,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| {
                this.open_ai_settings(window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn render_ai_context_usage_indicator(&self, cx: &mut Context<Self>) -> AnyElement {
        let breakdown = self.ai_context_token_breakdown();
        let total_tokens = breakdown.total;
        let max_tokens = breakdown.max_tokens;
        let percentage = if max_tokens == 0 {
            0.0
        } else {
            ((total_tokens as f32 / max_tokens as f32) * 100.0).min(100.0)
        };
        let usage = AiContextUsage {
            percentage,
            warning: percentage > 70.0,
            danger: percentage > 85.0,
        };
        let indicator = ai_context_usage_indicator(
            &self.tokens,
            usage,
            ai_format_tokens(total_tokens),
            Self::render_lucide_icon(
                LucideIcon::Info,
                12.0,
                rgb(if usage.danger {
                    0xef4444
                } else if usage.warning {
                    0xf59e0b
                } else {
                    self.tokens.ui.text_muted
                }),
            ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                let next_open = !this.ai_context_popover_open;
                this.close_ai_sidebar_popovers();
                this.ai_context_popover_open = next_open;
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let workspace = cx.entity();
        select_anchor_probe(
            SelectAnchorId::AiContextPopover,
            indicator,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_ai_context_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let breakdown = self.ai_context_token_breakdown();
        let total_tokens = breakdown.total;
        let max_tokens = breakdown.max_tokens;
        let percentage = if max_tokens == 0 {
            0.0
        } else {
            ((total_tokens as f32 / max_tokens as f32) * 100.0).min(100.0)
        };
        let usage = AiContextUsage {
            percentage,
            warning: percentage > 70.0,
            danger: percentage > 85.0,
        };
        ai_context_popover(&self.tokens)
            .child(ai_context_popover_header(
                &self.tokens,
                self.i18n.t("ai.context.breakdown"),
                usage,
                format!(
                    "{} / {} tokens",
                    ai_format_tokens(total_tokens),
                    ai_format_tokens(max_tokens)
                ),
            ))
            .child(
                div()
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x1a)),
            )
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .mb(px(6.0))
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("ai.context.system")),
                    )
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.system_instructions"),
                        ai_context_percent(breakdown.system_instructions, max_tokens),
                    ))
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.tool_definitions"),
                        ai_context_percent(breakdown.tool_definitions, max_tokens),
                    ))
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.reserved_output"),
                        ai_context_percent(breakdown.reserved_output, max_tokens),
                    )),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x1a)),
            )
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .mb(px(6.0))
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("ai.context.user_context")),
                    )
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.messages_label"),
                        ai_context_percent(breakdown.messages, max_tokens),
                    ))
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.tool_results"),
                        ai_context_percent(breakdown.tool_results, max_tokens),
                    )),
            )
            .when(
                self.ai_chat
                    .active_conversation()
                    .is_some_and(|conversation| conversation.messages.len() >= 4),
                |popover| {
                    popover
                        .child(
                            div()
                                .border_t_1()
                                .border_color(rgba((self.tokens.ui.border << 8) | 0x1a)),
                        )
                        .child(
                            div().px(px(12.0)).py(px(8.0)).child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(6.0))
                                    .rounded(px(self.tokens.radii.md))
                                    .px(px(12.0))
                                    .py(px(6.0))
                                    .text_size(px(11.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .bg(rgba((self.tokens.ui.border << 8) | 0x1a))
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(rgba((self.tokens.ui.border << 8) | 0x33))
                                    })
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::Archive,
                                        12.0,
                                        rgb(self.tokens.ui.text),
                                    ))
                                    .child(self.i18n.t("ai.context.compress_dialog"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.ai_context_popover_open = false;
                                            this.start_ai_compact_conversation(cx);
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            ),
                        )
                },
            )
            .into_any_element()
    }

    fn render_ai_context_breakdown_row(&self, label: String, value: String) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .py(px(2.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(label),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .font_family(settings_ui_font_family(""))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(value),
            )
            .into_any_element()
    }

    fn ai_context_token_breakdown(&self) -> AiContextTokenBreakdown {
        let settings = self.settings_store.settings();
        let providers = ai_provider_views(&settings.ai.providers);
        let active_provider = active_provider_view(&providers, settings.ai.active_provider_id.as_deref());
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
        let system_prompt = settings.ai.custom_system_prompt.trim();
        let conversation = self.ai_chat.active_conversation();
        let cache_key = AiContextTokenBreakdownKey {
            conversation_id: conversation.map(|conversation| conversation.id.clone()),
            conversation_fingerprint: ai_conversation_token_fingerprint(conversation),
            provider_id: provider_id.to_string(),
            model: model.clone(),
            max_tokens,
            system_prompt_fingerprint: ai_text_shape_fingerprint(system_prompt),
            tool_use_enabled: settings.ai.tool_use.enabled,
        };
        {
            let cache = self.ai_context_token_cache.borrow();
            if cache.key.as_ref() == Some(&cache_key)
                && let Some(cached) = cache.breakdown_without_draft.as_ref()
            {
                return ai_context_breakdown_with_draft(cached.clone(), &self.ai_chat_draft);
            }
        }
        let system_instructions = ai_estimated_tokens(if system_prompt.is_empty() {
            DEFAULT_AI_SYSTEM_PROMPT
        } else {
            system_prompt
        });
        let tool_definitions = if settings.ai.tool_use.enabled {
            ai_estimated_tool_definitions_tokens()
        } else {
            0
        };
        let reserved_output = ai_response_reserve(max_tokens);
        let message_tokens = conversation
            .map(|conversation| {
                conversation
                    .messages
                    .iter()
                    .filter(|message| {
                        matches!(message.role, AiChatRole::User | AiChatRole::Assistant)
                    })
                    .map(ai_message_estimated_tokens)
                    .sum::<usize>()
            })
            .unwrap_or(0);
        let tool_results = conversation.map(ai_conversation_tool_result_tokens).unwrap_or(0);
        let breakdown_without_draft = AiContextTokenBreakdown {
            system_instructions,
            tool_definitions,
            reserved_output,
            messages: message_tokens,
            tool_results,
            total: system_instructions
                .saturating_add(tool_definitions)
                .saturating_add(reserved_output)
                .saturating_add(message_tokens)
                .saturating_add(tool_results),
            max_tokens,
        };
        let mut cache = self.ai_context_token_cache.borrow_mut();
        cache.key = Some(cache_key);
        cache.breakdown_without_draft = Some(breakdown_without_draft.clone());
        ai_context_breakdown_with_draft(breakdown_without_draft, &self.ai_chat_draft)
    }

    fn ai_should_show_context_chips(&self, cx: &mut Context<Self>) -> bool {
        self.ai_active_terminal_context_available()
            || self.ai_active_tab_has_split_panes()
            || self.ai_has_ide_context(cx)
            || self.ai_has_sftp_context()
    }

    fn render_ai_context_chips(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut chips = ai_chat_input_chips(&self.tokens);
        if self.ai_active_terminal_context_available() {
            chips = chips.child(
                ai_context_chip(
                    &self.tokens,
                    self.i18n.t("ai.input.context"),
                    AiTone::Accent,
                    self.ai_chat_include_context,
                    Self::render_lucide_icon(
                        LucideIcon::Terminal,
                        12.0,
                        rgb(if self.ai_chat_include_context {
                            self.tokens.ui.accent
                        } else {
                            self.tokens.ui.text_muted
                        }),
                    ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.ai_chat_include_context = !this.ai_chat_include_context;
                        if !this.ai_chat_include_context {
                            this.ai_chat_include_all_panes = false;
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            );
        }
        if self.ai_active_tab_has_split_panes() && self.ai_chat_include_context {
            chips = chips.child(
                ai_context_chip(
                    &self.tokens,
                    self.i18n.t("ai.input.panes"),
                    AiTone::Blue,
                    self.ai_chat_include_all_panes,
                    Self::render_lucide_icon(
                        LucideIcon::SplitSquareHorizontal,
                        12.0,
                        rgb(if self.ai_chat_include_all_panes {
                            0x3b82f6
                        } else {
                            self.tokens.ui.text_muted
                        }),
                    ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.ai_chat_include_all_panes = !this.ai_chat_include_all_panes;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            );
        }
        if self.ai_has_ide_context(cx) {
            chips = chips.child(ai_context_chip(
                &self.tokens,
                self.i18n.t("ai.input.ide_context"),
                AiTone::Emerald,
                true,
                Self::render_lucide_icon(LucideIcon::Code2, 12.0, rgb(0x10b981)),
            ));
        }
        if self.ai_has_sftp_context() {
            chips = chips.child(ai_context_chip(
                &self.tokens,
                self.i18n.t("ai.input.sftp_context"),
                AiTone::Orange,
                true,
                Self::render_lucide_icon(LucideIcon::FolderOpen, 12.0, rgb(0xf97316)),
            ));
        }
        chips.into_any_element()
    }

    fn ai_chat_autocomplete_items(&self) -> Vec<AiAutocompleteCandidate> {
        if !self.ai_chat_input_focused || self.ai_chat_autocomplete_suppressed {
            return Vec::new();
        }
        ai_autocomplete_candidates(&self.ai_chat_draft, self.ai_chat_draft.len())
    }

    fn render_ai_autocomplete_popup(
        &self,
        items: &[AiAutocompleteCandidate],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_index = self.ai_chat_autocomplete_index.min(items.len().saturating_sub(1));
        let mut popup = ai_autocomplete_popup(&self.tokens, "ai-chat-autocomplete");
        for (index, item) in items.iter().enumerate() {
            let prefix = match item.kind {
                AiAutocompleteKind::Slash => "/",
                AiAutocompleteKind::Participant => "@",
                AiAutocompleteKind::Reference => "#",
            };
            let candidate = item.clone();
            popup = popup.child(
                ai_autocomplete_item(
                    &self.tokens,
                    prefix,
                    item.name,
                    self.i18n.t(item.description_key),
                    index == active_index,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.apply_ai_chat_autocomplete_candidate(&candidate, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        popup.into_any_element()
    }

    fn apply_ai_chat_autocomplete_candidate(
        &mut self,
        candidate: &AiAutocompleteCandidate,
        cx: &mut Context<Self>,
    ) {
        self.ai_chat_draft = apply_ai_autocomplete_candidate(
            &self.ai_chat_draft,
            self.ai_chat_draft.len(),
            candidate,
        );
        self.ai_chat_autocomplete_index = 0;
        self.ai_chat_autocomplete_suppressed = true;
        self.ime_marked_text = None;
        cx.notify();
    }
}

fn ai_format_tokens(tokens: usize) -> String {
    if tokens >= 1000 {
        format!("{:.1}K", tokens as f32 / 1000.0)
    } else {
        tokens.to_string()
    }
}

fn ai_context_percent(tokens: usize, max_tokens: usize) -> String {
    if max_tokens == 0 {
        return "0%".to_string();
    }
    let percent = (tokens as f32 / max_tokens as f32) * 100.0;
    if percent > 0.0 && percent < 0.1 {
        "<0.1%".to_string()
    } else {
        format!("{percent:.1}%")
    }
}

fn ai_context_breakdown_with_draft(
    mut breakdown: AiContextTokenBreakdown,
    draft: &str,
) -> AiContextTokenBreakdown {
    let draft_tokens = ai_estimated_tokens(draft);
    breakdown.messages = breakdown.messages.saturating_add(draft_tokens);
    breakdown.total = breakdown.total.saturating_add(draft_tokens);
    breakdown
}

fn ai_conversation_token_fingerprint(conversation: Option<&AiConversation>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let Some(conversation) = conversation else {
        return 0;
    };
    std::hash::Hash::hash(&conversation.id, &mut hasher);
    std::hash::Hash::hash(&conversation.messages.len(), &mut hasher);
    for message in &conversation.messages {
        std::hash::Hash::hash(&message.id, &mut hasher);
        std::hash::Hash::hash(&ai_role_fingerprint(&message.role), &mut hasher);
        std::hash::Hash::hash(&message.is_streaming, &mut hasher);
        std::hash::Hash::hash(&message.timestamp_ms, &mut hasher);
        ai_hash_text_shape(&message.content, &mut hasher);
        if let Some(context) = message.context.as_deref() {
            ai_hash_text_shape(context, &mut hasher);
        }
        if let Some(thinking) = message.thinking_content.as_deref() {
            ai_hash_text_shape(thinking, &mut hasher);
        }
        std::hash::Hash::hash(&message.tool_calls.len(), &mut hasher);
        for tool_call in &message.tool_calls {
            ai_hash_tool_call_shape(tool_call, &mut hasher);
        }
    }
    std::hash::Hasher::finish(&hasher)
}

fn ai_role_fingerprint(role: &AiChatRole) -> u8 {
    match role {
        AiChatRole::User => 0,
        AiChatRole::Assistant => 1,
        AiChatRole::System => 2,
        AiChatRole::Tool => 3,
    }
}

fn ai_text_shape_fingerprint(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ai_hash_text_shape(text, &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

fn ai_hash_text_shape(text: &str, hasher: &mut std::collections::hash_map::DefaultHasher) {
    let bytes = text.as_bytes();
    std::hash::Hash::hash(&bytes.len(), hasher);
    let head = bytes.len().min(32);
    std::hash::Hash::hash(&&bytes[..head], hasher);
    if bytes.len() > head {
        let tail = bytes.len().saturating_sub(32);
        std::hash::Hash::hash(&&bytes[tail..], hasher);
    }
}

fn ai_hash_tool_call_shape(
    tool_call: &serde_json::Value,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) {
    for key in ["id", "name", "status", "risk"] {
        if let Some(value) = tool_call.get(key).and_then(serde_json::Value::as_str) {
            ai_hash_text_shape(value, hasher);
        }
    }
    if let Some(arguments) = tool_call
        .get("arguments")
        .and_then(serde_json::Value::as_str)
    {
        ai_hash_text_shape(arguments, hasher);
    }
    if let Some(output) = tool_call
        .get("result")
        .and_then(|result| result.get("output"))
        .and_then(serde_json::Value::as_str)
    {
        ai_hash_text_shape(output, hasher);
    } else {
        std::hash::Hash::hash(&tool_call.as_object().map(|object| object.len()), hasher);
    }
}

fn ai_conversation_tool_result_tokens(conversation: &AiConversation) -> usize {
    conversation
        .messages
        .iter()
        .flat_map(|message| message.tool_calls.iter())
        .map(ai_tool_call_estimated_tokens)
        .sum()
}

fn ai_tool_call_estimated_tokens(tool_call: &serde_json::Value) -> usize {
    let arguments = tool_call
        .get("arguments")
        .and_then(serde_json::Value::as_str)
        .map(ai_estimated_tokens)
        .unwrap_or(0);
    let result_output = tool_call
        .get("result")
        .and_then(|result| result.get("output"))
        .and_then(serde_json::Value::as_str)
        .map(ai_estimated_tokens)
        .unwrap_or(0);
    if arguments > 0 || result_output > 0 {
        arguments.saturating_add(result_output)
    } else {
        ai_estimated_tokens(&tool_call.to_string())
    }
}

fn ai_estimated_tool_definitions_tokens() -> usize {
    AI_ORCHESTRATOR_TOOL_DEFINITION_ESTIMATES
        .iter()
        .map(|(name, description)| 10 + ai_estimated_tokens(name) + ai_estimated_tokens(description))
        .sum()
}

const AI_ORCHESTRATOR_TOOL_DEFINITION_ESTIMATES: &[(&str, &str)] = &[
    (
        "list_targets",
        "List available OxideTerm targets by view. Default view is connections for remote host discovery.",
    ),
    (
        "select_target",
        "Select exactly one target from OxideTerm targets when the user named a specific target.",
    ),
    (
        "connect_target",
        "Connect or open a selected target, including saved SSH connections.",
    ),
    (
        "run_command",
        "Run a command on an explicit target.",
    ),
    (
        "observe_terminal",
        "Read a terminal target screen, buffer, readiness, and waiting-for-input hints.",
    ),
    (
        "send_terminal_input",
        "Send text, Enter, or control input to a visible terminal target.",
    ),
    (
        "read_resource",
        "Read a resource from a target: settings, remote file, SFTP directory, IDE file, or RAG search.",
    ),
    (
        "write_resource",
        "Safely write a settings value or remote file resource.",
    ),
    (
        "transfer_resource",
        "Start an SFTP upload or download against an explicit SSH/SFTP target.",
    ),
    (
        "open_app_surface",
        "Open an OxideTerm app surface such as settings, connection manager, SFTP, IDE, file manager, or terminal.",
    ),
    (
        "get_state",
        "Read compact state for connections, transfers, settings, targets, health, or active context.",
    ),
    (
        "remember_preference",
        "Save a long-lived user preference for OxideSens memory.",
    ),
    (
        "recall_preferences",
        "Read saved long-lived OxideSens user preferences.",
    ),
];
