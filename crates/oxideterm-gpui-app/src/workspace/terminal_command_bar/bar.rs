// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::StatefulInteractiveElement;
use oxideterm_gpui_ui::dropdown_menu::{
    DropdownMenuItemKind, dropdown_menu_content, dropdown_menu_item, dropdown_menu_separator,
};

const TERMINAL_RECORDING_MENU_WIDTH: f32 = 220.0;
const TERMINAL_RECORDING_MENU_BOTTOM: f32 = 30.0;

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
        let is_local_terminal = self.active_terminal_kind(cx)
            == Some(oxideterm_terminal::TerminalSessionKind::LocalPty);
        let split_controls_visible = matches!(
            self.active_terminal_kind(cx),
            Some(
                oxideterm_terminal::TerminalSessionKind::LocalPty
                    | oxideterm_terminal::TerminalSessionKind::SshPty
            )
        );
        let can_configure_remote_integration = self.active_ssh_terminal_node_id(cx).is_some();
        let remote_integration_pending = self.remote_shell_integration_pending(cx);
        let remote_integration_tooltip_id = "terminal-command-configure-directory-tracking";
        let remote_integration_tooltip_title = self
            .i18n
            .t("settings_view.connections.shell_integration.toolbar_action");
        let target_indicator_is_local =
            is_local_terminal && target_label == self.i18n.t("terminal.command_bar.local_shell");
        let can_split = self.can_split_active_pane(cx);
        let broadcast_targets =
            self.terminal_broadcast_target_panes(active_pane_id.unwrap_or(PaneId(0)), cx);
        let broadcast_enabled = active_pane_id
            .and_then(|pane| self.terminal.read(cx).sync_groups().member(pane))
            .is_some_and(|member| {
                !member.isolated && self.terminal.read(cx).sync_groups().enabled(member.group)
            });
        let broadcast_label = if broadcast_enabled {
            broadcast_targets.len().to_string()
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
        let session_log_active =
            self.active_terminal_session_log_status(cx).state != TerminalSessionLogState::Idle;
        let timestamps_active = self.active_terminal_timestamps_enabled(cx);
        let highlight_override_active = self.active_terminal_highlight_override(cx);
        let timestamps_tooltip_title = if timestamps_active {
            self.i18n.t("terminal.recording.hide_timestamps")
        } else {
            self.i18n.t("terminal.recording.show_timestamps")
        };
        let recording_toggle_tooltip_title = match recording_status.state {
            TerminalRecordingState::Idle => self.i18n.t("terminal.recording.title"),
            TerminalRecordingState::Recording => self.i18n.t("terminal.recording.pause"),
            TerminalRecordingState::Paused => self.i18n.t("terminal.recording.resume"),
        };
        let capture_menu_tooltip_title = if session_log_active {
            self.i18n.t("terminal.session_log.title")
        } else {
            self.i18n.t("terminal.recording.title")
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
            .when(self.terminal_highlight_popover_open, |bar| {
                bar.child(self.render_terminal_highlight_popover(cx))
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
                                        this.blur_terminal_quick_commands_input(cx);
                                        this.terminal_command_sender.update(cx, |sender, cx| {
                                            sender.set_compact_focused(true, cx);
                                        });
                                        this.clear_ime_selection();
                                        window.focus(&this.focus_handle, cx);
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
                                row.child(self.terminal_command_context_chip_slot(
                                    TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH,
                                    self.render_terminal_cwd_chip(cwd_snapshot, cx),
                                ))
                            })
                            .when_some(git_snapshot, |row, snapshot| {
                                row.child(self.terminal_command_context_chip_slot(
                                    TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH,
                                    self.render_terminal_git_chip(snapshot, cx),
                                ))
                            })
                            .when_some(project_snapshot, |row, snapshot| {
                                row.child(self.terminal_command_context_chip_slot(
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
                            .when(split_controls_visible, |actions| {
                                // Transport readiness controls the disabled state; keeping SSH
                                // actions visible makes their placement match local terminals.
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
                                        this.close_terminal_quick_commands_panel(cx);
                                        this.close_terminal_command_overlays(cx);
                                        this.ime_marked_text = None;
                                    }
                                    this.terminal_command_sender.update(cx, |sender, cx| {
                                        sender.toggle_expanded(cx);
                                    });
                                    let sender_id =
                                        this.terminal_command_sender.read(cx).active_document_id();
                                    if expanding {
                                        this.focus_terminal_command_sender_editor(
                                            sender_id, window, cx,
                                        );
                                    } else {
                                        this.terminal_command_sender.update(cx, |sender, cx| {
                                            sender.set_compact_focused(true, cx);
                                        });
                                        this.clear_ime_selection();
                                        window.focus(&this.focus_handle, cx);
                                    }
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
                                        |this, _event, window, cx| {
                                            this.toggle_terminal_quick_commands_panel(window, cx);
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                                },
                            )
                            .when_some(active_pane_id, |actions, pane_id| {
                                // Capture the visible pane so the shortcut cannot retarget after a tab switch.
                                actions.child(self.terminal_command_action_button(
                                    LucideIcon::Activity,
                                    rgb(theme.text_muted),
                                    false,
                                    None,
                                    "terminal-command-session-triggers",
                                    self.i18n.t("terminal.command_selection.manage_triggers"),
                                    move |this, _event, window, cx| {
                                        this.open_terminal_trigger_settings_for_pane(
                                            pane_id, window, cx,
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
                                    if broadcast_enabled {
                                        rgb(theme.accent)
                                    } else {
                                        rgb(theme.text_muted)
                                    },
                                    false,
                                    Some(if broadcast_enabled {
                                        rgba((theme.accent << 8) | 0x26)
                                    } else {
                                        rgba(theme.bg_hover << 8)
                                    }),
                                    "terminal-command-broadcast",
                                    self.i18n.t("terminal.broadcast.select_targets"),
                                    |this, _event, _window, cx| {
                                        this.terminal_highlight_popover_open = false;
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
                            .child(select_anchor_probe(
                                SelectAnchorId::TerminalHighlightRuleSet,
                                self.terminal_command_action_button(
                                    LucideIcon::Hash,
                                    if highlight_override_active {
                                        rgb(theme.accent)
                                    } else {
                                        rgb(theme.text_muted)
                                    },
                                    false,
                                    Some(if highlight_override_active {
                                        rgba((theme.accent << 8) | 0x26)
                                    } else {
                                        rgba(0x00000000)
                                    }),
                                    "terminal-command-highlight-rules",
                                    self.i18n.t("terminal.highlight_override.title"),
                                    |this, _event, _window, cx| {
                                        this.toggle_terminal_highlight_popover(cx);
                                        cx.stop_propagation();
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
                                if self.search_visible(cx) {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                },
                                false,
                                Some(if self.search_visible(cx) {
                                    rgba((theme.accent << 8) | 0x26)
                                } else {
                                    rgba(0x00000000)
                                }),
                                "terminal-command-search",
                                self.i18n.t("search.placeholder"),
                                |this, _event, window, cx| {
                                    if this.search_visible(cx) {
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
                            .when(!recording_active, |actions| {
                                actions.child(
                                    div()
                                        .relative()
                                        .flex_none()
                                        .child(self.terminal_command_action_button(
                                            if session_log_active {
                                                LucideIcon::FileText
                                            } else {
                                                LucideIcon::FileVideo
                                            },
                                            if session_log_active {
                                                rgb(theme.accent)
                                            } else {
                                                rgb(theme.text_muted)
                                            },
                                            false,
                                            Some(if self.terminal_recording_menu_open {
                                                rgba((theme.accent << 8) | 0x26)
                                            } else {
                                                rgba(0x00000000)
                                            }),
                                            "terminal-command-recording-menu",
                                            capture_menu_tooltip_title.clone(),
                                            |this, _event, _window, cx| {
                                                this.toggle_terminal_recording_menu(cx);
                                                cx.stop_propagation();
                                            },
                                            cx,
                                        ))
                                        .when(self.terminal_recording_menu_open, |anchor| {
                                            // Keep the menu in the toolbar button's local coordinate
                                            // owner so its anchor is current in the same draw pass.
                                            anchor.child(self.render_terminal_recording_menu(cx))
                                        }),
                                )
                            })
                            .when(recording_active, |actions| {
                                actions.child(
                                    div()
                                        .relative()
                                        .flex_none()
                                        .child(self.terminal_command_action_button(
                                            LucideIcon::FileText,
                                            if session_log_active {
                                                rgb(theme.accent)
                                            } else {
                                                rgb(theme.text_muted)
                                            },
                                            false,
                                            Some(if self.terminal_recording_menu_open {
                                                rgba((theme.accent << 8) | 0x26)
                                            } else {
                                                rgba(0x00000000)
                                            }),
                                            "terminal-command-session-log-menu",
                                            self.i18n.t("terminal.session_log.title"),
                                            |this, _event, _window, cx| {
                                                this.toggle_terminal_recording_menu(cx);
                                                cx.stop_propagation();
                                            },
                                            cx,
                                        ))
                                        .when(self.terminal_recording_menu_open, |anchor| {
                                            anchor.child(self.render_terminal_recording_menu(cx))
                                        }),
                                )
                            })
                            .when(recording_active, |actions| {
                                actions.child(self.terminal_command_action_button(
                                    if recording_status.state == TerminalRecordingState::Paused {
                                        LucideIcon::Play
                                    } else {
                                        LucideIcon::Circle
                                    },
                                    rgb(theme.error),
                                    false,
                                    Some(rgba((theme.error << 8) | 0x26)),
                                    "terminal-command-recording-toggle",
                                    recording_toggle_tooltip_title.clone(),
                                    move |this, _event, _window, cx| {
                                        match recording_status.state {
                                            TerminalRecordingState::Recording => {
                                                this.pause_active_terminal_recording(cx)
                                            }
                                            TerminalRecordingState::Paused => {
                                                this.resume_active_terminal_recording(cx)
                                            }
                                            TerminalRecordingState::Idle => {}
                                        }
                                        cx.stop_propagation();
                                    },
                                    cx,
                                ))
                            })
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
                            }),
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

    fn toggle_terminal_recording_menu(&mut self, cx: &mut Context<Self>) {
        let should_open = !self.terminal_recording_menu_open;
        self.terminal_recording_menu_open = should_open;
        if should_open {
            self.blur_terminal_quick_commands_input(cx);
            self.dismiss_terminal_broadcast_menu(cx);
            self.dismiss_terminal_highlight_popover();
            self.close_terminal_cwd_picker(cx);
            self.close_terminal_git_branch_picker(cx);
            self.close_terminal_project_panel(cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn dismiss_terminal_recording_menu(&mut self) -> bool {
        std::mem::take(&mut self.terminal_recording_menu_open)
    }

    fn render_terminal_recording_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let session_log_status = self.active_terminal_session_log_status(cx);
        let session_log_available = self.active_terminal_session_log_available(cx);
        let menu = context_menu_event_boundary(
            dropdown_menu_content(&self.tokens)
                .absolute()
                .bottom(px(TERMINAL_RECORDING_MENU_BOTTOM))
                .right(px(0.0))
                .w(px(TERMINAL_RECORDING_MENU_WIDTH))
                .occlude(),
        );
        let start_item = dropdown_menu_item(
            &self.tokens,
            self.i18n.t("terminal.recording.start"),
            DropdownMenuItemKind::Plain,
            false,
            false,
        );
        let open_item = dropdown_menu_item(
            &self.tokens,
            self.i18n.t("terminal.recording.open_cast"),
            DropdownMenuItemKind::Plain,
            false,
            false,
        );

        let menu = menu
            .child(self.workspace_context_menu_styled_action(
                start_item,
                false,
                false,
                ContextMenuActionableStyle::default(),
                |this| {
                    this.terminal_recording_menu_open = false;
                },
                |this, _event, _window, cx| {
                    this.start_active_terminal_recording(cx);
                },
                cx,
            ))
            .child(self.workspace_context_menu_styled_action(
                open_item,
                false,
                false,
                ContextMenuActionableStyle::default(),
                |this| {
                    this.terminal_recording_menu_open = false;
                },
                |this, _event, window, cx| {
                    this.open_terminal_cast_file(window, cx);
                },
                cx,
            ))
            .child(dropdown_menu_separator(&self.tokens));

        let menu = match session_log_status.state {
            TerminalSessionLogState::Idle => {
                let item = dropdown_menu_item(
                    &self.tokens,
                    self.i18n.t("terminal.session_log.start"),
                    DropdownMenuItemKind::Plain,
                    false,
                    !session_log_available,
                );
                menu.child(self.workspace_context_menu_styled_action(
                    item,
                    !session_log_available,
                    false,
                    ContextMenuActionableStyle::default(),
                    |this| this.terminal_recording_menu_open = false,
                    |this, _event, _window, cx| this.start_active_terminal_session_log(cx),
                    cx,
                ))
            }
            TerminalSessionLogState::Logging => {
                let pause_item = dropdown_menu_item(
                    &self.tokens,
                    self.i18n.t("terminal.session_log.pause"),
                    DropdownMenuItemKind::Plain,
                    false,
                    false,
                );
                let stop_item = dropdown_menu_item(
                    &self.tokens,
                    self.i18n.t("terminal.session_log.stop"),
                    DropdownMenuItemKind::Plain,
                    false,
                    false,
                );
                menu.child(self.workspace_context_menu_styled_action(
                    pause_item,
                    false,
                    false,
                    ContextMenuActionableStyle::default(),
                    |this| this.terminal_recording_menu_open = false,
                    |this, _event, _window, cx| this.pause_active_terminal_session_log(cx),
                    cx,
                ))
                .child(self.workspace_context_menu_styled_action(
                    stop_item,
                    false,
                    false,
                    ContextMenuActionableStyle::default(),
                    |this| this.terminal_recording_menu_open = false,
                    |this, _event, _window, cx| this.stop_active_terminal_session_log(cx),
                    cx,
                ))
            }
            TerminalSessionLogState::Paused => {
                let resume_item = dropdown_menu_item(
                    &self.tokens,
                    self.i18n.t("terminal.session_log.resume"),
                    DropdownMenuItemKind::Plain,
                    false,
                    false,
                );
                let stop_item = dropdown_menu_item(
                    &self.tokens,
                    self.i18n.t("terminal.session_log.stop"),
                    DropdownMenuItemKind::Plain,
                    false,
                    false,
                );
                menu.child(self.workspace_context_menu_styled_action(
                    resume_item,
                    false,
                    false,
                    ContextMenuActionableStyle::default(),
                    |this| this.terminal_recording_menu_open = false,
                    |this, _event, _window, cx| this.resume_active_terminal_session_log(cx),
                    cx,
                ))
                .child(self.workspace_context_menu_styled_action(
                    stop_item,
                    false,
                    false,
                    ContextMenuActionableStyle::default(),
                    |this| this.terminal_recording_menu_open = false,
                    |this, _event, _window, cx| this.stop_active_terminal_session_log(cx),
                    cx,
                ))
            }
        };

        let menu = menu.when(session_log_status.path.is_some(), |menu| {
            let item = dropdown_menu_item(
                &self.tokens,
                self.i18n.t("terminal.session_log.open_file"),
                DropdownMenuItemKind::Plain,
                false,
                false,
            );
            menu.child(self.workspace_context_menu_styled_action(
                item,
                false,
                false,
                ContextMenuActionableStyle::default(),
                |this| this.terminal_recording_menu_open = false,
                |this, _event, _window, cx| this.open_active_terminal_session_log(cx),
                cx,
            ))
        });
        let directory_item = dropdown_menu_item(
            &self.tokens,
            self.i18n.t("terminal.session_log.open_directory"),
            DropdownMenuItemKind::Plain,
            false,
            false,
        );
        menu.child(self.workspace_context_menu_styled_action(
            directory_item,
            false,
            false,
            ContextMenuActionableStyle::default(),
            |this| this.terminal_recording_menu_open = false,
            |this, _event, _window, cx| this.open_terminal_session_log_directory(cx),
            cx,
        ))
        .into_any_element()
    }

    pub(in crate::workspace) fn render_terminal_broadcast_menu(
        &self,
        placement: TerminalBroadcastMenuPlacement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let entries = self.terminal_broadcast_entries(cx);
        let groups = self.terminal_broadcast_groups().to_vec();
        let active_pane_id = self.active_pane_id(cx);
        let selected_group_id = self.terminal.read(cx).selected_broadcast_group_id();
        let group_editor = self
            .terminal
            .read(cx)
            .broadcast_group_editor()
            .map(|(kind, value)| (kind, value.to_string()));
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
                .max_h(px(TERMINAL_BROADCAST_MENU_MAX_HEIGHT))
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
        .id("terminal-broadcast-menu-scroll")
        .overflow_y_scroll()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(6.0))
                .py(px(4.0))
                .text_size(px(11.0))
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("terminal.broadcast.groups"))
                .child(
                    div()
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(6.0))
                        .rounded(px(self.tokens.radii.sm))
                        .cursor_pointer()
                        .hover(|button| button.bg(rgb(theme.bg_hover)))
                        .child(Self::render_lucide_icon(
                            LucideIcon::Plus,
                            12.0,
                            rgb(theme.accent),
                        ))
                        .child(self.i18n.t("terminal.broadcast.create_group"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, window, cx| {
                                this.begin_terminal_broadcast_group_create(window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                ),
        );
        menu = match placement {
            TerminalBroadcastMenuPlacement::Bottom(offset) => menu.bottom(px(offset)),
            TerminalBroadcastMenuPlacement::Top(offset) => menu.top(px(offset)),
        };

        if groups.is_empty() && group_editor.is_none() {
            menu = menu.child(
                div()
                    .px(px(8.0))
                    .pb(px(8.0))
                    .text_size(px(11.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("terminal.broadcast.no_groups")),
            );
        }

        menu = menu.child(
            div()
                .h(px(30.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .rounded(px(self.tokens.radii.md))
                .cursor_pointer()
                .when(selected_group_id.is_none(), |row| {
                    row.bg(rgba((theme.accent << 8) | 0x1f))
                })
                .hover(|row| row.bg(rgb(theme.bg_hover)))
                .child(if selected_group_id.is_none() {
                    Self::render_lucide_icon(LucideIcon::Check, 12.0, rgb(theme.accent))
                } else {
                    div().size(px(12.0)).into_any_element()
                })
                .child(self.i18n.t("terminal.broadcast.temporary_group"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.terminal.update(cx, |terminal, _cx| {
                            terminal.clear_selected_broadcast_group();
                        });
                        cx.notify();
                        cx.stop_propagation();
                    }),
                ),
        );

        for group in &groups {
            let group_id = group.id;
            let selected = selected_group_id == Some(group_id);
            let member_count = self
                .terminal
                .read(cx)
                .sync_groups()
                .panes(Some(group_id))
                .len();
            let name = group.name.clone();
            menu = menu.child(
                div()
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .when(selected, |row| row.bg(rgba((theme.accent << 8) | 0x1f)))
                    .hover(|row| row.bg(rgb(theme.bg_hover)))
                    .child(if selected {
                        Self::render_lucide_icon(LucideIcon::Check, 12.0, rgb(theme.accent))
                    } else {
                        div().size(px(12.0)).into_any_element()
                    })
                    .child(div().flex_1().truncate().child(name))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.accent))
                            .child(self.i18n.t(
                                if self.terminal.read(cx).sync_groups().enabled(Some(group_id)) {
                                    "terminal.broadcast.sync_enabled"
                                } else {
                                    "terminal.broadcast.sync_disabled"
                                },
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(member_count.to_string()),
                    )
                    .child(
                        div()
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .hover(|button| button.bg(rgb(theme.bg_panel)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Pencil,
                                11.0,
                                rgb(theme.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, window, cx| {
                                    this.begin_terminal_broadcast_group_rename(
                                        group_id, window, cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .hover(|button| button.bg(rgba((theme.error << 8) | 0x22)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Trash2,
                                11.0,
                                rgb(theme.error),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.delete_terminal_broadcast_group(group_id, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.select_terminal_broadcast_group(group_id, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }

        if let Some((edit_kind, value)) = group_editor {
            let target = WorkspaceImeTarget::TerminalBroadcastGroupName;
            let valid = self.terminal_broadcast_group_name_valid(edit_kind, &value);
            menu = menu.child(
                div()
                    .mt(px(4.0))
                    .px(px(6.0))
                    .pb(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        self.text_input_with_workspace_ime(
                            target,
                            text_input(
                                &self.tokens,
                                TextInputView {
                                    value: &value,
                                    placeholder: self
                                        .i18n
                                        .t("terminal.broadcast.group_name_placeholder"),
                                    focused: true,
                                    caret_visible: self.input_caret.visible(),
                                    secret: false,
                                    selected_all: false,
                                    selected_range: self.ime_selected_range_for_target(target, cx),
                                    marked_text: self.marked_text_for_target(target, cx),
                                },
                            )
                            .h(px(28.0)),
                            |_this, _cx| {},
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .when(valid, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|button| button.bg(rgb(theme.bg_hover)))
                            })
                            .child(Self::render_lucide_icon(
                                LucideIcon::Save,
                                12.0,
                                rgb(if valid {
                                    theme.accent
                                } else {
                                    theme.text_muted
                                }),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    if valid {
                                        this.commit_terminal_broadcast_group_edit(cx);
                                    }
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .cursor_pointer()
                            .hover(|button| button.bg(rgb(theme.bg_hover)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::X,
                                12.0,
                                rgb(theme.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.terminal.update(cx, |terminal, _cx| {
                                        terminal.cancel_broadcast_group_edit();
                                    });
                                    this.clear_ime_selection();
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            );
        }

        let enabled = self.terminal.read(cx).broadcast_enabled();
        let selected_count = self
            .terminal
            .read(cx)
            .sync_groups()
            .panes(selected_group_id)
            .len();
        menu = menu.child(
            div()
                .border_t_1()
                .border_color(rgb(theme.border))
                .mt(px(4.0))
                .p(px(6.0))
                .flex()
                .items_center()
                .justify_between()
                .child(self.i18n.t("terminal.broadcast.group_members"))
                .child(self.terminal_sync_action_button(
                    self.i18n.t(if enabled {
                        "terminal.broadcast.pause_group"
                    } else {
                        "terminal.broadcast.start_group"
                    }),
                    selected_count > 0,
                    |this, _, _, cx| {
                        this.terminal
                            .update(cx, |terminal, _| terminal.toggle_broadcast());
                        cx.stop_propagation();
                        cx.notify();
                    },
                    cx,
                )),
        );
        menu = menu.child(
            div()
                .px(px(6.0))
                .pb(px(6.0))
                .text_size(px(11.0))
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("terminal.broadcast.members_hint")),
        );
        if entries.is_empty() {
            menu = menu.child(
                div()
                    .p(px(8.0))
                    .child(self.i18n.t("terminal.broadcast.no_targets")),
            );
        }
        for entry in entries {
            let pane_id = entry.pane_id;
            let member = self.terminal.read(cx).sync_groups().member(pane_id);
            let checked = member.is_some_and(|member| member.group == selected_group_id);
            let other_group = member.filter(|member| member.group != selected_group_id);
            let row = div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(5.0))
                .child(if checked {
                    Self::render_lucide_icon(LucideIcon::Check, 12.0, rgb(theme.accent))
                } else {
                    div().size(px(12.0)).into_any_element()
                })
                .child(div().flex_1().min_w_0().truncate().child(entry.label))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(10.0))
                        .text_color(rgb(theme.text_muted))
                        .child(format!("#{}", pane_id.0)),
                )
                .when(Some(pane_id) == active_pane_id, |row| {
                    row.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.accent))
                            .child(self.i18n.t("terminal.broadcast.current")),
                    )
                })
                .when_some(other_group, |row, member| {
                    row.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.terminal_sync_group_name(member.group)),
                    )
                })
                .when(checked, |row| {
                    row.child(
                        self.terminal_sync_action_button(
                            self.i18n
                                .t(if member.is_some_and(|member| member.isolated) {
                                    "terminal.broadcast.resume_member"
                                } else {
                                    "terminal.broadcast.isolate_member"
                                }),
                            true,
                            move |this, _, _, cx| {
                                this.toggle_terminal_sync_isolation(pane_id, cx);
                                cx.stop_propagation();
                            },
                            cx,
                        ),
                    )
                });
            menu = menu.child(self.render_terminal_broadcast_menu_action(
                row,
                other_group.is_some(),
                false,
                Some(rgb(theme.bg_hover)),
                move |this, _, _, cx| {
                    this.toggle_terminal_broadcast_group_member(pane_id, cx);
                },
                cx,
            ));
        }

        menu.into_any_element()
    }

    pub(in crate::workspace) fn terminal_sync_action_button(
        &self,
        label: String,
        enabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_toolbar_action_button(
            label,
            None,
            oxideterm_gpui_ui::button::ToolbarButtonOptions {
                button: oxideterm_gpui_ui::button::ButtonOptions {
                    variant: oxideterm_gpui_ui::button::ButtonVariant::Ghost,
                    disabled: !enabled,
                    ..Default::default()
                },
                height: Some(22.0),
                padding_x: Some(6.0),
                ..Default::default()
            },
            cx.listener(listener),
        )
    }

    pub(in crate::workspace) fn render_terminal_sync_member_header(
        &self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let member = self.terminal.read(cx).sync_groups().member(pane_id)?;
        let enabled = self.terminal.read(cx).sync_groups().enabled(member.group);
        let theme = self.tokens.ui;
        let state = if member.isolated {
            "terminal.broadcast.sync_isolated"
        } else if enabled {
            "terminal.broadcast.sync_enabled"
        } else {
            "terminal.broadcast.sync_disabled"
        };
        Some(
            div()
                .w_full()
                .flex_none()
                .h(px(TERMINAL_SYNC_HEADER_HEIGHT))
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .bg(self.workspace_chrome_background(theme.bg))
                .border_b_1()
                .border_color(self.workspace_chrome_divider())
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(if member.isolated {
                    theme.warning
                } else if enabled {
                    theme.accent
                } else {
                    theme.text_muted
                }))
                .child(Self::render_lucide_icon(
                    LucideIcon::Radio,
                    12.0,
                    rgb(theme.accent),
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(self.terminal_sync_group_name(member.group)),
                )
                .child(div().flex_none().child(self.i18n.t(state)))
                .child(self.terminal_sync_action_button(
                    self.i18n.t(if member.isolated {
                        "terminal.broadcast.resume_member"
                    } else {
                        "terminal.broadcast.isolate_member"
                    }),
                    true,
                    move |this, _, _, cx| {
                        this.toggle_terminal_sync_isolation(pane_id, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .into_any_element(),
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
        // A pane owned by another group stays disabled until explicitly removed there.
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
