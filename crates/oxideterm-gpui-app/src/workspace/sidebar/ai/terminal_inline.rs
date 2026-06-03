use std::sync::mpsc::{Receiver, TryRecvError};

const AI_INLINE_PANEL_WIDTH: f32 = 520.0;
const AI_INLINE_PANEL_TOP: f32 = 48.0;
const AI_INLINE_PANEL_MARGIN: f32 = 12.0;
const AI_INLINE_PANEL_VERTICAL_OFFSET: f32 = 4.0;
const AI_INLINE_PANEL_COLLAPSED_HEIGHT: f32 = 56.0;
const AI_INLINE_PANEL_EXPANDED_HEIGHT: f32 = 160.0;
const AI_INLINE_PANEL_LOADING_BAR_HEIGHT: f32 = 2.0;
const AI_INLINE_POLL_INTERVAL_MS: u64 = 50;
const AI_INLINE_MAX_EVENTS_PER_POLL: usize = 128;

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
    pub(in crate::workspace) rx: Option<Receiver<AiInlinePanelDelivery>>,
    pub(in crate::workspace) polling: bool,
}

pub(in crate::workspace) enum AiInlinePanelDelivery {
    KeyStatus {
        generation: u64,
        has_key: bool,
    },
    Content {
        generation: u64,
        chunk: String,
    },
    Done {
        generation: u64,
    },
    Error {
        generation: u64,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AiInlinePanelPlacement {
    left: f32,
    top: f32,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn toggle_terminal_ai_inline_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai_inline_panel.open {
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
        self.terminal_command_bar_focused = false;
        self.close_terminal_command_overlays(cx);
        self.close_ai_model_selector();
        self.ai_inline_panel.open = true;
        self.ai_inline_panel.prompt.clear();
        self.ai_inline_panel.response.clear();
        self.ai_inline_panel.error = None;
        self.ai_inline_panel.loading = false;
        self.ai_inline_panel.copied = false;
        self.ai_inline_panel.prompt_focused = true;
        self.ai_inline_panel.has_api_key = None;
        self.ai_inline_panel.generation = self.ai_inline_panel.generation.wrapping_add(1);
        self.ai_inline_panel.rx = None;
        self.ai_inline_panel.polling = false;

        let selection = self
            .active_pane()
            .and_then(|pane| pane.read(cx).selected_text_snapshot())
            .unwrap_or_default();
        let sanitized_selection = truncate_ai_inline_context(
            oxideterm_ai::sanitize_for_ai(&selection),
            self.settings_store.settings().ai.context_max_chars,
        );
        self.ai_inline_panel.has_selection = !sanitized_selection.trim().is_empty();
        self.ai_inline_panel.selection_context = sanitized_selection;

        window.focus(&self.focus_handle);
        self.refresh_terminal_ai_inline_key_status(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_ai_inline_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_inline_panel.open = false;
        self.ai_inline_panel.prompt_focused = false;
        self.ai_inline_panel.loading = false;
        self.ai_inline_panel.error = None;
        self.ai_inline_panel.rx = None;
        self.ai_inline_panel.polling = false;
        self.ai_inline_panel.generation = self.ai_inline_panel.generation.wrapping_add(1);
        self.ime_marked_text = None;
        self.close_ai_model_selector();
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn handle_ai_inline_panel_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ai_inline_panel.open || event.keystroke.modifiers.platform {
            return false;
        }
        if self.ai_model_selector_open
            && self.ai_model_selector_scope == Some(AiModelSelectorScope::TerminalInline)
            && self.ai_model_selector_search_focused
        {
            return self.handle_ai_sidebar_key(event, cx);
        }

        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_terminal_ai_inline_panel(window, cx);
                true
            }
            "enter" if self.marked_text_for_target(WorkspaceImeTarget::AiInlinePrompt).is_some() => {
                true
            }
            "enter" if !event.keystroke.modifiers.shift => {
                if self.ai_inline_panel.loading {
                    return true;
                }
                if self.ai_inline_panel.response.trim().is_empty() {
                    self.send_terminal_ai_inline_prompt(cx);
                } else {
                    self.execute_terminal_ai_inline_response(window, cx);
                }
                true
            }
            "tab" if !self.ai_inline_panel.response.trim().is_empty() && !self.ai_inline_panel.loading => {
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
        if !self.ai_inline_panel.open {
            return div().into_any_element();
        }

        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::AiInlinePrompt;
        let focused = self.ai_inline_panel.prompt_focused;
        let marked_text = self.marked_text_for_target(target);
        let selected_range = self.ime_selected_range_for_target(target);
        let showing_placeholder =
            self.ai_inline_panel.prompt.is_empty() && marked_text.is_none();
        let placeholder = if self.ai_inline_panel.has_selection {
            self.i18n.t("terminal.ai.selection_placeholder")
        } else {
            self.i18n.t("terminal.ai.inline_placeholder")
        };
        let prompt_text = if showing_placeholder {
            placeholder
        } else {
            self.ai_inline_panel.prompt.clone()
        };
        let prompt_range = selected_range
            .clone()
            .filter(|_| focused && !self.ai_inline_panel.prompt.is_empty() && marked_text.is_none());
        let selection_range = prompt_range.clone().filter(|range| range.start < range.end);
        let caret_offset = prompt_range
            .as_ref()
            .filter(|range| range.start == range.end)
            .map(|range| range.start);
        let response_command = extract_terminal_ai_inline_command(&self.ai_inline_panel.response);
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
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_elevated))
                    .shadow_lg()
                    .when(self.ai_inline_panel.loading, |panel| {
                        panel.child(
                            div()
                                .absolute()
                                .top(px(0.0))
                                .left(px(0.0))
                                .right(px(0.0))
                                .h(px(AI_INLINE_PANEL_LOADING_BAR_HEIGHT))
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
                                                this.ai_inline_panel.prompt_focused = true;
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
                                        )
                                        .when(focused && showing_placeholder, |input| {
                                            input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
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
                                                self.new_connection_caret_visible,
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
                                                    self.new_connection_caret_visible,
                                                ))
                                            },
                                        ),
                                    Self::deferred_ai_text_input_anchor_update(workspace.clone()),
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
                        self.ai_inline_panel.has_api_key == Some(false) && !self.ai_inline_panel.loading,
                        |panel| panel.child(self.render_terminal_ai_inline_notice(
                            LucideIcon::AlertCircle,
                            self.i18n.t("terminal.ai.api_key_hint"),
                            rgba((0xf59e0b << 8) | 0x1a),
                            rgba((0xf59e0b << 8) | 0x4d),
                            rgba(0xfbbf24ff),
                        )),
                    )
                    .when_some(self.ai_inline_panel.error.as_ref(), |panel, error| {
                        panel.child(self.render_terminal_ai_inline_notice(
                            LucideIcon::AlertCircle,
                            error.clone(),
                            rgba((0xef4444 << 8) | 0x1a),
                            rgba((0xef4444 << 8) | 0x4d),
                            rgba(0xfca5a5ff),
                        ))
                    })
                    .when(
                        (self.ai_inline_panel.loading || !self.ai_inline_panel.response.is_empty())
                            && self.ai_inline_panel.error.is_none(),
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
                                        !self.ai_inline_panel.response.is_empty()
                                            && !self.ai_inline_panel.loading,
                                        |preview| {
                                            preview.child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(4.0))
                                                    .border_t_1()
                                                    .border_color(rgb(theme.border))
                                                    .bg(rgb(theme.bg_elevated))
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
                                                        if self.ai_inline_panel.copied {
                                                            LucideIcon::Check
                                                        } else {
                                                            LucideIcon::Copy
                                                        },
                                                        if self.ai_inline_panel.copied {
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
                        self.ai_model_selector_open
                            && self.ai_model_selector_scope
                                == Some(AiModelSelectorScope::TerminalInline),
                        |panel| {
                        panel.child(
                            div()
                                .absolute()
                                .top(px(40.0))
                                .left(px(32.0))
                                .child(self.render_ai_model_selector_dropdown(
                                    &ai_provider_views(&self.settings_store.settings().ai.providers),
                                    cx,
                                )),
                        )
                    }),
            )
            .into_any_element()
    }

    fn terminal_ai_inline_panel_placement(&self, cx: &mut Context<Self>) -> AiInlinePanelPlacement {
        let expanded = self.ai_inline_panel.loading
            || self.ai_inline_panel.error.is_some()
            || !self.ai_inline_panel.response.is_empty();
        let estimated_height = if expanded {
            AI_INLINE_PANEL_EXPANDED_HEIGHT
        } else {
            AI_INLINE_PANEL_COLLAPSED_HEIGHT
        };
        let anchor = self.active_pane().and_then(|pane| pane.read(cx).cursor_anchor());
        terminal_ai_inline_panel_placement(anchor, estimated_height)
    }

    fn render_terminal_ai_inline_hints(&self, _cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(10.0))
            .text_color(rgb(theme.text_muted))
            .when(
                self.ai_inline_panel.response.trim().is_empty()
                    && !self.ai_inline_panel.loading
                    && !self.ai_inline_panel.prompt.trim().is_empty(),
                |hints| {
                    hints
                        .child(inline_ai_keycap(&self.tokens, "Enter"))
                        .child(self.i18n.t("terminal.ai.to_send"))
                },
            )
            .when(
                !self.ai_inline_panel.response.trim().is_empty() && !self.ai_inline_panel.loading,
                |hints| {
                    hints
                        .child(inline_ai_keycap(&self.tokens, "Tab"))
                        .child(self.i18n.t("terminal.ai.to_insert"))
                        .child(inline_ai_keycap(&self.tokens, "Enter"))
                        .child(self.i18n.t("terminal.ai.to_run"))
                },
            )
            .into_any_element()
    }

    fn render_terminal_ai_inline_notice(
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

    fn render_terminal_ai_inline_action(
        &self,
        icon: LucideIcon,
        label: String,
        primary: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let fg = if primary { rgb(0xffffff) } else { rgb(theme.text) };
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(4.0))
            .text_size(px(11.0))
            .text_color(fg)
            .bg(if primary { rgb(theme.accent) } else { rgba(0x00000000) })
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

    fn send_terminal_ai_inline_prompt(&mut self, cx: &mut Context<Self>) {
        if self.ai_inline_panel.loading || self.ai_inline_panel.prompt.trim().is_empty() {
            return;
        }

        let generation = self.ai_inline_panel.generation.wrapping_add(1);
        self.ai_inline_panel.generation = generation;
        self.ai_inline_panel.response.clear();
        self.ai_inline_panel.error = None;
        self.ai_inline_panel.copied = false;
        self.ai_inline_panel.loading = true;
        self.ai_inline_panel.has_api_key = None;

        let mut config = match self.resolve_terminal_ai_inline_config() {
            Ok(config) => config,
            Err(message) => {
                self.ai_inline_panel.loading = false;
                self.ai_inline_panel.error = Some(message);
                cx.notify();
                return;
            }
        };
        let requires_key = ai_provider_chat_requires_key(&config.provider_type);
        let provider_id = config.provider_id.clone();
        let prompt = oxideterm_ai::sanitize_for_ai(&self.ai_inline_panel.prompt);
        let selection = self.ai_inline_panel.selection_context.clone();
        let messages = terminal_ai_inline_messages(
            terminal_ai_inline_os_context(self.active_tab()),
            selection,
            prompt,
        );
        let key_store = self.ai_key_store.clone();
        let api_key_not_found = self.i18n.t("ai.model_selector.api_key_not_found");
        let failed_to_get_key = self.i18n.t("ai.model_selector.failed_to_get_api_key");
        let (ui_tx, ui_rx) = std::sync::mpsc::channel();
        self.ai_inline_panel.rx = Some(ui_rx);
        self.schedule_terminal_ai_inline_poll(cx);
        self.forwarding_runtime.spawn(async move {
            if let Some(provider_id) = provider_id {
                let key_result = tokio::task::spawn_blocking(move || key_store.get_provider_key(&provider_id))
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|result| result.map_err(|error| error.to_string()));
                match key_result {
                    Ok(api_key) => {
                        let has_key = api_key.as_ref().is_some_and(|key| !key.trim().is_empty());
                        let _ = ui_tx.send(AiInlinePanelDelivery::KeyStatus { generation, has_key });
                        if requires_key && !has_key {
                            let _ = ui_tx.send(AiInlinePanelDelivery::Error {
                                generation,
                                message: api_key_not_found,
                            });
                            return;
                        }
                        config.api_key = api_key;
                    }
                    Err(_) if requires_key => {
                        let _ = ui_tx.send(AiInlinePanelDelivery::Error {
                            generation,
                            message: failed_to_get_key,
                        });
                        return;
                    }
                    Err(_) => {}
                }
            }

            let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(stream_chat_completion(
                config,
                oxideterm_ai::sanitize_api_messages_for_provider(messages),
                stream_tx,
            ));
            while let Some(event) = stream_rx.recv().await {
                match event {
                    AiStreamEvent::Content(chunk) => {
                        let _ = ui_tx.send(AiInlinePanelDelivery::Content { generation, chunk });
                    }
                    AiStreamEvent::Done => {
                        let _ = ui_tx.send(AiInlinePanelDelivery::Done { generation });
                        break;
                    }
                    AiStreamEvent::Error(message) => {
                        let _ = ui_tx.send(AiInlinePanelDelivery::Error { generation, message });
                        break;
                    }
                    AiStreamEvent::Thinking(_)
                    | AiStreamEvent::ToolCall { .. }
                    | AiStreamEvent::ToolCallComplete { .. } => {}
                }
            }
        });
        cx.notify();
    }

    fn regenerate_terminal_ai_inline_response(&mut self, cx: &mut Context<Self>) {
        if self.ai_inline_panel.loading {
            return;
        }
        self.ai_inline_panel.response.clear();
        self.ai_inline_panel.error = None;
        self.send_terminal_ai_inline_prompt(cx);
    }

    fn insert_terminal_ai_inline_response(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = extract_terminal_ai_inline_command(&self.ai_inline_panel.response);
        if command.trim().is_empty() {
            return;
        }
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| {
                pane.send_ai_input_bytes(command.as_bytes(), cx);
            });
        }
        self.close_terminal_ai_inline_panel(window, cx);
    }

    fn execute_terminal_ai_inline_response(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = extract_terminal_ai_inline_command(&self.ai_inline_panel.response);
        if command.trim().is_empty() {
            return;
        }
        if let Some(pane) = self.active_pane() {
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

    fn copy_terminal_ai_inline_response(&mut self, cx: &mut Context<Self>) {
        let command = extract_terminal_ai_inline_command(&self.ai_inline_panel.response);
        if command.trim().is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(command));
        self.ai_inline_panel.copied = true;
        let generation = self.ai_inline_panel.generation;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(1500)).await;
            let _ = weak.update(cx, |this, cx| {
                if this.ai_inline_panel.generation == generation {
                    this.ai_inline_panel.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn refresh_terminal_ai_inline_key_status(&mut self, cx: &mut Context<Self>) {
        let Ok(config) = self.resolve_terminal_ai_inline_config() else {
            self.ai_inline_panel.has_api_key = Some(false);
            return;
        };
        let requires_key = ai_provider_chat_requires_key(&config.provider_type);
        let Some(provider_id) = config.provider_id else {
            self.ai_inline_panel.has_api_key = Some(!requires_key);
            return;
        };
        if !requires_key {
            self.ai_inline_panel.has_api_key = Some(true);
            return;
        }
        let generation = self.ai_inline_panel.generation;
        let key_store = self.ai_key_store.clone();
        let (ui_tx, ui_rx) = std::sync::mpsc::channel();
        self.ai_inline_panel.rx = Some(ui_rx);
        self.schedule_terminal_ai_inline_poll(cx);
        self.forwarding_runtime.spawn(async move {
            // Opening the inline panel only needs the key existence hint; reading
            // the secret here would trigger Touch ID before the user sends a prompt.
            let has_key =
                tokio::task::spawn_blocking(move || key_store.has_provider_key(&provider_id))
                    .await
                    .unwrap_or(false);
            let _ = ui_tx.send(AiInlinePanelDelivery::KeyStatus {
                generation,
                has_key,
            });
        });
    }

    fn poll_terminal_ai_inline_delivery(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.ai_inline_panel.rx.take() else {
            self.ai_inline_panel.polling = false;
            return;
        };
        let mut keep_rx = true;
        let mut processed = 0;
        loop {
            if processed >= AI_INLINE_MAX_EVENTS_PER_POLL {
                break;
            }
            processed += 1;
            match rx.try_recv() {
                Ok(delivery) => self.apply_terminal_ai_inline_delivery(delivery, &mut keep_rx, cx),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    keep_rx = false;
                    self.ai_inline_panel.loading = false;
                    break;
                }
            }
        }
        if keep_rx && self.ai_inline_panel.open {
            self.ai_inline_panel.rx = Some(rx);
        } else {
            self.ai_inline_panel.polling = false;
        }
        cx.notify();
    }

    fn apply_terminal_ai_inline_delivery(
        &mut self,
        delivery: AiInlinePanelDelivery,
        keep_rx: &mut bool,
        _cx: &mut Context<Self>,
    ) {
        match delivery {
            AiInlinePanelDelivery::KeyStatus { generation, has_key }
                if generation == self.ai_inline_panel.generation =>
            {
                self.ai_inline_panel.has_api_key = Some(has_key);
            }
            AiInlinePanelDelivery::Content { generation, chunk }
                if generation == self.ai_inline_panel.generation =>
            {
                self.ai_inline_panel.response.push_str(&chunk);
            }
            AiInlinePanelDelivery::Done { generation }
                if generation == self.ai_inline_panel.generation =>
            {
                self.ai_inline_panel.loading = false;
                *keep_rx = false;
            }
            AiInlinePanelDelivery::Error { generation, message }
                if generation == self.ai_inline_panel.generation =>
            {
                self.ai_inline_panel.loading = false;
                self.ai_inline_panel.error = Some(message);
                *keep_rx = false;
            }
            _ => {}
        }
    }

    fn schedule_terminal_ai_inline_poll(&mut self, cx: &mut Context<Self>) {
        if self.ai_inline_panel.polling {
            return;
        }
        self.ai_inline_panel.polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_millis(AI_INLINE_POLL_INTERVAL_MS)).await;
                let keep_polling = weak
                    .update(cx, |this, cx| {
                        this.poll_terminal_ai_inline_delivery(cx);
                        this.ai_inline_panel.polling
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    fn resolve_terminal_ai_inline_config(&self) -> Result<AiChatStreamConfig, String> {
        let settings = self.settings_store.settings();
        let providers = ai_provider_views(&settings.ai.providers);
        let provider = active_provider_view(&providers, settings.ai.active_provider_id.as_deref())
            .cloned()
            .ok_or_else(|| self.i18n.t("ai.model_selector.no_provider"))?;
        let model = active_model_or_provider_default(settings.ai.active_model.as_deref(), &provider)
            .ok_or_else(|| {
                "No model selected. Please refresh models or select one in Settings > AI.".to_string()
            })?;
        Ok(AiChatStreamConfig {
            provider_id: Some(provider.id.clone()),
            provider_type: provider.provider_type,
            base_url: provider.base_url,
            model: model.clone(),
            api_key: None,
            max_response_tokens: ai_model_max_response_tokens(
                &settings.ai.model_max_response_tokens,
                &provider.id,
                &model,
            ),
            reasoning_effort: Some(resolve_terminal_ai_inline_reasoning_effort(settings)),
            safety_mode: AiPolicySafetyMode::Default,
            profile_id: None,
            tool_policy: AiToolUsePolicy::default(),
            tools: Vec::new(),
            tool_choice: oxideterm_ai::AiToolChoice::Auto,
        })
    }
}

fn inline_ai_keycap(tokens: &ThemeTokens, label: &'static str) -> AnyElement {
    div()
        .rounded(px(tokens.radii.sm))
        .bg(rgb(tokens.ui.bg_hover))
        .px(px(4.0))
        .py(px(1.0))
        .text_size(px(9.0))
        .child(label)
        .into_any_element()
}

fn terminal_ai_inline_panel_placement(
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

fn terminal_ai_inline_os_context(tab: Option<&oxideterm_workspace::Tab>) -> String {
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
        _ => format!("Local terminal on {local_os}"),
    }
}

fn terminal_ai_inline_messages(
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

fn truncate_ai_inline_context(mut context: String, max_chars: i64) -> String {
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

fn resolve_terminal_ai_inline_reasoning_effort(settings: &PersistedSettings) -> String {
    let value = serde_json::to_value(settings.ai.reasoning_effort)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "auto".to_string());
    match value.as_str() {
        "none" | "minimal" => "off".to_string(),
        "xhigh" => "max".to_string(),
        other => other.to_string(),
    }
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

fn extract_fenced_code_block(text: &str) -> Option<&str> {
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

fn extract_inline_code(text: &str) -> Option<&str> {
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
        truncate_ai_inline_context,
    };

    #[test]
    fn extracts_multiline_fenced_command() {
        let command = extract_terminal_ai_inline_command("```bash\nmkdir demo\ncd demo\n```");
        assert_eq!(command, "mkdir demo\ncd demo");
    }

    #[test]
    fn strips_shell_prompt_from_first_non_empty_line() {
        assert_eq!(
            extract_terminal_ai_inline_command("\n$ cargo test\nexplanation"),
            "cargo test",
        );
    }

    #[test]
    fn truncates_context_from_the_end_like_tauri_selection_context() {
        assert_eq!(truncate_ai_inline_context("abcdef".to_string(), 3), "def");
    }

    #[test]
    fn places_panel_below_cursor_when_space_allows() {
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
    fn flips_panel_above_cursor_near_bottom() {
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
