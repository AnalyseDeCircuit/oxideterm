pub(in crate::workspace) const AI_INLINE_PANEL_WIDTH: f32 = 520.0;
pub(in crate::workspace) const AI_INLINE_PANEL_TOP: f32 = 48.0;
pub(in crate::workspace) const AI_INLINE_PANEL_MARGIN: f32 = 12.0;
pub(in crate::workspace) const AI_INLINE_PANEL_VERTICAL_OFFSET: f32 = 4.0;
pub(in crate::workspace) const AI_INLINE_PANEL_COLLAPSED_HEIGHT: f32 = 56.0;
pub(in crate::workspace) const AI_INLINE_PANEL_EXPANDED_HEIGHT: f32 = 160.0;
pub(in crate::workspace) const AI_INLINE_PANEL_LOADING_BAR_HEIGHT: f32 = 2.0;
#[derive(Default)]
pub(in crate::workspace) struct AiInlinePanelState {
    pub(in crate::workspace) open: bool,
    pub(in crate::workspace) prompt: String,
    pub(in crate::workspace) response: String,
    pub(in crate::workspace) error: Option<String>,
    pub(in crate::workspace) loading: bool,
    pub(in crate::workspace) copied: bool,
    pub(in crate::workspace) prompt_focused: bool,
    pub(in crate::workspace) has_api_key: Option<bool>,
    pub(in crate::workspace) has_selection: bool,
    pub(in crate::workspace) selection_context: String,
    pub(in crate::workspace) generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::workspace) struct AiInlinePanelPlacement {
    left: f32,
    top: f32,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn toggle_terminal_ai_inline_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai_entity.read(cx).terminal_inline_panel().open {
            self.close_terminal_ai_inline_panel(window, cx);
        } else {
            self.open_terminal_ai_inline_panel(window, cx);
        }
    }

    pub(in crate::workspace) fn open_terminal_ai_inline_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search.visible = false;
        self.close_terminal_command_overlays(cx);
        self.close_ai_model_selector(cx);

        let selection = self
            .active_pane(cx)
            .and_then(|pane| pane.read(cx).selected_text_snapshot())
            .unwrap_or_default();
        let sanitized_selection = truncate_ai_inline_context(
            oxideterm_ai::sanitize_for_ai(&selection),
            self.settings_store.settings().ai.context_max_chars,
        );
        self.ai_entity.update(cx, |ai, _cx| {
            ai.open_terminal_inline_panel(sanitized_selection);
        });

        window.focus(&self.focus_handle, cx);
        self.refresh_terminal_ai_inline_key_status(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_ai_inline_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity
            .update(cx, |ai, _cx| ai.close_terminal_inline_panel());
        self.ime_marked_text = None;
        self.close_ai_model_selector(cx);
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn handle_ai_inline_panel_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (panel_open, panel_loading, response_is_empty) = {
            let panel = self.ai_entity.read(cx).terminal_inline_panel();
            (panel.open, panel.loading, panel.response.trim().is_empty())
        };
        if !panel_open || event.keystroke.modifiers.platform {
            return false;
        }
        if self
            .ai_entity
            .read(cx)
            .model_selector_is_open(AiModelSelectorScope::TerminalInline)
            && self.ai_entity.read(cx).model_selector_search_focused()
        {
            return self.handle_ai_sidebar_key(event, cx);
        }

        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_terminal_ai_inline_panel(window, cx);
                true
            }
            "enter"
                if self
                    .marked_text_for_target(WorkspaceImeTarget::AiInlinePrompt, cx)
                    .is_some() =>
            {
                true
            }
            "enter" if !event.keystroke.modifiers.shift => {
                if panel_loading {
                    return true;
                }
                if response_is_empty {
                    self.send_terminal_ai_inline_prompt(cx);
                } else {
                    self.execute_terminal_ai_inline_response(window, cx);
                }
                true
            }
            "tab" if !response_is_empty && !panel_loading => {
                self.insert_terminal_ai_inline_response(window, cx);
                true
            }
            _ => true,
        }
    }

    pub(in crate::workspace) fn render_terminal_ai_inline_panel(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (
            panel_open,
            prompt_focused,
            prompt,
            response_is_empty,
            response_command,
            panel_error,
            panel_loading,
            panel_copied,
            has_api_key,
            has_selection,
        ) = {
            let panel = self.ai_entity.read(cx).terminal_inline_panel();
            (
                panel.open,
                panel.prompt_focused,
                panel.prompt.clone(),
                panel.response.is_empty(),
                extract_terminal_ai_inline_command(&panel.response),
                panel.error.clone(),
                panel.loading,
                panel.copied,
                panel.has_api_key,
                panel.has_selection,
            )
        };
        if !panel_open {
            return div().into_any_element();
        }

        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::AiInlinePrompt;
        let focused = prompt_focused;
        let marked_text = self.marked_text_for_target(target, cx);
        let selected_range = self.ime_selected_range_for_target(target, cx);
        let showing_placeholder = prompt.is_empty() && marked_text.is_none();
        let placeholder = if has_selection {
            self.i18n.t("terminal.ai.selection_placeholder")
        } else {
            self.i18n.t("terminal.ai.inline_placeholder")
        };
        let prompt_text = if showing_placeholder {
            placeholder
        } else {
            prompt.clone()
        };
        let prompt_range = selected_range.clone().filter(|_| {
            focused && !prompt.is_empty() && marked_text.is_none()
        });
        let selection_range = prompt_range.clone().filter(|range| range.start < range.end);
        let caret_offset = prompt_range
            .as_ref()
            .filter(|range| range.start == range.end)
            .map(|range| range.start);
        let workspace = cx.entity();
        let placement = self.terminal_ai_inline_panel_placement(cx);

        div()
            .absolute()
            .top(px(placement.top))
            .left(px(placement.left))
            .child(
                div()
                    .relative()
                    .w(px(AI_INLINE_PANEL_WIDTH))
                    .rounded(px(self.tokens.radii.md))
                    // Tauri inline panels clip loading strips and action bars
                    // through the rounded shell; GPUI needs the same explicit
                    // panel clipping before any edge child paints.
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_elevated))
                    .shadow_lg()
                    .when(panel_loading, |panel| {
                        panel.child(
                            div()
                                .absolute()
                                .top(px(0.0))
                                .left(px(0.0))
                                .right(px(0.0))
                                .h(px(AI_INLINE_PANEL_LOADING_BAR_HEIGHT))
                                .rounded_t(px(
                                    oxideterm_gpui_ui::modal::rounded_shell_child_radius(
                                        self.tokens.radii.md,
                                    ),
                                ))
                                .bg(rgb(theme.accent)),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Sparkles,
                                16.0,
                                rgb(theme.accent),
                            ))
                            .child(self.render_ai_model_selector(
                                AiModelSelectorScope::TerminalInline,
                                SelectAnchorId::AiInlineModelSelector,
                                cx,
                            ))
                            .child(
                                text_input_anchor_probe(
                                    target.anchor_id(),
                                    div()
                                        .h(px(22.0))
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .overflow_hidden()
                                        .font_family(settings_mono_font_family(self.settings_store.settings()))
                                        .text_size(px(13.0))
                                        .text_color(if showing_placeholder {
                                            rgb(theme.text_muted)
                                        } else {
                                            rgb(theme.text)
                                        })
                                        .cursor(CursorStyle::IBeam)
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                                this.ai_entity.update(cx, |ai, _cx| {
                                                    ai.terminal_inline_panel_mut().prompt_focused = true;
                                                });
                                                this.ai_entity.update(cx, |ai, _cx| {
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
                                        )
                                        .when(focused && showing_placeholder, |input| {
                                            input.child(text_caret(&self.tokens, self.input_caret.visible()))
                                        })
                                        .child(if showing_placeholder {
                                            div().truncate().child(prompt_text).into_any_element()
                                        } else {
                                            text_input_value_segments_with_color(
                                                &self.tokens,
                                                &prompt_text,
                                                false,
                                                selection_range,
                                                caret_offset,
                                                self.input_caret.visible(),
                                                Some(theme.text),
                                            )
                                            .into_any_element()
                                        })
                                        .when_some(marked_text, |input, marked| {
                                            input.child(
                                                div()
                                                    .underline()
                                                    .text_color(rgb(theme.text))
                                                    .child(marked.to_string()),
                                            )
                                        })
                                        .when(
                                            focused
                                                && !showing_placeholder
                                                && selected_range.is_none()
                                                && marked_text.is_none(),
                                            |input| {
                                                input.child(text_caret(
                                                    &self.tokens,
                                                    self.input_caret.visible(),
                                                ))
                                            },
                                        ),
                                    Self::deferred_ai_text_input_anchor_update(workspace),
                                ),
                            )
                            .child(self.render_terminal_ai_inline_hints(cx))
                            .child(
                                self.workspace_icon_action_button(
                                    LucideIcon::X,
                                    14.0,
                                    rgb(theme.text_muted),
                                    IconButtonOptions {
                                        background: Some(rgba(0x00000000)),
                                        hover_background: Some(rgb(theme.bg_hover)),
                                        ..IconButtonOptions::opaque_toolbar(24.0, ButtonRadius::Md)
                                    },
                                    |this, _event, window, cx| {
                                        this.close_terminal_ai_inline_panel(window, cx);
                                        cx.stop_propagation();
                                    },
                                    cx,
                                )
                            ),
                    )
                    .when(
                        has_api_key == Some(false) && !panel_loading,
                        |panel| panel.child(self.render_terminal_ai_inline_notice(
                            LucideIcon::AlertCircle,
                            self.i18n.t("terminal.ai.api_key_hint"),
                            rgba((self.tokens.ui.warning << 8) | 0x1a),
                            rgba((self.tokens.ui.warning << 8) | 0x4d),
                            rgb(self.tokens.ui.warning),
                        )),
                    )
                    .when_some(panel_error.as_ref(), |panel, error| {
                        panel.child(self.render_terminal_ai_inline_notice(
                            LucideIcon::AlertCircle,
                            error.clone(),
                            rgba((self.tokens.ui.error << 8) | 0x1a),
                            rgba((self.tokens.ui.error << 8) | 0x4d),
                            rgb(self.tokens.ui.error),
                        ))
                    })
                    .when(
                        (panel_loading || !response_is_empty) && panel_error.is_none(),
                        |panel| {
                            panel.child(
                                div()
                                    .border_t_1()
                                    .border_color(rgb(theme.border))
                                    .child(
                                        div()
                                            .max_h(px(120.0))
                                            .overflow_hidden()
                                            .px(px(12.0))
                                            .py(px(8.0))
                                            .bg(rgb(theme.bg_sunken))
                                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                                            .text_size(px(13.0))
                                            .line_height(px(20.0))
                                            .text_color(rgb(theme.accent))
                                            .child(if response_command.is_empty() {
                                                self.i18n.t("terminal.ai.generating")
                                            } else {
                                                response_command.clone()
                                            }),
                                    )
                                    .when(
                                        !response_is_empty && !panel_loading,
                                        |preview| {
                                            preview.child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(4.0))
                                                    .border_t_1()
                                                    .border_color(rgb(theme.border))
                                                    .bg(rgb(theme.bg_elevated))
                                                    .rounded_b(px(
                                                        oxideterm_gpui_ui::modal::rounded_shell_child_radius(
                                                            self.tokens.radii.md,
                                                        ),
                                                    ))
                                                    .px(px(8.0))
                                                    .py(px(6.0))
                                                    .child(self.render_terminal_ai_inline_action(
                                                        LucideIcon::Play,
                                                        self.i18n.t("terminal.ai.execute"),
                                                        true,
                                                        |this, _event, window, cx| {
                                                            this.execute_terminal_ai_inline_response(window, cx);
                                                            cx.stop_propagation();
                                                        },
                                                        cx,
                                                    ))
                                                    .child(self.render_terminal_ai_inline_action(
                                                        LucideIcon::CornerDownLeft,
                                                        self.i18n.t("terminal.ai.insert"),
                                                        false,
                                                        |this, _event, window, cx| {
                                                            this.insert_terminal_ai_inline_response(window, cx);
                                                            cx.stop_propagation();
                                                        },
                                                        cx,
                                                    ))
                                                    .child(self.render_terminal_ai_inline_action(
                                                        if panel_copied {
                                                            LucideIcon::Check
                                                        } else {
                                                            LucideIcon::Copy
                                                        },
                                                        if panel_copied {
                                                            self.i18n.t("terminal.ai.copied")
                                                        } else {
                                                            self.i18n.t("terminal.ai.copy")
                                                        },
                                                        false,
                                                        |this, _event, _window, cx| {
                                                            this.copy_terminal_ai_inline_response(cx);
                                                            cx.stop_propagation();
                                                        },
                                                        cx,
                                                    ))
                                                    .child(self.render_terminal_ai_inline_action(
                                                        LucideIcon::RotateCcw,
                                                        self.i18n.t("terminal.ai.regenerate"),
                                                        false,
                                                        |this, _event, _window, cx| {
                                                            this.regenerate_terminal_ai_inline_response(cx);
                                                            cx.stop_propagation();
                                                        },
                                                        cx,
                                                    )),
                                            )
                                        },
                                    ),
                            )
                        },
                    )
                    .when(
                        self.ai_entity
                            .read(cx)
                            .model_selector_is_open(AiModelSelectorScope::TerminalInline),
                        |panel| {
                        panel.child(
                            div()
                                .absolute()
                                .top(px(40.0))
                                .left(px(32.0))
                                .child(self.render_ai_model_selector_dropdown(
                                    &self.ai_model_selector_providers(cx),
                                    cx,
                                )),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn terminal_ai_inline_panel_placement(
        &self,
        cx: &mut Context<Self>,
    ) -> AiInlinePanelPlacement {
        let expanded = {
            let panel = self.ai_entity.read(cx).terminal_inline_panel();
            panel.loading || panel.error.is_some() || !panel.response.is_empty()
        };
        let estimated_height = if expanded {
            AI_INLINE_PANEL_EXPANDED_HEIGHT
        } else {
            AI_INLINE_PANEL_COLLAPSED_HEIGHT
        };
        let anchor = self
            .active_pane(cx)
            .and_then(|pane| pane.read(cx).cursor_anchor());
        terminal_ai_inline_panel_placement(anchor, estimated_height)
    }

    pub(in crate::workspace) fn render_terminal_ai_inline_hints(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (prompt_has_text, response_has_text, loading) = {
            let panel = self.ai_entity.read(cx).terminal_inline_panel();
            (
                !panel.prompt.trim().is_empty(),
                !panel.response.trim().is_empty(),
                panel.loading,
            )
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(10.0))
            .text_color(rgb(theme.text_muted))
            .when(
                !response_has_text && !loading && prompt_has_text,
                |hints| {
                    hints
                        .child(inline_ai_keycap(&self.tokens, "Enter"))
                        .child(self.i18n.t("terminal.ai.to_send"))
                },
            )
            .when(response_has_text && !loading, |hints| {
                    hints
                        .child(inline_ai_keycap(&self.tokens, "Tab"))
                        .child(self.i18n.t("terminal.ai.to_insert"))
                        .child(inline_ai_keycap(&self.tokens, "Enter"))
                        .child(self.i18n.t("terminal.ai.to_run"))
                })
            .into_any_element()
    }

    pub(in crate::workspace) fn render_terminal_ai_inline_notice(
        &self,
        icon: LucideIcon,
        message: String,
        bg: Rgba,
        border: Rgba,
        fg: Rgba,
    ) -> AnyElement {
        div()
            .mx(px(12.0))
            .mb(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(border)
            .bg(bg)
            .px(px(8.0))
            .py(px(6.0))
            .text_size(px(12.0))
            .text_color(fg)
            .child(Self::render_lucide_icon(icon, 14.0, fg))
            .child(div().truncate().child(message))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_terminal_ai_inline_action(
        &self,
        icon: LucideIcon,
        label: String,
        primary: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let fg = if primary {
            rgb(0xffffff)
        } else {
            rgb(theme.text)
        };
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(4.0))
            .text_size(px(11.0))
            .text_color(fg)
            .bg(if primary {
                rgb(theme.accent)
            } else {
                rgba(0x00000000)
            })
            .hover(move |style| {
                style.bg(if primary {
                    rgb(theme.accent_hover)
                } else {
                    rgb(theme.bg_hover)
                })
            })
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, cx.listener(listener))
            .child(Self::render_lucide_icon(icon, 12.0, fg))
            .child(label)
            .into_any_element()
    }

    pub(in crate::workspace) fn send_terminal_ai_inline_prompt(&mut self, cx: &mut Context<Self>) {
        let Some((prompt, selection)) = self
            .ai_entity
            .read(cx)
            .terminal_inline_request_context()
        else {
            return;
        };
        let messages = terminal_ai_inline_messages(
            terminal_ai_inline_os_context(self.active_tab(cx)),
            selection,
            prompt,
        );
        let config_result = self.resolve_terminal_ai_inline_config();
        let api_key_not_found = self.i18n.t("ai.model_selector.api_key_not_found");
        let failed_to_get_key = self.i18n.t("ai.model_selector.failed_to_get_api_key");
        let stream_failed = self.i18n.t("settings_view.ai.acp_agent_error_unknown");
        self.ai_entity.update(cx, |ai, _cx| {
            ai.request_terminal_inline(
                config_result,
                messages,
                api_key_not_found,
                failed_to_get_key,
                stream_failed,
            );
        });
        cx.notify();
    }

    pub(in crate::workspace) fn regenerate_terminal_ai_inline_response(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.ai_entity.read(cx).terminal_inline_panel().loading {
            return;
        }
        self.ai_entity.update(cx, |ai, _cx| {
            let panel = ai.terminal_inline_panel_mut();
            panel.response.clear();
            panel.error = None;
        });
        self.send_terminal_ai_inline_prompt(cx);
    }

    pub(in crate::workspace) fn insert_terminal_ai_inline_response(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = extract_terminal_ai_inline_command(
            &self.ai_entity.read(cx).terminal_inline_panel().response,
        );
        if command.trim().is_empty() {
            return;
        }
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| {
                pane.send_ai_input_bytes(command.as_bytes(), cx);
            });
        }
        self.close_terminal_ai_inline_panel(window, cx);
    }

    pub(in crate::workspace) fn execute_terminal_ai_inline_response(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = extract_terminal_ai_inline_command(
            &self.ai_entity.read(cx).terminal_inline_panel().response,
        );
        if command.trim().is_empty() {
            return;
        }
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| {
                pane.begin_command_mark(
                    &command,
                    oxideterm_terminal::TerminalCommandMarkDetectionSource::Ai,
                    cx,
                );
                pane.send_command_line(&command, cx);
            });
        }
        self.close_terminal_ai_inline_panel(window, cx);
    }

    pub(in crate::workspace) fn copy_terminal_ai_inline_response(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let command = extract_terminal_ai_inline_command(
            &self.ai_entity.read(cx).terminal_inline_panel().response,
        );
        if command.trim().is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(command));
        let generation = self.ai_entity.update(cx, |ai, _cx| {
            let panel = ai.terminal_inline_panel_mut();
            panel.copied = true;
            panel.generation
        });
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(1500)).await;
            let _ = weak.update(cx, |this, cx| {
                let changed = this.ai_entity.update(cx, |ai, _cx| {
                    let panel = ai.terminal_inline_panel_mut();
                    if panel.generation == generation {
                        panel.copied = false;
                        true
                    } else {
                        false
                    }
                });
                if changed {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn refresh_terminal_ai_inline_key_status(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let config_result = self.resolve_terminal_ai_inline_config();
        self.ai_entity.update(cx, |ai, _cx| {
            ai.refresh_terminal_inline_key_status(config_result);
        });
    }

    pub(in crate::workspace) fn resolve_terminal_ai_inline_config(
        &self,
    ) -> Result<AiChatStreamConfig, String> {
        let settings = self.settings_store.settings();
        let providers = ai_provider_views(&settings.ai.providers);
        let provider = active_provider_view(&providers, settings.ai.active_provider_id.as_deref())
            .cloned()
            .ok_or_else(|| self.i18n.t("ai.model_selector.no_provider"))?;
        let model = active_model_selection(settings.ai.active_model.as_deref()).ok_or_else(|| {
            self.i18n.t("ai.model_selector.no_model_selected")
        })?;
        let reasoning_effort = settings
            .ai
            .reasoning_model_overrides
            .get(&provider.id)
            .and_then(|models| models.get(&model))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto");
        let reasoning_effort = oxideterm_ai::normalize_reasoning_level_for_model(
            &provider.provider_type,
            &model,
            reasoning_effort,
        )
        .as_str()
        .to_string();
        Ok(AiChatStreamConfig {
            execution_backend: AiExecutionBackend::Provider,
            provider_id: Some(provider.id.clone()),
            acp_agent_id: None,
            acp_session_id: None,
            acp_config_selection: None,
            provider_type: provider.provider_type,
            base_url: provider.base_url,
            model: model.clone(),
            api_key: None,
            max_response_tokens: ai_model_max_response_tokens(
                &settings.ai.model_max_response_tokens,
                &provider.id,
                &model,
            ),
            reasoning_effort: Some(reasoning_effort),
            safety_mode: AiPolicySafetyMode::Default,
            profile_id: None,
            memory_context: None,
            memory_entry_ids: Vec::new(),
            tool_policy: AiToolUsePolicy::default(),
            tools: Vec::new(),
            tool_choice: oxideterm_ai::AiToolChoice::Auto,
        })
    }
}

pub(in crate::workspace) fn inline_ai_keycap(
    tokens: &ThemeTokens,
    label: &'static str,
) -> AnyElement {
    div()
        .rounded(px(tokens.radii.sm))
        .bg(rgb(tokens.ui.bg_hover))
        .px(px(4.0))
        .py(px(1.0))
        .text_size(px(9.0))
        .child(label)
        .into_any_element()
}

pub(in crate::workspace) fn terminal_ai_inline_panel_placement(
    anchor: Option<oxideterm_gpui_terminal::TerminalCursorAnchor>,
    estimated_height: f32,
) -> AiInlinePanelPlacement {
    let Some(anchor) = anchor else {
        return AiInlinePanelPlacement {
            left: AI_INLINE_PANEL_MARGIN,
            top: AI_INLINE_PANEL_TOP,
        };
    };

    let mut top = anchor.y + anchor.line_height + AI_INLINE_PANEL_VERTICAL_OFFSET;
    let mut left = (anchor.container_width - AI_INLINE_PANEL_WIDTH) / 2.0;
    if left < AI_INLINE_PANEL_MARGIN {
        left = AI_INLINE_PANEL_MARGIN;
    } else if left + AI_INLINE_PANEL_WIDTH > anchor.container_width - AI_INLINE_PANEL_MARGIN {
        left = anchor.container_width - AI_INLINE_PANEL_WIDTH - AI_INLINE_PANEL_MARGIN;
    }

    if top + estimated_height > anchor.container_height - AI_INLINE_PANEL_MARGIN {
        top = anchor.y - estimated_height - AI_INLINE_PANEL_VERTICAL_OFFSET;
        if top < AI_INLINE_PANEL_MARGIN {
            top = AI_INLINE_PANEL_MARGIN;
        }
    }

    AiInlinePanelPlacement { left, top }
}

pub(in crate::workspace) fn terminal_ai_inline_os_context(
    tab: Option<&oxideterm_workspace::Tab>,
) -> String {
    let local_os = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "Unknown"
    };
    match tab.map(|tab| tab.kind.clone()) {
        Some(oxideterm_workspace::TabKind::SshTerminal) => {
            format!("SSH terminal (remote OS unknown, local: {local_os})")
        }
        Some(oxideterm_workspace::TabKind::MoshTerminal) => {
            format!("Mosh terminal (remote OS unknown, local: {local_os})")
        }
        _ => format!("Local terminal on {local_os}"),
    }
}

pub(in crate::workspace) fn terminal_ai_inline_messages(
    os_context: String,
    selection_context: String,
    prompt: String,
) -> Vec<AiChatMessage> {
    let mut user_content = String::new();
    if !selection_context.trim().is_empty() {
        user_content.push_str("### Context (Selected Text):\n");
        user_content.push_str(&selection_context);
        user_content.push_str("\n\n");
    }
    user_content.push_str("### Question/Instruction:\n");
    user_content.push_str(&prompt);

    vec![
        AiChatMessage {
            id: "terminal-inline-system".to_string(),
            role: AiChatRole::System,
            content: format!(
                "You are OxideSens, an expert terminal assistant. Environment: {os_context}. Respond ONLY with the command or code itself unless asked for explanation. If asked which AI model you are, answer truthfully."
            ),
            timestamp_ms: 0,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
        AiChatMessage {
            id: "terminal-inline-user".to_string(),
            role: AiChatRole::User,
            content: user_content,
            timestamp_ms: 0,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
    ]
}

pub(in crate::workspace) fn truncate_ai_inline_context(
    mut context: String,
    max_chars: i64,
) -> String {
    let max_chars = usize::try_from(max_chars).unwrap_or_default();
    if max_chars == 0 || context.chars().count() <= max_chars {
        return context;
    }
    let keep_from = context
        .char_indices()
        .rev()
        .nth(max_chars.saturating_sub(1))
        .map(|(index, _)| index)
        .unwrap_or_default();
    context.drain(..keep_from);
    context
}

pub(in crate::workspace) fn extract_terminal_ai_inline_command(text: &str) -> String {
    if let Some(command) = extract_fenced_code_block(text) {
        return command.trim().to_string();
    }
    if let Some(command) = extract_inline_code(text) {
        return command.trim().to_string();
    }
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            line.trim()
                .strip_prefix("$ ")
                .or_else(|| line.trim().strip_prefix("> "))
                .unwrap_or_else(|| line.trim())
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| text.trim().to_string())
}

pub(in crate::workspace) fn extract_fenced_code_block(text: &str) -> Option<&str> {
    let start = text.find("```")?;
    let after_start = &text[start + 3..];
    let content_start = after_start
        .find('\n')
        .map(|index| index + 1)
        .unwrap_or_default();
    let content = &after_start[content_start..];
    let end = content.find("```")?;
    Some(&content[..end])
}

pub(in crate::workspace) fn extract_inline_code(text: &str) -> Option<&str> {
    let start = text.find('`')?;
    let rest = &text[start + 1..];
    let end = rest.find('`')?;
    Some(&rest[..end])
}

#[cfg(test)]
mod terminal_inline_tests {
    use oxideterm_gpui_terminal::TerminalCursorAnchor;

    use super::{
        AI_INLINE_PANEL_COLLAPSED_HEIGHT, AI_INLINE_PANEL_EXPANDED_HEIGHT,
        extract_terminal_ai_inline_command, terminal_ai_inline_panel_placement,
    };

    #[test]
    pub(in crate::workspace) fn extracts_multiline_fenced_command() {
        let command = extract_terminal_ai_inline_command("```bash\nmkdir demo\ncd demo\n```");
        assert_eq!(command, "mkdir demo\ncd demo");
    }

    #[test]
    pub(in crate::workspace) fn strips_shell_prompt_from_first_non_empty_line() {
        assert_eq!(
            extract_terminal_ai_inline_command("\n$ cargo test\nexplanation"),
            "cargo test",
        );
    }


    #[test]
    pub(in crate::workspace) fn places_panel_below_cursor_when_space_allows() {
        let placement = terminal_ai_inline_panel_placement(
            Some(TerminalCursorAnchor {
                x: 80.0,
                y: 100.0,
                line_height: 20.0,
                char_width: 8.0,
                container_width: 800.0,
                container_height: 600.0,
            }),
            AI_INLINE_PANEL_COLLAPSED_HEIGHT,
        );
        assert_eq!(placement.left, 140.0);
        assert_eq!(placement.top, 124.0);
    }

    #[test]
    pub(in crate::workspace) fn flips_panel_above_cursor_near_bottom() {
        let placement = terminal_ai_inline_panel_placement(
            Some(TerminalCursorAnchor {
                x: 80.0,
                y: 560.0,
                line_height: 20.0,
                char_width: 8.0,
                container_width: 800.0,
                container_height: 600.0,
            }),
            AI_INLINE_PANEL_EXPANDED_HEIGHT,
        );
        assert_eq!(placement.left, 140.0);
        assert_eq!(placement.top, 396.0);
    }
}
