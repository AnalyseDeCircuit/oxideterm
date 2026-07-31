impl WorkspaceApp {
    pub(in crate::workspace) fn active_ai_safety_mode(&self, cx: &App) -> AiSafetyMode {
        self.ai_entity.read(cx).active_conversation_safety_mode()
    }

    pub(in crate::workspace) fn render_ai_sidebar_model_bar(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            .bg(self.context_sidebar_content_background(self.tokens.ui.bg))
            .child(self.render_ai_model_selector(
                AiModelSelectorScope::Sidebar,
                SelectAnchorId::AiModelSelector,
                cx,
            ))
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

    pub(in crate::workspace) fn render_ai_sidebar_input(
        &self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let model_selected = if settings.ai.active_backend == AiActiveBackend::Acp {
            settings.ai.active_acp_agent_id.is_some()
        } else {
            settings.ai.active_provider_id.is_some()
                && active_model_selection(settings.ai.active_model.as_deref()).is_some()
        };
        let placeholder = if !enabled {
            self.i18n.t("ai.input.placeholder_disabled")
        } else if !model_selected {
            self.i18n.t("ai.model_selector.select_model")
        } else {
            self.i18n.t("ai.input.placeholder")
        };
        let target = WorkspaceImeTarget::AiChatInput;
        let focused = self.ai_entity.read(cx).chat_ui().input_focused;
        let autocomplete_items = self.ai_chat_autocomplete_items(cx);
        let draft = self.ai_entity.read(cx).chat_ui().draft.clone();
        let marked_range = self.ime_marked_virtual_range_for_target(target, cx);
        let selected_range = self.ime_selected_range_for_target(target, cx);
        let showing_placeholder = draft.is_empty() && marked_range.is_none();
        let input_text = if showing_placeholder {
            placeholder
        } else {
            // IME composition is a virtual replacement of the draft selection.
            // Render that projection inline instead of appending marked text as
            // another visual line below the editor.
            self.ime_text_with_marked_text_for_target(target, cx)
                .unwrap_or(draft)
        };
        let caret_offset = selected_range
            .as_ref()
            .filter(|range| range.start == range.end)
            .map(|range| range.start);
        let visual_lines = ai_input_visual_lines(
            &input_text,
            ai_input_soft_wrap_columns(self.ai_entity.read(cx).chat_ui().sidebar_width),
        );
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
        for (index, visual_line) in visual_lines.iter().enumerate() {
            let is_last_line = index + 1 == visual_lines.len();
            let line = visual_line.text;
            let line_len = visual_line.utf16_len();
            let line_range = visual_line.utf16_start..visual_line.utf16_end;
            let line_marked_range = marked_range
                .as_ref()
                .and_then(|marked| ai_input_local_marked_range(marked, &line_range));
            let line_selection = if showing_placeholder || marked_range.is_some() {
                None
            } else {
                selected_range.as_ref().and_then(|selection| {
                    let start = selection.start.max(line_range.start).min(line_range.end);
                    let end = selection.end.max(line_range.start).min(line_range.end);
                    (start < end).then_some(start - line_range.start..end - line_range.start)
                })
            };
            let line_caret = if showing_placeholder || marked_range.is_some() {
                None
            } else {
                caret_offset
                    .filter(|offset| {
                        *offset >= line_range.start
                            && if is_last_line {
                                *offset <= line_range.end
                            } else {
                                *offset < line_range.end
                            }
                    })
                    .map(|offset| offset.saturating_sub(line_range.start).min(line_len))
            };
            input = input.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .when(focused && showing_placeholder && index == 0, |line| {
                        line.child(text_caret(&self.tokens, self.input_caret.visible()))
                    })
                    .child(ai_input_line_segments(
                        &self.tokens,
                        line,
                        line_selection,
                        line_caret,
                        self.input_caret.visible(),
                        line_marked_range,
                    ))
                    .when(
                        focused
                            && is_last_line
                            && !showing_placeholder
                            && selected_range.is_none()
                            && marked_range.is_none(),
                        |line| {
                            line.child(text_caret(&self.tokens, self.input_caret.visible()))
                        },
                    ),
            );
        }
        let input = input
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.ai_entity.update(cx, |ai, _cx| {
                        ai.focus_chat_input();
                        ai.set_model_selector_search_focused(false);
                    });
                    this.ime_marked_text = None;
window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
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
        let send_disabled = !enabled || !model_selected || self.ai_entity.read(cx).chat_ui().draft.trim().is_empty();
        let action_focused = self.ai_entity.read(cx).chat_ui().footer_focus == Some(AiChatFooterAction::Submit)
            && (self.ai_entity.read(cx).chat_is_loading() || !send_disabled);
        let action = if self.ai_entity.read(cx).chat_is_loading() {
            ai_stop_button(
                &self.tokens,
                self.i18n.t("ai.input.stop"),
                Self::render_lucide_icon(LucideIcon::StopCircle, 12.0, rgb(self.tokens.ui.error)),
                action_focused,
            )
        } else {
            ai_send_button(
                &self.tokens,
                self.i18n.t("ai.input.send_btn"),
                send_disabled,
                action_focused,
            )
        };
        let frame = ai_chat_input_frame(&self.tokens, focused)
            .when(!autocomplete_items.is_empty(), |frame| {
                frame.child(self.render_ai_autocomplete_popup(&autocomplete_items, cx))
            })
            .child(ai_chat_input_editor(&self.tokens, input));
        let footer_leading = if self.ai_entity.read(cx).chat_is_loading() {
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
                .child(div().truncate().child(self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "ai-input-footer",
                    "thinking",
                    self.i18n.t("ai.input.thinking"),
                    self.tokens.ui.accent,
                    cx,
                )))
                .into_any_element()
        } else {
            self.render_ai_context_usage_indicator(cx)
                .into_any_element()
        };
        let footer_trailing = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(!self.ai_entity.read(cx).chat_is_loading(), |row| {
                row.child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x33))
                        .child("SHIFT+ENTER"),
                )
            })
            .child(action.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.ai_entity.update(cx, |ai, _cx| {
                        ai.clear_chat_footer_focus();
                    });
                    if this.ai_entity.read(cx).chat_is_loading() {
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
        ai_chat_input_root_with_background(
            &self.tokens,
            self.context_sidebar_content_background(self.tokens.ui.bg),
        )
            .relative()
            .when_some(self.render_ai_acp_authentication_prompt(cx), |root, prompt| {
                root.child(prompt)
            })
            .when(self.ai_should_show_context_chips(cx), |root| {
                root.child(self.render_ai_context_chips(cx))
            })
            .child(frame)
            .into_any_element()
    }

    fn render_ai_acp_authentication_prompt(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let conversation_id = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone()?;
        let methods = self
            .acp_entity
            .read(cx)
            .authentication_methods(&conversation_id)?;
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
                        row.child(
                            self.workspace_toolbar_action_button(
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
                                    this.acp_entity.update(cx, |entity, _cx| {
                                        entity.authenticate(
                                            &target_conversation_id,
                                            method_id.clone(),
                                        );
                                    });
                                    cx.stop_propagation();
                                }),
                            ),
                        )
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

    pub(in crate::workspace) fn render_ai_safety_indicator(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                        let next_open = !this.ai_entity.read(cx).chat_ui().safety_menu_open;
                        this.close_ai_sidebar_popovers(cx);
                        this.ai_entity.update(cx, |ai, _cx| {
                            ai.set_chat_popover_open(AiChatPopover::Safety, next_open);
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
                Self::deferred_ai_select_anchor_update(cx.entity()),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_safety_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        // Tauri DropdownMenuContent uses w-64 and opens upward from the compact status bar.
        let menu = div()
            .w(px(256.0))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .shadow_lg()
            // Safety mode dropdown follows the same menu wheel boundary as
            // Tauri DropdownMenuContent.
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .py(px(self.tokens.spacing.one))
            .child(
                div()
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.one))
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-safety-menu",
                        "title",
                        self.i18n.t("ai.safety_mode.menu_title"),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .child(self.render_ai_safety_menu_item(
                AiSafetyMode::ReadOnly,
                self.i18n.t("ai.safety_mode.read_only_mode"),
                self.i18n.t("ai.safety_mode.read_only_desc"),
                cx,
            ))
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
                self.render_ai_menu_action(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(self.tokens.spacing.two))
                        .px(px(self.tokens.spacing.three))
                        .py(px(self.tokens.spacing.two))
                        .text_size(px(12.0))
                        .text_color(rgb(self.tokens.ui.text))
                        .child(
                            div()
                                .flex_none()
                                .w(px(16.0))
                                .h(px(16.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Settings,
                                    14.0,
                                    rgb(self.tokens.ui.text_muted),
                                )),
                        )
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::NonSelectable,
                            "ai-safety-menu",
                            "open-settings",
                            self.i18n.t("ai.safety_mode.open_settings"),
                            self.tokens.ui.text,
                            cx,
                        )),
                    false,
                    false,
                    Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
                    |this, _event, window, cx| {
                        this.open_ai_settings(window, cx);
                    },
                    cx,
                ),
            );
        menu.into_any_element()
    }

    pub(in crate::workspace) fn render_ai_safety_menu_item(
        &self,
        mode: AiSafetyMode,
        title: String,
        description: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon = match mode {
            AiSafetyMode::Default => LucideIcon::ShieldCheck,
            AiSafetyMode::ReadOnly => LucideIcon::Shield,
            AiSafetyMode::Bypass => LucideIcon::ShieldAlert,
        };
        let bypass = mode == AiSafetyMode::Bypass;
        let title_color = if bypass {
            self.tokens.ui.warning
        } else {
            self.tokens.ui.text
        };
        let mode_key = match mode {
            AiSafetyMode::Default => "default",
            AiSafetyMode::ReadOnly => "read-only",
            AiSafetyMode::Bypass => "bypass",
        };
        let item = div()
            .flex()
            .items_start()
            .gap(px(self.tokens.spacing.two))
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.two))
            .child(
                div()
                    .flex_none()
                    .w(px(16.0))
                    .h(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    // A fixed icon slot keeps every label aligned even when
                    // individual SVG paths have different optical bounds.
                    .child(Self::render_lucide_icon(
                        icon,
                        14.0,
                        rgb(if bypass {
                            self.tokens.ui.warning
                        } else {
                            self.tokens.ui.accent
                        }),
                    )),
            )
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
                            .text_color(rgb(title_color))
                            // Safety mode rows are menu items; text must bubble mouse-down like Tauri select-none labels.
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "ai-safety-menu-item-title",
                                mode_key,
                                title,
                                title_color,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .line_height(px(15.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "ai-safety-menu-item-description",
                                mode_key,
                                description,
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    ),
            );
        // Safety rows behave as menu actions; disabled/loading semantics stay
        // centralized even though these two actions are currently always enabled.
        self.render_ai_menu_action(
            item,
            false,
            false,
            Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
            move |this, _event, window, cx| {
                match mode {
                    AiSafetyMode::Default => this.set_ai_safety_mode_default(cx),
                    AiSafetyMode::ReadOnly => this.set_ai_safety_mode_read_only(cx),
                    AiSafetyMode::Bypass => {
                        if this.active_ai_safety_mode(cx) != AiSafetyMode::Bypass {
                            // The safety menu is itself a floating overlay.
                            // Open the confirm dialog after this click/update
                            // cycle so GPUI does not re-enter WorkspaceApp while
                            // the old menu frame is still being processed.
                            cx.defer_in(window, |this, _window, cx| {
                                this.open_ai_safety_confirm(cx);
                            });
                        }
                    }
                }
            },
            cx,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_safety_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "ai-safety-confirm-motion",
            self.ai_entity.read(cx).chat_ui().safety_confirm_presence.phase(),
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-safety-confirm",
                        "title",
                        self.i18n.t("ai.safety_mode.confirm_title"),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: Some(
                    div()
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::NonSelectable,
                            "ai-safety-confirm",
                            "description",
                            self.i18n.t("ai.safety_mode.confirm_description"),
                            self.tokens.ui.text_muted,
                            cx,
                        ))
                        .into_any_element(),
                ),
                cancel_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-safety-confirm",
                        "cancel",
                        self.i18n.t("ai.safety_mode.confirm_cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-safety-confirm",
                        "confirm",
                        self.i18n.t("ai.safety_mode.confirm_enable"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            self.standard_confirm_focus_owner(),
            cx.listener(|this, _event, _window, cx| {
                this.begin_ai_safety_confirm_exit(cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                if this.begin_ai_safety_confirm_exit(cx) {
                    this.confirm_ai_safety_bypass(cx);
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    pub(in crate::workspace) fn render_ai_tool_indicator(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tool_use = &self.settings_store.settings().ai.tool_use;
        let enabled = tool_use.enabled;
        let tool_policy = ai_tool_use_policy_from_settings(tool_use);
        let active_tool_count = ai_active_tool_count(
            enabled,
            &tool_policy,
            self.ai_entity.read(cx).mcp_registry(),
        );
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
                this.open_ai_settings(window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_context_usage_indicator(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let breakdown = self.ai_context_token_breakdown(cx);
        let acp_usage = self.active_ai_acp_usage(cx);
        let (total_tokens, max_tokens) =
            acp_usage.unwrap_or((breakdown.total, breakdown.max_tokens));
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
            acp_usage.is_none(),
        )
        .when(acp_usage.is_none(), |indicator| indicator.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                let next_open = !this.ai_entity.read(cx).chat_ui().context_popover_open;
                this.close_ai_sidebar_popovers(cx);
                this.ai_entity.update(cx, |ai, _cx| {
                    ai.set_chat_popover_open(AiChatPopover::Context, next_open);
                });
                cx.stop_propagation();
                cx.notify();
            }),
        ));
        if acp_usage.is_some() {
            return indicator.into_any_element();
        }
        let workspace = cx.entity();
        select_anchor_probe(
            SelectAnchorId::AiContextPopover,
            indicator,
            Self::deferred_ai_select_anchor_update(workspace),
        )
        .into_any_element()
    }

    fn active_ai_acp_usage(&self, cx: &App) -> Option<(usize, usize)> {
        if self.settings_store.settings().ai.active_backend != AiActiveBackend::Acp {
            return None;
        }
        let usage = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation()
            .and_then(ai_acp_session_state)?
            .usage?;
        let used = usage.get("used")?.as_u64()?.try_into().ok()?;
        let size = usage.get("size")?.as_u64()?.try_into().ok()?;
        Some((used, size))
    }

    fn render_ai_acp_plan_indicator(&self, cx: &App) -> Option<AnyElement> {
        if self.settings_store.settings().ai.active_backend != AiActiveBackend::Acp {
            return None;
        }
        let plan = self
            .ai_entity
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

    pub(in crate::workspace) fn render_ai_context_popover(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let breakdown = self.ai_context_token_breakdown(cx);
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
        let popover = ai_context_popover(&self.tokens)
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
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "ai-context-popover-section",
                                "system",
                                self.i18n.t("ai.context.system"),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    )
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.system_instructions"),
                        ai_context_percent(breakdown.system_instructions, max_tokens),
                        cx,
                    ))
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.tool_definitions"),
                        ai_context_percent(breakdown.tool_definitions, max_tokens),
                        cx,
                    ))
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.reserved_output"),
                        ai_context_percent(breakdown.reserved_output, max_tokens),
                        cx,
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
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "ai-context-popover-section",
                                "user",
                                self.i18n.t("ai.context.user_context"),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    )
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.messages_label"),
                        ai_context_percent(breakdown.messages, max_tokens),
                        cx,
                    ))
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.tool_results"),
                        ai_context_percent(breakdown.tool_results, max_tokens),
                        cx,
                    )),
            )
            .when(
                self.ai_entity.read(cx).conversation_state()
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
                                    // Popover command label mirrors Tauri select-none button text.
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::NonSelectable,
                                        "ai-context-popover-action",
                                        "compress",
                                        self.i18n.t("ai.context.compress_dialog"),
                                        self.tokens.ui.text,
                                        cx,
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.ai_entity.update(cx, |ai, _cx| {
                                                ai.set_chat_popover_open(
                                                    AiChatPopover::Context,
                                                    false,
                                                );
                                            });
                                            this.start_ai_compact_conversation(cx);
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            ),
                        )
                },
            );
        popover.into_any_element()
    }

    pub(in crate::workspace) fn render_ai_context_breakdown_row(
        &self,
        label: String,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .py(px(2.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "ai-context-breakdown-label",
                        (&label, &value),
                        label.clone(),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_selectable_text_scoped(
                        "ai-context-breakdown-value",
                        (&label, &value),
                        value.clone(),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_context_token_breakdown(
        &self,
        cx: &App,
    ) -> AiContextTokenBreakdown {
        let settings = self.settings_store.settings();
        let providers = ai_provider_views(&settings.ai.providers);
        let active_provider =
            active_provider_view(&providers, settings.ai.active_provider_id.as_deref());
        let model = active_model_selection(settings.ai.active_model.as_deref()).unwrap_or_default();
        let provider_id = active_provider
            .map(|provider| provider.id.as_str())
            .unwrap_or("");
        let max_tokens = ai_context_window_from_maps(
            &settings.ai.user_context_windows,
            &settings.ai.model_context_windows,
            provider_id,
            &model,
        )
        .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW);
        let system_prompt = settings.ai.custom_system_prompt.trim();
        let conversation = self.ai_entity.read(cx).conversation_state().active_conversation();
        let cache_key = AiContextTokenBreakdownKey {
            conversation_id: conversation.map(|conversation| conversation.id.clone()),
            conversation_fingerprint: ai_conversation_token_fingerprint(conversation),
            provider_id: provider_id.to_string(),
            model: model,
            max_tokens,
            system_prompt_fingerprint: ai_text_shape_fingerprint(system_prompt),
            tool_use_enabled: settings.ai.tool_use.enabled,
        };
        {
            let cache = self.ai_entity.read(cx).chat_ui().context_token_cache.borrow();
            if cache.key.as_ref() == Some(&cache_key)
                && let Some(cached) = cache.breakdown_without_draft.as_ref()
            {
                return ai_context_breakdown_with_draft(cached.clone(), &self.ai_entity.read(cx).chat_ui().draft);
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
        let tool_results = conversation
            .map(ai_conversation_tool_result_tokens)
            .unwrap_or(0);
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
        let mut cache = self.ai_entity.read(cx).chat_ui().context_token_cache.borrow_mut();
        cache.key = Some(cache_key);
        cache.breakdown_without_draft = Some(breakdown_without_draft.clone());
        ai_context_breakdown_with_draft(breakdown_without_draft, &self.ai_entity.read(cx).chat_ui().draft)
    }

    pub(in crate::workspace) fn ai_should_show_context_chips(
        &self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.ai_active_terminal_context_available(cx)
            || self.ai_active_tab_has_split_panes(cx)
            || self.ai_has_ide_context(cx)
            || self.ai_has_sftp_context(cx)
    }

    pub(in crate::workspace) fn render_ai_context_chips(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut chips = ai_chat_input_chips(&self.tokens);
        if self.ai_active_terminal_context_available(cx) {
            chips = chips.child(
                ai_context_chip(
                    &self.tokens,
                    self.i18n.t("ai.input.context"),
                    AiTone::Accent,
                    self.ai_entity.read(cx).chat_ui().include_context,
                    Self::render_lucide_icon(
                        LucideIcon::Terminal,
                        12.0,
                        rgb(if self.ai_entity.read(cx).chat_ui().include_context {
                            self.tokens.ui.accent
                        } else {
                            self.tokens.ui.text_muted
                        }),
                    ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.ai_entity.update(cx, |ai, _cx| {
                            ai.toggle_chat_context();
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            );
        }
        if self.ai_active_tab_has_split_panes(cx) && self.ai_entity.read(cx).chat_ui().include_context {
            chips = chips.child(
                ai_context_chip(
                    &self.tokens,
                    self.i18n.t("ai.input.panes"),
                    AiTone::Blue,
                    self.ai_entity.read(cx).chat_ui().include_all_panes,
                    Self::render_lucide_icon(
                        LucideIcon::SplitSquareHorizontal,
                        12.0,
                        rgb(if self.ai_entity.read(cx).chat_ui().include_all_panes {
                            self.tokens.ui.info
                        } else {
                            self.tokens.ui.text_muted
                        }),
                    ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.ai_entity.update(cx, |ai, _cx| {
                            ai.toggle_chat_all_panes();
                        });
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
                Self::render_lucide_icon(LucideIcon::Code2, 12.0, rgb(self.tokens.ui.success)),
            ));
        }
        if self.ai_has_sftp_context(cx) {
            chips = chips.child(ai_context_chip(
                &self.tokens,
                self.i18n.t("ai.input.sftp_context"),
                AiTone::Orange,
                true,
                Self::render_lucide_icon(LucideIcon::FolderOpen, 12.0, rgb(self.tokens.ui.warning)),
            ));
        }
        chips.into_any_element()
    }

    pub(in crate::workspace) fn ai_chat_autocomplete_items(
        &self,
        cx: &App,
    ) -> Vec<AiAutocompleteCandidate> {
        let ai = self.ai_entity.read(cx);
        if !ai.chat_ui().input_focused || ai.chat_ui().autocomplete_suppressed {
            return Vec::new();
        }
        let draft = &ai.chat_ui().draft;
        let mut candidates = ai_autocomplete_candidates(draft, draft.len());
        if self.settings_store.settings().ai.active_backend != AiActiveBackend::Acp
            || ai_input_token_at_cursor(draft, draft.len()).token_type
                != Some(oxideterm_ai::AiInputTokenType::Slash)
        {
            return candidates;
        }

        let partial = ai_input_token_at_cursor(draft, draft.len())
            .partial
            .to_lowercase();
        let Some(session) = ai
            .conversation_state()
            .active_conversation()
            .and_then(ai_acp_session_state)
        else {
            return candidates;
        };
        for command in session.available_commands {
            let Some(name) = command.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !name.to_lowercase().starts_with(&partial)
                || candidates.iter().any(|candidate| candidate.name == name)
            {
                continue;
            }
            candidates.push(AiAutocompleteCandidate {
                kind: AiAutocompleteKind::Slash,
                name: name.to_string(),
                description: command
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                description_is_i18n_key: false,
                accepts_value: command.get("input").is_some_and(|input| !input.is_null()),
            });
        }
        candidates
    }

    pub(in crate::workspace) fn render_ai_autocomplete_popup(
        &self,
        items: &[AiAutocompleteCandidate],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_index = self.ai_entity.read(cx).chat_ui().autocomplete_index
            .min(items.len().saturating_sub(1));
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
                    item.name.clone(),
                    if item.description_is_i18n_key {
                        self.i18n.t(&item.description)
                    } else {
                        item.description.clone()
                    },
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

    pub(in crate::workspace) fn apply_ai_chat_autocomplete_candidate(
        &mut self,
        candidate: &AiAutocompleteCandidate,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.apply_chat_autocomplete(candidate);
        });
        self.ime_marked_text = None;
        cx.notify();
    }
}

pub(in crate::workspace) fn ai_input_line_segments(
    tokens: &oxideterm_theme::ThemeTokens,
    line: &str,
    selection_range: Option<std::ops::Range<usize>>,
    caret_offset: Option<usize>,
    caret_visible: bool,
    marked_range: Option<std::ops::Range<usize>>,
) -> Div {
    // Reuse the shared input renderer so caret and selection overlays never
    // split the editable line into separate layout text runs.
    let segments = if let Some(marked_range) = marked_range {
        text_input_value_segments_with_marked_range(tokens, line, marked_range)
    } else {
        text_input_value_segments_with_color(
            tokens,
            line,
            false,
            selection_range,
            caret_offset,
            caret_visible,
            Some(tokens.ui.text),
        )
    };
    segments
    .min_w_0()
    .max_w_full()
    .flex()
    .items_center()
}

pub(in crate::workspace) fn ai_input_local_marked_range(
    marked_range: &std::ops::Range<usize>,
    line_range: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    // A composition may cross a soft-wrap boundary. Each rendered line owns
    // only the intersecting UTF-16 segment of the virtual marked range.
    let start = marked_range.start.max(line_range.start);
    let end = marked_range.end.min(line_range.end);
    (start < end).then(|| start - line_range.start..end - line_range.start)
}

#[derive(Clone, Copy)]
pub(in crate::workspace) struct AiInputVisualLine<'a> {
    text: &'a str,
    utf16_start: usize,
    utf16_end: usize,
}

impl AiInputVisualLine<'_> {
    pub(in crate::workspace) fn utf16_len(&self) -> usize {
        self.utf16_end.saturating_sub(self.utf16_start)
    }
}

pub(in crate::workspace) const AI_INPUT_SOFT_WRAP_CHROME_PX: f32 = 56.0;
pub(in crate::workspace) const AI_INPUT_SOFT_WRAP_HALF_WIDTH_PX: f32 = 7.0;
pub(in crate::workspace) const AI_INPUT_SOFT_WRAP_MIN_COLUMNS: usize = 12;

pub(in crate::workspace) fn ai_input_soft_wrap_columns(sidebar_width: f32) -> usize {
    let text_width = (sidebar_width - AI_INPUT_SOFT_WRAP_CHROME_PX).max(80.0);
    ((text_width / AI_INPUT_SOFT_WRAP_HALF_WIDTH_PX).floor() as usize)
        .max(AI_INPUT_SOFT_WRAP_MIN_COLUMNS)
}

pub(in crate::workspace) fn ai_input_visual_lines(
    input: &str,
    wrap_columns: usize,
) -> Vec<AiInputVisualLine<'_>> {
    let wrap_columns = wrap_columns.max(AI_INPUT_SOFT_WRAP_MIN_COLUMNS);
    let mut visual_lines = Vec::new();
    let mut utf16_line_start = 0;

    for line in input.split('\n') {
        ai_push_wrapped_input_line(line, utf16_line_start, wrap_columns, &mut visual_lines);
        utf16_line_start += line.encode_utf16().count() + 1;
    }

    if visual_lines.is_empty() {
        visual_lines.push(AiInputVisualLine {
            text: "",
            utf16_start: 0,
            utf16_end: 0,
        });
    }
    visual_lines
}

pub(in crate::workspace) fn ai_push_wrapped_input_line<'a>(
    line: &'a str,
    utf16_line_start: usize,
    wrap_columns: usize,
    visual_lines: &mut Vec<AiInputVisualLine<'a>>,
) {
    if line.is_empty() {
        visual_lines.push(AiInputVisualLine {
            text: line,
            utf16_start: utf16_line_start,
            utf16_end: utf16_line_start,
        });
        return;
    }

    let mut segment_byte_start = 0;
    let mut segment_utf16_start = utf16_line_start;
    let mut segment_columns = 0;
    let mut utf16_offset = utf16_line_start;

    for (byte_index, ch) in line.char_indices() {
        let char_columns = ai_input_char_columns(ch);
        if segment_columns > 0 && segment_columns + char_columns > wrap_columns {
            visual_lines.push(AiInputVisualLine {
                text: &line[segment_byte_start..byte_index],
                utf16_start: segment_utf16_start,
                utf16_end: utf16_offset,
            });
            segment_byte_start = byte_index;
            segment_utf16_start = utf16_offset;
            segment_columns = 0;
        }

        segment_columns += char_columns;
        utf16_offset += ch.len_utf16();
    }

    visual_lines.push(AiInputVisualLine {
        text: &line[segment_byte_start..],
        utf16_start: segment_utf16_start,
        utf16_end: utf16_offset,
    });
}

pub(in crate::workspace) fn ai_input_char_columns(ch: char) -> usize {
    // GPUI does not expose textarea-style wrapping here, so this estimates
    // terminal-adjacent text width with UTF-16-safe boundaries for IME state.
    if ch == '\t' {
        4
    } else if ch.is_ascii() {
        1
    } else {
        2
    }
}

pub(in crate::workspace) fn ai_format_tokens(tokens: usize) -> String {
    if tokens >= 1000 {
        format!("{:.1}K", tokens as f32 / 1000.0)
    } else {
        tokens.to_string()
    }
}

pub(in crate::workspace) fn ai_context_percent(tokens: usize, max_tokens: usize) -> String {
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

pub(in crate::workspace) fn ai_context_breakdown_with_draft(
    mut breakdown: AiContextTokenBreakdown,
    draft: &str,
) -> AiContextTokenBreakdown {
    let draft_tokens = ai_estimated_tokens(draft);
    breakdown.messages = breakdown.messages.saturating_add(draft_tokens);
    breakdown.total = breakdown.total.saturating_add(draft_tokens);
    breakdown
}

pub(in crate::workspace) fn ai_conversation_token_fingerprint(
    conversation: Option<&AiConversation>,
) -> u64 {
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

pub(in crate::workspace) fn ai_role_fingerprint(role: &AiChatRole) -> u8 {
    match role {
        AiChatRole::User => 0,
        AiChatRole::Assistant => 1,
        AiChatRole::System => 2,
        AiChatRole::Tool => 3,
    }
}

pub(in crate::workspace) fn ai_text_shape_fingerprint(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ai_hash_text_shape(text, &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

pub(in crate::workspace) fn ai_hash_text_shape(
    text: &str,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) {
    let bytes = text.as_bytes();
    std::hash::Hash::hash(&bytes.len(), hasher);
    let head = bytes.len().min(32);
    std::hash::Hash::hash(&&bytes[..head], hasher);
    if bytes.len() > head {
        let tail = bytes.len().saturating_sub(32);
        std::hash::Hash::hash(&&bytes[tail..], hasher);
    }
}

pub(in crate::workspace) fn ai_hash_tool_call_shape(
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

pub(in crate::workspace) fn ai_conversation_tool_result_tokens(
    conversation: &AiConversation,
) -> usize {
    conversation
        .messages
        .iter()
        .filter(|message| matches!(message.role, AiChatRole::User | AiChatRole::Assistant))
        .flat_map(|message| message.tool_calls.iter())
        .map(ai_tool_call_estimated_tokens)
        .sum()
}

pub(in crate::workspace) fn ai_tool_call_estimated_tokens(tool_call: &serde_json::Value) -> usize {
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

pub(in crate::workspace) fn ai_estimated_tool_definitions_tokens() -> usize {
    ai_tool_definitions_estimated_tokens(&oxideterm_ai::orchestrator_tool_definitions())
}

#[cfg(test)]
mod input_render_tests {
    use super::ai_input_local_marked_range;

    #[test]
    fn marked_range_is_projected_once_across_visual_lines() {
        let marked = 2..6;

        assert_eq!(ai_input_local_marked_range(&marked, &(0..4)), Some(2..4));
        assert_eq!(ai_input_local_marked_range(&marked, &(4..8)), Some(0..2));
        assert_eq!(ai_input_local_marked_range(&marked, &(8..12)), None);
    }
}
