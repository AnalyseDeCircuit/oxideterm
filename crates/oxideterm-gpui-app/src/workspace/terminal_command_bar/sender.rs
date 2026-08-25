// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

// Hallmark · pre-emit critique: P4 H5 E4 S5 R5 V4
// Hallmark · macrostructure: Workbench · genre: modern-minimal · tone: technical and restrained

use super::*;
use crate::workspace::terminal_command_sender::{
    TERMINAL_SENDER_RESIZE_HOTZONE_HEIGHT, TerminalCommandSenderDocumentSnapshot,
    TerminalCommandSenderFailure, TerminalCommandSenderId, TerminalCommandSenderStatus,
    TerminalCommandSenderTarget, TerminalCommandSenderTargetScope,
};
use oxideterm_terminal::{TerminalSenderInputMode, TerminalSenderPacing};
use zeroize::Zeroizing;

const TERMINAL_SENDER_CONTROL_HEIGHT: f32 = 28.0;
const TERMINAL_SENDER_COMPACT_HEIGHT: f32 = 32.0;
const TERMINAL_SENDER_PANEL_PADDING: f32 = 8.0;
const TERMINAL_SENDER_COMPACT_HORIZONTAL_PADDING: f32 = 12.0;
const TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT: f32 = 24.0;
const TERMINAL_SENDER_COMPACT_BACKGROUND_ALPHA: u32 = 0xf2;
const TERMINAL_SENDER_COMPACT_BORDER_ALPHA: u32 = 0x73;
const TERMINAL_SENDER_INTERVAL_STEP_LINE_MS: i64 = 100;
const TERMINAL_SENDER_INTERVAL_STEP_CHARACTER_MS: i64 = 20;
const TERMINAL_SENDER_MODE_CONTROL_WIDTH: f32 = 144.0;
const TERMINAL_SENDER_PACING_CONTROL_WIDTH: f32 = 196.0;
const TERMINAL_SENDER_SCOPE_CONTROL_WIDTH: f32 = 288.0;
const TERMINAL_SENDER_TARGET_LIST_MIN_WIDTH: f32 = 160.0;
const TERMINAL_SENDER_TARGET_CONTEXT_MAX_WIDTH: f32 = 220.0;
const TERMINAL_SENDER_PROGRESS_HEIGHT: f32 = 2.0;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_terminal_command_sender_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (sender_visible, sender_expanded) = {
            let sender = self.terminal_command_sender.read(cx);
            (sender.is_visible(), sender.is_expanded())
        };
        if !sender_visible {
            return div().into_any_element();
        }
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
        let viewport_height = f32::from(window.viewport_size().height);
        let panel_height = self
            .terminal_command_sender
            .read(cx)
            .panel_height_for_viewport(viewport_height);
        let documents = self.terminal_command_sender.read(cx).document_snapshots();
        let targets = self.terminal_command_sender_target_entries(cx);
        let sender_is_resizing = self.terminal_command_sender.read(cx).is_resizing();

        div()
            .relative()
            .flex_none()
            .h(px(panel_height))
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
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
                    .cursor(CursorStyle::ResizeRow)
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            if event.click_count >= 2 {
                                this.terminal_command_sender.update(cx, |sender, cx| {
                                    sender
                                        .reset_height(f32::from(window.viewport_size().height), cx);
                                });
                            } else {
                                this.terminal_command_sender.update(cx, |sender, cx| {
                                    sender.start_resize(
                                        event.position.y,
                                        f32::from(window.viewport_size().height),
                                        cx,
                                    );
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
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .child(active.editor.clone()),
            )
            .child(self.render_terminal_command_sender_controls(&active, &targets, cx))
            .into_any_element()
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
        let suggestions_open = sender.compact_suggestions_open();
        let suggestion_highlighted = sender.compact_suggestion_highlighted();
        let suggestions = if focused
            && suggestions_open
            && snapshot.input_mode == TerminalSenderInputMode::Text
            && snapshot.target_scope == TerminalCommandSenderTargetScope::Current
            && snapshot.status != TerminalCommandSenderStatus::Running
        {
            self.terminal_command_sender_visible_history_suggestions(&draft, cx)
        } else {
            Vec::new()
        };
        let selected_range = self.ime_selected_range_for_target(target, cx);
        let marked_text = self.marked_text_for_target(target, cx);
        let active_offset = self.ime_active_offset_for_target(target, cx);
        let ghost_text = (focused && !suggestions_open)
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
                    this.terminal_command_sender.update(cx, |sender, cx| {
                        sender.set_compact_focused(true, cx);
                    });
                    window.focus(&this.focus_handle, cx);
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
            .when(!suggestions.is_empty(), |row| {
                row.child(self.render_terminal_command_sender_suggestions(
                    &suggestions,
                    suggestion_highlighted,
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
                            this.terminal_command_sender.update(cx, |sender, cx| {
                                sender.set_compact_focused(true, cx);
                            });
                            window.focus(&this.focus_handle, cx);
                            this.ime_marked_text = None;
                            this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        LucideIcon::ChevronRight,
                        16.0,
                        rgb(theme.text_muted),
                    ))
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
                            cx.listener(|this, _event, _window, cx| {
                                // Compact mode restores the original
                                // quick-command affordance without creating
                                // a second command draft.
                                this.terminal.update(cx, |terminal, _cx| {
                                    terminal.quick_commands.toggle_open()
                                });
                                this.terminal_command_sender.update(cx, |sender, cx| {
                                    sender.set_compact_focused(false, cx);
                                });
                                this.dismiss_terminal_broadcast_menu(cx);
                                this.dismiss_terminal_recording_menu();
                                this.close_terminal_cwd_picker(cx);
                                this.close_terminal_git_branch_picker(cx);
                                this.close_terminal_project_panel(cx);
                                cx.stop_propagation();
                                cx.notify();
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

    fn terminal_command_sender_compact_history_suggestions(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        if snapshot.input_mode != TerminalSenderInputMode::Text
            || snapshot.target_scope != TerminalCommandSenderTargetScope::Current
            || snapshot.status == TerminalCommandSenderStatus::Running
        {
            return Vec::new();
        }
        let draft = Zeroizing::new(
            self.terminal_command_sender
                .read(cx)
                .active_compact_draft()
                .unwrap_or_default()
                .to_string(),
        );
        self.terminal_command_sender_visible_history_suggestions(&draft, cx)
    }

    pub(in crate::workspace) fn accept_terminal_command_sender_suggestion(
        &mut self,
        suggestion: &TerminalCommandSuggestion,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderId {
        let mut draft = Zeroizing::new(
            self.terminal_command_sender
                .read(cx)
                .active_compact_draft()
                .unwrap_or_default()
                .to_string(),
        );
        let start = suggestion.replacement.start.min(draft.len());
        let end = suggestion.replacement.end.min(draft.len()).max(start);
        draft.replace_range(start..end, &suggestion.insert_text);
        let caret = draft.encode_utf16().count();
        let sender_id = self.terminal_command_sender.update(cx, |sender, cx| {
            sender.replace_active_compact_text(std::mem::take(&mut *draft), cx)
        });
        let target = WorkspaceImeTarget::TerminalCommandSenderCompact;
        self.set_ime_selection_from_anchor(target, caret, caret);
        self.ime_marked_text = None;
        self.show_active_input_caret(cx);
        cx.notify();
        sender_id
    }

    fn render_terminal_command_sender_tabs(
        &self,
        documents: &[TerminalCommandSenderDocumentSnapshot],
        active_id: TerminalCommandSenderId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut tab_row = div().h_full().flex_none().flex().flex_row().items_stretch();
        for document in documents {
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
            let foreground = if active {
                rgb(self.tokens.ui.accent)
            } else {
                rgb(self.tokens.ui.text_muted)
            };
            tab_row = tab_row.child(
                div()
                    .flex_none()
                    .h_full()
                    .max_w(px(180.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(if active {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgba(0x00000000)
                    })
                    .bg(if active {
                        rgb(self.tokens.ui.bg_hover)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .text_size(px(11.0))
                    .text_color(foreground)
                    .hover(|tab| tab.bg(rgb(self.tokens.ui.bg_hover)))
                    .child(Self::render_lucide_icon(status_icon, 12.0, foreground))
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
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(self.terminal_command_action_button(
                LucideIcon::Plus,
                rgb(self.tokens.ui.text_muted),
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
            ))
            .when(documents.len() > 1, |actions| {
                actions.child(self.terminal_command_action_button(
                    LucideIcon::X,
                    rgb(self.tokens.ui.text_muted),
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
            });

        div()
            .h(px(36.0))
            .flex_none()
            .px(px(TERMINAL_SENDER_PANEL_PADDING))
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_panel))
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
        let status_tone = match snapshot.status {
            TerminalCommandSenderStatus::Completed => StatusTone::Success,
            TerminalCommandSenderStatus::Failed => StatusTone::Error,
            TerminalCommandSenderStatus::Running => StatusTone::Accent,
            TerminalCommandSenderStatus::Stopped => StatusTone::Warning,
            TerminalCommandSenderStatus::Idle => StatusTone::Neutral,
        };
        let format_controls = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .flex_wrap()
            .child(self.render_terminal_sender_mode_control(snapshot, cx))
            .child(self.render_terminal_sender_control_divider())
            .child(self.render_terminal_sender_pacing_control(snapshot, cx))
            .child(self.render_terminal_sender_control_divider())
            .child(self.render_terminal_sender_stepper(
                self.i18n.t("terminal.sender.interval"),
                format!("{} ms", snapshot.interval_ms),
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
            .child(status_pill(
                &self.tokens,
                status_label,
                StatusPillOptions::new(status_tone).compact(),
            ))
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
            .child(self.terminal_sender_run_button(sender_id, running, cx));

        div()
            .flex_none()
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_panel))
            .child(
                div()
                    .h(px(TERMINAL_SENDER_PROGRESS_HEIGHT))
                    .w_full()
                    .bg(rgb(self.tokens.ui.border))
                    .child(
                        div()
                            .h_full()
                            .w(relative((progress_percent / 100.0).clamp(0.0, 1.0)))
                            .bg(rgb(self.tokens.ui.accent)),
                    ),
            )
            .child(
                div()
                    .min_h(px(40.0))
                    .px(px(TERMINAL_SENDER_PANEL_PADDING))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .child(format_controls),
            )
            .child(
                div()
                    .min_h(px(40.0))
                    .px(px(TERMINAL_SENDER_PANEL_PADDING))
                    .py(px(4.0))
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                    .flex()
                    .items_center()
                    .child(target_controls),
            )
            .into_any_element()
    }

    fn render_terminal_sender_mode_control(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let active_index = usize::from(snapshot.input_mode == TerminalSenderInputMode::Hex);
        let items = [TerminalSenderInputMode::Text, TerminalSenderInputMode::Hex]
            .into_iter()
            .enumerate()
            .map(|(index, mode)| {
                let label = self.i18n.t(match mode {
                    TerminalSenderInputMode::Text => "terminal.sender.text",
                    TerminalSenderInputMode::Hex => "terminal.sender.hex",
                });
                oxideterm_gpui_ui::segmented_control_item(
                    &self.tokens,
                    label,
                    index == active_index,
                )
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
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            ("terminal-sender-mode", sender_id.0),
            oxideterm_gpui_ui::SegmentedControlOptions::new(active_index, active_index, 2)
                .compact(TERMINAL_SENDER_MODE_CONTROL_WIDTH),
            items,
        )
        .into_any_element()
    }

    fn render_terminal_sender_pacing_control(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let active_index = usize::from(snapshot.pacing == TerminalSenderPacing::Character);
        let items = [TerminalSenderPacing::Line, TerminalSenderPacing::Character]
            .into_iter()
            .enumerate()
            .map(|(index, pacing)| {
                let label = self.i18n.t(match pacing {
                    TerminalSenderPacing::Line => "terminal.sender.by_line",
                    TerminalSenderPacing::Character
                        if snapshot.input_mode == TerminalSenderInputMode::Hex =>
                    {
                        "terminal.sender.by_byte"
                    }
                    TerminalSenderPacing::Character => "terminal.sender.by_character",
                });
                oxideterm_gpui_ui::segmented_control_item(
                    &self.tokens,
                    label,
                    index == active_index,
                )
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
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            ("terminal-sender-pacing", sender_id.0),
            oxideterm_gpui_ui::SegmentedControlOptions::new(active_index, active_index, 2)
                .compact(TERMINAL_SENDER_PACING_CONTROL_WIDTH),
            items,
        )
        .into_any_element()
    }

    fn render_terminal_sender_scope_control(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        selected_target_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender_id = snapshot.id;
        let active_index = match snapshot.target_scope {
            TerminalCommandSenderTargetScope::Current => 0,
            TerminalCommandSenderTargetScope::All => 1,
            TerminalCommandSenderTargetScope::Selected => 2,
            TerminalCommandSenderTargetScope::Group => 3,
        };
        let items = [
            TerminalCommandSenderTargetScope::Current,
            TerminalCommandSenderTargetScope::All,
            TerminalCommandSenderTargetScope::Selected,
            TerminalCommandSenderTargetScope::Group,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, scope)| {
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
            oxideterm_gpui_ui::segmented_control_item(&self.tokens, label, index == active_index)
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
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            ("terminal-sender-scope", sender_id.0),
            oxideterm_gpui_ui::SegmentedControlOptions::new(active_index, active_index, 4)
                .compact(TERMINAL_SENDER_SCOPE_CONTROL_WIDTH),
            items,
        )
        .into_any_element()
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
            .overflow_x_scrollbar()
            .child(group_row)
            .into_any_element()
    }

    fn render_terminal_sender_control_divider(&self) -> AnyElement {
        div()
            .w(px(1.0))
            .h(px(18.0))
            .flex_none()
            .bg(rgba((self.tokens.ui.border << 8) | 0x99))
            .into_any_element()
    }

    fn render_terminal_sender_stepper(
        &self,
        label: String,
        value: String,
        decrement: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        increment: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h(px(TERMINAL_SENDER_CONTROL_HEIGHT))
            .flex()
            .items_center()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0xb3))
            .bg(rgb(self.tokens.ui.bg))
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(label),
            )
            .child(
                div()
                    .h_full()
                    .w(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
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
                    .min_w(px(56.0))
                    .px(px(4.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.0))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .child(value),
            )
            .child(
                div()
                    .h_full()
                    .w(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
                    .child("+")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            increment(this, cx);
                            cx.stop_propagation();
                        }),
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
        let label = if running {
            self.i18n.t("terminal.sender.stop")
        } else {
            self.i18n.t("terminal.sender.start")
        };
        oxideterm_gpui_ui::button::button_with(
            &self.tokens,
            label,
            oxideterm_gpui_ui::button::ButtonOptions {
                variant: if running {
                    oxideterm_gpui_ui::button::ButtonVariant::Destructive
                } else {
                    oxideterm_gpui_ui::button::ButtonVariant::Default
                },
                size: oxideterm_gpui_ui::button::ButtonSize::Sm,
                radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                disabled: false,
            },
        )
        .min_w(px(68.0))
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
        &self,
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
            let focus_handle = editor.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
    }

    pub(in crate::workspace) fn focus_terminal_command_sender_input(
        &mut self,
        sender_id: TerminalCommandSenderId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let expanded = self.terminal_command_sender.read(cx).is_expanded();
        if expanded {
            self.focus_terminal_command_sender_editor(sender_id, window, cx);
            return;
        }
        self.terminal_command_sender.update(cx, |sender, cx| {
            if !sender.is_visible() {
                sender.toggle_visible(cx);
            }
            sender.set_compact_focused(true, cx);
        });
        self.clear_ime_selection();
        window.focus(&self.focus_handle, cx);
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
        let suggestions_open = sender.compact_suggestions_open();
        let suggestion_highlighted = sender.compact_suggestion_highlighted();

        let modifiers = event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "escape" if !modifiers.platform => {
                if suggestions_open {
                    self.terminal_command_sender.update(cx, |sender, cx| {
                        if sender.dismiss_compact_suggestions() {
                            cx.notify();
                        }
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
                let sender_id = if suggestions_open {
                    let suggestions =
                        self.terminal_command_sender_compact_history_suggestions(&document, cx);
                    suggestion_highlighted
                        .and_then(|index| suggestions.get(index))
                        .map(|suggestion| {
                            self.accept_terminal_command_sender_suggestion(suggestion, cx)
                        })
                        .unwrap_or(document.id)
                } else {
                    document.id
                };
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
                let suggestions =
                    self.terminal_command_sender_compact_history_suggestions(&document, cx);
                if suggestions.is_empty() {
                    return false;
                }
                let move_down = matches!(event.keystroke.key.as_str(), "down" | "arrowdown");
                self.terminal_command_sender.update(cx, |sender, cx| {
                    sender.move_compact_suggestion_selection(suggestions.len(), move_down, cx);
                });
                true
            }
            "tab"
                if !modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.shift =>
            {
                if suggestions_open {
                    let suggestions =
                        self.terminal_command_sender_compact_history_suggestions(&document, cx);
                    if let Some(suggestion) =
                        suggestion_highlighted.and_then(|index| suggestions.get(index))
                    {
                        self.accept_terminal_command_sender_suggestion(suggestion, cx);
                        return true;
                    }
                }
                self.accept_terminal_command_sender_ghost_text(&document, cx)
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
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.update_resize(
                event.position.y,
                f32::from(window.viewport_size().height),
                event.dragging(),
                cx,
            );
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
    }

    pub(in crate::workspace) fn render_terminal_quick_bar(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target_fields = self.terminal_command_context(cx).target_fields();
        let protocol = self
            .active_pane_id(cx)
            .and_then(|pane_id| self.quick_command_context_for_pane(pane_id, cx))
            .map(|context| context.protocol);
        let (categories, commands) = self
            .terminal
            .read(cx)
            .quick_commands
            .quick_bar_snapshot(&target_fields, protocol);
        if commands.is_empty() {
            return div().into_any_element();
        }

        let mut content_row = div()
            .h_full()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(5.0));
        for category in categories {
            let category_icon = terminal_quick_bar_icon(category.icon);
            let category_commands = commands
                .iter()
                .filter(|command| command.category == category.id)
                .cloned()
                .collect::<Vec<_>>();
            if category_commands.is_empty() {
                continue;
            }
            content_row = content_row.child(
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
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .child(category.name),
            );
            for command in category_commands {
                let command_for_run = command.clone();
                content_row = content_row.child(
                    action_chip(
                        &self.tokens,
                        command.name,
                        Some(Self::render_lucide_icon(
                            category_icon,
                            11.0,
                            rgb(self.tokens.ui.text_muted),
                        )),
                        ActionChipOptions::new().idle_text_tone(ActionChipTextTone::Muted),
                    )
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
            .overflow_x_scrollbar()
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg))
            .child(content_row)
            .into_any_element()
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
