// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::workspace::terminal_command_sender::TerminalCommandSenderStatus;
use oxideterm_gpui_ui::context_menu::{ContextMenuItemKind, context_menu_item_row};
use oxideterm_gpui_ui::dropdown_menu::{dropdown_menu_content, dropdown_menu_separator};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::workspace) enum TerminalToolbarMenu {
    Tools,
    Split,
    Context,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn terminal_toolbar_and_sender_render_without_losing_state(cx: &mut TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_TOOLBAR_RENDER_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Storage discovery is process-wide; use the same portable subprocess
            // boundary as the workspace palette test without reading user stores.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            std::fs::hard_link(&executable, &child).unwrap();
            std::fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .arg(cx.test_function_name().unwrap())
                .arg("--nocapture")
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "toolbar regression failed:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let path = default_settings_path();
        assert!(path.starts_with(std::path::PathBuf::from(fixture_dir)));
        let mut settings = SettingsStore::load_from_path(path).unwrap();
        settings.settings_mut().onboarding_completed = true;
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().terminal.command_bar.enabled = true;
        settings
            .settings_mut()
            .terminal
            .command_bar
            .quick_commands_enabled = true;
        settings.save().unwrap();

        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.update(|window, cx| {
            let pane = cx.new(|cx| {
                TerminalPane::new_recording_playback(80, 24, Default::default(), window, cx)
                    .unwrap()
            });
            workspace.update(cx, |workspace, cx| {
                workspace
                    .tokens
                    .apply_motion(oxideterm_theme::UiMotionProfile::Off);
                let tab_id = workspace.alloc_tab_id(cx);
                let pane_id = workspace.alloc_pane_id(cx);
                let session_id = workspace.alloc_session_id(cx);
                workspace.tab_host.update(cx, |host, cx| {
                    host.insert_and_select_main_tab(Tab {
                        id: tab_id,
                        kind: TabKind::LocalTerminal,
                        title: "Toolbar regression".into(),
                        title_source: TabTitleSource::Static,
                        root_pane: Some(PaneNode::leaf(pane_id, session_id)),
                        active_pane_id: Some(pane_id),
                    });
                    host.register_terminal_pane(
                        pane_id,
                        session_id,
                        pane,
                        window.window_handle(),
                        cx,
                    );
                    host.bind_terminal_location(session_id, TerminalLocation { tab_id, pane_id });
                });
                workspace.sync_active_tab_surface(cx);
                assert!(!workspace.active_terminal_timestamps_enabled(cx));
                workspace.toggle_terminal_toolbar_menu(TerminalToolbarMenu::Tools, window, cx);
            });
            // This runs the actual toolbar/menu builders, including GPUI's
            // single-hover-style assertion, rather than only compiling them.
            window.draw(cx).clear(cx);
        });
        cx.simulate_keystrokes("down down down down down down enter");
        workspace.update(cx, |workspace, cx| {
            assert!(workspace.active_terminal_timestamps_enabled(cx));
            assert!(workspace.terminal_toolbar_menu.is_none());
        });
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.toggle_terminal_toolbar_menu(TerminalToolbarMenu::Tools, window, cx);
            });
            window.draw(cx).clear(cx);
        });
        cx.simulate_keystrokes("escape");
        workspace.update(cx, |workspace, cx| {
            assert!(workspace.terminal_toolbar_menu.is_none());
            assert!(workspace.active_terminal_timestamps_enabled(cx));
        });
        let editor = workspace.read_with(cx, |workspace, cx| {
            workspace
                .terminal_command_sender
                .read(cx)
                .active_document_snapshot()
                .unwrap()
                .editor
                .entity_id()
        });
        for (mode, scope) in [
            (oxideterm_terminal::TerminalSenderInputMode::Text, crate::workspace::terminal_command_sender::TerminalCommandSenderTargetScope::Current),
            (oxideterm_terminal::TerminalSenderInputMode::Hex, crate::workspace::terminal_command_sender::TerminalCommandSenderTargetScope::Selected),
        ] {
            cx.update(|window, cx| {
                workspace.update(cx, |workspace, cx| {
                    if !workspace.terminal_command_sender.read(cx).is_expanded() {
                        workspace.toggle_terminal_sender_panel(window, cx);
                    }
                    workspace.terminal_command_sender.update(cx, |sender, cx| {
                        let id = sender.active_document_id();
                        sender.set_input_mode(id, mode, cx);
                        sender.set_target_scope(id, scope, cx);
                    });
                });
                window.draw(cx).clear(cx);
            });
            workspace.read_with(cx, |workspace, cx| {
                assert_eq!(workspace.terminal_command_sender.read(cx).active_document_snapshot().unwrap().editor.entity_id(), editor,
                    "changing sender controls must preserve the editing session");
            });
        }
    }
}

#[derive(Clone)]
pub(in crate::workspace) struct TerminalToolbarMenuState {
    pub(super) kind: TerminalToolbarMenu,
    pane: PaneId,
    selected: Option<usize>,
    scroll: gpui::ScrollHandle,
}

#[derive(Clone, Copy)]
enum ToolbarAction {
    Sender,
    QuickCommands,
    Input,
    Broadcast,
    Highlight,
    Timestamps,
    Recording,
    Triggers,
    DirectoryTracking,
    SplitHorizontal,
    SplitVertical,
    Cwd,
    Git,
    Project,
}

impl WorkspaceApp {
    pub(super) fn terminal_toolbar_icon_button(
        &self,
        label: String,
        icon: LucideIcon,
        active: bool,
        disabled: bool,
        tooltip_id: &'static str,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        self.terminal_command_action_button(
            icon,
            rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }),
            disabled,
            active.then(|| rgba((self.tokens.ui.accent << 8) | 0x26)),
            tooltip_id,
            label,
            move |this, event, window, cx| {
                listener(this, event, window, cx);
                cx.stop_propagation();
            },
            cx,
        )
    }

    pub(super) fn terminal_toolbar_button(
        &self,
        label: String,
        icon: LucideIcon,
        active: bool,
        disabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let options = ActionChipOptions::new()
            .height(24.0)
            .font_size(self.tokens.metrics.ui_text_sm)
            .active(active)
            .disabled(disabled);
        let chip = action_chip(
            &self.tokens,
            label,
            Some(Self::render_lucide_icon(
                icon,
                14.0,
                action_chip_foreground(&self.tokens, options),
            )),
            options,
        )
        .border_0();
        self.workspace_context_menu_action(chip, disabled, false, |_| {}, listener, cx)
    }

    pub(super) fn toggle_terminal_sender_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let expanding = !self.terminal_command_sender.read(cx).is_expanded();
        if expanding {
            self.close_terminal_quick_commands_panel(cx);
            self.close_terminal_command_overlays(cx);
            self.ime_marked_text = None;
        }
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.toggle_expanded(cx);
        });
        let id = self.terminal_command_sender.read(cx).active_document_id();
        if expanding {
            self.focus_terminal_command_sender_editor(id, window, cx);
        } else {
            self.terminal_command_sender.update(cx, |sender, cx| {
                sender.set_compact_focused(true, cx);
            });
            self.clear_ime_selection();
            window.focus(&self.focus_handle, cx);
        }
    }

    pub(super) fn toggle_terminal_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_terminal_command_overlays(cx);
        let visible = self
            .terminal_command_sender
            .update(cx, |sender, cx| sender.toggle_visible(cx));
        if visible {
            self.blur_terminal_quick_commands_input(cx);
            self.terminal_command_sender
                .update(cx, |sender, cx| sender.set_compact_focused(true, cx));
            self.clear_ime_selection();
            window.focus(&self.focus_handle, cx);
        } else {
            self.focus_active_pane(window, cx);
        }
    }

    pub(in crate::workspace) fn dismiss_terminal_toolbar_menu(&mut self) -> bool {
        self.terminal_toolbar_menu.take().is_some()
    }

    pub(super) fn toggle_terminal_toolbar_menu(
        &mut self,
        kind: TerminalToolbarMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_open = self
            .terminal_toolbar_menu
            .as_ref()
            .is_some_and(|menu| menu.kind == kind);
        self.close_terminal_command_overlays(cx);
        if !was_open {
            let Some(pane) = self.active_pane_id(cx) else {
                return;
            };
            self.blur_terminal_quick_commands_input(cx);
            self.terminal_command_sender.update(cx, |sender, cx| {
                sender.set_compact_focused(false, cx);
                sender.dismiss_compact_suggestions();
            });
            self.ime_marked_text = None;
            self.terminal_toolbar_menu = Some(TerminalToolbarMenuState {
                kind,
                pane,
                selected: None,
                scroll: gpui::ScrollHandle::new(),
            });
            window.focus(&self.focus_handle, cx);
        } else {
            self.focus_active_pane(window, cx);
        }
        cx.notify();
    }

    fn terminal_toolbar_items(
        &self,
        kind: TerminalToolbarMenu,
        cx: &mut Context<Self>,
    ) -> Vec<(ToolbarAction, String, bool)> {
        use ToolbarAction::*;
        let mut items = Vec::new();
        match kind {
            TerminalToolbarMenu::Tools => {
                items.push((Sender, self.i18n.t("terminal.command_bar.sender"), false));
                if self
                    .settings_store
                    .settings()
                    .terminal
                    .command_bar
                    .quick_commands_enabled
                {
                    items.push((
                        QuickCommands,
                        self.i18n.t("terminal.quick_commands.title"),
                        false,
                    ));
                }
                items.push((Input, self.i18n.t("terminal.command_bar.input"), false));
                items.push((
                    Broadcast,
                    self.i18n.t("terminal.command_bar.broadcast"),
                    false,
                ));
                items.push((
                    Highlight,
                    self.i18n.t("terminal.command_bar.highlight"),
                    false,
                ));
                items.push((
                    Timestamps,
                    self.i18n.t("terminal.command_bar.timestamps"),
                    false,
                ));
                items.push((
                    Recording,
                    self.i18n.t("terminal.command_bar.capture"),
                    false,
                ));
                items.push((
                    Triggers,
                    self.i18n.t("terminal.command_bar.triggers"),
                    false,
                ));
                if self.active_ssh_terminal_node_id(cx).is_some() {
                    items.push((
                        DirectoryTracking,
                        self.i18n
                            .t("settings_view.connections.shell_integration.toolbar_action"),
                        self.remote_shell_integration_pending(cx),
                    ));
                }
            }
            TerminalToolbarMenu::Split => {
                let disabled = !self.can_split_active_pane(cx);
                items.push((
                    SplitHorizontal,
                    self.i18n.t("command_palette.cmd_split_horizontal"),
                    disabled,
                ));
                items.push((
                    SplitVertical,
                    self.i18n.t("command_palette.cmd_split_vertical"),
                    disabled,
                ));
            }
            TerminalToolbarMenu::Context => {
                if self.terminal_current_directory_awareness_enabled()
                    && self
                        .settings_store
                        .settings()
                        .terminal
                        .command_bar
                        .show_current_directory
                    && self.active_terminal_cwd_scope_and_pane(cx).is_some()
                {
                    items.push((Cwd, self.i18n.t("terminal.command_bar.directory"), false));
                }
                if self.active_terminal_git_snapshot(cx).is_some() {
                    items.push((Git, self.i18n.t("terminal.command_bar.git"), false));
                }
                if self.terminal_project_tasks_enabled()
                    && self.active_terminal_project_snapshot(cx).is_some()
                {
                    items.push((Project, self.i18n.t("terminal.command_bar.project"), false));
                }
            }
        }
        items
    }

    fn activate_terminal_toolbar_action(
        &mut self,
        action: ToolbarAction,
        pane: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_terminal_toolbar_menu();
        if self.active_pane_id(cx) != Some(pane) {
            cx.notify();
            return;
        }
        use ToolbarAction::*;
        match action {
            Sender => self.toggle_terminal_sender_panel(window, cx),
            QuickCommands => self.toggle_terminal_quick_commands_panel(window, cx),
            Input => self.toggle_terminal_input(window, cx),
            Broadcast => self.toggle_terminal_broadcast_menu(cx),
            Highlight => self.toggle_terminal_highlight_popover(cx),
            Timestamps => {
                self.toggle_active_terminal_timestamps(cx);
                self.focus_active_pane(window, cx);
            }
            Recording => self.toggle_terminal_recording_menu(cx),
            Triggers => self.open_terminal_trigger_settings_for_pane(pane, window, cx),
            DirectoryTracking => self.open_remote_shell_integration_confirm(cx),
            SplitHorizontal => self.split_active_pane(SplitDirection::Horizontal, window, cx),
            SplitVertical => self.split_active_pane(SplitDirection::Vertical, window, cx),
            Cwd => self.open_terminal_cwd_picker(cx),
            Git => self.open_terminal_git_branch_picker(cx),
            Project => self.open_terminal_project_panel(cx),
        }
        cx.notify();
    }

    pub(in crate::workspace) fn handle_terminal_toolbar_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.terminal_toolbar_menu.clone() else {
            return false;
        };
        if self.active_pane_id(cx) != Some(menu.pane) {
            self.dismiss_terminal_toolbar_menu();
            return false;
        }
        let items = self.terminal_toolbar_items(menu.kind, cx);
        match event.keystroke.key.as_str() {
            "escape" => {
                self.dismiss_terminal_toolbar_menu();
                self.focus_active_pane(window, cx);
            }
            "up" | "arrowup" | "down" | "arrowdown" | "home" | "end" => {
                let enabled = items
                    .iter()
                    .enumerate()
                    .filter_map(|(i, row)| (!row.2).then_some(i))
                    .collect::<Vec<_>>();
                if !enabled.is_empty() {
                    let current = menu
                        .selected
                        .and_then(|selected| enabled.iter().position(|i| *i == selected));
                    let index = match event.keystroke.key.as_str() {
                        "home" => 0,
                        "end" => enabled.len() - 1,
                        "up" | "arrowup" => current
                            .map(|i| (i + enabled.len() - 1) % enabled.len())
                            .unwrap_or(enabled.len() - 1),
                        _ => current.map(|i| (i + 1) % enabled.len()).unwrap_or(0),
                    };
                    self.terminal_toolbar_menu.as_mut().unwrap().selected = Some(enabled[index]);
                    let separators = items
                        .iter()
                        .take(enabled[index] + 1)
                        .filter(|item| {
                            matches!(item.0, ToolbarAction::Broadcast | ToolbarAction::Recording)
                        })
                        .count();
                    menu.scroll.scroll_to_item(enabled[index] + separators);
                }
            }
            "enter" | "space" => {
                if let Some((action, _, false)) = menu.selected.and_then(|index| items.get(index)) {
                    self.activate_terminal_toolbar_action(*action, menu.pane, window, cx);
                }
            }
            _ => return true,
        }
        cx.notify();
        true
    }

    fn terminal_toolbar_item_detail(
        &self,
        action: ToolbarAction,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        use ToolbarAction::*;
        let key = match action {
            Input => {
                if self.terminal_command_sender.read(cx).is_visible() {
                    "terminal.command_bar.visible"
                } else {
                    "terminal.command_bar.hidden"
                }
            }
            Timestamps => {
                if self.active_terminal_timestamps_enabled(cx) {
                    "common.enabled"
                } else {
                    "common.disabled"
                }
            }
            Highlight => {
                if self.active_terminal_highlight_override(cx) {
                    "terminal.command_bar.custom"
                } else {
                    "terminal.command_bar.inherited"
                }
            }
            Broadcast => {
                if let Some(member) = self
                    .active_pane_id(cx)
                    .and_then(|pane| self.terminal.read(cx).sync_groups().member(pane))
                    && self.terminal.read(cx).sync_groups().enabled(member.group)
                {
                    return Some(if member.isolated {
                        self.i18n.t("terminal.broadcast.sync_isolated")
                    } else {
                        self.terminal_sync_group_name(member.group)
                    });
                }
                "common.disabled"
            }
            Recording => match self.active_terminal_recording_status(cx).state {
                TerminalRecordingState::Recording => "terminal.recording.recording",
                TerminalRecordingState::Paused => "terminal.recording.paused",
                TerminalRecordingState::Idle => return None,
            },
            _ => return None,
        };
        Some(self.i18n.t(key))
    }

    pub(super) fn render_terminal_toolbar_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.terminal_toolbar_menu.as_ref()?;
        if self.active_pane_id(cx) != Some(menu.pane) {
            return None;
        }
        let pane = menu.pane;
        let header = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(self.tokens.metrics.ui_menu_item_padding_x))
            .py(px(self.tokens.spacing.two))
            .border_b_1()
            .border_color(self.workspace_chrome_divider())
            .child(
                div()
                    .flex_none()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(self.i18n.t(match menu.kind {
                        TerminalToolbarMenu::Tools => "terminal.command_bar.tools_title",
                        TerminalToolbarMenu::Split => "terminal.command_bar.split",
                        TerminalToolbarMenu::Context => "terminal.command_bar.context",
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.terminal_command_active_target_label(cx)),
            )
            .child(self.terminal_command_action_button(
                LucideIcon::X,
                rgb(self.tokens.ui.text_muted),
                false,
                None,
                "terminal-toolbar-menu-close",
                self.i18n.t("window_controls.close"),
                |this, _, window, cx| {
                    this.dismiss_terminal_toolbar_menu();
                    this.focus_active_pane(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                },
                cx,
            ));
        let rows = self
            .terminal_toolbar_items(menu.kind, cx)
            .into_iter()
            .enumerate()
            .fold(
                div().flex().flex_col(),
                |content, (index, (action, label, disabled))| {
                    let content =
                        if matches!(action, ToolbarAction::Broadcast | ToolbarAction::Recording) {
                            content.child(dropdown_menu_separator(&self.tokens))
                        } else {
                            content
                        };
                    let icon = match action {
                        ToolbarAction::Sender => LucideIcon::ListChecks,
                        ToolbarAction::QuickCommands => LucideIcon::Zap,
                        ToolbarAction::Input => LucideIcon::ChevronDown,
                        ToolbarAction::Broadcast => LucideIcon::Radio,
                        ToolbarAction::Highlight => LucideIcon::Hash,
                        ToolbarAction::Timestamps => LucideIcon::Clock,
                        ToolbarAction::Recording => LucideIcon::Circle,
                        ToolbarAction::Triggers => LucideIcon::Activity,
                        ToolbarAction::DirectoryTracking => LucideIcon::FolderSync,
                        ToolbarAction::SplitHorizontal => LucideIcon::SplitSquareHorizontal,
                        ToolbarAction::SplitVertical => LucideIcon::SplitSquareVertical,
                        ToolbarAction::Cwd => LucideIcon::Folder,
                        ToolbarAction::Git => LucideIcon::GitFork,
                        ToolbarAction::Project => LucideIcon::ListChecks,
                    };
                    let detail = self.terminal_toolbar_item_detail(action, cx);
                    let has_detail = detail.is_some();
                    let row = context_menu_item_row(
                        &self.tokens,
                        ContextMenuItemKind::Plain,
                        false,
                        disabled,
                    )
                    .gap(px(8.0))
                    .child(Self::render_lucide_icon(
                        icon,
                        14.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .child(div().flex_1().min_w_0().child(label))
                    .when_some(detail, |row, detail| {
                        row.child(
                            div()
                                .flex_none()
                                .max_w(px(130.0))
                                .truncate()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(detail),
                        )
                    })
                    .when(
                        !has_detail
                            && !matches!(
                                action,
                                ToolbarAction::Input
                                    | ToolbarAction::Timestamps
                                    | ToolbarAction::SplitHorizontal
                                    | ToolbarAction::SplitVertical
                            ),
                        |row| {
                            row.child(Self::render_lucide_icon(
                                LucideIcon::ChevronRight,
                                12.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                        },
                    )
                    .when(menu.selected == Some(index) && !disabled, |row| {
                        row.bg(rgb(self.tokens.ui.bg_hover))
                    })
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if let Some(menu) = this.terminal_toolbar_menu.as_mut()
                            && menu.pane == pane
                            && menu.selected != Some(index)
                            && !disabled
                        {
                            menu.selected = Some(index);
                            cx.notify();
                        }
                    }));
                    content.child(self.workspace_context_menu_action(
                        row,
                        disabled,
                        false,
                        |_| {},
                        move |this, _, window, cx| {
                            this.activate_terminal_toolbar_action(action, pane, window, cx)
                        },
                        cx,
                    ))
                },
            );
        Some(
            context_menu_event_boundary(
                dropdown_menu_content(&self.tokens)
                    .absolute()
                    .bottom_full()
                    .mb(px(4.0))
                    .right(px(8.0))
                    .w(px(350.0))
                    .max_w_full()
                    .occlude()
                    .child(header)
                    .child(
                        rows.id("terminal-toolbar-menu-rows")
                            .max_h(px(
                                (self.terminal_toolbar_popup_available_height() - 48.0).max(40.0)
                            ))
                            .overflow_y_scroll()
                            .track_scroll(&menu.scroll),
                    ),
            )
            .into_any_element(),
        )
    }

    pub(super) fn terminal_toolbar_popup_available_height(&self) -> f32 {
        self.select_anchors
            .get(&SelectAnchorId::TerminalCommandBar)
            .map(|anchor| {
                (f32::from(anchor.bounds.top())
                    - self.tokens.metrics.tabbar_height
                    - self.tokens.metrics.titlebar_height
                    - 12.0)
                    .max(80.0)
            })
            .unwrap_or(420.0)
    }

    pub(super) fn render_terminal_toolbar_status(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let record = self.active_terminal_recording_status(cx);
        let log = self.active_terminal_session_log_status(cx);
        let sender_count = self.terminal_command_sender.read(cx).running_count();
        let member = self
            .active_pane_id(cx)
            .and_then(|pane| self.terminal.read(cx).sync_groups().member(pane));
        let broadcast = member.filter(|m| self.terminal.read(cx).sync_groups().enabled(m.group));
        if record.state == TerminalRecordingState::Idle
            && log.state == TerminalSessionLogState::Idle
            && sender_count == 0
            && broadcast.is_none()
        {
            return None;
        }
        let row = div()
            .w_full()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .border_t_1()
            .border_color(self.workspace_chrome_divider())
            .px(px(12.0))
            .py(px(3.0));
        Some(
            row.when(record.state != TerminalRecordingState::Idle, |row| {
                let paused = record.state == TerminalRecordingState::Paused;
                row.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(self.terminal_toolbar_button(
                            format!(
                                "{} {}",
                                self.i18n.t(if paused {
                                    "terminal.recording.paused"
                                } else {
                                    "terminal.recording.recording"
                                }),
                                format_recording_elapsed(record.elapsed)
                            ),
                            LucideIcon::Circle,
                            true,
                            false,
                            |this, _, _, cx| {
                                this.toggle_terminal_recording_menu(cx);
                            },
                            cx,
                        ))
                        .child(self.terminal_command_action_button(
                            if paused {
                                LucideIcon::Play
                            } else {
                                LucideIcon::Pause
                            },
                            rgb(self.tokens.ui.error),
                            false,
                            None,
                            "terminal-recording-status-pause",
                            self.i18n.t(if paused {
                                "terminal.recording.resume"
                            } else {
                                "terminal.recording.pause"
                            }),
                            move |this, _, _, cx| {
                                if paused {
                                    this.resume_active_terminal_recording(cx);
                                } else {
                                    this.pause_active_terminal_recording(cx);
                                }
                                cx.stop_propagation();
                            },
                            cx,
                        ))
                        .child(self.terminal_command_action_button(
                            LucideIcon::Square,
                            rgb(self.tokens.ui.error),
                            false,
                            None,
                            "terminal-recording-status-stop",
                            self.i18n.t("terminal.recording.stop"),
                            |this, _, _, cx| {
                                this.stop_active_terminal_recording(cx);
                                cx.stop_propagation();
                            },
                            cx,
                        )),
                )
            })
            .when(log.state != TerminalSessionLogState::Idle, |row| {
                let label = if log.state == TerminalSessionLogState::Paused {
                    format!(
                        "{} · {}",
                        self.i18n.t("terminal.session_log.title"),
                        self.i18n.t("terminal.recording.paused")
                    )
                } else {
                    self.i18n.t("terminal.session_log.title")
                };
                row.child(self.terminal_toolbar_button(
                    label,
                    LucideIcon::FileText,
                    false,
                    false,
                    |this, _, _, cx| this.toggle_terminal_recording_menu(cx),
                    cx,
                ))
            })
            .when_some(broadcast, |row, member| {
                let label = format!(
                    "{} · {}",
                    self.terminal_sync_group_name(member.group),
                    self.i18n.t(if member.isolated {
                        "terminal.broadcast.sync_isolated"
                    } else {
                        "terminal.broadcast.sync_enabled"
                    })
                );
                row.child(self.terminal_toolbar_button(
                    label,
                    LucideIcon::Radio,
                    false,
                    false,
                    |this, _, _, cx| this.toggle_terminal_broadcast_menu(cx),
                    cx,
                ))
            })
            .when(sender_count > 0, |row| {
                row.child(self.terminal_toolbar_button(
                    format!(
                        "{} · {}",
                        self.i18n.t("terminal.sender.running"),
                        sender_count
                    ),
                    LucideIcon::ListChecks,
                    false,
                    false,
                    |this, _, window, cx| {
                        this.close_terminal_command_overlays(cx);
                        let running = this
                            .terminal_command_sender
                            .read(cx)
                            .document_snapshots()
                            .into_iter()
                            .find(|document| {
                                document.status == TerminalCommandSenderStatus::Running
                            })
                            .map(|document| document.id);
                        if let Some(id) = running {
                            this.terminal_command_sender
                                .update(cx, |sender, cx| sender.set_active_document(id, cx));
                        }
                        if !this.terminal_command_sender.read(cx).is_expanded() {
                            this.toggle_terminal_sender_panel(window, cx);
                        } else if let Some(id) = running {
                            this.focus_terminal_command_sender_editor(id, window, cx);
                        }
                    },
                    cx,
                ))
            })
            .into_any_element(),
        )
    }
}
