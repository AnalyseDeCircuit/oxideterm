// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn render_terminal_command_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        const COMMAND_BAR_BG_ALPHA: u32 = 0xf2; // Tauri bg-theme-bg/95
        const COMMAND_BAR_BORDER_ALPHA: u32 = 0xb3; // Tauri border-theme-border/70

        let theme = self.tokens.ui;
        let command_bar_background = if self.window_background_preferences().is_some() {
            self.workspace_chrome_background(theme.bg)
        } else {
            rgba((theme.bg << 8) | COMMAND_BAR_BG_ALPHA)
        };
        let workspace = cx.entity();
        // The visible chip and completion providers share Tauri's target-label
        // inference so local shells that are currently inside SSH show the
        // remote identity consistently in both places.
        let target_label = self.terminal_command_active_target_label(cx);
        let cwd_display_enabled = self.terminal_current_directory_awareness_enabled()
            && self
                .settings_store
                .settings()
                .terminal
                .command_bar
                .show_current_directory;
        let cwd_snapshot = cwd_display_enabled
            .then(|| self.active_terminal_cwd_snapshot(cx))
            .flatten();
        let cwd_supported =
            cwd_display_enabled && self.active_terminal_cwd_scope_and_pane(cx).is_some();
        let git_snapshot = self.active_terminal_git_snapshot(cx);
        let project_tasks_enabled = self.terminal_project_tasks_enabled();
        let project_snapshot = project_tasks_enabled
            .then(|| self.active_terminal_project_snapshot(cx))
            .flatten();
        let active_pane_id = self.active_pane_id(cx);
        let is_local_terminal = self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == TabKind::LocalTerminal);
        let can_configure_remote_integration = self.active_ssh_terminal_node_id(cx).is_some();
        let remote_integration_pending = self.remote_shell_integration_pending(cx);
        let remote_integration_tooltip_id = "terminal-command-configure-directory-tracking";
        let remote_integration_tooltip_title = self
            .i18n
            .t("settings_view.connections.shell_integration.toolbar_action");
        let target_indicator_is_local =
            is_local_terminal && target_label == self.i18n.t("terminal.command_bar.local_shell");
        let can_split = self.active_tab(cx).is_some_and(|tab| {
            tab.kind == TabKind::LocalTerminal
                && !self.active_tab_has_serial_terminal(cx)
                && tab
                    .root_pane
                    .as_ref()
                    .is_some_and(|root| root.pane_count() < MAX_PANES_PER_TAB)
        });
        let broadcast_targets =
            self.terminal_broadcast_target_panes(active_pane_id.unwrap_or(PaneId(0)), cx);
        let (broadcast_enabled, broadcast_targets_empty) = {
            let terminal = self.terminal.read(cx);
            (
                terminal.broadcast_enabled(),
                terminal.broadcast_targets_empty(),
            )
        };
        let broadcast_label = if broadcast_enabled {
            if broadcast_targets_empty {
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
        let quick_commands_open = self.terminal.read(cx).quick_commands.is_open();
        let (command_sender_visible, command_sender_expanded, command_sender_running_count) = {
            let sender = self.terminal_command_sender.read(cx);
            (
                sender.is_visible(),
                sender.is_expanded(),
                sender.running_count(),
            )
        };
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;
        let timestamps_active = self.active_terminal_timestamps_enabled(cx);
        let timestamps_tooltip_title = if timestamps_active {
            self.i18n.t("terminal.recording.hide_timestamps")
        } else {
            self.i18n.t("terminal.recording.show_timestamps")
        };
        let recording_toggle_tooltip_title = match recording_status.state {
            TerminalRecordingState::Idle => self.i18n.t("terminal.recording.start"),
            TerminalRecordingState::Recording => self.i18n.t("terminal.recording.pause"),
            TerminalRecordingState::Paused => self.i18n.t("terminal.recording.resume"),
        };
        let bar = div()
            .relative()
            .flex_none()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | COMMAND_BAR_BORDER_ALPHA))
            .bg(command_bar_background)
            .px(px(12.0))
            .py(px(4.0))
            .shadow_lg()
            .when(quick_commands_enabled && quick_commands_open, |bar| {
                // Tauri renders QuickCommandsPopover as a child of the relative
                // TerminalCommandBar (`absolute bottom-full right-3`). Keep the
                // native popover on the same local coordinate owner; routing it
                // through the root backdrop makes the existing bottom/right
                // placement resolve against the wrong box.
                bar.child(self.render_terminal_quick_commands_popover(cx))
            })
            .when(self.terminal.read(cx).git_panel_open(), |bar| {
                bar.child(self.render_terminal_git_branch_picker(cx))
            })
            .when(
                cwd_display_enabled && self.terminal.read(cx).cwd_picker_open(),
                |bar| bar.child(self.render_terminal_cwd_picker(cx)),
            )
            .when(
                project_tasks_enabled && self.terminal.read(cx).project_panel_open(),
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
                            .child(self.terminal_command_action_button(
                                if command_sender_visible {
                                    LucideIcon::ChevronDown
                                } else {
                                    LucideIcon::ChevronRight
                                },
                                rgb(theme.text_muted),
                                false,
                                Some(if command_sender_visible {
                                    rgba(0x00000000)
                                } else {
                                    rgba((theme.bg_hover << 8) | 0x99)
                                }),
                                "terminal-command-sender-visibility",
                                if command_sender_visible {
                                    self.i18n.t("terminal.sender.hide")
                                } else {
                                    self.i18n.t("terminal.sender.show")
                                },
                                |this, _event, window, cx| {
                                    let visible = this
                                        .terminal_command_sender
                                        .update(cx, |sender, cx| sender.toggle_visible(cx));
                                    if visible {
                                        let sender_id = this
                                            .terminal_command_sender
                                            .read(cx)
                                            .active_document_id();
                                        this.focus_terminal_command_sender_editor(
                                            sender_id, window, cx,
                                        );
                                    } else {
                                        this.focus_active_pane(window, cx);
                                    }
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
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
                                broadcast_enabled && !broadcast_label.is_empty(),
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
                                        "terminal-command-split-horizontal",
                                        self.i18n.t("command_palette.cmd_split_horizontal"),
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
                                        "terminal-command-split-vertical",
                                        self.i18n.t("command_palette.cmd_split_vertical"),
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
                            .when(can_configure_remote_integration, |actions| {
                                actions.child(self.terminal_command_action_button(
                                    LucideIcon::FolderSync,
                                    rgb(theme.text_muted),
                                    remote_integration_pending,
                                    None,
                                    remote_integration_tooltip_id,
                                    remote_integration_tooltip_title,
                                    |this, _event, _window, cx| {
                                        this.open_remote_shell_integration_confirm(cx);
                                        cx.stop_propagation();
                                    },
                                    cx,
                                ))
                            })
                            .child(self.terminal_command_action_button(
                                LucideIcon::ListChecks,
                                if command_sender_expanded {
                                    rgb(theme.accent)
                                } else if command_sender_running_count > 0 {
                                    rgb(theme.warning)
                                } else {
                                    rgb(theme.text_muted)
                                },
                                false,
                                Some(if command_sender_expanded {
                                    rgba((theme.accent << 8) | 0x26)
                                } else {
                                    rgba(0x00000000)
                                }),
                                "terminal-command-sender-toggle",
                                if command_sender_expanded {
                                    self.i18n.t("terminal.sender.collapse")
                                } else if command_sender_running_count > 0 {
                                    format!(
                                        "{} ({})",
                                        self.i18n.t("terminal.sender.running"),
                                        command_sender_running_count
                                    )
                                } else {
                                    self.i18n.t("terminal.sender.expand")
                                },
                                move |this, _event, window, cx| {
                                    let expanding =
                                        !this.terminal_command_sender.read(cx).is_expanded();
                                    if expanding {
                                        this.close_terminal_command_overlays(cx);
                                        this.ime_marked_text = None;
                                    }
                                    this.terminal_command_sender.update(cx, |sender, cx| {
                                        sender.toggle_expanded(cx);
                                    });
                                    let sender_id =
                                        this.terminal_command_sender.read(cx).active_document_id();
                                    this.focus_terminal_command_sender_editor(
                                        sender_id, window, cx,
                                    );
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .when(command_sender_running_count > 0, |actions| {
                                actions.child(
                                    div()
                                        .h(px(20.0))
                                        .min_w(px(20.0))
                                        .px(px(5.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_full()
                                        .bg(rgba((theme.warning << 8) | 0x24))
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme.warning))
                                        .child(command_sender_running_count.to_string()),
                                )
                            })
                            .when(
                                quick_commands_enabled
                                    && (!command_sender_visible || command_sender_expanded),
                                |actions| {
                                    actions.child(self.terminal_command_action_button(
                                        LucideIcon::Zap,
                                        if quick_commands_open {
                                            rgb(theme.accent)
                                        } else {
                                            rgb(theme.text_muted)
                                        },
                                        false,
                                        Some(if quick_commands_open {
                                            rgba((theme.accent << 8) | 0x26)
                                        } else {
                                            rgba(0x00000000)
                                        }),
                                        "terminal-command-quick-commands",
                                        self.i18n.t("terminal.quick_commands.title"),
                                        |this, _event, _window, cx| {
                                            this.terminal.update(cx, |terminal, _cx| {
                                                terminal.quick_commands.toggle_open()
                                            });
                                            this.dismiss_terminal_broadcast_menu(cx);
                                            this.close_terminal_cwd_picker(cx);
                                            this.close_terminal_git_branch_picker(cx);
                                            this.close_terminal_project_panel(cx);
                                            cx.stop_propagation();
                                            cx.notify();
                                        },
                                        cx,
                                    ))
                                },
                            )
                            .child(select_anchor_probe(
                                SelectAnchorId::TerminalBroadcastMenu,
                                self.terminal_command_action_button(
                                    LucideIcon::Radio,
                                    if broadcast_enabled {
                                        rgb(theme.accent)
                                    } else {
                                        rgb(theme.text_muted)
                                    },
                                    false,
                                    Some(if broadcast_enabled {
                                        rgba((theme.accent << 8) | 0x26)
                                    } else {
                                        rgba((theme.bg_hover << 8) | 0x00)
                                    }),
                                    "terminal-command-broadcast",
                                    self.i18n.t("terminal.broadcast.select_targets"),
                                    |this, _event, _window, cx| {
                                        this.toggle_terminal_broadcast_menu(cx);
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
                                "terminal-command-search",
                                self.i18n.t("search.placeholder"),
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
                                "terminal-command-timestamps",
                                timestamps_tooltip_title,
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
                                "terminal-command-recording-toggle",
                                recording_toggle_tooltip_title,
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
                                        "terminal-command-recording-stop",
                                        self.i18n.t("terminal.recording.stop"),
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
                                        "terminal-command-recording-discard",
                                        self.i18n.t("terminal.recording.discard"),
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
                                "terminal-command-recording-open",
                                self.i18n.t("terminal.recording.open_cast"),
                                |this, _event, window, cx| {
                                    this.open_terminal_cast_file(window, cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    ),
            );
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
        let entries = self.terminal_broadcast_entries(cx);
        let active_pane_id = self.active_pane_id(cx);
        let selectable = entries
            .iter()
            .filter(|(pane_id, _, _)| Some(*pane_id) != active_pane_id)
            .map(|(pane_id, _, _)| *pane_id)
            .collect::<Vec<_>>();
        let all_selected = !selectable.is_empty() && {
            let terminal = self.terminal.read(cx);
            selectable
                .iter()
                .all(|pane_id| terminal.broadcast_target_selected(*pane_id))
        };
        let broadcast_enabled = self.terminal.read(cx).broadcast_enabled();
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
                let checked = self.terminal.read(cx).broadcast_target_selected(pane_id);
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
                    move |this, _event, _window, cx| {
                        this.terminal.update(cx, |terminal, _cx| {
                            terminal.toggle_broadcast_target(pane_id);
                        });
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
                        move |this, _event, _window, cx| {
                            this.terminal.update(cx, |terminal, _cx| {
                                let targets = if all_selected {
                                    &[][..]
                                } else {
                                    selectable.as_slice()
                                };
                                terminal.set_broadcast_targets(targets);
                            });
                        },
                        cx,
                    ))
                    .when(broadcast_enabled, |footer| {
                        footer.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme.accent))
                                .child(self.i18n.t("terminal.broadcast.target_count")),
                        )
                    }),
            );
        }

        menu.into_any_element()
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
