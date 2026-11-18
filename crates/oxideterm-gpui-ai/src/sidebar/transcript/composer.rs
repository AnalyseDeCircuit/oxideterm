impl AiAssistantSidebarView {
    pub fn render_ai_sidebar_model_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .min_w_0()
            .px(px(12.0))
            // A shorter footer lets the chat body lower the divider together with its controls.
            .pt(px(self.tokens.spacing.one))
            .border_t_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x33))
            .bg(self.context_sidebar_content_background(self.tokens.ui.shell_bg()))
            .child(self.views.model_trigger.clone())
            .when_some(self.render_ai_reasoning_indicator(cx), |bar, indicator| {
                bar.child(indicator)
            })
            .when_some(self.render_ai_acp_plan_indicator(cx), |bar, indicator| {
                bar.child(indicator)
            })
            .child(self.render_ai_safety_indicator(cx))
            .child(self.render_ai_tool_indicator(cx))
            .into_any_element()
    }

    pub fn render_ai_sidebar_input(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let model_selected = self.frame().composer.model_selected;
        let images_allowed = self.frame().composer.images_allowed;
        let input = self.views.composer.clone();
        let send_disabled = !enabled
            || !model_selected
            || !images_allowed
            || self.state.read(cx).chat_ui().image_import.is_some()
            || (self.state.read(cx).chat_ui().draft.trim().is_empty()
                && self.state.read(cx).chat_ui().images.is_empty());
        let action_focused = self.state.read(cx).chat_ui().footer_focus
            == Some(AiChatFooterAction::Submit)
            && (self.state.read(cx).chat_is_loading() || !send_disabled);
        let can_supplement = self.state.read(cx).can_supplement_agent()
            && self.state.read(cx).chat_ui().images.is_empty();
        let loading = self.state.read(cx).chat_is_loading();
        let action = ai_send_button(
            &self.tokens,
            self.i18n
                .t(if self.state.read(cx).active_user_question().is_some() {
                    "ai.questions.answer"
                } else if loading {
                    "ai.queue.enqueue"
                } else {
                    "ai.input.send_btn"
                }),
            send_disabled,
            action_focused,
        );
        let focused = self.state.read(cx).chat_ui().input_focused;
        let frame = ai_chat_input_frame(&self.tokens, focused)
            .children(self.render_ai_image_controls(cx))
            .child(ai_chat_input_editor(&self.tokens, input));
        let footer_leading = if self.state.read(cx).chat_is_loading() {
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
                .child(
                    div().truncate().child(
                        self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "ai-input-footer",
                            "thinking",
                            self.i18n
                                .t(if self.state.read(cx).active_user_question().is_some() {
                                    "ai.questions.waiting"
                                } else {
                                    "ai.input.thinking"
                                }),
                            self.tokens.ui.accent,
                            cx,
                        ),
                    ),
                )
                .into_any_element()
        } else {
            self.render_ai_context_usage_indicator(cx)
                .into_any_element()
        };
        let footer_leading = div()
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(6.0))
            .child(self.render_ai_image_add_button(enabled, cx))
            .child(footer_leading);
        let footer_trailing = div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_wrap()
            .justify_end()
            .items_center()
            .gap(px(6.0))
            .when(loading, |row| {
                row.child(self.agent_control(
                    "ai-stop-running-group".into(),
                    self.i18n.t("ai.input.stop"),
                    ai_stop_button(
                        &self.tokens,
                        self.i18n.t("ai.input.stop"),
                        Self::render_lucide_icon(
                            LucideIcon::StopCircle,
                            12.0,
                            rgb(self.tokens.ui.error),
                        ),
                        false,
                    ),
                    |this, _, cx| (this.bindings.request)(AiTranscriptRequest::Stop, None, cx),
                    cx,
                ))
            })
            .when(loading && !send_disabled, |row| {
                let row = if can_supplement {
                    row.child(self.agent_button(
                        "ai-steer-draft".into(),
                        self.i18n.t("ai.queue.steer"),
                        |this, _, cx| (this.bindings.request)(AiTranscriptRequest::Steer, None, cx),
                        cx,
                    ))
                } else {
                    row
                };
                row.child(self.agent_button(
                    "ai-interrupt-draft".into(),
                    self.i18n.t("ai.queue.interrupt_send"),
                    |this, _, cx| {
                        (this.bindings.request)(AiTranscriptRequest::InterruptAndSend, None, cx)
                    },
                    cx,
                ))
            })
            .child(action.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.state.update(cx, |ai, _cx| {
                        ai.clear_chat_footer_focus();
                    });
                    if !send_disabled {
                        (this.bindings.request)(AiTranscriptRequest::Send, None, cx);
                    }
                    cx.stop_propagation();
                }),
            ));
        let frame = frame.child(ai_chat_input_footer(
            &self.tokens,
            footer_leading,
            footer_trailing,
        ));
        let frame = select_anchor_probe(
            SelectAnchorId::AiAutocomplete,
            frame,
            self.place_select_anchor(),
        );
        ai_chat_input_root_with_background(
            &self.tokens,
            self.context_sidebar_content_background(self.tokens.ui.shell_bg()),
        )
        // The toolbar owns the spacing above the composer.
        .pt(px(0.0))
        .relative()
        .can_drop(|drag, _, _| drag.is::<gpui::ExternalPaths>())
        .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
            (this.bindings.request)(
                AiTranscriptRequest::ImportImages(paths.paths().to_vec()),
                None,
                cx,
            );
        }))
        .when_some(
            self.render_ai_acp_authentication_prompt(cx),
            |root, prompt| root.child(prompt),
        )
        .child(oxideterm_gpui_ui::motion::auto_height(
            &self.tokens,
            "ai-context-region",
            self.ai_should_show_context_chips(cx)
                .then(|| self.render_ai_context_chips(cx)),
        ))
        .child(oxideterm_gpui_ui::motion::auto_height(
            &self.tokens,
            "ai-queue-region",
            self.render_ai_message_queue(cx),
        ))
        .child(frame)
        .into_any_element()
    }

    fn render_ai_message_queue(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ai = self.state.read(cx);
        let conversation = ai.conversation_state().active_conversation_id.clone()?;
        let turns = ai.queued_chat_turns.get(&conversation)?;
        if turns.is_empty() {
            return None;
        }
        let loading = ai.chat_is_loading();
        let can_steer = ai.can_supplement_agent();
        let rows: Vec<_> = turns
            .iter()
            .map(|turn| {
                (
                    turn.id.clone(),
                    turn.content.to_string(),
                    turn.config.model.clone(),
                    turn.images.len(),
                )
            })
            .collect();
        let mut block = ai_tool_block(&self.tokens)
            .w_full()
            .min_w_0()
            .child(ai_tool_heading(
                &self.tokens,
                self.i18n
                    .t(if loading {
                        "ai.queue.heading"
                    } else {
                        "ai.queue.paused"
                    })
                    .replace("{{count}}", &rows.len().to_string()),
            ));
        for (index, (id, text, model, image_count)) in rows.into_iter().enumerate() {
            let mut actions = div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap(px(self.tokens.spacing.one));
            for (action, key) in [
                ("edit", "ai.queue.edit"),
                ("remove", "ai.queue.remove"),
                ("up", "ai.queue.move_up"),
                ("send", "ai.queue.send_now"),
            ] {
                if (action == "up" && index == 0)
                    || (action == "send" && loading && (!can_steer || image_count > 0))
                {
                    continue;
                }
                let cid = conversation.clone();
                let id = id.clone();
                let key = if action == "send" && loading {
                    "ai.queue.steer"
                } else {
                    key
                };
                actions = actions.child(self.agent_button(
                    format!("queue-{action}-{id}"),
                    self.i18n.t(key),
                    move |this, _, cx| {
                        (this.bindings.request)(
                            AiTranscriptRequest::QueueAction(cid.clone(), id.clone(), action),
                            None,
                            cx,
                        );
                    },
                    cx,
                ));
            }
            block = block.child(oxideterm_gpui_ui::motion::fade_in(
                &self.tokens,
                gpui::SharedString::from(format!("ai-queue-row-{conversation}-{id}")),
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .child(text),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(if image_count == 0 {
                                model
                            } else {
                                format!(
                                    "{model} · {}",
                                    self.i18n
                                        .t("ai.images.count")
                                        .replace("{{count}}", &image_count.to_string())
                                )
                            }),
                    )
                    .child(actions),
                oxideterm_gpui_ui::motion::MotionDuration::Micro,
            ));
        }
        Some(
            div()
                .id("ai-message-queue")
                .w_full()
                .max_h(px(180.0))
                .overflow_y_scroll()
                .child(block)
                .into_any_element(),
        )
    }

    fn render_ai_acp_authentication_prompt(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let conversation_id = self
            .state
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone()?;
        let methods = self.frame().composer.auth_methods.clone()?;
        if methods.is_empty() {
            return None;
        }

        let mut method_list = div().flex().flex_col().gap(px(6.0));
        for method in methods {
            let method_id = method.method_id.clone();
            let target_conversation_id = conversation_id.clone();
            let supported = method.kind == oxideterm_ai::AcpAuthMethodKind::Agent;
            let setup_hint = match method.kind {
                oxideterm_ai::AcpAuthMethodKind::Agent => None,
                oxideterm_ai::AcpAuthMethodKind::Environment => {
                    let variables = method.environment_variables.join(", ");
                    Some(if variables.is_empty() {
                        self.i18n.t("ai.acp.auth_environment_hint")
                    } else {
                        format!(
                            "{}: {variables}",
                            self.i18n.t("ai.acp.auth_environment_hint")
                        )
                    })
                }
                oxideterm_ai::AcpAuthMethodKind::Terminal => {
                    Some(self.i18n.t("ai.acp.auth_terminal_hint"))
                }
                oxideterm_ai::AcpAuthMethodKind::Unsupported => {
                    Some(self.i18n.t("ai.acp.auth_unsupported_hint"))
                }
            };
            method_list = method_list.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(method.name),
                            )
                            .when_some(method.description, |column, description| {
                                column.child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(description),
                                )
                            })
                            .when_some(setup_hint, |column, hint| {
                                column.child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(hint),
                                )
                            }),
                    )
                    .when(supported, |row| {
                        row.child(self.workspace_toolbar_action_button(
                            self.i18n.t("ai.acp.authenticate"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Secondary,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                                height: Some(28.0),
                                font_size: Some(self.tokens.metrics.ui_text_xs),
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(move |this, _event, _window, cx| {
                                (this.bindings.request)(
                                    AiTranscriptRequest::Authenticate(
                                        target_conversation_id.clone(),
                                        method_id.clone(),
                                    ),
                                    None,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ))
                    }),
            );
        }

        Some(
            div()
                .mx(px(8.0))
                .mt(px(8.0))
                .p(px(8.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgba((self.tokens.ui.warning << 8) | 0x66))
                .bg(rgba((self.tokens.ui.warning << 8) | 0x0f))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(self.tokens.ui.warning))
                        .child(self.i18n.t("ai.acp.auth_required")),
                )
                .child(method_list)
                .into_any_element(),
        )
    }

    pub fn render_ai_safety_indicator(&self, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.active_ai_safety_mode(cx);
        let icon = match mode {
            AiSafetyMode::Default => LucideIcon::ShieldCheck,
            AiSafetyMode::ReadOnly => LucideIcon::Shield,
            AiSafetyMode::Bypass => LucideIcon::ShieldAlert,
        };
        let label = match mode {
            AiSafetyMode::Default => self.i18n.t("ai.safety_mode.default_label"),
            AiSafetyMode::ReadOnly => self.i18n.t("ai.safety_mode.read_only_label"),
            AiSafetyMode::Bypass => self.i18n.t("ai.safety_mode.bypass_label"),
        };
        div()
            .relative()
            .flex_none()
            .child(select_anchor_probe(
                SelectAnchorId::AiSafetyMenu,
                ai_safety_indicator(
                    &self.tokens,
                    mode,
                    label,
                    Self::render_lucide_icon(
                        icon,
                        10.0,
                        rgb(if mode == AiSafetyMode::Bypass {
                            self.tokens.ui.warning
                        } else {
                            self.tokens.ui.accent
                        }),
                    ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        let next_open = !this.state.read(cx).chat_ui().safety_menu_open;
                        this.close_popovers(cx);
                        this.state.update(cx, |ai, _cx| {
                            ai.set_chat_popover_open(AiChatPopover::Safety, next_open);
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
                self.place_select_anchor(),
            ))
            .into_any_element()
    }

    pub fn render_ai_tool_indicator(&self, cx: &mut Context<Self>) -> AnyElement {
        let (enabled, active_tool_count) = self.frame().composer.tools;
        let label = if enabled {
            self.i18n
                .t("ai.tool_status.tools_short")
                .replace("{{count}}", &active_tool_count.to_string())
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
                (this.bindings.request)(AiTranscriptRequest::OpenSettings, Some(window), cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub fn render_ai_context_usage_indicator(&self, cx: &mut Context<Self>) -> AnyElement {
        let acp_backend_active = self.frame().composer.backend == AiActiveBackend::Acp;
        let counts = self.frame().composer.context_counts;
        let (total_tokens, max_tokens) = counts.unwrap_or((0, 0));
        let percentage = ai_context_percentage(total_tokens, max_tokens);
        let usage = AiContextUsage {
            percentage,
            warning: percentage > 70.0,
            danger: percentage > 85.0,
        };
        let indicator = ai_context_usage_indicator(
            &self.tokens,
            usage,
            if acp_backend_active && counts.is_none() {
                "—".to_string()
            } else {
                ai_format_tokens(total_tokens)
            },
            !acp_backend_active,
        )
        .when(!acp_backend_active, |indicator| {
            indicator.on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    let next_open = !this.state.read(cx).chat_ui().context_popover_open;
                    this.close_popovers(cx);
                    this.state.update(cx, |ai, _cx| {
                        ai.set_chat_popover_open(AiChatPopover::Context, next_open);
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
        });
        if acp_backend_active {
            return indicator.into_any_element();
        }
        select_anchor_probe(
            SelectAnchorId::AiContextPopover,
            indicator,
            self.place_select_anchor(),
        )
        .into_any_element()
    }

    fn render_ai_acp_plan_indicator(&self, cx: &App) -> Option<AnyElement> {
        if self.frame().composer.backend != AiActiveBackend::Acp {
            return None;
        }
        let plan = self
            .state
            .read(cx)
            .conversation_state()
            .active_conversation()
            .and_then(ai_acp_session_state)?
            .plan?;
        let entries = plan
            .pointer("/plan/entries")
            .or_else(|| plan.get("entries"))
            .and_then(serde_json::Value::as_array)?;
        if entries.is_empty() {
            return None;
        }
        let completed = entries
            .iter()
            .filter(|entry| {
                entry.get("status").and_then(serde_json::Value::as_str) == Some("completed")
            })
            .count();
        let active_content = entries
            .iter()
            .find(|entry| {
                entry.get("status").and_then(serde_json::Value::as_str) == Some("in_progress")
            })
            .or_else(|| {
                entries.iter().find(|entry| {
                    entry.get("status").and_then(serde_json::Value::as_str) == Some("pending")
                })
            })
            .and_then(|entry| entry.get("content"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        Some(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(4.0))
                .text_size(px(10.0))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(Self::render_lucide_icon(
                    LucideIcon::ListChecks,
                    11.0,
                    rgb(self.tokens.ui.text_muted),
                ))
                .child(format!("{completed}/{}", entries.len()))
                .when(!active_content.is_empty(), |indicator| {
                    indicator
                        .child("·")
                        .child(div().min_w_0().truncate().child(active_content.to_string()))
                })
                .into_any_element(),
        )
    }

    fn ai_should_show_context_chips(&self, _cx: &mut Context<Self>) -> bool {
        self.frame().composer.terminal_context
            || self.frame().composer.split_context
            || self.frame().composer.ide_context
            || self.frame().composer.sftp_context
    }

    pub fn render_ai_context_chips(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut chips = ai_chat_input_chips(&self.tokens);
        if self.frame().composer.terminal_context {
            chips = chips.child(oxideterm_gpui_ui::motion::fade_in(
                &self.tokens,
                "ai-terminal-context-enter",
                ai_context_chip(
                    &self.tokens,
                    self.i18n.t("ai.input.context"),
                    AiTone::Accent,
                    self.state.read(cx).chat_ui().include_context,
                    Self::render_lucide_icon(
                        LucideIcon::Terminal,
                        12.0,
                        rgb(if self.state.read(cx).chat_ui().include_context {
                            self.tokens.ui.accent
                        } else {
                            self.tokens.ui.text_muted
                        }),
                    ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.state.update(cx, |ai, _cx| {
                            ai.toggle_chat_context();
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
                oxideterm_gpui_ui::motion::MotionDuration::Micro,
            ));
        }
        if self.frame().composer.split_context && self.state.read(cx).chat_ui().include_context {
            chips = chips.child(oxideterm_gpui_ui::motion::fade_in(
                &self.tokens,
                "ai-split-context-enter",
                ai_context_chip(
                    &self.tokens,
                    self.i18n.t("ai.input.panes"),
                    AiTone::Blue,
                    self.state.read(cx).chat_ui().include_all_panes,
                    Self::render_lucide_icon(
                        LucideIcon::SplitSquareHorizontal,
                        12.0,
                        rgb(if self.state.read(cx).chat_ui().include_all_panes {
                            self.tokens.ui.info
                        } else {
                            self.tokens.ui.text_muted
                        }),
                    ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.state.update(cx, |ai, _cx| {
                            ai.toggle_chat_all_panes();
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
                oxideterm_gpui_ui::motion::MotionDuration::Micro,
            ));
        }
        if self.frame().composer.ide_context {
            chips = chips.child(oxideterm_gpui_ui::motion::fade_in(
                &self.tokens,
                "ai-ide-context-enter",
                ai_context_chip(
                    &self.tokens,
                    self.i18n.t("ai.input.ide_context"),
                    AiTone::Emerald,
                    true,
                    Self::render_lucide_icon(LucideIcon::Code2, 12.0, rgb(self.tokens.ui.success)),
                ),
                oxideterm_gpui_ui::motion::MotionDuration::Micro,
            ));
        }
        if self.frame().composer.sftp_context {
            chips = chips.child(oxideterm_gpui_ui::motion::fade_in(
                &self.tokens,
                "ai-sftp-context-enter",
                ai_context_chip(
                    &self.tokens,
                    self.i18n.t("ai.input.sftp_context"),
                    AiTone::Orange,
                    true,
                    Self::render_lucide_icon(
                        LucideIcon::FolderOpen,
                        12.0,
                        rgb(self.tokens.ui.warning),
                    ),
                ),
                oxideterm_gpui_ui::motion::MotionDuration::Micro,
            ));
        }
        chips.into_any_element()
    }

    pub fn render_ai_reasoning_indicator(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let selected = self.frame().composer.reasoning?;
        let open = self.state.read(cx).chat_ui().reasoning_menu_open;
        let trigger = div()
            .flex()
            .flex_none()
            .items_center()
            .rounded(px(self.tokens.radii.md))
            .px(px(self.tokens.spacing.one))
            .py(px(self.tokens.spacing.one / 2.0))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if open {
                rgb(self.tokens.ui.text)
            } else {
                rgb(self.tokens.ui.text_muted)
            })
            .bg(if open {
                rgba((self.tokens.ui.accent << 8) | 0x1a)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgba((self.tokens.ui.accent << 8) | 0x1a))
                    .text_color(rgb(self.tokens.ui.text))
            })
            // The compact value stays English because it is a protocol level.
            .child(selected.display_name())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    let next_open = !this.state.read(cx).chat_ui().reasoning_menu_open;
                    this.close_popovers(cx);
                    this.state.update(cx, |ai, _cx| {
                        ai.set_chat_popover_open(AiChatPopover::Reasoning, next_open);
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        Some(
            select_anchor_probe(
                SelectAnchorId::AiReasoningMenu,
                trigger,
                self.place_select_anchor(),
            )
            .into_any_element(),
        )
    }

    fn render_ai_image_add_button(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let importing = self.state.read(cx).chat_ui().image_import.is_some();
        let label = self.i18n.t(if importing {
            "ai.images.loading"
        } else {
            "ai.images.add"
        });
        let disabled = !enabled || importing;
        let body = oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(
                if importing {
                    LucideIcon::LoaderCircle
                } else {
                    LucideIcon::Image
                },
                16.0,
                rgb(self.tokens.ui.text_muted),
            ),
            IconButtonOptions {
                disabled,
                ..IconButtonOptions::compact(24.0)
            },
        );
        let tokens = self.tokens;
        self.agent_control(
            "add-ai-image".into(),
            label.clone(),
            body,
            move |this, _, cx| {
                if !disabled {
                    (this.bindings.request)(AiTranscriptRequest::SelectImages, None, cx);
                }
            },
            cx,
        )
        .tooltip(move |_, cx| {
            oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
        })
        .into_any_element()
    }

    fn render_ai_image_controls(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ai = self.state.read(cx);
        let pending = ai
            .conversation_state()
            .active_conversation_id
            .as_ref()
            .and_then(|id| {
                ai.pending_image_requests
                    .get(id)
                    .map(|pending| (id.clone(), pending.config.model.clone()))
            });
        let images = ai.chat_ui().images.clone();
        if images.is_empty() && pending.is_none() {
            return None;
        }
        let mut column = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(8.0));
        if !images.is_empty() {
            column = column.child(
                div()
                    .id("ai-draft-images")
                    .max_h(px(220.0))
                    .overflow_x_scroll()
                    .overflow_y_scroll()
                    .child(self.render_ai_images(&images, true, cx)),
            );
        }
        if let Some((id, model)) = pending {
            column = column
                .child(
                    div().text_size(px(self.tokens.metrics.ui_text_sm)).child(
                        self.i18n
                            .t("ai.images.history_prompt")
                            .replace("{{model}}", &model),
                    ),
                )
                .child(self.agent_button(
                    "ai-image-text-only".into(),
                    self.i18n.t("ai.images.continue_text"),
                    move |this, _, cx| {
                        (this.bindings.request)(
                            AiTranscriptRequest::ContinueTextOnly(id.clone()),
                            None,
                            cx,
                        );
                    },
                    cx,
                ));
        }
        Some(column.into_any_element())
    }
}

pub fn ai_format_tokens(tokens: usize) -> String {
    if tokens >= 1000 {
        format!("{:.1}K", tokens as f32 / 1000.0)
    } else {
        tokens.to_string()
    }
}

pub fn ai_context_percent(tokens: usize, max_tokens: usize) -> String {
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
