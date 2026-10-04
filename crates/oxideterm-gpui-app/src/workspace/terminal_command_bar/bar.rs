// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::StatefulInteractiveElement;
use oxideterm_gpui_ui::dropdown_menu::{
    DropdownMenuItemKind, dropdown_menu_content, dropdown_menu_item, dropdown_menu_separator,
};

const TERMINAL_RECORDING_MENU_WIDTH: f32 = 220.0;

impl WorkspaceApp {
    pub(super) fn render_terminal_command_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let workspace = cx.entity();
        let sender_visible = self.terminal_command_sender.read(cx).is_visible();
        let target_label = self.terminal_command_active_target_label(cx);
        let target_is_local = self.active_terminal_kind(cx)
            == Some(oxideterm_terminal::TerminalSessionKind::LocalPty)
            && target_label == self.i18n.t("terminal.command_bar.local_shell");
        let cwd_enabled = self.terminal_current_directory_awareness_enabled()
            && self
                .settings_store
                .settings()
                .terminal
                .command_bar
                .show_current_directory;
        let cwd_supported = cwd_enabled && self.active_terminal_cwd_scope_and_pane(cx).is_some();
        let cwd = cwd_enabled
            .then(|| self.active_terminal_cwd_snapshot(cx))
            .flatten();
        let git = self.active_terminal_git_snapshot(cx);
        let project = self
            .terminal_project_tasks_enabled()
            .then(|| self.active_terminal_project_snapshot(cx))
            .flatten();
        // Use the terminal surface width, not the window width: sidebars share the window.
        let compact = self
            .select_anchors
            .get(&SelectAnchorId::TerminalCommandBar)
            .is_some_and(|anchor| f32::from(anchor.bounds.size.width) < 640.0);
        let has_context = cwd_supported || git.is_some() || project.is_some();
        let split_visible = matches!(
            self.active_terminal_kind(cx),
            Some(
                oxideterm_terminal::TerminalSessionKind::LocalPty
                    | oxideterm_terminal::TerminalSessionKind::SshPty
            )
        );
        let tools_active = self
            .terminal_toolbar_menu
            .as_ref()
            .is_some_and(|menu| menu.kind == TerminalToolbarMenu::Tools)
            || self.terminal_highlight_popover_open
            || self.terminal_recording_menu_open
            || self.terminal.read(cx).broadcast_menu_open();
        let bar = div()
            .relative()
            .flex_none()
            .border_t_1()
            .border_color(self.workspace_chrome_divider())
            .bg(self.workspace_chrome_background(theme.bg))
            .px(px(self.tokens.spacing.two))
            .py(px(4.0))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .min_h(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .child(self.terminal_command_action_button(
                                if sender_visible {
                                    LucideIcon::ChevronDown
                                } else {
                                    LucideIcon::ChevronRight
                                },
                                rgb(theme.text_muted),
                                false,
                                None,
                                "terminal-command-sender-visibility",
                                self.i18n.t(if sender_visible {
                                    "terminal.sender.hide"
                                } else {
                                    "terminal.sender.show"
                                }),
                                |this, _, window, cx| {
                                    this.toggle_terminal_input(window, cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .child(self.render_terminal_target_indicator(
                                target_label,
                                target_is_local,
                                cx,
                            ))
                            .when(cwd_supported, |row| {
                                row.child(self.terminal_command_context_chip_slot(
                                    TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH,
                                    self.render_terminal_cwd_chip(cwd, cx),
                                ))
                            })
                            .when(!compact, |row| {
                                row.when_some(git, |row, snapshot| {
                                    row.child(self.terminal_command_context_chip_slot(
                                        TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH,
                                        self.render_terminal_git_chip(snapshot, cx),
                                    ))
                                })
                                .when_some(
                                    project,
                                    |row, snapshot| {
                                        row.child(self.terminal_command_context_chip_slot(
                                            TERMINAL_COMMAND_PROJECT_CHIP_MAX_WIDTH,
                                            self.render_terminal_project_chip(snapshot, cx),
                                        ))
                                    },
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(4.0))
                            .when(compact && has_context, |row| {
                                row.child(self.terminal_command_action_button(
                                    LucideIcon::MoreVertical,
                                    rgb(theme.text_muted),
                                    false,
                                    None,
                                    "terminal-context-overflow",
                                    self.i18n.t("terminal.command_bar.context"),
                                    |this, _, window, cx| {
                                        this.toggle_terminal_toolbar_menu(
                                            TerminalToolbarMenu::Context,
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    },
                                    cx,
                                ))
                            })
                            .child(self.terminal_toolbar_icon_button(
                                self.i18n.t("terminal.command_bar.search"),
                                LucideIcon::Search,
                                self.search_visible(cx),
                                false,
                                "terminal-command-search",
                                |this, _, window, cx| {
                                    this.close_terminal_command_overlays(cx);
                                    if this.search_visible(cx) {
                                        this.close_search(window, cx);
                                    } else {
                                        this.open_search(window, cx);
                                    }
                                },
                                cx,
                            ))
                            .when(split_visible, |row| {
                                row.child(self.terminal_toolbar_icon_button(
                                    self.i18n.t("terminal.command_bar.split"),
                                    LucideIcon::SplitSquareHorizontal,
                                    self.terminal_toolbar_menu.as_ref().is_some_and(|menu| {
                                        menu.kind == TerminalToolbarMenu::Split
                                    }),
                                    !self.can_split_active_pane(cx),
                                    "terminal-command-split",
                                    |this, _, window, cx| {
                                        this.toggle_terminal_toolbar_menu(
                                            TerminalToolbarMenu::Split,
                                            window,
                                            cx,
                                        )
                                    },
                                    cx,
                                ))
                            })
                            .when(
                                self.settings_store
                                    .settings()
                                    .terminal
                                    .command_bar
                                    .quick_commands_enabled,
                                |row| {
                                    row.child(self.terminal_toolbar_icon_button(
                                        self.i18n.t("terminal.quick_commands.title"),
                                        LucideIcon::Zap,
                                        self.terminal.read(cx).quick_commands.is_open(),
                                        false,
                                        "terminal-command-quick-commands",
                                        |this, _, window, cx| {
                                            this.toggle_terminal_quick_commands_panel(window, cx)
                                        },
                                        cx,
                                    ))
                                },
                            )
                            .child(self.terminal_toolbar_icon_button(
                                self.i18n.t(
                                    if self.terminal_command_sender.read(cx).is_expanded() {
                                        "terminal.sender.collapse"
                                    } else {
                                        "terminal.sender.expand"
                                    },
                                ),
                                LucideIcon::ListChecks,
                                self.terminal_command_sender.read(cx).is_expanded(),
                                false,
                                "terminal-command-sender-toggle",
                                |this, _, window, cx| this.toggle_terminal_sender_panel(window, cx),
                                cx,
                            ))
                            .child(select_anchor_probe(
                                SelectAnchorId::TerminalToolsMenu,
                                self.terminal_toolbar_icon_button(
                                    self.i18n.t("terminal.command_bar.tools"),
                                    LucideIcon::Settings,
                                    tools_active,
                                    false,
                                    "terminal-command-tools",
                                    |this, _, window, cx| {
                                        this.toggle_terminal_toolbar_menu(
                                            TerminalToolbarMenu::Tools,
                                            window,
                                            cx,
                                        )
                                    },
                                    cx,
                                ),
                                {
                                    let workspace = workspace.clone();
                                    move |anchor, _, cx| {
                                        let _ = workspace.update(cx, |this, cx| {
                                            this.update_select_anchor(anchor, cx)
                                        });
                                    }
                                },
                            )),
                    ),
            )
            .when(self.terminal_highlight_popover_open, |bar| {
                bar.child(self.render_terminal_highlight_popover(cx))
            })
            .when(self.terminal_recording_menu_open, |bar| {
                bar.child(self.render_terminal_recording_menu(cx))
            })
            .when(self.terminal.read(cx).git_panel_open(), |bar| {
                bar.child(self.render_terminal_git_branch_picker(cx))
            })
            .when(
                cwd_enabled && self.terminal.read(cx).cwd_picker_open(),
                |bar| bar.child(self.render_terminal_cwd_picker(cx)),
            )
            .when(
                self.terminal_project_tasks_enabled()
                    && self.terminal.read(cx).project_panel_open(),
                |bar| bar.child(self.render_terminal_project_panel(cx)),
            )
            .when_some(self.render_terminal_toolbar_menu(cx), |bar, menu| {
                bar.child(menu)
            });
        select_anchor_probe(
            SelectAnchorId::TerminalCommandBar,
            bar,
            move |anchor, _, cx| {
                let _ = workspace.update(cx, |this, cx| this.update_select_anchor(anchor, cx));
            },
        )
        .into_any_element()
    }

    pub(super) fn toggle_terminal_recording_menu(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_toolbar_menu();
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
        let recording = self.active_terminal_recording_status(cx).state;
        let session_log_status = self.active_terminal_session_log_status(cx);
        let session_log_available = self.active_terminal_session_log_available(cx);
        let menu = context_menu_event_boundary(
            dropdown_menu_content(&self.tokens)
                .absolute()
                .bottom_full()
                .mb(px(4.0))
                .right(px(8.0))
                .w(px(TERMINAL_RECORDING_MENU_WIDTH))
                .max_w_full()
                .max_h(px(self.terminal_toolbar_popup_available_height()))
                .occlude(),
        )
        .id("terminal-recording-menu-scroll")
        .overflow_y_scroll();
        let start_item = dropdown_menu_item(
            &self.tokens,
            self.i18n.t(match recording {
                TerminalRecordingState::Idle => "terminal.recording.start",
                TerminalRecordingState::Recording => "terminal.recording.pause",
                TerminalRecordingState::Paused => "terminal.recording.resume",
            }),
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
                move |this, _event, _window, cx| match recording {
                    TerminalRecordingState::Idle => this.start_active_terminal_recording(cx),
                    TerminalRecordingState::Recording => this.pause_active_terminal_recording(cx),
                    TerminalRecordingState::Paused => this.resume_active_terminal_recording(cx),
                },
                cx,
            ))
            .when(recording != TerminalRecordingState::Idle, |mut menu| {
                for (discard, key) in [
                    (false, "terminal.recording.stop"),
                    (true, "terminal.recording.discard"),
                ] {
                    let item = dropdown_menu_item(
                        &self.tokens,
                        self.i18n.t(key),
                        DropdownMenuItemKind::Plain,
                        false,
                        false,
                    );
                    menu = menu.child(self.workspace_context_menu_action(
                        item,
                        false,
                        false,
                        |this| {
                            this.terminal_recording_menu_open = false;
                        },
                        move |this, _, _, cx| {
                            if discard {
                                this.discard_active_terminal_recording(cx);
                            } else {
                                this.stop_active_terminal_recording(cx);
                            }
                        },
                        cx,
                    ));
                }
                menu
            })
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
            .get(&SelectAnchorId::TerminalToolsMenu)
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
                .max_h(px(match placement {
                    TerminalBroadcastMenuPlacement::Bottom(_) => self
                        .terminal_toolbar_popup_available_height()
                        .min(TERMINAL_BROADCAST_MENU_MAX_HEIGHT),
                    TerminalBroadcastMenuPlacement::Top(_) => TERMINAL_BROADCAST_MENU_MAX_HEIGHT,
                }))
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
