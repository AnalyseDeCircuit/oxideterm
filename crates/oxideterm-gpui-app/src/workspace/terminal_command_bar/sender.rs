// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

// Hallmark · pre-emit critique: P4 H5 E4 S5 R5 V4
// Hallmark · macrostructure: Workbench · genre: modern-minimal · tone: technical and restrained

use super::*;
use crate::workspace::terminal_command_sender::{
    CompactSuggestionMode, TERMINAL_SENDER_MIN_HEIGHT, TERMINAL_SENDER_RESIZE_HOTZONE_HEIGHT,
    TerminalCommandSenderDocumentSnapshot, TerminalCommandSenderFailure, TerminalCommandSenderId,
    TerminalCommandSenderStatus, TerminalCommandSenderTarget, TerminalCommandSenderTargetScope,
};
use oxideterm_editor_core::utf16::byte_index_for_utf16;
use oxideterm_terminal::{TerminalSenderInputMode, TerminalSenderPacing};
use zeroize::Zeroizing;

const TERMINAL_SENDER_CONTROL_HEIGHT: f32 = 26.0;
const TERMINAL_SENDER_PANEL_PADDING: f32 = 12.0;
const TERMINAL_SENDER_COMPACT_HORIZONTAL_PADDING: f32 = 12.0;
const TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT: f32 = 24.0;
const TERMINAL_SENDER_COMPACT_BACKGROUND_ALPHA: u32 = 0xf2;
const TERMINAL_SENDER_COMPACT_BORDER_ALPHA: u32 = 0x73;
const TERMINAL_SENDER_INTERVAL_STEP_LINE_MS: i64 = 100;
const TERMINAL_SENDER_INTERVAL_STEP_CHARACTER_MS: i64 = 20;
const TERMINAL_SENDER_TARGET_LIST_MIN_WIDTH: f32 = 160.0;
const TERMINAL_SENDER_TARGET_CONTEXT_MAX_WIDTH: f32 = 220.0;
const TERMINAL_SENDER_PROGRESS_HEIGHT: f32 = 2.0;
const QUICK_BAR_COMMAND_MAX_WIDTH: f32 = 220.0;
const QUICK_BAR_PREVIEW_MAX_CHARS: usize = 600;
const QUICK_BAR_PREVIEW_MAX_LINES: usize = 8;
const QUICK_BAR_PREVIEW_MAX_WIDTH: f32 = 480.0;

struct QuickCommandPreview {
    tokens: ThemeTokens,
    text: Zeroizing<String>,
}

impl Render for QuickCommandPreview {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Keep command text out of tooltip animation identities and diagnostic state.
        oxideterm_gpui_ui::tooltip::tooltip_surface(&self.tokens, self.text.to_string(), None)
            .max_w(px(
                QUICK_BAR_PREVIEW_MAX_WIDTH.min(f32::from(window.viewport_size().width))
            ))
    }
}

fn quick_command_preview(name: &str, command: &str) -> Zeroizing<String> {
    let mut preview = Zeroizing::new(format!("{name}\n"));
    let mut lines = 1;
    for (index, ch) in command.chars().enumerate() {
        if index == QUICK_BAR_PREVIEW_MAX_CHARS
            || (ch == '\n' && lines == QUICK_BAR_PREVIEW_MAX_LINES)
        {
            preview.push('…');
            break;
        }
        preview.push(ch);
        if ch == '\n' {
            lines += 1;
        }
    }
    preview
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_terminal_command_sender_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender = self.terminal_command_sender.read(cx);
        let target_height = sender.panel_height();
        let mut tokens = self.tokens;
        if sender.is_resizing() {
            tokens.motion.spatial_enabled = false;
        }
        let allow_overflow = target_height
            >= if sender.is_expanded() {
                TERMINAL_SENDER_MIN_HEIGHT
            } else {
                TERMINAL_SENDER_COMPACT_HEIGHT
            };
        let reveal = oxideterm_gpui_ui::motion::auto_height(
            &tokens,
            "terminal-command-sender-height",
            Some(self.render_terminal_command_sender_content(window, cx)),
        )
        .target_height(target_height);
        if allow_overflow {
            reveal.overflow_when_settled().into_any_element()
        } else {
            reveal.into_any_element()
        }
    }

    fn render_terminal_command_sender_content(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let sender_expanded = self.terminal_command_sender.read(cx).is_expanded();
        let Some(active) = self
            .terminal_command_sender
            .read(cx)
            .active_document_snapshot()
        else {
            return div().into_any_element();
        };
        if !sender_expanded {
            return self.render_compact_terminal_command_sender(&active, window, cx);
        }
        let panel_height = self.terminal_command_sender.read(cx).panel_height();
        let documents = self.terminal_command_sender.read(cx).document_snapshots();
        let targets = self.terminal_command_sender_target_entries(cx);
        let sender_is_resizing = self.terminal_command_sender.read(cx).is_resizing();
        let has_background = self.window_background_preferences().is_some();

        let panel = div()
            .relative()
            .when(panel_height < TERMINAL_SENDER_MIN_HEIGHT, |panel| {
                panel.overflow_hidden()
            })
            .flex_none()
            .h(px(panel_height))
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(self.workspace_chrome_divider())
            .bg(if has_background {
                rgba(
                    (theme.bg << 8)
                        | oxideterm_gpui_ui::surface::theme_glass_card_background_alpha(
                            &self.tokens,
                        ),
                )
            } else {
                rgb(theme.bg)
            })
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_terminal_command_sender_resize(event, window, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.finish_terminal_command_sender_resize(cx);
                }),
            )
            .child(
                div()
                    .id("terminal-command-sender-resize")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(TERMINAL_SENDER_RESIZE_HOTZONE_HEIGHT))
                    .flex()
                    .items_start()
                    .justify_center()
                    .pt(px(2.0))
                    .cursor(CursorStyle::ResizeRow)
                    .occlude()
                    .child(
                        div()
                            .w(px(28.0))
                            .h(px(2.0))
                            .rounded_full()
                            .bg(rgba((theme.text_muted << 8) | 0x38)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            if event.click_count >= 2 {
                                this.terminal_command_sender.update(cx, |sender, cx| {
                                    sender.reset_height(cx);
                                });
                            } else {
                                this.terminal_command_sender.update(cx, |sender, cx| {
                                    sender.start_resize(event.position.y, cx);
                                });
                            }
                            window.prevent_default();
                            cx.stop_propagation();
                        }),
                    ),
            )
            .when(sender_is_resizing, |panel| {
                // Keep pointer capture inside the sender surface while its divider moves.
                panel.cursor(CursorStyle::ResizeRow)
            })
            .child(self.render_terminal_command_sender_tabs(&documents, active.id, window, cx))
            .child({
                let active_id = active.id;
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .bg(rgba(
                        (theme.bg_sunken << 8) | if has_background { 0x6b } else { 0xff },
                    ))
                    .px(px(TERMINAL_SENDER_PANEL_PADDING))
                    .py(px(8.0))
                    .overflow_hidden()
                    .cursor_text()
                    .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        // The editor consumes the click before the workspace blur handler.
                        if event.button == MouseButton::Left && this.blur_host_tools_input(cx) {
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.focus_terminal_command_sender_editor(active_id, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(active.editor.clone())
            })
            .child(self.render_terminal_command_sender_controls(&active, &targets, cx))
            .into_any_element();
        if has_background {
            oxideterm_gpui_ui::surface::tab_surface(&self.tokens, panel)
        } else {
            panel
        }
    }

    fn render_compact_terminal_command_sender(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::TerminalCommandSenderCompact;
        let sender = self.terminal_command_sender.read(cx);
        let focused = sender.compact_focused();
        let draft = Zeroizing::new(
            sender
                .active_compact_draft()
                .unwrap_or_default()
                .to_string(),
        );
        let viewport = sender.active_compact_viewport().unwrap_or_default();
        let placeholder = sender.compact_placeholder().to_string();
        let suggestion_mode = sender.compact_suggestion_mode();
        let suggestion_highlighted = sender.compact_suggestion_highlighted();
        let suggestions = if focused {
            self.terminal_command_sender_compact_suggestions(snapshot, suggestion_mode, cx)
        } else {
            Vec::new()
        };
        let show_history = suggestion_mode == CompactSuggestionMode::History
            && Self::terminal_command_sender_supports_suggestions(snapshot);
        let show_suggestions = focused && (show_history || !suggestions.is_empty());
        let selected_range = self.ime_selected_range_for_target(target, cx);
        let marked_text = self.marked_text_for_target(target, cx);
        let active_offset = self.ime_active_offset_for_target(target, cx);
        let ghost_text = (focused && !show_history && suggestion_highlighted.is_none())
            .then(|| self.terminal_command_sender_compact_ghost_text(snapshot, &draft, cx))
            .flatten();
        let quick_commands_enabled = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_commands_enabled;
        let quick_commands_open = self.terminal.read(cx).quick_commands.is_open();
        let background = if self.window_background_preferences().is_some() {
            self.workspace_chrome_background(theme.bg)
        } else {
            rgba((theme.bg << 8) | TERMINAL_SENDER_COMPACT_BACKGROUND_ALPHA)
        };
        let terminal_settings = &self.settings_store.settings().terminal;
        let workspace = cx.entity();
        let compact_input = text_input_anchor_probe(
            target.anchor_id(),
            text_input_with_viewport_and_ghost_text(
                &self.tokens,
                TextInputView {
                    value: &draft,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range,
                    marked_text,
                },
                &viewport,
                active_offset,
                ghost_text.as_deref(),
            )
            .h(px(TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT))
            .px_0()
            .border_0()
            .rounded_none()
            .bg(rgba(0x00000000))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(terminal_settings.font_size as f32))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.focus_compact_terminal_command_sender(window, cx);
                    this.ime_marked_text = None;
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        );

        div()
            .relative()
            .flex_none()
            .h(px(TERMINAL_SENDER_COMPACT_HEIGHT))
            .px(px(TERMINAL_SENDER_COMPACT_HORIZONTAL_PADDING))
            .py(px(4.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_t_1()
            .border_color(rgba(
                ((if focused { theme.accent } else { theme.border }) << 8)
                    | TERMINAL_SENDER_COMPACT_BORDER_ALPHA,
            ))
            .bg(background)
            .when(show_suggestions, |row| {
                row.child(self.render_terminal_command_sender_suggestions(
                    &suggestions,
                    suggestion_highlighted,
                    show_history,
                    cx,
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .overflow_hidden()
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.focus_compact_terminal_command_sender(window, cx);
                            this.ime_marked_text = None;
                            this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .size(px(TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Self::render_lucide_icon(
                                LucideIcon::ChevronRight,
                                16.0,
                                rgb(theme.text_muted),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h(px(TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT))
                            .overflow_hidden()
                            .child(compact_input),
                    ),
            )
            .when(quick_commands_enabled, |row| {
                row.child(
                    div()
                        .id("terminal-command-quick-commands-compact")
                        .flex_none()
                        .size(px(TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.md))
                        .cursor_pointer()
                        .bg(if quick_commands_open {
                            rgba((theme.accent << 8) | 0x1a)
                        } else {
                            rgba(0x00000000)
                        })
                        .hover(move |style| style.bg(rgb(theme.bg_hover)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, window, cx| {
                                this.toggle_terminal_quick_commands_panel(window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .child(Self::render_lucide_icon(
                            LucideIcon::Zap,
                            14.0,
                            if quick_commands_open {
                                rgb(theme.accent)
                            } else {
                                rgb(theme.text_muted)
                            },
                        )),
                )
            })
            .into_any_element()
    }

    fn terminal_command_sender_compact_ghost_text(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        draft: &str,
        cx: &App,
    ) -> Option<String> {
        if snapshot.input_mode != TerminalSenderInputMode::Text
            || snapshot.target_scope != TerminalCommandSenderTargetScope::Current
            || snapshot.status == TerminalCommandSenderStatus::Running
            || draft.is_empty()
            || draft
                .chars()
                .any(|character| matches!(character, '\n' | '\r'))
        {
            return None;
        }
        self.active_pane(cx)
            .and_then(|pane| pane.read(cx).history_ghost_text_for_input(draft))
            .filter(|suffix| {
                !suffix
                    .chars()
                    .any(|character| matches!(character, '\n' | '\r'))
            })
    }

    fn terminal_command_sender_supports_suggestions(
        snapshot: &TerminalCommandSenderDocumentSnapshot,
    ) -> bool {
        snapshot.input_mode == TerminalSenderInputMode::Text
            && snapshot.target_scope == TerminalCommandSenderTargetScope::Current
            && snapshot.status != TerminalCommandSenderStatus::Running
    }

    fn terminal_command_sender_compact_suggestions(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        mode: CompactSuggestionMode,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        if !Self::terminal_command_sender_supports_suggestions(snapshot) {
            return Vec::new();
        }
        let draft = self
            .terminal_command_sender
            .read(cx)
            .active_compact_draft()
            .unwrap_or_default();
        match mode {
            CompactSuggestionMode::Automatic => {
                let target = WorkspaceImeTarget::TerminalCommandSenderCompact;
                if self.marked_text_for_target(target, cx).is_some() {
                    return Vec::new();
                }
                let cursor = self
                    .ime_active_selection_offset(target)
                    .map_or(draft.len(), |offset| byte_index_for_utf16(draft, offset));
                self.terminal_command_sender_spec_suggestions(draft, cursor)
            }
            CompactSuggestionMode::History => {
                let draft = Zeroizing::new(draft.to_string());
                self.terminal_command_sender_visible_history_suggestions(&draft, cx)
            }
            CompactSuggestionMode::Dismissed => Vec::new(),
        }
    }

    pub(in crate::workspace) fn accept_terminal_command_sender_suggestion(
        &mut self,
        suggestion: &TerminalCommandSuggestion,
        cx: &mut Context<Self>,
    ) {
        let mut draft = Zeroizing::new(
            self.terminal_command_sender
                .read(cx)
                .active_compact_draft()
                .unwrap_or_default()
                .to_string(),
        );
        let replacement = suggestion.replacement.clone();
        if draft.get(replacement.clone()).is_none() {
            return;
        }
        let end = replacement.start + suggestion.insert_text.len();
        draft.replace_range(replacement, &suggestion.insert_text);
        let caret = draft[..end].encode_utf16().count();
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.replace_active_compact_text(std::mem::take(&mut *draft), cx);
        });
        let target = WorkspaceImeTarget::TerminalCommandSenderCompact;
        self.set_ime_selection_from_anchor(target, caret, caret);
        self.ime_marked_text = None;
        self.show_active_input_caret(cx);
        cx.notify();
    }

    fn render_terminal_command_sender_tabs(
        &self,
        documents: &[TerminalCommandSenderDocumentSnapshot],
        active_id: TerminalCommandSenderId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut tab_row = div().h_full().flex_none().flex().flex_row().items_stretch();
        for (index, document) in documents.iter().enumerate() {
            let sender_id = document.id;
            let active = sender_id == active_id;
            let label = format!("{} {}", self.i18n.t("terminal.sender.task"), sender_id.0);
            let status_icon = match document.status {
                TerminalCommandSenderStatus::Running => LucideIcon::LoaderCircle,
                TerminalCommandSenderStatus::Completed => LucideIcon::Check,
                TerminalCommandSenderStatus::Stopped => LucideIcon::Square,
                TerminalCommandSenderStatus::Failed => LucideIcon::AlertCircle,
                TerminalCommandSenderStatus::Idle => LucideIcon::FileText,
            };
            let icon_color = if active {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            };
            let text_color = if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            };
            let left_padding = if index == 0 {
                TERMINAL_SENDER_PANEL_PADDING
            } else {
                10.0
            };
            tab_row = tab_row.child(
                div()
                    .flex_none()
                    .h_full()
                    .max_w(px(180.0))
                    .pl(px(left_padding))
                    .pr(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .border_b_2()
                    .border_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgba(0x00000000)
                    })
                    .bg(rgba(0x00000000))
                    .cursor_pointer()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(if active {
                        gpui::FontWeight::MEDIUM
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .text_color(text_color)
                    .hover(move |tab| tab.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
                    .child(Self::render_lucide_icon(status_icon, 12.0, icon_color))
                    .child(div().truncate().child(label))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.terminal_command_sender.update(cx, |sender, cx| {
                                sender.set_active_document(sender_id, cx);
                            });
                            this.focus_terminal_command_sender_editor(sender_id, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        // Keep the add-task action right beside the final tab instead of isolating it on the far right.
        tab_row = tab_row.child(div().flex_none().pl(px(4.0)).flex().items_center().child(
            self.terminal_command_action_button(
                LucideIcon::Plus,
                rgb(theme.text_muted),
                false,
                None,
                "terminal-command-sender-add",
                self.i18n.t("terminal.sender.add_task"),
                |this, _event, window, cx| {
                    let sender_id = this
                        .terminal_command_sender
                        .update(cx, |sender, cx| sender.add_document(cx));
                    this.focus_terminal_command_sender_editor(sender_id, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ),
        ));
        // Scrollable moves the viewport's own style onto an outer wrapper.
        // Keep the tab flex row as a child so its horizontal layout survives
        // that transfer and additional tasks cannot stack behind the editor.
        let tab_list = div()
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .overflow_x_scrollbar()
            .child(tab_row);

        let task_actions = div()
            .flex_none()
            .pr(px(TERMINAL_SENDER_PANEL_PADDING))
            .flex()
            .items_center()
            .gap(px(2.0))
            .when(documents.len() > 1, |actions| {
                actions.child(self.terminal_command_action_button(
                    LucideIcon::X,
                    rgb(theme.text_muted),
                    false,
                    None,
                    "terminal-command-sender-remove",
                    self.i18n.t("terminal.sender.remove_task"),
                    move |this, _event, window, cx| {
                        this.terminal_command_sender.update(cx, |sender, cx| {
                            sender.remove_document(active_id, cx);
                        });
                        let next_id = this.terminal_command_sender.read(cx).active_document_id();
                        this.focus_terminal_command_sender_editor(next_id, window, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
            })
            .child(self.terminal_command_action_button(
                LucideIcon::ChevronDown,
                rgb(theme.text_muted),
                false,
                None,
                "terminal-command-sender-collapse",
                self.i18n.t("terminal.sender.collapse"),
                |this, _event, window, cx| {
                    this.terminal_command_sender
                        .update(cx, |sender, cx| sender.toggle_expanded(cx));
                    this.clear_ime_selection();
                    this.focus_compact_terminal_command_sender(window, cx);
                    cx.stop_propagation();
                },
                cx,
            ));

        div()
            .h(px(32.0))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(self.workspace_chrome_divider())
            .when(self.window_background_preferences().is_none(), |strip| {
                strip.bg(rgb(theme.bg))
            })
            .child(tab_list)
            .child(task_actions)
            .into_any_element()
    }

    fn render_terminal_command_sender_controls(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        targets: &[(TerminalCommandSenderTarget, String, TabKind)],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let interval_step = match snapshot.pacing {
            TerminalSenderPacing::Line => TERMINAL_SENDER_INTERVAL_STEP_LINE_MS,
            TerminalSenderPacing::Character => TERMINAL_SENDER_INTERVAL_STEP_CHARACTER_MS,
        };
        let status_label = self.terminal_command_sender_status_label(snapshot);
        let progress_percent = if snapshot.total_units == 0 {
            0.0
        } else {
            (snapshot.completed_units as f32 / snapshot.total_units as f32) * 100.0
        };
        let running = snapshot.status == TerminalCommandSenderStatus::Running;
        let selected_target_count = targets
            .iter()
            .filter(|(target, _, _)| snapshot.selected_targets.contains(&target.pane_id))
            .count();
        let current_target_label = self.terminal_command_active_target_label(cx);
        let status_color = match snapshot.status {
            TerminalCommandSenderStatus::Completed => self.tokens.ui.success,
            TerminalCommandSenderStatus::Failed => self.tokens.ui.error,
            TerminalCommandSenderStatus::Running => self.tokens.ui.accent,
            TerminalCommandSenderStatus::Stopped => self.tokens.ui.warning,
            TerminalCommandSenderStatus::Idle => self.tokens.ui.text_muted,
        };
        let format_controls = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.0))
            .flex_wrap()
            .child(self.render_terminal_sender_mode_control(snapshot, cx))
            .child(self.render_terminal_sender_pacing_control(snapshot, cx))
            .child(self.render_terminal_sender_stepper(
                self.i18n.t("terminal.sender.interval"),
                format!("{} ms", snapshot.interval_ms),
                54.0,
                move |this, cx| {
                    this.terminal_command_sender.update(cx, |sender, cx| {
                        sender.adjust_interval(sender_id, -interval_step, cx);
                    });
                },
                move |this, cx| {
                    this.terminal_command_sender.update(cx, |sender, cx| {
                        sender.adjust_interval(sender_id, interval_step, cx);
                    });
                },
                cx,
            ))
            .child(self.render_terminal_sender_stepper(
                self.i18n.t("terminal.sender.repeat"),
                format!("{}×", snapshot.repeat_count),
                36.0,
                move |this, cx| {
                    this.terminal_command_sender.update(cx, |sender, cx| {
                        sender.adjust_repeat_count(sender_id, -1, cx);
                    });
                },
                move |this, cx| {
                    this.terminal_command_sender.update(cx, |sender, cx| {
                        sender.adjust_repeat_count(sender_id, 1, cx);
                    });
                },
                cx,
            ));

        let target_context = (snapshot.target_scope == TerminalCommandSenderTargetScope::Current)
            .then(|| {
                div()
                    .flex_none()
                    .max_w(px(TERMINAL_SENDER_TARGET_CONTEXT_MAX_WIDTH))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Terminal,
                        12.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .child(div().truncate().child(current_target_label))
                    .into_any_element()
            });
        let selected_targets = (snapshot.target_scope
            == TerminalCommandSenderTargetScope::Selected)
            .then(|| self.render_terminal_sender_target_list(snapshot, targets, cx));
        let selected_group = (snapshot.target_scope == TerminalCommandSenderTargetScope::Group)
            .then(|| self.render_terminal_sender_group_list(snapshot, cx));
        let target_controls = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .flex_wrap()
            .child(
                div()
                    .flex_none()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("terminal.sender.targets")),
            )
            .child(self.render_terminal_sender_scope_control(snapshot, selected_target_count, cx))
            .when_some(target_context, |row, context| row.child(context))
            .when_some(selected_targets, |row, target_list| {
                row.child(
                    div()
                        .min_w(px(TERMINAL_SENDER_TARGET_LIST_MIN_WIDTH))
                        .flex_1()
                        .child(target_list),
                )
            })
            .when_some(selected_group, |row, group_list| {
                row.child(div().min_w(px(160.0)).flex_1().child(group_list))
            })
            .when(
                !matches!(
                    snapshot.target_scope,
                    TerminalCommandSenderTargetScope::Selected
                        | TerminalCommandSenderTargetScope::Group
                ),
                |row| row.child(div().flex_1().min_w(px(8.0))),
            )
            .when(
                snapshot.status != TerminalCommandSenderStatus::Idle,
                |row| {
                    row.child(
                        div()
                            .flex_none()
                            .text_size(px(11.0))
                            .text_color(rgb(status_color))
                            .child(status_label),
                    )
                    .child(
                        monospace_datum(
                            &self.tokens,
                            format!(
                                "{}/{} · {} {} · {} {}",
                                snapshot.completed_units,
                                snapshot.total_units,
                                self.i18n.t("terminal.sender.accepted"),
                                snapshot.accepted_writes,
                                self.i18n.t("terminal.sender.skipped"),
                                snapshot.skipped_writes
                            ),
                            Some(settings_mono_font_family(self.settings_store.settings())),
                            MonospaceDatumOptions::new(MonospaceDatumTone::Muted).text_size(10.0),
                        )
                        .whitespace_nowrap(),
                    )
                },
            )
            .child(self.terminal_sender_run_button(sender_id, running, cx));

        div()
            .flex_none()
            .border_t_1()
            .border_color(self.workspace_chrome_divider())
            .when(self.window_background_preferences().is_none(), |controls| {
                controls.bg(rgb(self.tokens.ui.bg))
            })
            .when(snapshot.total_units > 0, |panel| {
                panel.child(
                    div()
                        .h(px(TERMINAL_SENDER_PROGRESS_HEIGHT))
                        .w_full()
                        .bg(self.workspace_chrome_divider())
                        .child(
                            div()
                                .h_full()
                                .w(relative((progress_percent / 100.0).clamp(0.0, 1.0)))
                                .bg(rgb(self.tokens.ui.accent)),
                        ),
                )
            })
            .child(
                div()
                    .min_h(px(36.0))
                    .px(px(TERMINAL_SENDER_PANEL_PADDING))
                    .py(px(5.0))
                    .flex()
                    .items_center()
                    .child(format_controls),
            )
            .child(
                div()
                    .min_h(px(36.0))
                    .px(px(TERMINAL_SENDER_PANEL_PADDING))
                    .py(px(5.0))
                    .border_t_1()
                    .border_color(self.workspace_chrome_divider())
                    .flex()
                    .items_center()
                    .child(target_controls),
            )
            .into_any_element()
    }

    fn render_terminal_sender_segmented_group(&self, items: Vec<AnyElement>) -> AnyElement {
        div()
            .flex_none()
            .h(px(TERMINAL_SENDER_CONTROL_HEIGHT))
            .p(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(self.workspace_chrome_divider())
            .bg(rgba((self.tokens.ui.bg_sunken << 8) | 0xa6))
            .flex()
            .items_center()
            .gap(px(2.0))
            .children(items)
            .into_any_element()
    }

    fn terminal_sender_segment_item(&self, label: String, active: bool) -> gpui::Div {
        let theme = self.tokens.ui;
        div()
            .h_full()
            .px(px(10.0))
            .rounded(px((self.tokens.radii.sm - 1.0).max(2.0)))
            .flex()
            .items_center()
            .justify_center()
            .whitespace_nowrap()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(if active {
                gpui::FontWeight::MEDIUM
            } else {
                gpui::FontWeight::NORMAL
            })
            .bg(if active {
                rgba((theme.accent << 8) | 0x1e)
            } else {
                rgba(0x00000000)
            })
            .text_color(if active {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            })
            .cursor_pointer()
            .when(!active, |item| {
                item.hover(move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
            })
            .child(label)
    }

    fn render_terminal_sender_mode_control(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let items = [TerminalSenderInputMode::Text, TerminalSenderInputMode::Hex]
            .into_iter()
            .map(|mode| {
                let active = snapshot.input_mode == mode;
                let label = self.i18n.t(match mode {
                    TerminalSenderInputMode::Text => "terminal.sender.text",
                    TerminalSenderInputMode::Hex => "terminal.sender.hex",
                });
                self.terminal_sender_segment_item(label, active)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.terminal_command_sender.update(cx, |sender, cx| {
                                sender.set_input_mode(sender_id, mode, cx);
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .into_any_element()
            })
            .collect();
        self.render_terminal_sender_segmented_group(items)
    }

    fn render_terminal_sender_pacing_control(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let items = [TerminalSenderPacing::Line, TerminalSenderPacing::Character]
            .into_iter()
            .map(|pacing| {
                let active = snapshot.pacing == pacing;
                let label = self.i18n.t(match pacing {
                    TerminalSenderPacing::Line => "terminal.sender.by_line",
                    TerminalSenderPacing::Character
                        if snapshot.input_mode == TerminalSenderInputMode::Hex =>
                    {
                        "terminal.sender.by_byte"
                    }
                    TerminalSenderPacing::Character => "terminal.sender.by_character",
                });
                self.terminal_sender_segment_item(label, active)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.terminal_command_sender.update(cx, |sender, cx| {
                                sender.set_pacing(sender_id, pacing, cx);
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .into_any_element()
            })
            .collect();
        self.render_terminal_sender_segmented_group(items)
    }

    fn render_terminal_sender_scope_control(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        selected_target_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let items = [
            TerminalCommandSenderTargetScope::Current,
            TerminalCommandSenderTargetScope::All,
            TerminalCommandSenderTargetScope::Selected,
            TerminalCommandSenderTargetScope::Group,
        ]
        .into_iter()
        .map(|scope| {
            let active = snapshot.target_scope == scope;
            let label = match scope {
                TerminalCommandSenderTargetScope::Current => self.i18n.t("terminal.sender.current"),
                TerminalCommandSenderTargetScope::All => self.i18n.t("terminal.sender.all"),
                TerminalCommandSenderTargetScope::Selected => format!(
                    "{} ({selected_target_count})",
                    self.i18n.t("terminal.sender.selected")
                ),
                TerminalCommandSenderTargetScope::Group => format!(
                    "{} ({selected_target_count})",
                    self.i18n.t("terminal.sender.group")
                ),
            };
            self.terminal_sender_segment_item(label, active)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.terminal_command_sender.update(cx, |sender, cx| {
                            sender.set_target_scope(sender_id, scope, cx);
                        });
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
        })
        .collect();
        self.render_terminal_sender_segmented_group(items)
    }

    fn render_terminal_sender_target_list(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        targets: &[(TerminalCommandSenderTarget, String, TabKind)],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let mut target_row = div().flex_none().flex().items_center().gap(px(4.0));
        for (target, label, _kind) in targets {
            let pane_id = target.pane_id;
            let selected = snapshot.selected_targets.contains(&pane_id);
            let options = ActionChipOptions::new()
                .active(selected)
                .height(24.0)
                .radius(ButtonRadius::Sm)
                .idle_text_tone(ActionChipTextTone::Muted);
            target_row = target_row.child(
                action_chip(
                    &self.tokens,
                    label.clone(),
                    Some(Self::render_lucide_icon(
                        if selected {
                            LucideIcon::CheckSquare
                        } else {
                            LucideIcon::Square
                        },
                        12.0,
                        action_chip_foreground(&self.tokens, options),
                    )),
                    options,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.terminal_command_sender.update(cx, |sender, cx| {
                            sender.toggle_selected_target(sender_id, pane_id, cx);
                        });
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        // The inner row retains content-sized target chips while the outer
        // viewport owns horizontal overflow at narrow terminal widths.
        div()
            .w_full()
            .overflow_x_scrollbar()
            .child(target_row)
            .into_any_element()
    }

    fn render_terminal_sender_group_list(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let groups = self.terminal_broadcast_groups();
        if groups.is_empty() {
            return div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("terminal.sender.no_groups"))
                .into_any_element();
        }

        let mut group_row = div().flex_none().flex().items_center().gap(px(4.0));
        for group in groups {
            let group_id = group.id;
            let selected = snapshot.selected_group_id == Some(group_id);
            let options = ActionChipOptions::new()
                .active(selected)
                .height(24.0)
                .radius(ButtonRadius::Sm)
                .idle_text_tone(ActionChipTextTone::Muted);
            group_row = group_row.child(
                action_chip(
                    &self.tokens,
                    group.name.clone(),
                    Some(Self::render_lucide_icon(
                        if selected {
                            LucideIcon::CheckSquare
                        } else {
                            LucideIcon::Square
                        },
                        12.0,
                        action_chip_foreground(&self.tokens, options),
                    )),
                    options,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        let targets = this.resolve_terminal_broadcast_group(group_id, cx);
                        this.terminal_command_sender.update(cx, |sender, cx| {
                            sender.set_target_group(sender_id, group_id, &targets, cx);
                        });
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(div().overflow_x_scrollbar().child(group_row))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("terminal.sender.group_hint")),
            )
            .into_any_element()
    }

    fn render_terminal_sender_stepper(
        &self,
        label: String,
        value: String,
        min_value_width: f32,
        decrement: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        increment: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let divider = self.workspace_chrome_divider();
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(label),
            )
            .child(
                div()
                    .h(px(TERMINAL_SENDER_CONTROL_HEIGHT))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(divider)
                    .bg(rgba((theme.bg_sunken << 8) | 0xa6))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .w(px(22.0))
                            .border_r_1()
                            .border_color(divider)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_secondary))
                            .cursor_pointer()
                            .hover(move |button| button.bg(rgb(theme.bg_hover)))
                            .child("−")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    decrement(this, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .h_full()
                            .min_w(px(min_value_width))
                            .px(px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_color(rgb(theme.text))
                            .child(value),
                    )
                    .child(
                        div()
                            .h_full()
                            .w(px(22.0))
                            .border_l_1()
                            .border_color(divider)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_secondary))
                            .cursor_pointer()
                            .hover(move |button| button.bg(rgb(theme.bg_hover)))
                            .child("+")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    increment(this, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn terminal_sender_run_button(
        &self,
        sender_id: TerminalCommandSenderId,
        running: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = if running {
            self.i18n.t("terminal.sender.stop")
        } else {
            self.i18n.t("terminal.sender.start")
        };
        let tone_hex = if running { theme.error } else { theme.accent };
        let mut options = oxideterm_gpui_ui::button::ToolbarButtonOptions::compact_text_min_width(
            oxideterm_gpui_ui::button::ButtonVariant::Outline,
            oxideterm_gpui_ui::button::ButtonRadius::Sm,
            TERMINAL_SENDER_CONTROL_HEIGHT,
            64.0,
            12.0,
            self.tokens.metrics.ui_text_xs,
        );
        options.background = Some(rgba((tone_hex << 8) | 0x1a));
        options.border = Some(rgba((tone_hex << 8) | 0x66));
        options.text_color = Some(rgb(tone_hex));
        options.hover_background = Some(rgba((tone_hex << 8) | 0x28));
        options.hover_border = Some(rgb(tone_hex));
        options.hover_text_color = Some(rgb(tone_hex));
        oxideterm_gpui_ui::button::toolbar_button(&self.tokens, label, None, options)
            .font_weight(gpui::FontWeight::MEDIUM)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if running {
                        this.terminal_command_sender.update(cx, |sender, cx| {
                            sender.stop(sender_id, cx);
                        });
                    } else {
                        this.start_terminal_command_sender(sender_id, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn terminal_command_sender_status_label(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
    ) -> String {
        if let Some(failure) = snapshot.failure {
            return self.i18n.t(match failure {
                TerminalCommandSenderFailure::EmptyInput => "terminal.sender.error_empty",
                TerminalCommandSenderFailure::InvalidHex => "terminal.sender.error_hex",
                TerminalCommandSenderFailure::NoTargets => "terminal.sender.error_targets",
                TerminalCommandSenderFailure::TargetBusy => "terminal.sender.error_busy",
            });
        }
        self.i18n.t(match snapshot.status {
            TerminalCommandSenderStatus::Idle => "terminal.sender.idle",
            TerminalCommandSenderStatus::Running => "terminal.sender.running",
            TerminalCommandSenderStatus::Completed => "terminal.sender.completed",
            TerminalCommandSenderStatus::Stopped => "terminal.sender.stopped",
            TerminalCommandSenderStatus::Failed => "terminal.sender.failed",
        })
    }

    fn terminal_command_sender_target_entries(
        &self,
        cx: &App,
    ) -> Vec<(TerminalCommandSenderTarget, String, TabKind)> {
        let tab_host = self.tab_host.read(cx);
        self.terminal_broadcast_entries(cx)
            .into_iter()
            .filter_map(|entry| {
                tab_host.panes().get(&entry.pane_id).map(|pane| {
                    (
                        TerminalCommandSenderTarget {
                            pane_id: entry.pane_id,
                            pane: pane.downgrade(),
                        },
                        format!("{} · #{}", entry.label, entry.pane_id.0),
                        entry.kind,
                    )
                })
            })
            .collect()
    }

    fn start_terminal_command_sender(
        &mut self,
        sender_id: TerminalCommandSenderId,
        cx: &mut Context<Self>,
    ) -> bool {
        let selected_group_id = self
            .terminal_command_sender
            .read(cx)
            .document_snapshots()
            .into_iter()
            .find(|document| document.id == sender_id)
            .filter(|document| document.target_scope == TerminalCommandSenderTargetScope::Group)
            .and_then(|document| document.selected_group_id);
        if let Some(group_id) = selected_group_id {
            // Resolve at run time so closed and newly opened saved sessions are reflected
            // without opening any connection on behalf of the sender.
            let targets = self.resolve_terminal_broadcast_group(group_id, cx);
            self.terminal_command_sender.update(cx, |sender, cx| {
                sender.set_target_group(sender_id, group_id, &targets, cx);
            });
        }
        let current_pane_id = self.active_pane_id(cx);
        let entries = self.terminal_command_sender_target_entries(cx);
        let live_panes = entries
            .iter()
            .map(|(target, _, _)| target.pane_id)
            .collect::<HashSet<_>>();
        let candidates = entries.into_iter().map(|(target, _, _)| target).collect();
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.retain_live_targets(&live_panes, cx);
            sender.start(sender_id, current_pane_id, candidates, cx)
        })
    }

    pub(in crate::workspace) fn ensure_terminal_command_sender_visible(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal_command_sender.update(cx, |sender, cx| {
            if !sender.is_visible() {
                sender.toggle_visible(cx);
            }
        });
    }

    pub(in crate::workspace) fn replace_terminal_command_sender_text(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderId {
        self.terminal_command_sender
            .update(cx, |sender, cx| sender.replace_active_text(text, cx))
    }

    pub(in crate::workspace) fn append_terminal_command_sender_text(
        &mut self,
        text: &str,
        separate_with_space: bool,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderId {
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.append_active_text(text, separate_with_space, cx)
        })
    }

    pub(in crate::workspace) fn focus_terminal_command_sender_editor(
        &mut self,
        sender_id: TerminalCommandSenderId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self
            .terminal_command_sender
            .read(cx)
            .document_snapshots()
            .into_iter()
            .find(|document| document.id == sender_id)
            .map(|document| document.editor);
        if let Some(editor) = editor {
            if self.blur_host_tools_input(cx) {
                cx.notify();
            }
            let focus_handle = editor.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
    }

    pub(super) fn focus_compact_terminal_command_sender(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.blur_terminal_quick_commands_input(cx);
        // Both inputs use the workspace FocusHandle, so GPUI cannot blur the sidebar for us.
        if self.blur_host_tools_input(cx) {
            cx.notify();
        }
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.set_compact_focused(true, cx);
        });
        window.focus(&self.focus_handle, cx);
    }

    pub(in crate::workspace) fn focus_terminal_command_sender_input(
        &mut self,
        sender_id: TerminalCommandSenderId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let expanded = self.terminal_command_sender.read(cx).is_expanded();
        if expanded {
            self.blur_terminal_quick_commands_input(cx);
            self.focus_terminal_command_sender_editor(sender_id, window, cx);
            return;
        }
        self.terminal_command_sender.update(cx, |sender, cx| {
            if !sender.is_visible() {
                sender.toggle_visible(cx);
            }
        });
        self.clear_ime_selection();
        self.focus_compact_terminal_command_sender(window, cx);
    }

    pub(in crate::workspace) fn terminal_command_sender_editor_focused(
        &self,
        window: &Window,
        cx: &App,
    ) -> bool {
        let sender = self.terminal_command_sender.read(cx);
        sender.is_expanded()
            && sender.active_document_snapshot().is_some_and(|document| {
                document.editor.read(cx).focus_handle(cx).is_focused(window)
            })
    }

    pub(in crate::workspace) fn handle_compact_terminal_command_sender_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let sender = self.terminal_command_sender.read(cx);
        if !sender.is_visible() || sender.is_expanded() {
            return false;
        }
        let Some(document) = sender.active_document_snapshot() else {
            return false;
        };
        if !sender.compact_focused() {
            return false;
        }
        let mode = sender.compact_suggestion_mode();
        let suggestion_highlighted = sender.compact_suggestion_highlighted();

        let modifiers = event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "escape" if !modifiers.platform => {
                if mode == CompactSuggestionMode::History
                    || !self
                        .terminal_command_sender_compact_suggestions(&document, mode, cx)
                        .is_empty()
                {
                    self.terminal_command_sender.update(cx, |sender, cx| {
                        sender.hide_compact_suggestions(cx);
                    });
                    return true;
                }
                self.terminal_command_sender.update(cx, |sender, cx| {
                    sender.set_compact_focused(false, cx);
                });
                self.clear_ime_selection();
                self.focus_active_pane(window, cx);
                true
            }
            "enter" if !modifiers.platform && !modifiers.shift && !modifiers.alt => {
                if let Some(index) = suggestion_highlighted {
                    let suggestions =
                        self.terminal_command_sender_compact_suggestions(&document, mode, cx);
                    if let Some(suggestion) = suggestions.get(index) {
                        self.accept_terminal_command_sender_suggestion(suggestion, cx);
                    }
                    // The candidate list may have changed since it was highlighted.
                    // Never send a command in place of the missing candidate.
                    return true;
                }
                let sender_id = document.id;
                if self.start_terminal_command_sender(sender_id, cx) {
                    self.terminal_command_sender.update(cx, |sender, cx| {
                        sender.clear_compact_text(sender_id, cx);
                    });
                    self.clear_ime_selection();
                }
                true
            }
            "up" | "arrowup" | "down" | "arrowdown"
                if !modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.shift
                    && self
                        .marked_text_for_target(
                            WorkspaceImeTarget::TerminalCommandSenderCompact,
                            cx,
                        )
                        .is_none() =>
            {
                let move_down = matches!(event.keystroke.key.as_str(), "down" | "arrowdown");
                // Up still opens history until the user selects a Spec candidate.
                let mut mode = if mode == CompactSuggestionMode::Dismissed
                    || (!move_down && suggestion_highlighted.is_none())
                {
                    CompactSuggestionMode::History
                } else {
                    mode
                };
                let mut suggestions =
                    self.terminal_command_sender_compact_suggestions(&document, mode, cx);
                if suggestions.is_empty() && mode == CompactSuggestionMode::Automatic {
                    mode = CompactSuggestionMode::History;
                    suggestions =
                        self.terminal_command_sender_compact_suggestions(&document, mode, cx);
                }
                if suggestions.is_empty() {
                    return false;
                }
                self.terminal_command_sender.update(cx, |sender, cx| {
                    sender.move_compact_suggestion_selection(
                        suggestions.len(),
                        move_down,
                        mode,
                        cx,
                    );
                });
                true
            }
            "tab"
                if !modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.shift =>
            {
                if let Some(index) = suggestion_highlighted
                    && let Some(suggestion) = self
                        .terminal_command_sender_compact_suggestions(&document, mode, cx)
                        .get(index)
                {
                    self.accept_terminal_command_sender_suggestion(suggestion, cx);
                    return true;
                }
                if self.accept_terminal_command_sender_ghost_text(&document, cx) {
                    return true;
                }
                if mode == CompactSuggestionMode::Automatic
                    && let Some(suggestion) = self
                        .terminal_command_sender_compact_suggestions(&document, mode, cx)
                        .first()
                {
                    self.accept_terminal_command_sender_suggestion(suggestion, cx);
                    return true;
                }
                false
            }
            "right" | "arrowright"
                if !modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.shift =>
            {
                self.accept_terminal_command_sender_ghost_text(&document, cx)
            }
            _ => false,
        }
    }

    fn accept_terminal_command_sender_ghost_text(
        &mut self,
        document: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> bool {
        let target = WorkspaceImeTarget::TerminalCommandSenderCompact;
        let mut draft = Zeroizing::new(
            self.terminal_command_sender
                .read(cx)
                .active_compact_draft()
                .unwrap_or_default()
                .to_string(),
        );
        let draft_len = draft.encode_utf16().count();
        let caret_is_at_end = self
            .ime_selection_range_for_target(target, cx)
            .is_some_and(|range| range.start == draft_len && range.end == draft_len)
            && self.marked_text_for_target(target, cx).is_none();
        let Some(ghost_text) = caret_is_at_end
            .then(|| self.terminal_command_sender_compact_ghost_text(&document, &draft, cx))
            .flatten()
        else {
            return false;
        };
        draft.push_str(&ghost_text);
        let caret = draft.encode_utf16().count();
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.replace_active_compact_text(std::mem::take(&mut *draft), cx);
        });
        self.set_ime_selection_from_anchor(target, caret, caret);
        self.show_active_input_caret(cx);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn update_terminal_command_sender_resize(
        &mut self,
        event: &MouseMoveEvent,
        _window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.update_resize(event.position.y, event.dragging(), cx);
        });
    }

    pub(in crate::workspace) fn finish_terminal_command_sender_resize(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal_command_sender
            .update(cx, |sender, cx| sender.finish_resize(cx));
    }

    pub(in crate::workspace) fn sync_terminal_command_sender_appearance(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let terminal_settings = &self.settings_store.settings().terminal;
        let command_bar_enabled = terminal_settings.command_bar.enabled;
        let quick_commands_enabled = terminal_settings.command_bar.quick_commands_enabled;
        let font_family = settings_mono_font_family(self.settings_store.settings()).to_string();
        let font_size = terminal_settings.font_size as f32;
        let line_height = terminal_settings.line_height as f32;
        let tokens = self.tokens;
        let background_active = self.window_background_preferences().is_some();
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.sync_editor_appearance(
                tokens,
                font_family,
                font_size,
                line_height,
                background_active,
                cx,
            );
            if !command_bar_enabled {
                // Disabling the owning surface must not leave hidden jobs with
                // no visible stop control.
                sender.set_expanded(false, cx);
                sender.stop_all(cx);
            }
        });
        if !command_bar_enabled || !quick_commands_enabled {
            // Removing the dock's owning surface also releases any parameter draft and IME target.
            self.close_terminal_quick_commands_panel(cx);
        }
    }

    pub(in crate::workspace) fn render_terminal_quick_bar(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = self
            .active_pane_id(cx)
            .and_then(|pane_id| self.quick_command_context_for_pane(pane_id, cx));
        let (categories, commands) = self.terminal.read(cx).quick_commands.quick_bar_snapshot(
            target.as_ref(),
            self.settings_store
                .settings()
                .terminal
                .command_bar
                .quick_bar_category
                .as_deref(),
        );
        let selected = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_bar_category
            .as_deref()
            .and_then(|id| categories.iter().find(|category| category.id == id));
        let label = selected
            .map(|category| category.name.clone())
            .unwrap_or_else(|| self.i18n.t("terminal.quick_commands.all_groups"));
        let workspace = cx.entity().downgrade();
        let selector_width = oxideterm_gpui_ui::select::estimate_select_trigger_width(
            &self.tokens,
            160.0,
            260.0,
            [label.as_str()],
        );
        let selector =
            oxideterm_gpui_ui::select::select_trigger(&self.tokens, label.clone(), false, false)
                .id("quick-bar-category")
                .w(px(selector_width))
                .h(px(28.0))
                .flex_none()
                .role(gpui::Role::Button)
                .aria_label(label)
                .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.open_quick_bar_category_picker(window, cx)
                    });
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.open_quick_bar_category_picker(window, cx);
                        cx.stop_propagation();
                    }),
                );
        let filtered = selected.is_some();

        let mut content_row = div()
            .h_full()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .when(commands.is_empty(), |row| {
                row.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("terminal.quick_commands.group_empty")),
                )
            });
        for category in categories {
            let category_icon = terminal_quick_bar_icon(category.icon);
            let icon_color = category
                .icon_color
                .as_deref()
                .and_then(oxideterm_quick_commands::parse_icon_color)
                .unwrap_or(self.tokens.ui.text_muted);
            let category_commands = commands
                .iter()
                .filter(|command| command.category == category.id)
                .cloned()
                .collect::<Vec<_>>();
            if category_commands.is_empty() {
                continue;
            }
            content_row = content_row.when(!filtered, |row| {
                row.child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        .text_size(px(10.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(Self::render_lucide_icon(
                            category_icon,
                            11.0,
                            rgb(icon_color),
                        ))
                        .child(category.name),
                )
            });
            for command in category_commands {
                let command_for_run = command.clone();
                let tooltip_name = command.name.clone();
                // The render callback owns this preview source only while the chip exists.
                let tooltip_command = Zeroizing::new(command.command);
                let tokens = self.tokens;
                content_row = content_row.child(
                    action_chip(
                        &self.tokens,
                        command.name,
                        Some(Self::render_lucide_icon(
                            category_icon,
                            11.0,
                            rgb(icon_color),
                        )),
                        ActionChipOptions::new()
                            .idle_text_tone(ActionChipTextTone::Muted)
                            .max_width(QUICK_BAR_COMMAND_MAX_WIDTH),
                    )
                    .id(format!("quick-bar-command-{}", command.id))
                    .tooltip(move |_, cx| {
                        cx.new(|_| QuickCommandPreview {
                            tokens,
                            text: quick_command_preview(&tooltip_name, &tooltip_command),
                        })
                        .into()
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.run_quick_command_model(&command_for_run, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
                );
            }
        }
        // Scrollable transfers the viewport style to its outer wrapper, so the
        // content row must remain a child to preserve horizontal flex layout.
        div()
            .flex_none()
            .h(px(34.0))
            .min_w_0()
            .flex()
            .items_center()
            .pl(px(self.tokens.spacing.two))
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg))
            .child(selector)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_x_scrollbar()
                    .child(content_row),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn quick_bar_category_choices(
        &self,
        cx: &App,
    ) -> Vec<(Option<String>, String, LucideIcon)> {
        let mut choices = vec![(
            None,
            self.i18n.t("terminal.quick_commands.all_groups"),
            LucideIcon::Layers,
        )];
        choices.extend(
            self.terminal
                .read(cx)
                .quick_commands
                .store
                .categories
                .iter()
                .map(|category| {
                    (
                        Some(category.id.clone()),
                        category.name.clone(),
                        terminal_quick_bar_icon(category.icon),
                    )
                }),
        );
        choices
    }
}

fn terminal_quick_bar_icon(icon: crate::workspace::quick_commands::QuickCommandIcon) -> LucideIcon {
    match icon {
        crate::workspace::quick_commands::QuickCommandIcon::Terminal => LucideIcon::Terminal,
        crate::workspace::quick_commands::QuickCommandIcon::Server => LucideIcon::Server,
        crate::workspace::quick_commands::QuickCommandIcon::Folder => LucideIcon::Folder,
        crate::workspace::quick_commands::QuickCommandIcon::Docker => LucideIcon::Server,
        crate::workspace::quick_commands::QuickCommandIcon::Zap => LucideIcon::Zap,
    }
}

#[cfg(test)]
mod quick_command_preview_tests {
    use super::*;

    #[test]
    fn preview_preserves_short_commands_and_bounds_unicode_and_multiline_text() {
        assert_eq!(
            quick_command_preview("状态", "echo '你好'\nprintf ok").as_str(),
            "状态\necho '你好'\nprintf ok"
        );
        let exact = "中".repeat(600);
        assert_eq!(
            quick_command_preview("命令", &exact).as_str(),
            format!("命令\n{exact}")
        );
        assert_eq!(
            quick_command_preview("命令", &format!("{exact}🙂")).as_str(),
            format!("命令\n{exact}…")
        );
        assert_eq!(
            quick_command_preview("多行", "1\n2\n3\n4\n5\n6\n7\n8\n9").as_str(),
            "多行\n1\n2\n3\n4\n5\n6\n7\n8…"
        );
    }

    struct ChipFixture;

    impl Render for ChipFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().flex().gap(px(5.0)).children(
                [
                    ("short", "Status".to_string()),
                    ("long", "Long command name ".repeat(30)),
                ]
                .into_iter()
                .map(|(id, label)| {
                    action_chip(
                        &oxideterm_theme::default_tokens(),
                        label,
                        Some(
                            div()
                                .size(px(11.0))
                                .debug_selector(move || format!("{id}-icon"))
                                .into_any_element(),
                        ),
                        ActionChipOptions::new().max_width(QUICK_BAR_COMMAND_MAX_WIDTH),
                    )
                    .debug_selector(move || id.into())
                }),
            )
        }
    }

    #[gpui::test]
    fn long_names_keep_chips_bounded_without_shrinking_icons(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| ChipFixture);
        cx.simulate_resize(gpui::size(px(800.0), px(100.0)));
        cx.update(|window, app| {
            window.draw(app).clear(app);
        });
        let short = cx.debug_bounds("short").unwrap();
        let long = cx.debug_bounds("long").unwrap();
        assert!(short.size.width < px(220.0));
        assert_eq!(long.size.width, px(220.0));
        assert_eq!(long.left(), short.right() + px(5.0));
        for id in ["short-icon", "long-icon"] {
            assert_eq!(
                cx.debug_bounds(id).unwrap().size,
                gpui::size(px(11.0), px(11.0))
            );
        }
    }
}
