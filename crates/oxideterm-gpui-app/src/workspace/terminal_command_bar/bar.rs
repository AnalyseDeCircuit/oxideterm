// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn render_terminal_command_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        const COMMAND_BAR_BG_ALPHA: u32 = 0xf2; // Tauri bg-theme-bg/95
        const COMMAND_BAR_BORDER_ALPHA: u32 = 0xb3; // Tauri border-theme-border/70
        const COMMAND_BAR_INPUT_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-border/45
        const COMMAND_BAR_FOCUSED_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-accent/45

        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::TerminalCommandBar;
        let workspace = cx.entity();
        let input_collapsed = self.terminal_command_input_collapsed;
        let focused = self.terminal_command_bar_focused && !input_collapsed;
        let marked_text = self.marked_text_for_target(target);
        let selected_range = self.ime_selected_range_for_target(target);
        let command_is_empty = self.terminal_command_bar_draft.is_empty();
        let command_suggestions = if focused {
            self.terminal_command_bar_suggestions(false, cx)
        } else {
            Vec::new()
        };
        let ghost_text = self.terminal_command_ghost_text(&command_suggestions);
        let showing_placeholder = command_is_empty && marked_text.is_none();
        let command_text = if showing_placeholder {
            self.i18n.t("terminal.command_bar.command_placeholder")
        } else {
            self.terminal_command_bar_draft.clone()
        };
        let input_range = selected_range
            .clone()
            .filter(|_| focused && !command_is_empty && marked_text.is_none());
        let selection_range = input_range.clone().filter(|range| range.start < range.end);
        let caret_offset = input_range
            .as_ref()
            .filter(|range| range.start == range.end)
            .map(|range| range.start);
        let shows_selection = selection_range.is_some();
        let shows_positioned_caret = caret_offset.is_some() && !shows_selection;
        let command_lines = terminal_command_input_lines(&command_text);
        let command_visible_lines = command_lines
            .len()
            .clamp(1, TERMINAL_COMMAND_INPUT_MAX_VISIBLE_LINES);
        let command_input_height = (command_visible_lines as f32
            * TERMINAL_COMMAND_INPUT_LINE_HEIGHT)
            .max(TERMINAL_COMMAND_INPUT_MIN_HEIGHT);
        let mut command_input_content = div()
            .h(px(command_input_height))
            .max_h(px(command_input_height))
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_y_scrollbar()
            .text_size(px(13.0))
            .line_height(px(TERMINAL_COMMAND_INPUT_LINE_HEIGHT))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(if showing_placeholder {
                rgb(theme.text_muted)
            } else {
                rgb(theme.text)
            });
        for (index, line) in command_lines.iter().copied().enumerate() {
            let is_last_line = index + 1 == command_lines.len();
            let line_selection = terminal_command_line_selection(line, selection_range.as_ref());
            let line_caret = terminal_command_line_caret(line, caret_offset);
            let line_ghost = is_last_line.then(|| ghost_text.as_deref()).flatten();

            command_input_content = command_input_content.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(if showing_placeholder {
                        div().child(line.text.to_string()).into_any_element()
                    } else {
                        text_input_value_segments_with_color(
                            &self.tokens,
                            line.text,
                            false,
                            line_selection,
                            line_caret,
                            self.new_connection_caret_visible,
                            Some(theme.text),
                        )
                        .into_any_element()
                    })
                    .when(
                        focused
                            && is_last_line
                            && !showing_placeholder
                            && !shows_selection
                            && !shows_positioned_caret,
                        |line| {
                            line.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                        },
                    )
                    .when_some(line_ghost, |line, ghost| {
                        line.child(
                            div()
                                .text_color(rgba((theme.text_muted << 8) | 0x99))
                                .child(ghost.to_string()),
                        )
                    }),
            );
        }
        if let Some(marked) = marked_text {
            command_input_content = command_input_content.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }
        // The visible chip and completion providers share Tauri's target-label
        // inference so local shells that are currently inside SSH show the
        // remote identity consistently in both places.
        let target_label = self.terminal_command_active_target_label(cx);
        let cwd_awareness_enabled = self.terminal_current_directory_awareness_enabled();
        let cwd_snapshot = cwd_awareness_enabled
            .then(|| self.active_terminal_cwd_snapshot(cx))
            .flatten();
        let cwd_supported =
            cwd_awareness_enabled && self.active_terminal_cwd_scope_and_pane().is_some();
        let git_snapshot = self.active_terminal_git_snapshot(cx);
        let project_tasks_enabled = self.terminal_project_tasks_enabled();
        let project_snapshot = project_tasks_enabled
            .then(|| self.active_terminal_project_snapshot(cx))
            .flatten();
        let active_pane_id = self.active_pane_id();
        let is_local_terminal = self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::LocalTerminal);
        let target_indicator_is_local =
            is_local_terminal && target_label == self.i18n.t("terminal.command_bar.local_shell");
        let can_split = self.active_tab().is_some_and(|tab| {
            tab.kind == TabKind::LocalTerminal
                && !self.active_tab_has_serial_terminal()
                && tab
                    .root_pane
                    .as_ref()
                    .is_some_and(|root| root.pane_count() < MAX_PANES_PER_TAB)
        });
        let broadcast_targets =
            self.terminal_broadcast_target_panes(active_pane_id.unwrap_or(PaneId(0)));
        let broadcast_label = if self.terminal_broadcast_enabled {
            if self.terminal_broadcast_targets.is_empty() {
                self.i18n.t("terminal.command_bar.all_targets")
            } else {
                format!("{}", broadcast_targets.len())
            }
        } else {
            String::new()
        };
        let quick_commands_enabled = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_commands_enabled;
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;
        let timestamps_active = self.active_terminal_timestamps_enabled(cx);
        let input_toggle_tooltip_id = "terminal-command-input-toggle";
        let input_toggle_title = if input_collapsed {
            self.i18n.t("terminal.command_bar.expand_input")
        } else {
            self.i18n.t("terminal.command_bar.collapse_input")
        };

        let bar = div()
            .relative()
            .flex_none()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | COMMAND_BAR_BORDER_ALPHA))
            .bg(rgba((theme.bg << 8) | COMMAND_BAR_BG_ALPHA))
            .px(px(12.0))
            .py(px(4.0))
            .shadow_lg()
            .when(
                !input_collapsed
                    && focused
                    && self.terminal_command_suggestions_open
                    && !command_suggestions.is_empty(),
                |bar| bar.child(self.render_terminal_command_suggestions(&command_suggestions, cx)),
            )
            .when(
                !input_collapsed && quick_commands_enabled && self.terminal_quick_commands_open,
                |bar| {
                    // Tauri renders QuickCommandsPopover as a child of the relative
                    // TerminalCommandBar (`absolute bottom-full right-3`). Keep the
                    // native popover on the same local coordinate owner; routing it
                    // through the root backdrop makes the existing bottom/right
                    // placement resolve against the wrong box.
                    bar.child(self.render_terminal_quick_commands_popover(cx))
                },
            )
            .when(self.terminal_git_branch_picker.open, |bar| {
                bar.child(self.render_terminal_git_branch_picker(cx))
            })
            .when(
                cwd_awareness_enabled && self.terminal_cwd_picker.open,
                |bar| bar.child(self.render_terminal_cwd_picker(cx)),
            )
            .when(
                project_tasks_enabled && self.terminal_project_panel.open,
                |bar| bar.child(self.render_terminal_project_panel(cx)),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .min_h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .child(
                                oxideterm_gpui_ui::button::icon_button(
                                    &self.tokens,
                                    self.render_animated_chevron(
                                        (
                                            "terminal-command-input-chevron",
                                            (!input_collapsed) as usize,
                                        ),
                                        !input_collapsed,
                                        14.0,
                                        rgb(theme.text_muted),
                                    ),
                                    IconButtonOptions {
                                        background: Some(if input_collapsed {
                                            rgba((theme.bg_hover << 8) | 0x99)
                                        } else {
                                            rgba(0x00000000)
                                        }),
                                        hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                                        ..IconButtonOptions::opaque_toolbar(24.0, ButtonRadius::Md)
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.terminal_command_input_collapsed =
                                            !this.terminal_command_input_collapsed;
                                        // Collapsing is visual-only. Keep the draft, but release
                                        // hidden input ownership so keystrokes return to the pane.
                                        if this.terminal_command_input_collapsed {
                                            this.terminal_command_bar_focused = false;
                                            this.ime_marked_text = None;
                                            this.terminal_command_suggestions_open = false;
                                            this.terminal_command_suggestion_highlighted = None;
                                            this.close_terminal_quick_commands_popover();
                                            this.close_terminal_cwd_picker();
                                            this.close_terminal_project_panel();
                                        }
                                        this.clear_workspace_tooltip(input_toggle_tooltip_id, cx);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                )
                                .id(input_toggle_tooltip_id)
                                .on_mouse_move({
                                    let title = input_toggle_title.clone();
                                    cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                                        this.queue_workspace_tooltip(
                                            input_toggle_tooltip_id,
                                            title.clone(),
                                            f32::from(event.position.x) + 12.0,
                                            f32::from(event.position.y) + 16.0,
                                            cx,
                                        );
                                    })
                                })
                                .on_hover(cx.listener(
                                    move |this, hovered: &bool, _window, cx| {
                                        if !*hovered {
                                            this.clear_workspace_tooltip(
                                                input_toggle_tooltip_id,
                                                cx,
                                            );
                                        }
                                    },
                                )),
                            )
                            .child(self.render_terminal_target_indicator(
                                target_label,
                                target_indicator_is_local,
                                cx,
                            ))
                            .when(cwd_supported, |row| {
                                row.child(Self::terminal_command_context_chip_slot(
                                    TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH,
                                    self.render_terminal_cwd_chip(cwd_snapshot, cx),
                                ))
                            })
                            .when_some(git_snapshot, |row, snapshot| {
                                row.child(Self::terminal_command_context_chip_slot(
                                    TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH,
                                    self.render_terminal_git_chip(snapshot, cx),
                                ))
                            })
                            .when_some(project_snapshot, |row, snapshot| {
                                row.child(Self::terminal_command_context_chip_slot(
                                    TERMINAL_COMMAND_PROJECT_CHIP_MAX_WIDTH,
                                    self.render_terminal_project_chip(snapshot, cx),
                                ))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(4.0))
                            .when(
                                self.terminal_broadcast_enabled && !broadcast_label.is_empty(),
                                |actions| {
                                    actions.child(
                                        div()
                                            .h(px(20.0))
                                            .px(px(6.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .rounded(px(self.tokens.radii.md))
                                            .border_1()
                                            .border_color(rgba((theme.accent << 8) | 0x4d))
                                            .bg(rgba((theme.accent << 8) | 0x1a))
                                            .text_size(px(11.0))
                                            .text_color(rgb(theme.accent))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Radio,
                                                12.0,
                                                rgb(theme.accent),
                                            ))
                                            .child(broadcast_label),
                                    )
                                },
                            )
                            .when(is_local_terminal, |actions| {
                                actions
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::SplitSquareHorizontal,
                                        rgb(theme.text_muted),
                                        !can_split,
                                        None,
                                        |this, _event, window, cx| {
                                            this.split_active_pane(
                                                SplitDirection::Horizontal,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::SplitSquareVertical,
                                        rgb(theme.text_muted),
                                        !can_split,
                                        None,
                                        |this, _event, window, cx| {
                                            this.split_active_pane(
                                                SplitDirection::Vertical,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                            })
                            .child(select_anchor_probe(
                                SelectAnchorId::TerminalBroadcastMenu,
                                self.terminal_command_action_button(
                                    LucideIcon::Radio,
                                    if self.terminal_broadcast_enabled {
                                        rgb(theme.accent)
                                    } else {
                                        rgb(theme.text_muted)
                                    },
                                    false,
                                    Some(if self.terminal_broadcast_enabled {
                                        rgba((theme.accent << 8) | 0x26)
                                    } else {
                                        rgba((theme.bg_hover << 8) | 0x00)
                                    }),
                                    |this, _event, _window, cx| {
                                        this.toggle_terminal_broadcast_menu();
                                        cx.stop_propagation();
                                        cx.notify();
                                    },
                                    cx,
                                )
                                .relative(),
                                {
                                    let workspace = workspace.clone();
                                    move |anchor, _window, cx| {
                                        let _ = workspace.update(cx, |this, cx| {
                                            this.update_select_anchor(anchor, cx);
                                        });
                                    }
                                },
                            ))
                            .child(self.terminal_command_action_button(
                                LucideIcon::Search,
                                if self.search.visible {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                },
                                false,
                                Some(if self.search.visible {
                                    rgba((theme.accent << 8) | 0x26)
                                } else {
                                    rgba(0x00000000)
                                }),
                                |this, _event, window, cx| {
                                    if this.search.visible {
                                        this.close_search(window, cx);
                                    } else {
                                        this.open_search(window, cx);
                                    }
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .child(self.terminal_command_action_button(
                                LucideIcon::Clock,
                                if timestamps_active {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                },
                                false,
                                Some(if timestamps_active {
                                    rgba((theme.accent << 8) | 0x26)
                                } else {
                                    rgba(0x00000000)
                                }),
                                |this, _event, _window, cx| {
                                    this.toggle_active_terminal_timestamps(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .when(recording_active, |actions| {
                                actions.child(
                                    div()
                                        .h(px(20.0))
                                        .px(px(6.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .rounded(px(self.tokens.radii.md))
                                        .border_1()
                                        .border_color(rgba((theme.error << 8) | 0x4d))
                                        .bg(rgba((theme.error << 8) | 0x1a))
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme.error))
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Circle,
                                            10.0,
                                            rgb(theme.error),
                                        ))
                                        .child(format_recording_elapsed(recording_status.elapsed)),
                                )
                            })
                            .child(self.terminal_command_action_button(
                                match recording_status.state {
                                    TerminalRecordingState::Paused => LucideIcon::Play,
                                    _ => LucideIcon::Circle,
                                },
                                if recording_active {
                                    rgb(theme.error)
                                } else {
                                    rgb(theme.text_muted)
                                },
                                false,
                                Some(if recording_active {
                                    rgba((theme.error << 8) | 0x26)
                                } else {
                                    rgba(0x00000000)
                                }),
                                move |this, _event, _window, cx| {
                                    match recording_status.state {
                                        TerminalRecordingState::Idle => {
                                            this.start_active_terminal_recording(cx)
                                        }
                                        TerminalRecordingState::Recording => {
                                            this.pause_active_terminal_recording(cx)
                                        }
                                        TerminalRecordingState::Paused => {
                                            this.resume_active_terminal_recording(cx)
                                        }
                                    }
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .when(recording_active, |actions| {
                                actions
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::Square,
                                        rgb(theme.error),
                                        false,
                                        None,
                                        |this, _event, _window, cx| {
                                            this.stop_active_terminal_recording(cx);
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::Trash2,
                                        rgb(theme.error),
                                        false,
                                        None,
                                        |this, _event, _window, cx| {
                                            this.discard_active_terminal_recording(cx);
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                            })
                            .child(self.terminal_command_action_button(
                                LucideIcon::FilePlay,
                                rgb(theme.text_muted),
                                false,
                                None,
                                |this, _event, window, cx| {
                                    this.open_terminal_cast_file(window, cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    ),
            )
            .when(!input_collapsed, |bar| {
                bar.child(
                    div()
                        .mt(px(2.0))
                        .pt(px(4.0))
                        .border_t_1()
                        .border_color(if focused {
                            rgba((theme.accent << 8) | COMMAND_BAR_FOCUSED_BORDER_ALPHA)
                        } else {
                            rgba((theme.border << 8) | COMMAND_BAR_INPUT_BORDER_ALPHA)
                        })
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .cursor_text()
                                // Tauri only focuses the command textarea when the
                                // row background or textarea area receives the
                                // pointer. Keep the quick-command button outside
                                // this hit region so its click cannot be captured
                                // by IME selection before the toggle handler runs.
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &gpui::MouseDownEvent, window, cx| {
                                            this.terminal_command_bar_focused = true;
                                            this.ime_marked_text = None;
                                            window.focus(&this.focus_handle);
                                            this.begin_ime_selection_from_mouse_down(
                                                WorkspaceImeTarget::TerminalCommandBar,
                                                event,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                )
                                .on_mouse_move(cx.listener(
                                    |this, event: &gpui::MouseMoveEvent, window, cx| {
                                        this.update_ime_selection_drag_from_mouse_move(
                                            event, window, cx,
                                        );
                                    },
                                ))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::ChevronRight,
                                    16.0,
                                    rgb(theme.text_muted),
                                ))
                                .child(text_input_anchor_probe(
                                    target.anchor_id(),
                                    command_input_content,
                                    {
                                        let workspace = workspace.clone();
                                        move |anchor, _window, cx| {
                                            let _ = workspace.update(cx, |this, cx| {
                                                this.update_text_input_anchor(anchor, cx);
                                            });
                                        }
                                    },
                                )),
                        )
                        .when(quick_commands_enabled, |input_row| {
                            input_row.child(
                                div()
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.md))
                                    .cursor_pointer()
                                    .bg(if self.terminal_quick_commands_open {
                                        rgba((theme.accent << 8) | 0x1a)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .text_color(if self.terminal_quick_commands_open {
                                        rgb(theme.accent)
                                    } else {
                                        rgb(theme.text_muted)
                                    })
                                    .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.terminal_quick_commands_open =
                                                !this.terminal_quick_commands_open;
                                            this.dismiss_terminal_broadcast_menu();
                                            this.close_terminal_cwd_picker();
                                            this.close_terminal_git_branch_picker();
                                            if !this.terminal_quick_commands_open {
                                                this.close_terminal_quick_commands_popover();
                                            }
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    )
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::Zap,
                                        14.0,
                                        if self.terminal_quick_commands_open {
                                            rgb(theme.accent)
                                        } else {
                                            rgb(theme.text_muted)
                                        },
                                    )),
                            )
                        }),
                )
            });
        select_anchor_probe(
            SelectAnchorId::TerminalCommandBar,
            bar,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_terminal_quick_commands_popover(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_quick_commands_popover(cx)
    }

    pub(in crate::workspace) fn render_terminal_broadcast_menu(
        &self,
        placement: TerminalBroadcastMenuPlacement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let entries = self.terminal_broadcast_entries();
        let active_pane_id = self.active_pane_id();
        let selectable = entries
            .iter()
            .filter(|(pane_id, _, _)| Some(*pane_id) != active_pane_id)
            .map(|(pane_id, _, _)| *pane_id)
            .collect::<Vec<_>>();
        let all_selected = !selectable.is_empty()
            && selectable
                .iter()
                .all(|pane_id| self.terminal_broadcast_targets.contains(pane_id));
        let anchor_left = self
            .select_anchors
            .get(&SelectAnchorId::TerminalBroadcastMenu)
            .map(|anchor| {
                // Tauri uses Radix DropdownMenuContent with `align="end"`.
                // Align to the trigger instead of the workspace root, because
                // the AI sidebar changes the root width but not the terminal
                // command-bar button's visual anchor.
                terminal_broadcast_menu_left_for_trigger_right(f32::from(anchor.bounds.right()))
            });

        let mut menu = context_menu_event_boundary({
            let menu = div()
                .absolute()
                .w(px(TERMINAL_BROADCAST_MENU_WIDTH))
                .max_h(px(320.0))
                .overflow_hidden()
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgba((theme.bg_elevated << 8) | 0xf2))
                .shadow_lg()
                .p(px(6.0))
                .text_size(px(12.0));
            if let Some(left) = anchor_left {
                menu.left(px(left))
            } else {
                menu.right(px(12.0))
            }
        })
        .child(
            div()
                .px(px(6.0))
                .py(px(4.0))
                .text_size(px(11.0))
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("terminal.broadcast.select_targets")),
        );
        menu = match placement {
            TerminalBroadcastMenuPlacement::Bottom(offset) => menu.bottom(px(offset)),
            TerminalBroadcastMenuPlacement::Top(offset) => menu.top(px(offset)),
        };

        if entries.len() <= 1 {
            menu = menu.child(
                div()
                    .px(px(8.0))
                    .py(px(12.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("terminal.broadcast.no_targets")),
            );
        } else {
            for (pane_id, label, kind) in entries {
                let is_current = Some(pane_id) == active_pane_id;
                let checked = self.terminal_broadcast_targets.contains(&pane_id);
                let badge = match kind {
                    TabKind::LocalTerminal => self.i18n.t("terminal.typeLocal"),
                    TabKind::SshTerminal => self.i18n.t("terminal.typeSsh"),
                    _ => String::new(),
                };
                let row_color = if is_current {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                };
                let row = div()
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .rounded(px(self.tokens.radii.md))
                    .text_color(row_color)
                    .child(if checked {
                        Self::render_lucide_icon(LucideIcon::Check, 12.0, rgb(theme.accent))
                    } else if is_current {
                        div()
                            .size(px(12.0))
                            .rounded_full()
                            .bg(rgb(theme.accent))
                            .into_any_element()
                    } else {
                        div().size(px(12.0)).into_any_element()
                    })
                    .child(div().flex_1().truncate().child(label))
                    .when(!badge.is_empty(), |row| {
                        row.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(self.tokens.radii.md))
                                .text_size(px(10.0))
                                .text_color(rgb(theme.text_muted))
                                .bg(rgba((theme.bg_panel << 8) | 0x99))
                                .child(badge),
                        )
                    })
                    .when(is_current, |row| {
                        row.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(self.tokens.radii.md))
                                .text_size(px(10.0))
                                .text_color(rgb(theme.accent))
                                .bg(rgba((theme.accent << 8) | 0x26))
                                .child(self.i18n.t("terminal.broadcast.current")),
                        )
                    });
                // Broadcast rows are checkbox-style menu items. Keep current
                // pane disabled through the shared menu action guard.
                let row = self.render_terminal_broadcast_menu_action(
                    row,
                    is_current,
                    false,
                    Some(rgb(theme.bg_hover)),
                    move |this, _event, _window, _cx| {
                        if this.terminal_broadcast_targets.remove(&pane_id) {
                            if this.terminal_broadcast_targets.is_empty() {
                                this.terminal_broadcast_enabled = false;
                            }
                        } else {
                            this.terminal_broadcast_targets.insert(pane_id);
                            this.terminal_broadcast_enabled = true;
                        }
                        this.keep_terminal_broadcast_menu_open();
                    },
                    cx,
                );
                menu = menu.child(row);
            }

            let select_all_disabled = selectable.is_empty();
            let select_all_label = div()
                .text_size(px(11.0))
                .text_color(rgb(theme.text_muted))
                .child(if all_selected {
                    self.i18n.t("terminal.broadcast.deselect_all")
                } else {
                    self.i18n.t("terminal.broadcast.select_all")
                });
            menu = menu.child(
                div()
                    .mt(px(4.0))
                    .pt(px(6.0))
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(6.0))
                    .child(self.workspace_context_menu_persistent_styled_action(
                        select_all_label,
                        select_all_disabled,
                        false,
                        ContextMenuActionableStyle {
                            hover_background: None,
                            hover_text_color: Some(rgb(theme.accent)),
                        },
                        move |this, _event, _window, _cx| {
                            if all_selected {
                                this.terminal_broadcast_enabled = false;
                                this.terminal_broadcast_targets.clear();
                            } else {
                                this.terminal_broadcast_targets =
                                    selectable.iter().copied().collect();
                                this.terminal_broadcast_enabled =
                                    !this.terminal_broadcast_targets.is_empty();
                            }
                            this.keep_terminal_broadcast_menu_open();
                        },
                        cx,
                    ))
                    .when(self.terminal_broadcast_enabled, |footer| {
                        footer.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme.accent))
                                .child(self.i18n.t("terminal.broadcast.target_count")),
                        )
                    }),
            );
        }

        // Target toggles redraw the persistent menu without replaying its entrance.
        oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            "terminal-broadcast-menu-enter",
            menu,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        )
    }

    pub(super) fn render_terminal_broadcast_menu_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        hover_bg: Option<gpui::Rgba>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Tauri broadcast target rows are Radix menu items with a disabled
        // current-terminal row. Keep native hover/cursor and action blocking
        // coupled to the shared context-menu guard.
        // Persistent menu rows still use one shared cx.listener wrapper so
        // toggling targets cannot re-enter WorkspaceApp during the click.
        self.workspace_context_menu_persistent_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: hover_bg,
                hover_text_color: None,
            },
            listener,
            cx,
        )
    }
}
