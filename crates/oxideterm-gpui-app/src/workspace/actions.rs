use super::ime::WorkspaceImeTarget;
use super::tabs::TabCloseConfirmKeyAction;
use super::*;
use oxideterm_gpui_ui::text_input::{text_caret, text_input_value_segments_with_color};
use oxideterm_quick_commands::{
    PreparedQuickCommand, QuickCommand, QuickCommandContextValues, QuickCommandRisk,
    QuickCommandTargetContext, QuickCommandTargetProtocol,
    classify_command_risk as classify_quick_command_risk, prepare_quick_command,
};
use oxideterm_terminal::TerminalSessionKind;
use zeroize::Zeroizing;

const TERMINAL_FONT_SIZE_MIN: i64 = 8;
const TERMINAL_FONT_SIZE_MAX: i64 = 32;
const TERMINAL_FONT_SIZE_DEFAULT: i64 = 14;
const TERMINAL_FONT_SIZE_HUD_DURATION: Duration = Duration::from_millis(1200);

fn adjusted_terminal_font_size(current: i64, delta: i64) -> Option<i64> {
    let next = (current + delta).clamp(TERMINAL_FONT_SIZE_MIN, TERMINAL_FONT_SIZE_MAX);
    (next != current).then_some(next)
}

#[derive(Clone, Copy)]
pub(super) enum TerminalBroadcastMenuPlacement {
    Bottom(f32),
    Top(f32),
}

#[derive(Clone)]
pub(in crate::workspace) struct TerminalBroadcastEntry {
    pub(in crate::workspace) pane_id: PaneId,
    pub(in crate::workspace) label: String,
    pub(in crate::workspace) kind: TabKind,
    pub(in crate::workspace) saved_connection:
        Option<oxideterm_settings::TerminalBroadcastTargetRef>,
}

pub(in crate::workspace) fn terminal_broadcast_target_ref(
    saved: &oxideterm_terminal_triggers::SavedConnectionRef,
) -> oxideterm_settings::TerminalBroadcastTargetRef {
    let kind = match saved.kind {
        oxideterm_terminal_triggers::SavedConnectionKind::Ssh => {
            oxideterm_settings::TerminalBroadcastTargetKind::Ssh
        }
        oxideterm_terminal_triggers::SavedConnectionKind::Mosh => {
            oxideterm_settings::TerminalBroadcastTargetKind::Mosh
        }
        oxideterm_terminal_triggers::SavedConnectionKind::Telnet => {
            oxideterm_settings::TerminalBroadcastTargetKind::Telnet
        }
        oxideterm_terminal_triggers::SavedConnectionKind::Serial => {
            oxideterm_settings::TerminalBroadcastTargetKind::Serial
        }
    };
    oxideterm_settings::TerminalBroadcastTargetRef {
        kind,
        saved_connection_id: saved.id.clone(),
    }
}

fn resolve_terminal_broadcast_entries(
    members: &[oxideterm_settings::TerminalBroadcastTargetRef],
    entries: Vec<TerminalBroadcastEntry>,
) -> Vec<PaneId> {
    // Membership is durable, but delivery borrows only currently registered pane consumers.
    let members = members.iter().collect::<HashSet<_>>();
    entries
        .into_iter()
        .filter_map(|entry| {
            entry
                .saved_connection
                .as_ref()
                .filter(|target| members.contains(target))
                .map(|_| entry.pane_id)
        })
        .collect()
}

#[derive(Default)]
pub(super) struct SearchBarState {
    pub(super) visible: bool,
    pub(super) query: String,
    pub(super) active_match: Option<usize>,
    pub(super) match_count: usize,
}

impl SearchBarState {
    pub(super) fn sync_from_terminal(&mut self, status: TerminalSearchStatus) {
        self.active_match = status.active_match;
        self.match_count = status.match_count;
    }

    fn clear_match_state(&mut self) {
        self.active_match = None;
        self.match_count = 0;
    }
}

fn terminal_tab_capture_keystroke(keystroke: &gpui::Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    // Plain Tab and Shift+Tab are terminal protocol keys, but some platforms
    // also treat them as focus traversal keys. Capture only that collision;
    // Ctrl+Tab and other chords stay owned by the normal keybinding registry.
    keystroke.key.as_str() == "tab" && !modifiers.platform && !modifiers.control && !modifiers.alt
}

fn terminal_tab_capture_blocked_by_workspace_ui(
    active_ime_target: bool,
    quick_commands_open: bool,
) -> bool {
    // Text inputs, command palettes, and quick commands own Tab semantics while
    // they are active. The terminal fallback only handles the platform
    // focus-traversal path that would otherwise swallow a real terminal Tab.
    active_ime_target || quick_commands_open
}

impl WorkspaceApp {
    pub(in crate::workspace) fn begin_ai_clear_all_confirm_exit(
        &mut self,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        self.begin_ai_chat_confirm_exit(confirmed, cx)
    }

    pub(in crate::workspace) fn begin_ai_delete_message_confirm_exit(
        &mut self,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        self.begin_ai_chat_confirm_exit(confirmed, cx)
    }

    fn begin_ai_chat_confirm_exit(&mut self, confirmed: bool, cx: &mut Context<Self>) -> bool {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let (started, effect) = self.ai_entity.update(cx, |ai, cx| {
            ai.begin_chat_confirm_exit(confirmed, delay, cx)
        });
        if let Some(effect) = effect {
            match effect {
                ai_state::AiChatConfirmEffect::ClearAll => self.clear_ai_conversations(cx),
                ai_state::AiChatConfirmEffect::DeleteMessage { message_id } => {
                    self.delete_ai_message(&message_id, cx);
                }
            }
        }
        started
    }

    pub(in crate::workspace) fn begin_node_disconnect_confirm_exit(
        &mut self,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) -> (bool, Option<WorkspaceOverlayConfirmEffect>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.overlay.update(cx, |overlay, cx| {
            overlay.begin_confirm_exit(confirmed, delay, cx)
        })
    }

    pub(in crate::workspace) fn begin_tab_close_confirm_exit(
        &mut self,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) -> (bool, Option<TabCloseConfirm>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.tab_host.update(cx, |tab_host, cx| {
            tab_host.begin_close_confirm_exit(confirmed, delay, cx)
        })
    }

    pub(in crate::workspace) fn begin_keybinding_reset_all_confirm_exit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.settings_workspace.update(cx, |settings, cx| {
            settings.begin_keybinding_reset_confirm_exit(delay, cx)
        })
    }
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.visible = true;
        self.close_terminal_quick_commands_popover(cx);
        window.focus(&self.focus_handle, cx);
        if let Some(pane) = self.active_pane(cx) {
            let query = (!self.search.query.is_empty()).then(|| self.search.query.clone());
            let selected_match = query
                .as_ref()
                .map(|_| self.search.active_match.unwrap_or(0));
            let status = pane.update(cx, |pane, cx| {
                pane.set_search_query(query, selected_match, cx)
            });
            self.search.sync_from_terminal(status);
        } else {
            self.search.clear_match_state();
        }
        cx.notify();
    }

    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.visible = false;
        self.search.clear_match_state();
        self.ime_marked_text = None;
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| pane.set_search_query(None, None, cx));
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn update_search_query(&mut self, cx: &mut Context<Self>) {
        let query = (!self.search.query.is_empty()).then(|| self.search.query.clone());
        self.search.active_match = query.as_ref().map(|_| 0);
        if let Some(pane) = self.active_pane(cx) {
            let status = pane.update(cx, |pane, cx| {
                pane.set_search_query(query, self.search.active_match, cx)
            });
            self.search.sync_from_terminal(status);
        } else {
            self.search.clear_match_state();
        }
        cx.notify();
    }

    pub(super) fn search_next(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane(cx) {
            let status = pane.update(cx, |pane, cx| pane.select_next_search_result(forward, cx));
            self.search.sync_from_terminal(status);
            cx.notify();
        }
    }

    pub(super) fn copy(&mut self, cx: &mut Context<Self>) {
        if self.copy_remote_desktop(cx) {
            return;
        }
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| pane.copy_to_clipboard(cx));
        }
    }

    pub(super) fn paste(&mut self, cx: &mut Context<Self>) {
        if self.paste_remote_desktop(cx) {
            return;
        }
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| pane.paste_from_clipboard(cx));
        }
    }

    pub(super) fn clear_active_terminal_screen(&mut self, cx: &mut Context<Self>) -> bool {
        let terminal_active = self.active_tab(cx).is_some_and(|tab| {
            matches!(
                tab.kind,
                TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
            )
        });
        if !terminal_active {
            return false;
        }
        let Some(pane) = self.active_pane(cx) else {
            return false;
        };
        // Clear host-owned emulator state without writing control bytes into PTYs or serial
        // links.
        pane.update(cx, |pane, cx| pane.clear_buffer(cx));
        true
    }

    pub(super) fn cut(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.active_pane(cx) else {
            return false;
        };
        pane.update(cx, |pane, cx| pane.cut_to_clipboard(cx))
    }

    pub(super) fn toggle_zen_mode(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings_store.settings_mut();
        let entering = !settings.sidebar_ui.zen_mode;
        settings.sidebar_ui.zen_mode = entering;
        if entering {
            self.sidebar_collapsed = true;
            self.sidebar_motion_generation = self.sidebar_motion_generation.wrapping_add(1);
            self.context_sidebar_motion_generation =
                self.context_sidebar_motion_generation.wrapping_add(1);
            self.sidebar_rendered = false;
            self.context_sidebar_rendered = false;
            settings.sidebar_ui.collapsed = true;
            settings.sidebar_ui.ai_sidebar_collapsed = true;
            self.clear_ai_sidebar_keyboard_focus(cx);
            const ZEN_HINT_TTL: Duration = Duration::from_millis(2500);
            self.apply_workspace_overlay_intent(
                WorkspaceOverlayIntent::ShowZenHint { ttl: ZEN_HINT_TTL },
                cx,
            );
        } else {
            self.sidebar_collapsed = false;
            self.sidebar_motion_generation = self.sidebar_motion_generation.wrapping_add(1);
            self.sidebar_rendered = true;
            settings.sidebar_ui.collapsed = false;
            self.apply_workspace_overlay_intent(WorkspaceOverlayIntent::ClearZenHint, cx);
        }
        cx.notify();
    }

    pub(super) fn adjust_terminal_font_size(&mut self, delta: i64, cx: &mut Context<Self>) {
        let current = self.settings_store.settings().terminal.font_size;
        let Some(next) = adjusted_terminal_font_size(current, delta) else {
            return;
        };
        self.edit_settings(|settings| settings.terminal.font_size = next, cx);
        self.show_terminal_font_size_hud(next, cx);
    }

    pub(super) fn reset_terminal_font_size(&mut self, cx: &mut Context<Self>) {
        self.edit_settings(
            |settings| settings.terminal.font_size = TERMINAL_FONT_SIZE_DEFAULT,
            cx,
        );
        self.show_terminal_font_size_hud(TERMINAL_FONT_SIZE_DEFAULT, cx);
    }

    fn show_terminal_font_size_hud(&mut self, font_size: i64, cx: &mut Context<Self>) {
        self.apply_workspace_overlay_intent(
            WorkspaceOverlayIntent::ShowTerminalFontSizeHud {
                font_size,
                ttl: TERMINAL_FONT_SIZE_HUD_DURATION,
            },
            cx,
        );
    }

    pub(super) fn dispatch_registered_keybinding(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((definition, combo)) = crate::keybindings::matched_action_for_keystroke(
            &event.keystroke,
            &self.settings_store.settings().keybindings.overrides,
        ) else {
            return false;
        };

        let terminal_active = self.active_tab(cx).is_some_and(|tab| {
            matches!(
                tab.kind,
                TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
            )
        });
        if matches!(
            definition.scope,
            crate::keybindings::ActionScope::Terminal | crate::keybindings::ActionScope::Split
        ) && !terminal_active
        {
            return false;
        }

        let terminal_panel_open = self.search.visible
            || self.ai_entity.read(cx).terminal_inline_panel().open
            || self.context_sidebar_visible();
        if !crate::keybindings::action_allowed_by_terminal_behavior(
            definition,
            &combo,
            terminal_active,
            terminal_panel_open,
        ) {
            return false;
        }

        self.dispatch_keybinding_action(definition.id, window, cx)
    }

    pub(super) fn registered_keybinding_matches(&self, event: &KeyDownEvent) -> bool {
        // Tauri's capture dispatcher checks built-in actions before plugin
        // keybindings. Even when terminal gating lets the key pass through, the
        // plugin layer must not steal a built-in combo.
        crate::keybindings::matched_action_for_keystroke(
            &event.keystroke,
            &self.settings_store.settings().keybindings.overrides,
        )
        .is_some()
    }

    pub(super) fn dispatch_keybinding_action(
        &mut self,
        action_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match action_id {
            "app.newTerminal" => {
                let _ = self.create_local_terminal_tab(window, cx);
            }
            "app.shellLauncher" => self.open_local_shell_launcher(cx),
            "app.closeTab" => self.request_close_active_tab(window, cx),
            "app.closeOtherTabs" => self.request_close_other_tabs_or_active_pane(window, cx),
            "app.newConnection" => self.open_new_connection_form(window, cx),
            "app.settings" => self.open_settings(window, cx),
            "app.toggleSidebar" => self.toggle_sidebar(cx),
            "app.commandPalette" => self.open_command_palette(cx),
            "app.zenMode" => self.toggle_zen_mode(cx),
            "app.nextTab" => self.next_tab(true, window, cx),
            "app.prevTab" => self.next_tab(false, window, cx),
            "app.navBack" => self.navigate_tab_history(false, window, cx),
            "app.navForward" => self.navigate_tab_history(true, window, cx),
            "app.goToTab1" => self.go_to_tab(0, window, cx),
            "app.goToTab2" => self.go_to_tab(1, window, cx),
            "app.goToTab3" => self.go_to_tab(2, window, cx),
            "app.goToTab4" => self.go_to_tab(3, window, cx),
            "app.goToTab5" => self.go_to_tab(4, window, cx),
            "app.goToTab6" => self.go_to_tab(5, window, cx),
            "app.goToTab7" => self.go_to_tab(6, window, cx),
            "app.goToTab8" => self.go_to_tab(7, window, cx),
            "app.goToTab9" => self.go_to_tab(8, window, cx),
            "app.fontIncrease" => self.adjust_terminal_font_size(1, cx),
            "app.fontDecrease" => self.adjust_terminal_font_size(-1, cx),
            "app.fontReset" => self.reset_terminal_font_size(cx),
            "app.showShortcuts" => self.open_shortcuts_modal(cx),
            "terminal.search" => self.open_search(window, cx),
            "terminal.copy" => self.copy(cx),
            "terminal.cut" => {
                let _ = self.cut(cx);
            }
            "terminal.paste" => self.paste(cx),
            "terminal.clearScreen" => {
                self.clear_active_terminal_screen(cx);
            }
            "terminal.aiPanel" => {
                self.toggle_terminal_ai_inline_panel(window, cx);
            }
            "terminal.recording" => self.toggle_active_terminal_recording(cx),
            "terminal.toggleFreeTypeMode" => self.toggle_free_type_mode(cx),
            "terminal.closePanel" => self.close_terminal_panel(window, cx),
            "split.horizontal" => self.split_active_pane(SplitDirection::Horizontal, window, cx),
            "split.vertical" => self.split_active_pane(SplitDirection::Vertical, window, cx),
            "split.closePane" => self.close_active_pane(window, cx),
            "split.navLeft" => self.focus_adjacent_pane(false, window, cx),
            "split.navRight" => self.focus_adjacent_pane(true, window, cx),
            "palette.eventLog" => {
                // Tauri switches the Activity panel to the event log before
                // opening it, so the palette shortcut must not land on
                // Notifications when the previous activity view was different.
                self.notification_center.active_view = WorkspaceActivityView::EventLog;
                self.open_notification_center_tab(window, cx);
            }
            "palette.aiSidebar" => {
                let _ = self.toggle_ai_sidebar(cx);
            }
            "palette.broadcast" => self.toggle_terminal_broadcast(cx),
            _ => return false,
        }
        true
    }

    pub(super) fn toggle_free_type_mode(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.settings_store.settings().terminal.free_type_mode;
        // Route through the shared settings path so every open terminal receives
        // the new preference without changing terminal or SSH session ownership.
        self.edit_settings(|settings| settings.terminal.free_type_mode = enabled, cx);
        self.push_command_palette_toast(
            self.i18n.t("settings_view.terminal.free_type_mode"),
            Some(self.i18n.t(if enabled {
                "common.enabled"
            } else {
                "common.disabled"
            })),
            TerminalNoticeVariant::Default,
            cx,
        );
    }

    fn close_terminal_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_terminal_command_overlays(cx) {
            return;
        }
        if self.search.visible {
            self.close_search(window, cx);
            return;
        }
        if self.ai_entity.read(cx).terminal_inline_panel().open {
            self.close_terminal_ai_inline_panel(window, cx);
            return;
        }
        if self.context_sidebar_visible() {
            self.collapse_context_sidebar(cx);
            self.focus_active_pane(window, cx);
        }
    }

    pub(in crate::workspace) fn close_terminal_command_overlays(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.dismiss_terminal_recording_menu() {
            cx.notify();
            return true;
        }
        if self.dismiss_terminal_highlight_popover() {
            cx.notify();
            return true;
        }
        if self.dismiss_terminal_broadcast_menu(cx) {
            cx.notify();
            return true;
        }

        if self.terminal.read(cx).quick_commands.is_open() {
            self.close_terminal_quick_commands_popover(cx);
            cx.notify();
            return true;
        }

        if self.close_terminal_cwd_picker(cx) {
            cx.notify();
            return true;
        }

        if self.close_terminal_git_branch_picker(cx) {
            cx.notify();
            return true;
        }

        if self.close_terminal_project_panel(cx) {
            cx.notify();
            return true;
        }

        if self
            .terminal_command_sender
            .update(cx, |sender, _cx| sender.dismiss_compact_suggestions())
        {
            cx.notify();
            return true;
        }

        false
    }

    pub(super) fn handle_terminal_command_overlay_escape(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key.as_str() != "escape" || event.keystroke.modifiers.platform {
            return false;
        }

        self.close_terminal_command_overlays(cx)
    }

    pub(super) fn toggle_terminal_broadcast(&mut self, cx: &mut Context<Self>) {
        let (enabled, selected_group_id) = {
            let terminal = self.terminal.read(cx);
            (
                terminal.broadcast_enabled(),
                terminal.selected_broadcast_group_id(),
            )
        };
        if !enabled && let Some(group_id) = selected_group_id {
            self.select_terminal_broadcast_group(group_id, cx);
            self.terminal.update(cx, |terminal, _cx| {
                terminal.set_broadcast_menu_open(false);
            });
        } else {
            self.terminal
                .update(cx, |terminal, _cx| terminal.toggle_broadcast());
        }
        cx.notify();
    }

    pub(in crate::workspace) fn dismiss_terminal_broadcast_menu(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        // Broadcast target selection is rendered as a Radix-style context menu.
        // Keep Esc, outside click, command overlay close, and toolbar toggles
        // on the same owner path instead of mutating the open flag ad hoc.
        self.terminal
            .update(cx, |terminal, _cx| terminal.dismiss_broadcast_menu())
    }

    pub(in crate::workspace) fn toggle_terminal_broadcast_menu(&mut self, cx: &mut Context<Self>) {
        // Opening the broadcast target menu replaces sibling terminal command
        // popovers, matching browser overlay ownership where only one floating
        // command surface receives pointer/wheel events at a time.
        let should_open = !self.terminal.read(cx).broadcast_menu_open();
        self.dismiss_terminal_broadcast_menu(cx);
        if should_open {
            self.dismiss_terminal_recording_menu();
            self.dismiss_terminal_highlight_popover();
            self.close_terminal_quick_commands_popover(cx);
            self.close_terminal_cwd_picker(cx);
            self.close_terminal_git_branch_picker(cx);
            self.close_terminal_project_panel(cx);
            self.terminal.update(cx, |terminal, _cx| {
                terminal.set_broadcast_menu_open(true);
            });
        }
    }

    pub(super) fn handle_workspace_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.terminal_command_sender_editor_focused(window, cx) {
            // Child editor handlers own the bubble path while focused.
            return;
        }
        if active_ime_should_defer_input_key(
            self.active_ime_target(cx).is_some(),
            self.ime_marked_text.is_some(),
            &event.keystroke,
        ) {
            // The capture handler deliberately lets platform text input own text
            // and IME composition keys; the bubble fallback must follow the same
            // rule so inputs do not append or activate once per key path.
            return;
        }

        if self.connection_form_state(cx).form.is_some() {
            let _ = self.handle_new_connection_key(event, window, cx);
            return;
        }

        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if self.terminal.read(cx).broadcast_group_editor().is_some()
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.alt
        {
            match key {
                "escape" => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal.cancel_broadcast_group_edit();
                    });
                    self.clear_ime_selection();
                    cx.notify();
                    return;
                }
                "enter" => {
                    self.commit_terminal_broadcast_group_edit(cx);
                    return;
                }
                _ => {}
            }
        }

        if self.handle_native_plugin_confirm_key(event, cx) {
            return;
        }

        if self.handle_ai_settings_confirm_key(event, cx) {
            return;
        }

        if self.handle_ai_sidebar_confirm_key(event, cx) {
            return;
        }

        if self.handle_settings_confirm_key(event, window, cx) {
            return;
        }

        if self.handle_ai_mcp_add_dialog_key(event, cx) {
            return;
        }

        if self.handle_oxide_dialog_footer_key(event, cx) {
            return;
        }

        if self.handle_cloud_sync_confirm_key(event, cx) {
            return;
        }

        if self.handle_cloud_sync_select_key(event, cx) {
            return;
        }

        let connection_monitor_keys_visible = self.context_sidebar_visible()
            && self.active_context_sidebar_panel == ContextSidebarPanel::HostTools
            && matches!(
                self.host_tools.read(cx).active_tool(),
                ContextSidebarTool::Monitor
                    | ContextSidebarTool::Gpu
                    | ContextSidebarTool::Processes
                    | ContextSidebarTool::Services
                    | ContextSidebarTool::Logs
                    | ContextSidebarTool::Tmux
                    | ContextSidebarTool::Docker
                    | ContextSidebarTool::Ports
                    | ContextSidebarTool::Schedules
                    | ContextSidebarTool::Filesystems
                    | ContextSidebarTool::Packages
            );
        if connection_monitor_keys_visible && self.handle_connection_monitor_select_key(event, cx) {
            return;
        }

        if self.handle_host_process_search_key(event, cx) {
            return;
        }
        if self.handle_host_docker_search_key(event, cx) {
            return;
        }
        if self.handle_host_service_search_key(event, cx) {
            return;
        }
        if self.handle_host_log_search_key(event, cx) {
            return;
        }
        if self.handle_host_tmux_search_key(event, cx) {
            return;
        }
        if self.handle_host_port_search_key(event, cx) {
            return;
        }
        if self.handle_host_schedule_search_key(event, cx) {
            return;
        }
        if self.handle_host_filesystem_search_key(event, cx) {
            return;
        }
        if self.handle_host_package_search_key(event, cx) {
            return;
        }
        if self.handle_host_tmux_input_dialog_key(event, cx) {
            return;
        }

        if self.active_surface == ActiveSurface::Settings && self.open_settings_select.is_some() {
            if key == "escape" && !modifiers.platform {
                self.close_settings_select();
                cx.notify();
            }
            return;
        }

        if self.focused_settings_input.is_some()
            || self
                .settings_workspace
                .read(cx)
                .settings_entity_focused_input()
                .is_some()
            || self.ai_entity.read(cx).focused_settings_input().is_some()
        {
            let _ = self.handle_settings_input_key(event, cx);
            return;
        }

        let quick_commands_focused = {
            let quick_commands = &self.terminal.read(cx).quick_commands;
            quick_commands.is_open() && quick_commands.focused_input().is_some()
        };
        if quick_commands_focused {
            self.handle_quick_commands_key(event, cx);
            return;
        }

        if self.handle_terminal_cwd_picker_key(event, window, cx) {
            return;
        }

        if self.handle_terminal_git_branch_picker_key(event, cx) {
            return;
        }

        if self.handle_terminal_project_panel_key(event, cx) {
            return;
        }

        if self.handle_terminal_command_overlay_escape(event, cx) {
            return;
        }

        if self.handle_ai_inline_panel_key(event, window, cx) {
            return;
        }

        if self.ai_sidebar_visible()
            && (self.ai_entity.read(cx).chat_ui().input_focused
                || self.ai_entity.read(cx).model_selector_search_focused())
        {
            let _ = self.handle_ai_sidebar_key(event, cx);
            return;
        }

        if self.terminal.read(cx).cast_search_focused() {
            self.handle_terminal_cast_search_key(event, cx);
            return;
        }

        if self.active_session_manager_input(cx).is_some() {
            let _ = self.handle_session_manager_key(event, cx);
            return;
        }

        if self.sftp_view.read(cx).focused_input().is_some()
            || self
                .active_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::Sftp)
        {
            let _ = self.handle_sftp_key(event, window, cx);
            return;
        }

        if self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == TabKind::Graphics)
            && self.graphics.read(cx).focused_input().is_some()
        {
            let _ = self.handle_graphics_key(event, cx);
            return;
        }

        let close_panel_shortcut = crate::keybindings::keystroke_matches_action(
            &event.keystroke,
            "terminal.closePanel",
            &self.settings_store.settings().keybindings.overrides,
        );

        if close_panel_shortcut && self.search.visible {
            self.close_search(window, cx);
            return;
        }

        if close_panel_shortcut && self.context_sidebar_visible() {
            self.collapse_context_sidebar(cx);
            self.focus_active_pane(window, cx);
            return;
        }

        if self.active_surface == ActiveSurface::Settings && key == "escape" && !modifiers.platform
        {
            self.close_settings(window, cx);
            return;
        }

        if self.search.visible && !modifiers.platform {
            match key {
                "escape" => self.close_search(window, cx),
                "enter" => self.search_next(!modifiers.shift, cx),
                "backspace" => {
                    if self.search.query.pop().is_some() {
                        self.update_search_query(cx);
                    }
                }
                _ => {}
            }
            return;
        }

        if self.forward_unhandled_key_to_active_terminal(event, window, cx) {
            return;
        }
    }

    pub(super) fn forward_terminal_tab_from_capture(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !terminal_tab_capture_keystroke(&event.keystroke) {
            return false;
        }

        if terminal_tab_capture_blocked_by_workspace_ui(
            self.active_ime_target(cx).is_some(),
            self.terminal.read(cx).quick_commands.is_open(),
        ) {
            return false;
        }

        self.forward_unhandled_key_to_active_terminal(event, window, cx)
    }

    fn forward_unhandled_key_to_active_terminal(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let terminal_active = self.active_tab(cx).is_some_and(|tab| {
            matches!(
                tab.kind,
                TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
            )
        });
        if !terminal_active {
            return false;
        }

        let Some(pane) = self.active_pane(cx) else {
            return false;
        };
        let handled = pane.update(cx, |pane, cx| pane.handle_unfocused_key(event, cx));
        if handled {
            // The pane encoder wrote a terminal control sequence. Stop here so
            // GPUI focus traversal or default widget handling cannot also run.
            window.prevent_default();
            cx.stop_propagation();
        }
        handled
    }

    pub(super) fn standard_confirm_focus(&self) -> Option<ConfirmDialogAction> {
        self.standard_confirm_focused_action
    }

    pub(super) fn standard_confirm_focus_owner(&self) -> Option<ConfirmDialogAction> {
        self.standard_confirm_focused_action
    }

    pub(super) fn reset_standard_confirm_focus(&mut self) {
        // Tauri useConfirm does not paint a default footer button highlight.
        // Keyboard activation still falls back to Cancel inside
        // handle_standard_confirm_key; visible focus appears only after an
        // explicit Tab/arrow navigation writes an action owner.
        self.standard_confirm_focused_action = None;
    }

    pub(super) fn set_standard_confirm_focus(&mut self, action: ConfirmDialogAction) {
        self.standard_confirm_focused_action = Some(action);
    }

    pub(super) fn clear_standard_confirm_focus(&mut self) {
        self.standard_confirm_focused_action = None;
    }

    pub(super) fn handle_standard_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> Option<ConfirmKeyboardAction> {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return None;
        }

        match browser_behavior::modal_footer_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &CONFIRM_DIALOG_FOOTER_ACTIONS,
            self.standard_confirm_focused_action,
            ConfirmDialogAction::Cancel,
        ) {
            Some(browser_behavior::ModalFooterKeyAction::Cancel) => {
                self.clear_standard_confirm_focus();
                Some(ConfirmKeyboardAction::Cancel)
            }
            Some(browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                self.standard_confirm_focused_action = Some(action);
                cx.notify();
                Some(ConfirmKeyboardAction::Handled)
            }
            Some(browser_behavior::ModalFooterKeyAction::Activate(action)) => {
                self.clear_standard_confirm_focus();
                Some(match action {
                    ConfirmDialogAction::Cancel => ConfirmKeyboardAction::Cancel,
                    ConfirmDialogAction::Confirm => ConfirmKeyboardAction::Confirm,
                })
            }
            None => None,
        }
    }

    pub(super) fn handle_settings_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .settings_workspace
            .read(cx)
            .ssh_config_import_dialog_open()
        {
            if event.keystroke.key.as_str() == "escape" {
                self.close_settings_ssh_config_import_dialog(cx);
            }
            // The import dialog owns keyboard input while it is mounted.
            true
        } else if self.handle_keybinding_reset_confirm_key(event, window, cx) {
            true
        } else if self.handle_knowledge_delete_confirm_key(event, cx) {
            true
        } else if self.handle_knowledge_document_dialog_key(event, cx) {
            true
        } else if self.handle_knowledge_collection_dialog_key(event, cx) {
            true
        } else {
            self.handle_settings_data_directory_confirm_key(event, cx)
        }
    }

    pub(super) fn handle_keybinding_reset_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .settings_workspace
            .read(cx)
            .keybinding_reset_confirm_snapshot()
            .is_some_and(|snapshot| snapshot.phase == oxideterm_gpui_ui::motion::ExitPhase::Visible)
        {
            return false;
        }
        let key_action = self.settings_workspace.update(cx, |settings, cx| {
            settings.handle_keybinding_reset_confirm_key(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                event.keystroke.modifiers.platform || event.keystroke.modifiers.control,
                cx,
            )
        });
        match key_action {
            Some(settings::KeybindingResetConfirmKeyAction::Cancel) => {
                self.begin_keybinding_reset_all_confirm_exit(cx);
                cx.notify();
                true
            }
            Some(settings::KeybindingResetConfirmKeyAction::Confirm) => {
                if self.begin_keybinding_reset_all_confirm_exit(cx) {
                    self.reset_all_keybindings(window, cx);
                }
                true
            }
            Some(settings::KeybindingResetConfirmKeyAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_settings_data_directory_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .settings_workspace
            .read(cx)
            .data_directory_confirm_is_visible()
        {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.cancel_settings_data_directory_confirm(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_settings_data_directory(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_knowledge_collection_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ai_entity.read(cx).knowledge_create_dialog_open() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.ai_entity.update(cx, |entity, cx| {
                    entity.close_knowledge_create_dialog(Duration::ZERO, cx);
                });
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                if self
                    .ai_entity
                    .read(cx)
                    .knowledge_new_collection_name()
                    .trim()
                    .is_empty()
                {
                    // Disabled primary buttons retain ownership inside the dialog.
                    self.reset_standard_confirm_focus();
                    cx.notify();
                } else {
                    self.knowledge_create_collection(cx);
                    self.ai_entity.update(cx, |entity, cx| {
                        entity.close_knowledge_create_dialog(Duration::ZERO, cx);
                    });
                }
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_knowledge_document_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ai_entity.read(cx).knowledge_document_dialog_open() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.ai_entity.update(cx, |entity, cx| {
                    entity.close_knowledge_document_dialog(Duration::ZERO, cx);
                });
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                if self
                    .ai_entity
                    .read(cx)
                    .knowledge_new_document_title()
                    .trim()
                    .is_empty()
                {
                    // Disabled primary buttons retain ownership inside the dialog.
                    self.reset_standard_confirm_focus();
                    cx.notify();
                } else {
                    self.knowledge_create_blank_document(cx);
                    self.ai_entity.update(cx, |entity, cx| {
                        entity.close_knowledge_document_dialog(Duration::ZERO, cx);
                    });
                }
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_knowledge_delete_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.ai_entity.read(cx).knowledge_delete_confirm().is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.ai_entity.update(cx, |entity, cx| {
                    entity.clear_knowledge_delete_confirm();
                    cx.notify();
                });
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.knowledge_confirm_delete(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_settings_reset_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key_action = self.overlay.update(cx, |overlay, cx| {
            overlay.handle_confirm_key(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                event.keystroke.modifiers.platform || event.keystroke.modifiers.control,
                cx,
            )
        });
        match key_action {
            Some(WorkspaceOverlayConfirmKeyAction::Cancel) => {
                self.begin_settings_reset_confirm_exit(false, cx);
                true
            }
            Some(WorkspaceOverlayConfirmKeyAction::Confirm) => {
                self.begin_settings_reset_confirm_exit(true, cx);
                true
            }
            Some(WorkspaceOverlayConfirmKeyAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_tab_close_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(snapshot) = self.tab_host.read(cx).close_confirm_snapshot() else {
            return false;
        };
        if snapshot.phase == oxideterm_gpui_ui::motion::ExitPhase::Exiting {
            return true;
        }
        let key_action = self.tab_host.update(cx, |tab_host, cx| {
            tab_host.handle_close_confirm_key(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                event.keystroke.modifiers.platform || event.keystroke.modifiers.control,
                cx,
            )
        });
        match key_action {
            Some(TabCloseConfirmKeyAction::Cancel) => {
                self.cancel_tab_close_confirm(cx);
                true
            }
            Some(TabCloseConfirmKeyAction::Confirm) => {
                self.confirm_tab_close_confirm(window, cx);
                true
            }
            Some(TabCloseConfirmKeyAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_node_disconnect_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(snapshot) = self.overlay.read(cx).confirm_snapshot() else {
            return false;
        };
        if !matches!(
            snapshot.kind,
            WorkspaceOverlayConfirmKind::NodeDisconnect { .. }
        ) {
            return false;
        }
        if snapshot.phase == oxideterm_gpui_ui::motion::ExitPhase::Exiting {
            return true;
        }
        let key_action = self.overlay.update(cx, |overlay, cx| {
            overlay.handle_confirm_key(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                event.keystroke.modifiers.platform || event.keystroke.modifiers.control,
                cx,
            )
        });
        match key_action {
            Some(WorkspaceOverlayConfirmKeyAction::Cancel) => {
                self.cancel_node_disconnect_confirm(cx);
                true
            }
            Some(WorkspaceOverlayConfirmKeyAction::Confirm) => {
                self.confirm_node_disconnect_confirm(window, cx);
                true
            }
            Some(WorkspaceOverlayConfirmKeyAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_ai_sidebar_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        // Summarize is rendered after Safety and therefore owns keys if a
        // stale lower confirmation is still mounted during the same frame.
        self.handle_ai_summarize_confirm_key(event, cx)
            || self.handle_ai_safety_confirm_key(event, cx)
    }

    pub(super) fn handle_ai_safety_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ai_entity.read(cx).chat_ui().safety_confirm_open {
            return false;
        }
        if self
            .ai_entity
            .read(cx)
            .chat_ui()
            .safety_confirm_presence
            .phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Exiting
        {
            return true;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_ai_safety_confirm_exit(cx);
                cx.notify();
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                if self.begin_ai_safety_confirm_exit(cx) {
                    self.confirm_ai_safety_bypass(cx);
                }
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_ai_summarize_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ai_entity.read(cx).chat_ui().summarize_confirm_open {
            return false;
        }
        if self
            .ai_entity
            .read(cx)
            .chat_ui()
            .summarize_confirm_presence
            .phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Exiting
        {
            return true;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_ai_summarize_confirm_exit(cx);
                cx.notify();
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                if self.begin_ai_summarize_confirm_exit(cx) {
                    self.start_ai_summarize_conversation(cx);
                }
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_ai_chat_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key_action = self.ai_entity.update(cx, |ai, cx| {
            ai.handle_chat_confirm_key(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                event.keystroke.modifiers.platform || event.keystroke.modifiers.control,
                cx,
            )
        });
        match key_action {
            Some(ai_state::AiChatConfirmKeyAction::Cancel) => {
                self.begin_ai_chat_confirm_exit(false, cx);
                true
            }
            Some(ai_state::AiChatConfirmKeyAction::Confirm) => {
                self.begin_ai_chat_confirm_exit(true, cx);
                true
            }
            Some(ai_state::AiChatConfirmKeyAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_keybinding_recording_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let overrides = &self.settings_store.settings().keybindings.overrides;
        let action = self.settings_workspace.update(cx, |settings, cx| {
            settings.handle_keybinding_recording_key(event, overrides, cx)
        });
        if action == Some(settings::KeybindingRecordingKeyAction::Confirm) {
            self.confirm_keybinding_recording(window, cx);
        }
    }

    pub(super) fn activate_keybinding_recording_footer_action(
        &mut self,
        action: settings::KeybindingRecordingFooterAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_confirm = self.settings_workspace.update(cx, |settings, cx| {
            settings.activate_keybinding_recording_footer(action, cx)
        });
        if should_confirm {
            self.confirm_keybinding_recording(window, cx);
        }
    }

    pub(super) fn confirm_keybinding_recording(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(commit) = self.settings_workspace.update(cx, |settings, cx| {
            settings.take_keybinding_recording_commit(cx)
        }) else {
            return;
        };
        let Some(definition) = crate::keybindings::action_definition(&commit.action_id) else {
            return;
        };

        let side = crate::keybindings::KeybindingSide::current();
        let previous = crate::keybindings::effective_combo(
            definition,
            &self.settings_store.settings().keybindings.overrides,
            side,
        );
        let runtime_bindings = crate::keybindings::runtime_rebind_key_bindings(
            &commit.action_id,
            previous.as_ref(),
            Some(&commit.combo),
        );

        self.edit_settings(
            move |settings| {
                crate::keybindings::set_override(
                    &mut settings.keybindings.overrides,
                    &commit.action_id,
                    side,
                    commit.combo,
                );
            },
            cx,
        );
        self.apply_runtime_key_bindings(runtime_bindings, window, cx);
    }

    pub(super) fn cancel_keybinding_recording(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.stop_keybinding_recording(cx);
        });
    }

    pub(super) fn reset_keybinding(
        &mut self,
        action_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(definition) = crate::keybindings::action_definition(action_id) else {
            return;
        };
        let side = crate::keybindings::KeybindingSide::current();
        let previous = crate::keybindings::effective_combo(
            definition,
            &self.settings_store.settings().keybindings.overrides,
            side,
        );
        let next = definition.default_combo(side);
        let runtime_bindings = crate::keybindings::runtime_rebind_key_bindings(
            action_id,
            previous.as_ref(),
            Some(next),
        );
        self.edit_settings(
            |settings| {
                crate::keybindings::reset_override(
                    &mut settings.keybindings.overrides,
                    action_id,
                    side,
                );
            },
            cx,
        );
        self.cancel_keybinding_recording(cx);
        self.apply_runtime_key_bindings(runtime_bindings, window, cx);
    }

    pub(super) fn unbind_keybinding(
        &mut self,
        action_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(definition) = crate::keybindings::action_definition(action_id) else {
            return;
        };
        let side = crate::keybindings::KeybindingSide::current();
        let previous = crate::keybindings::effective_combo(
            definition,
            &self.settings_store.settings().keybindings.overrides,
            side,
        );
        let runtime_bindings =
            crate::keybindings::runtime_rebind_key_bindings(action_id, previous.as_ref(), None);
        self.edit_settings(
            |settings| {
                crate::keybindings::set_unbound_override(
                    &mut settings.keybindings.overrides,
                    action_id,
                    side,
                );
            },
            cx,
        );
        self.cancel_keybinding_recording(cx);
        self.apply_runtime_key_bindings(runtime_bindings, window, cx);
    }

    pub(super) fn reset_all_keybindings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = crate::keybindings::KeybindingSide::current();
        let runtime_bindings = {
            let overrides = &self.settings_store.settings().keybindings.overrides;
            crate::keybindings::ACTION_DEFINITIONS
                .iter()
                .flat_map(|definition| {
                    let previous = crate::keybindings::effective_combo(definition, overrides, side);
                    crate::keybindings::runtime_rebind_key_bindings(
                        definition.id,
                        previous.as_ref(),
                        Some(definition.default_combo(side)),
                    )
                })
                .collect::<Vec<_>>()
        };
        self.edit_settings(|settings| settings.keybindings.overrides.clear(), cx);
        self.cancel_keybinding_recording(cx);
        self.apply_runtime_key_bindings(runtime_bindings, window, cx);
    }

    pub(super) fn export_keybindings(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.keybindings.export"),
            )),
        });
        let selection = async move {
            match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            }
        };
        let overrides = self.settings_store.settings().keybindings.overrides.clone();
        let runtime = self.forwarding_runtime.handle().clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_keybinding_export(selection, overrides, runtime, cx);
        });
    }

    pub(super) fn import_keybindings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.keybindings.import"),
            )),
        });
        let selection = async move {
            match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            }
        };
        let runtime = self.forwarding_runtime.handle().clone();
        let target_window = window.window_handle();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_keybinding_import(selection, runtime, target_window, cx);
        });
    }

    fn apply_runtime_key_bindings(
        &self,
        bindings: Vec<gpui::KeyBinding>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_runtime_key_bindings_to_window_handle(bindings, window.window_handle(), cx);
    }

    pub(in crate::workspace) fn apply_runtime_key_bindings_to_window_handle(
        &self,
        bindings: Vec<gpui::KeyBinding>,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if bindings.is_empty() {
            return;
        }
        let _ = cx.update_window(window_handle, move |_root, _window, app| {
            app.bind_keys(bindings);
        });
    }

    pub(super) fn handle_terminal_cast_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform {
            return;
        }
        match key {
            "escape" => {
                self.terminal
                    .update(cx, |terminal, _cx| terminal.blur_cast_search());
                self.ime_marked_text = None;
                cx.notify();
            }
            "backspace" => {
                if self
                    .terminal
                    .update(cx, |terminal, cx| terminal.pop_cast_search(cx))
                {
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    pub(super) fn run_quick_command_model(
        &mut self,
        command: &QuickCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.terminal.read(cx).broadcast_enabled() {
            self.retain_live_terminal_broadcast_targets(cx);
        }
        let parameter_values = command
            .parameters
            .iter()
            .filter_map(|parameter| {
                parameter
                    .default_value
                    .clone()
                    .map(|value| (parameter.name.clone(), Zeroizing::new(value)))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let target_contexts = self.quick_command_target_contexts(cx);
        let contexts = target_contexts
            .iter()
            .map(|(_, context)| context.clone())
            .collect::<Vec<_>>();
        let prepared = prepare_quick_command(command, &contexts, &parameter_values);
        let global_confirmation = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_commands_confirm_before_run;
        let needs_dialog = !command.parameters.is_empty()
            || command.command.contains("{{")
            || target_contexts.len() > 1
            || global_confirmation
            || prepared.as_ref().is_ok_and(|prepared| {
                prepared.confirmation_required
                    || prepared.targets.is_empty()
                    || !prepared.unavailable_targets.is_empty()
            });
        if needs_dialog || prepared.is_err() {
            self.terminal.update(cx, |terminal, _cx| {
                terminal.quick_commands.request_execution(command.clone())
            });
            cx.notify();
            return;
        }
        if let Ok(prepared) = prepared {
            self.execute_prepared_quick_command(prepared, &target_contexts, window, cx);
        }
    }

    pub(super) fn quick_command_target_contexts(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<(PaneId, QuickCommandTargetContext)> {
        let Some(active_pane_id) = self.active_pane_id(cx) else {
            return Vec::new();
        };
        let mut pane_ids = vec![active_pane_id];
        if self.terminal.read(cx).broadcast_enabled() {
            pane_ids.extend(self.terminal_broadcast_target_panes(active_pane_id, cx));
        }
        pane_ids
            .into_iter()
            .filter_map(|pane_id| {
                self.quick_command_context_for_pane(pane_id, cx)
                    .map(|context| (pane_id, context))
            })
            .collect()
    }

    pub(super) fn quick_command_context_for_pane(
        &self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> Option<QuickCommandTargetContext> {
        // Context uses user-visible metadata and the explicit terminal selection;
        // protected connection credentials are never read into this path.
        let pane_entity = self.tab_host.read(cx).panes().get(&pane_id).cloned()?;
        let pane = pane_entity.read(cx);
        let entry = self
            .terminal_broadcast_entries(cx)
            .into_iter()
            .find(|entry| entry.pane_id == pane_id);
        let selected_group_name = self
            .terminal
            .read(cx)
            .selected_broadcast_group_id()
            .and_then(|group_id| {
                self.settings_store
                    .settings()
                    .terminal
                    .broadcast_groups
                    .iter()
                    .find(|group| group.id == group_id)
                    .map(|group| group.name.clone())
            });
        let mut values = QuickCommandContextValues {
            cwd: pane.current_working_directory().map(Zeroizing::new),
            host: pane.current_working_directory_host().map(Zeroizing::new),
            connection: Some(Zeroizing::new(pane.title().to_string())),
            group: selected_group_name.map(Zeroizing::new),
            selection: pane.selected_text_snapshot().map(Zeroizing::new),
            ..QuickCommandContextValues::default()
        };
        let mut protocol = match pane.session_kind() {
            TerminalSessionKind::LocalPty => QuickCommandTargetProtocol::Local,
            TerminalSessionKind::SshPty => QuickCommandTargetProtocol::Ssh,
            TerminalSessionKind::Telnet => QuickCommandTargetProtocol::Telnet,
            TerminalSessionKind::Mosh => QuickCommandTargetProtocol::Mosh,
            TerminalSessionKind::Serial => QuickCommandTargetProtocol::Serial,
        };
        if pane.is_tmux_control_mode() {
            protocol = QuickCommandTargetProtocol::Tmux;
        }
        if let Some(saved) = entry
            .as_ref()
            .and_then(|entry| entry.saved_connection.as_ref())
        {
            self.apply_saved_quick_command_context(saved, &mut values);
        }
        Some(QuickCommandTargetContext {
            target_id: pane_id.to_string(),
            label: entry
                .map(|entry| entry.label)
                .unwrap_or_else(|| pane.title().to_string()),
            protocol,
            values,
        })
    }

    fn apply_saved_quick_command_context(
        &self,
        saved: &oxideterm_settings::TerminalBroadcastTargetRef,
        values: &mut QuickCommandContextValues,
    ) {
        use oxideterm_settings::TerminalBroadcastTargetKind;
        match saved.kind {
            TerminalBroadcastTargetKind::Ssh => {
                if let Some(connection) = self.connection_store.get(&saved.saved_connection_id) {
                    values.host = Some(Zeroizing::new(connection.host.clone()));
                    values.username = Some(Zeroizing::new(connection.username.clone()));
                    values.port = Some(connection.port);
                    values.connection = Some(Zeroizing::new(connection.name.clone()));
                    values.group = connection
                        .group
                        .clone()
                        .map(Zeroizing::new)
                        .or_else(|| values.group.clone());
                }
            }
            TerminalBroadcastTargetKind::Mosh => {
                if let Some(profile) = self
                    .connection_store
                    .get_mosh_profile(&saved.saved_connection_id)
                {
                    values.host = Some(Zeroizing::new(profile.host.clone()));
                    values.username = Some(Zeroizing::new(profile.username.clone()));
                    values.port = Some(profile.ssh_port);
                    values.connection = Some(Zeroizing::new(profile.name.clone()));
                    values.group = profile
                        .group
                        .clone()
                        .map(Zeroizing::new)
                        .or_else(|| values.group.clone());
                }
            }
            TerminalBroadcastTargetKind::Telnet => {
                if let Some(profile) = self
                    .connection_store
                    .telnet_profiles()
                    .iter()
                    .find(|profile| profile.id == saved.saved_connection_id)
                {
                    values.host = Some(Zeroizing::new(profile.host.clone()));
                    values.port = Some(profile.port);
                    values.connection = Some(Zeroizing::new(profile.name.clone()));
                    values.group = profile
                        .group
                        .clone()
                        .map(Zeroizing::new)
                        .or_else(|| values.group.clone());
                }
            }
            TerminalBroadcastTargetKind::Serial => {
                if let Some(profile) = self
                    .connection_store
                    .serial_profiles()
                    .iter()
                    .find(|profile| profile.id == saved.saved_connection_id)
                {
                    values.host = Some(Zeroizing::new(profile.port_path.clone()));
                    values.connection = Some(Zeroizing::new(profile.name.clone()));
                    values.group = profile
                        .group
                        .clone()
                        .map(Zeroizing::new)
                        .or_else(|| values.group.clone());
                }
            }
        }
    }

    pub(super) fn confirm_quick_command_execution(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(execution) = self
            .terminal
            .read(cx)
            .quick_commands
            .pending_execution
            .clone()
        else {
            return;
        };
        let parameter_values = execution
            .command
            .parameters
            .iter()
            .zip(execution.parameter_values)
            .map(|(parameter, value)| (parameter.name.clone(), value))
            .collect::<std::collections::BTreeMap<_, _>>();
        let target_contexts = self.quick_command_target_contexts(cx);
        let contexts = target_contexts
            .iter()
            .map(|(_, context)| context.clone())
            .collect::<Vec<_>>();
        if let Ok(prepared) =
            prepare_quick_command(&execution.command, &contexts, &parameter_values)
        {
            self.execute_prepared_quick_command(prepared, &target_contexts, window, cx);
        }
    }

    fn execute_prepared_quick_command(
        &mut self,
        prepared: PreparedQuickCommand,
        target_contexts: &[(PaneId, QuickCommandTargetContext)],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Expansion is target-specific, so broadcast delivery cannot reuse the
        // old single-string path without losing per-pane context semantics.
        let active_pane_id = self.active_pane_id(cx);
        let focus_command = prepared
            .targets
            .first()
            .map(|target| target.command.clone());
        let mut sent = false;
        for target in prepared.targets {
            let Some((pane_id, _)) = target_contexts
                .iter()
                .find(|(_, context)| context.target_id == target.target_id)
            else {
                continue;
            };
            self.send_terminal_command_to_pane(
                *pane_id,
                &target.command,
                if Some(*pane_id) == active_pane_id {
                    TerminalCommandMarkDetectionSource::CommandBar
                } else {
                    TerminalCommandMarkDetectionSource::Broadcast
                },
                cx,
            );
            sent = true;
        }
        if sent {
            if focus_command
                .as_deref()
                .is_some_and(|command| self.terminal_command_should_handoff_focus(command))
            {
                self.focus_active_pane(window, cx);
            }
            if self
                .settings_store
                .settings()
                .terminal
                .command_bar
                .quick_commands_show_toast
            {
                self.push_workspace_notice(
                    TerminalNotice {
                        title: self.i18n.t("terminal.quick_commands.toast_executed"),
                        // The preview is the only UI boundary allowed to display
                        // expanded commands; notifications retain no command text.
                        description: None,
                        status_text: None,
                        progress: None,
                        variant: TerminalNoticeVariant::Success,
                    },
                    cx,
                );
            }
        }
        self.finish_terminal_quick_command_execution(cx);
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn active_terminal_recording_status(&self, cx: &App) -> TerminalRecordingStatus {
        self.active_pane(cx)
            .map(|pane| pane.read(cx).recording_status())
            .unwrap_or_default()
    }

    pub(super) fn active_terminal_session_log_status(&self, cx: &App) -> TerminalSessionLogStatus {
        self.active_pane(cx)
            .map(|pane| pane.read(cx).session_log_status())
            .unwrap_or_default()
    }

    pub(super) fn active_terminal_session_log_available(&self, cx: &App) -> bool {
        self.active_pane(cx)
            .is_some_and(|pane| pane.read(cx).session_log_available())
    }

    pub(super) fn start_active_terminal_session_log(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_recording_menu();
        let Some(pane) = self.active_pane(cx) else {
            return;
        };
        let result = pane.update(cx, |pane, cx| pane.start_session_log(cx));
        let (title_key, variant) = if result.is_ok() {
            (
                "terminal.session_log.started",
                TerminalNoticeVariant::Success,
            )
        } else {
            (
                "terminal.session_log.start_failed",
                TerminalNoticeVariant::Error,
            )
        };
        self.push_workspace_notice(
            TerminalNotice {
                title: self.i18n.t(title_key),
                description: None,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn pause_active_terminal_session_log(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_recording_menu();
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| pane.pause_session_log(cx));
        }
        cx.notify();
    }

    pub(super) fn resume_active_terminal_session_log(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_recording_menu();
        if let Some(pane) = self.active_pane(cx) {
            pane.update(cx, |pane, cx| pane.resume_session_log(cx));
        }
        cx.notify();
    }

    pub(super) fn stop_active_terminal_session_log(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_recording_menu();
        let Some(pane) = self.active_pane(cx) else {
            return;
        };
        let result = pane.update(cx, |pane, cx| pane.stop_session_log(cx));
        let (title_key, description, variant) = match result {
            Ok(Some(path)) => (
                "terminal.session_log.stopped",
                Some(path.to_string_lossy().to_string()),
                TerminalNoticeVariant::Success,
            ),
            Ok(None) => return,
            Err(_) => (
                "terminal.session_log.stop_failed",
                None,
                TerminalNoticeVariant::Error,
            ),
        };
        self.push_workspace_notice(
            TerminalNotice {
                title: self.i18n.t(title_key),
                description,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn open_active_terminal_session_log(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_recording_menu();
        let Some(pane) = self.active_pane(cx) else {
            return;
        };
        let status = pane.read(cx).session_log_status();
        let writer_failed = status.failed;
        let Some(path) = status.path else {
            return;
        };
        let flush_result = if writer_failed {
            Ok(())
        } else {
            pane.update(cx, |pane, _cx| pane.flush_session_log())
        };
        let result = flush_result
            .and_then(|()| settings::open_path_external(&path).map_err(|error| error.to_string()));
        if result.is_err() {
            self.push_workspace_notice(
                TerminalNotice {
                    title: self.i18n.t("terminal.session_log.open_failed"),
                    description: None,
                    status_text: None,
                    progress: None,
                    variant: TerminalNoticeVariant::Error,
                },
                cx,
            );
        }
    }

    pub(super) fn open_terminal_session_log_directory(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_recording_menu();
        let directory = self
            .active_terminal_session_log_status(cx)
            .path
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| {
                self.settings_store
                    .path()
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("logs")
                    .join("terminal")
            });
        let result =
            fs::create_dir_all(&directory).and_then(|()| settings::open_path_external(&directory));
        if result.is_err() {
            self.push_workspace_notice(
                TerminalNotice {
                    title: self.i18n.t("terminal.session_log.open_failed"),
                    description: None,
                    status_text: None,
                    progress: None,
                    variant: TerminalNoticeVariant::Error,
                },
                cx,
            );
        }
    }

    pub(in crate::workspace) fn sync_active_terminal_recording_elapsed_tick(
        &mut self,
        cx: &mut App,
    ) {
        let pane_id = self.active_pane_id(cx);
        let recording =
            self.active_terminal_recording_status(cx).state == TerminalRecordingState::Recording;
        self.tab_host.update(cx, |tab_host, cx| {
            tab_host.sync_recording_elapsed_tick(pane_id, recording, cx)
        });
    }

    pub(super) fn active_terminal_timestamps_enabled(&self, cx: &mut Context<Self>) -> bool {
        self.active_pane(cx)
            .is_some_and(|pane| pane.read(cx).terminal_timestamps_enabled())
    }

    pub(super) fn toggle_active_terminal_timestamps(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| pane.toggle_terminal_timestamps(cx));
        }
        cx.notify();
    }

    pub(super) fn start_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_recording_menu();
        let title = self.active_tab(cx).map(|tab| tab.title.clone());
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| pane.start_recording(title, cx));
            self.push_workspace_notice(
                TerminalNotice {
                    title: self.i18n.t("terminal.recording.started"),
                    description: None,
                    status_text: None,
                    progress: None,
                    variant: TerminalNoticeVariant::Success,
                },
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn toggle_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        match self.active_terminal_recording_status(cx).state {
            TerminalRecordingState::Idle => self.start_active_terminal_recording(cx),
            TerminalRecordingState::Recording | TerminalRecordingState::Paused => {
                self.stop_active_terminal_recording(cx)
            }
        }
    }

    pub(super) fn pause_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| pane.pause_recording(cx));
        }
        cx.notify();
    }

    pub(super) fn resume_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| pane.resume_recording(cx));
        }
        cx.notify();
    }

    pub(super) fn discard_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| pane.discard_recording(cx));
        }
        cx.notify();
    }

    pub(super) fn stop_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        let Some(pane_id) = self.active_pane_id(cx) else {
            return;
        };
        let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
            return;
        };
        let session_label = self
            .active_terminal_session_id(cx)
            .map(|id| id.0.to_string())
            .unwrap_or_else(|| pane_id.0.to_string());
        let content = pane.update(cx, |pane, cx| pane.stop_recording(cx));
        let Some(content) = content else {
            return;
        };
        self.prompt_save_terminal_recording(
            terminal_recording_default_name_label(&session_label),
            content,
            cx,
        );
        cx.notify();
    }

    fn prompt_save_terminal_recording(
        &mut self,
        session_label: String,
        content: String,
        cx: &mut Context<Self>,
    ) {
        let directory = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Downloads"))
            .unwrap_or_else(|| PathBuf::from("."));
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let suggested = format!("oxideterm-{session_label}-{timestamp}.cast");
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn(async move |weak, cx| {
            let result = match receiver.await {
                Ok(Ok(Some(path))) => fs::write(&path, content)
                    .map(|_| Some(path))
                    .map_err(|error| error.to_string()),
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(Some(path)) => {
                        this.push_workspace_notice(
                            TerminalNotice {
                                title: this.i18n.t("terminal.recording.saved"),
                                description: Some(path.to_string_lossy().to_string()),
                                status_text: None,
                                progress: None,
                                variant: TerminalNoticeVariant::Success,
                            },
                            cx,
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        this.push_workspace_notice(
                            TerminalNotice {
                                title: this.i18n.t("terminal.recording.save_failed"),
                                description: Some(error),
                                status_text: None,
                                progress: None,
                                variant: TerminalNoticeVariant::Error,
                            },
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn send_terminal_command_to_pane(
        &self,
        pane_id: PaneId,
        command: &str,
        mark_source: TerminalCommandMarkDetectionSource,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() {
            let _ = pane.update(cx, |pane, cx| {
                pane.begin_command_mark(command, mark_source, cx);
                pane.send_command_line(command, cx);
            });
        }
    }

    pub(super) fn terminal_broadcast_target_panes(
        &self,
        source_pane_id: PaneId,
        cx: &App,
    ) -> Vec<PaneId> {
        let tab_host = self.tab_host.read(cx);
        let mut candidates = Vec::new();
        for tab in self.tabs(cx) {
            if let Some(root) = tab.root_pane.as_ref() {
                root.collect_pane_ids(&mut candidates);
            }
        }
        candidates
            .retain(|pane_id| *pane_id != source_pane_id && tab_host.panes().contains_key(pane_id));

        self.terminal.read(cx).filter_broadcast_targets(candidates)
    }

    fn retain_live_terminal_broadcast_targets(&mut self, cx: &mut Context<Self>) {
        let tab_host = self.tab_host.read(cx);
        let live_panes = tab_host.panes().keys().copied().collect::<HashSet<_>>();
        self.terminal.update(cx, |terminal, _cx| {
            terminal.retain_live_broadcast_targets(&live_panes);
        });
    }

    pub(in crate::workspace) fn terminal_broadcast_entries(
        &self,
        cx: &App,
    ) -> Vec<TerminalBroadcastEntry> {
        let tab_host = self.tab_host.read(cx);
        let mut entries = Vec::new();
        for tab in self.tabs(cx) {
            let Some(root) = tab.root_pane.as_ref() else {
                continue;
            };
            let mut pane_ids = Vec::new();
            root.collect_pane_ids(&mut pane_ids);
            for pane_id in pane_ids {
                if !tab_host.panes().contains_key(&pane_id) {
                    continue;
                }
                let label = if root.pane_count() > 1 {
                    format!("{} · {}", tab.title, pane_id)
                } else {
                    tab.title.clone()
                };
                let saved_connection = root
                    .session_id_for_pane(pane_id)
                    .and_then(|session_id| self.terminal_saved_connection_refs.get(&session_id))
                    .map(terminal_broadcast_target_ref);
                entries.push(TerminalBroadcastEntry {
                    pane_id,
                    label,
                    kind: tab.kind.clone(),
                    saved_connection,
                });
            }
        }
        entries
    }

    pub(in crate::workspace) fn terminal_broadcast_groups(
        &self,
    ) -> &[oxideterm_settings::TerminalBroadcastGroup] {
        &self.settings_store.settings().terminal.broadcast_groups
    }

    pub(in crate::workspace) fn select_terminal_broadcast_group(
        &mut self,
        group_id: uuid::Uuid,
        cx: &mut Context<Self>,
    ) {
        let targets = self.resolve_terminal_broadcast_group(group_id, cx);
        self.terminal.update(cx, |terminal, _cx| {
            terminal.select_broadcast_group(group_id, &targets);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn resolve_terminal_broadcast_group(
        &self,
        group_id: uuid::Uuid,
        cx: &App,
    ) -> Vec<PaneId> {
        let Some(group) = self
            .terminal_broadcast_groups()
            .iter()
            .find(|group| group.id == group_id)
        else {
            return Vec::new();
        };
        resolve_terminal_broadcast_entries(&group.members, self.terminal_broadcast_entries(cx))
    }

    pub(in crate::workspace) fn begin_terminal_broadcast_group_create(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal.update(cx, |terminal, _cx| {
            terminal.begin_broadcast_group_create();
        });
        self.ime_marked_text = None;
        self.clear_ime_selection();
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn begin_terminal_broadcast_group_rename(
        &mut self,
        group_id: uuid::Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .terminal_broadcast_groups()
            .iter()
            .find(|group| group.id == group_id)
            .map(|group| group.name.clone())
        else {
            return;
        };
        self.terminal.update(cx, |terminal, _cx| {
            terminal.begin_broadcast_group_rename(group_id, name);
        });
        self.ime_marked_text = None;
        self.selected_ime_target = Some(WorkspaceImeTarget::TerminalBroadcastGroupName);
        self.clear_ime_selection();
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn terminal_broadcast_group_name_valid(
        &self,
        edit_kind: terminal_entity::TerminalBroadcastGroupEditKind,
        name: &str,
    ) -> bool {
        let name = name.trim();
        !name.is_empty()
            && self.terminal_broadcast_groups().iter().all(|group| {
                matches!(
                    edit_kind,
                    terminal_entity::TerminalBroadcastGroupEditKind::Rename(group_id)
                        if group.id == group_id
                ) || !group.name.eq_ignore_ascii_case(name)
            })
    }

    pub(in crate::workspace) fn commit_terminal_broadcast_group_edit(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some((edit_kind, value)) = self
            .terminal
            .read(cx)
            .broadcast_group_editor()
            .map(|(kind, value)| (kind, value.trim().to_string()))
        else {
            return;
        };
        if !self.terminal_broadcast_group_name_valid(edit_kind, &value) {
            return;
        }

        let group_id = match edit_kind {
            terminal_entity::TerminalBroadcastGroupEditKind::Create => uuid::Uuid::new_v4(),
            terminal_entity::TerminalBroadcastGroupEditKind::Rename(group_id) => group_id,
        };
        self.edit_settings(
            |settings| match edit_kind {
                terminal_entity::TerminalBroadcastGroupEditKind::Create => {
                    settings.terminal.broadcast_groups.push(
                        oxideterm_settings::TerminalBroadcastGroup {
                            id: group_id,
                            name: value,
                            members: Vec::new(),
                        },
                    );
                }
                terminal_entity::TerminalBroadcastGroupEditKind::Rename(_) => {
                    if let Some(group) = settings
                        .terminal
                        .broadcast_groups
                        .iter_mut()
                        .find(|group| group.id == group_id)
                    {
                        group.name = value;
                    }
                }
            },
            cx,
        );
        self.terminal.update(cx, |terminal, _cx| {
            terminal.cancel_broadcast_group_edit();
        });
        self.clear_ime_selection();
        if matches!(
            edit_kind,
            terminal_entity::TerminalBroadcastGroupEditKind::Create
        ) {
            self.select_terminal_broadcast_group(group_id, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn delete_terminal_broadcast_group(
        &mut self,
        group_id: uuid::Uuid,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            |settings| {
                settings
                    .terminal
                    .broadcast_groups
                    .retain(|group| group.id != group_id);
            },
            cx,
        );
        self.terminal.update(cx, |terminal, _cx| {
            if terminal.selected_broadcast_group_id() == Some(group_id) {
                terminal.clear_selected_broadcast_group();
            }
            terminal.cancel_broadcast_group_edit();
        });
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_terminal_broadcast_group_member(
        &mut self,
        group_id: uuid::Uuid,
        target: oxideterm_settings::TerminalBroadcastTargetRef,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            |settings| {
                let Some(group) = settings
                    .terminal
                    .broadcast_groups
                    .iter_mut()
                    .find(|group| group.id == group_id)
                else {
                    return;
                };
                if let Some(index) = group.members.iter().position(|member| member == &target) {
                    group.members.remove(index);
                } else {
                    group.members.push(target);
                }
            },
            cx,
        );
        self.select_terminal_broadcast_group(group_id, cx);
    }

    fn terminal_command_should_handoff_focus(&self, command: &str) -> bool {
        let Some(command_name) = terminal_command_executable(command) else {
            return false;
        };
        self.settings_store
            .settings()
            .terminal
            .command_bar
            .focus_handoff_commands
            .iter()
            .any(|candidate| candidate == &command_name)
    }

    pub(super) fn switch_locale(
        &mut self,
        locale: Locale,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Route language changes through the same settings mutation path as the
        // settings UI so native plugin language/settings subscriptions observe
        // menu-triggered locale switches too.
        self.edit_settings(
            |settings| settings.general.language = settings_language_from_locale(locale),
            cx,
        );

        let menus = crate::platform::app_menus(&self.i18n);
        let _ = cx.update_window(window.window_handle(), move |_root, _window, app| {
            app.set_menus(menus);
        });
        cx.notify();
    }

    pub(super) fn sync_tab_titles(&mut self, cx: &mut App) {
        // Localized tab titles are derived only when the locale changes. Keeping
        // this work out of render avoids allocating every translated title on
        // unrelated terminal repaint frames.
        let i18n = &self.i18n;
        self.tab_host.update(cx, |tab_host, _| {
            tab_host.sync_tab_titles(|key| i18n.t(key))
        });
    }

    pub(super) fn render_search_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        const SEARCH_PANEL_BG_ALPHA: u32 = 0xf5; // Tauri bg-theme-bg-elevated translated to native opacity.
        const SEARCH_PANEL_BORDER_ALPHA: u32 = 0xcc; // Tauri border-theme-border.

        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::Search;
        let has_query = !self.search.query.is_empty();
        let marked_text = self.marked_text_for_target(target, cx);
        let selected_range = self.ime_selected_range_for_target(target, cx);
        let input_range = selected_range.filter(|_| has_query && marked_text.is_none());
        let selection_range = input_range.clone().filter(|range| range.start < range.end);
        let caret_offset = input_range
            .as_ref()
            .filter(|range| range.start == range.end)
            .map(|range| range.start);
        let shows_selection = selection_range.is_some();
        let shows_positioned_caret = caret_offset.is_some() && !shows_selection;
        let query = if has_query {
            self.search.query.clone()
        } else {
            self.i18n.t("search.placeholder")
        };
        let match_count = self.search.match_count;
        let active_match = self
            .search
            .active_match
            .filter(|index| *index < match_count);
        let navigation_disabled = !has_query || match_count == 0;
        let result_label = if !has_query {
            String::new()
        } else if match_count == 0 {
            self.i18n.t("search.no_results")
        } else {
            format!("{}/{}", active_match.unwrap_or(0) + 1, match_count)
        };

        div()
            .absolute()
            .top(px(12.0))
            .right(px(12.0))
            .w(px(420.0))
            .max_w(relative(0.92))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | SEARCH_PANEL_BORDER_ALPHA))
            .bg(rgba((theme.bg_elevated << 8) | SEARCH_PANEL_BG_ALPHA))
            .shadow_lg()
            .text_size(px(13.0))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .h(px(44.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Search,
                        15.0,
                        rgb(theme.text_muted),
                    ))
                    .child(
                        self.text_input_with_workspace_ime(
                            target,
                            div()
                                .h(px(28.0))
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .items_center()
                                .overflow_hidden()
                                .rounded(px(self.tokens.radii.sm))
                                .px(px(2.0))
                                .cursor_text()
                                .text_color(if has_query {
                                    rgb(theme.text)
                                } else {
                                    rgb(theme.text_muted)
                                })
                                .when(!has_query && marked_text.is_none(), |input| {
                                    input
                                        .child(text_caret(&self.tokens, self.input_caret.visible()))
                                })
                                .child(if has_query {
                                    text_input_value_segments_with_color(
                                        &self.tokens,
                                        &query,
                                        false,
                                        selection_range,
                                        caret_offset,
                                        self.input_caret.visible(),
                                        Some(theme.text),
                                    )
                                    .into_any_element()
                                } else {
                                    div().child(query).into_any_element()
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
                                    has_query && !shows_selection && !shows_positioned_caret,
                                    |input| {
                                        input.child(text_caret(
                                            &self.tokens,
                                            self.input_caret.visible(),
                                        ))
                                    },
                                ),
                            |_this, _cx| {},
                            cx,
                        ),
                    )
                    .when(has_query, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .min_w(px(48.0))
                                .text_size(px(12.0))
                                .text_color(rgb(theme.text_muted))
                                .child(result_label),
                        )
                    })
                    .child(
                        div()
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.md))
                            .cursor_pointer()
                            .hover(move |style| {
                                if navigation_disabled {
                                    style
                                } else {
                                    style.bg(rgb(theme.bg_hover))
                                }
                            })
                            .child(Self::render_lucide_icon(
                                LucideIcon::ArrowUp,
                                14.0,
                                if navigation_disabled {
                                    rgba((theme.text_muted << 8) | 0x66)
                                } else {
                                    rgb(theme.text_muted)
                                },
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.search_next(false, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.md))
                            .cursor_pointer()
                            .hover(move |style| {
                                if navigation_disabled {
                                    style
                                } else {
                                    style.bg(rgb(theme.bg_hover))
                                }
                            })
                            .child(Self::render_lucide_icon(
                                LucideIcon::ArrowDown,
                                14.0,
                                if navigation_disabled {
                                    rgba((theme.text_muted << 8) | 0x66)
                                } else {
                                    rgb(theme.text_muted)
                                },
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.search_next(true, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.md))
                            .cursor_pointer()
                            .hover(move |style| style.bg(rgb(theme.bg_hover)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::X,
                                14.0,
                                rgb(theme.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, window, cx| {
                                    this.close_search(window, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .px(px(12.0))
                    .py(px(7.0))
                    .text_size(px(11.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("search.visible_terminal_hint")),
            )
            .into_any_element()
    }
}

fn terminal_command_executable(command: &str) -> Option<String> {
    let segment = command
        .trim()
        .split("&&")
        .flat_map(|part| part.split("||"))
        .flat_map(|part| part.split(';'))
        .find(|part| !part.trim().is_empty())?;
    let tokens = shell_words(segment);
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].trim();
        if token.is_empty()
            || token.starts_with('-')
            || token
                .split_once('=')
                .is_some_and(|(name, _)| is_shell_assignment_name(name))
        {
            index += 1;
            continue;
        }
        if matches!(token, "sudo" | "command" | "exec" | "env") {
            index += 1;
            continue;
        }
        return token.rsplit('/').next().map(|name| name.to_lowercase());
    }
    None
}

fn shell_words(segment: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in segment.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn is_shell_assignment_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod terminal_command_bar_behavior_tests {
    use super::*;

    fn tab_keystroke_with(modifiers: gpui::Modifiers) -> gpui::Keystroke {
        gpui::Keystroke {
            key: "tab".to_string(),
            modifiers,
            ..Default::default()
        }
    }

    #[test]
    fn terminal_tab_capture_matches_terminal_tab_chords_only() {
        assert!(terminal_tab_capture_keystroke(&tab_keystroke_with(
            gpui::Modifiers::default()
        )));
        assert!(terminal_tab_capture_keystroke(&tab_keystroke_with(
            gpui::Modifiers {
                shift: true,
                ..Default::default()
            }
        )));
        assert!(!terminal_tab_capture_keystroke(&tab_keystroke_with(
            gpui::Modifiers {
                control: true,
                ..Default::default()
            }
        )));
        assert!(!terminal_tab_capture_keystroke(&tab_keystroke_with(
            gpui::Modifiers {
                platform: true,
                ..Default::default()
            }
        )));
    }

    #[test]
    fn terminal_tab_capture_defers_to_workspace_text_ui() {
        assert!(!terminal_tab_capture_blocked_by_workspace_ui(false, false));
        assert!(terminal_tab_capture_blocked_by_workspace_ui(true, false));
        assert!(terminal_tab_capture_blocked_by_workspace_ui(false, true));
    }

    #[test]
    fn command_executable_supports_focus_handoff_detection() {
        assert_eq!(
            terminal_command_executable("vim src/main.rs").as_deref(),
            Some("vim")
        );
        assert_eq!(
            terminal_command_executable("FOO=1 sudo /usr/bin/nvim").as_deref(),
            Some("nvim")
        );
        assert_eq!(terminal_command_executable("A=1 B=2").as_deref(), None);
    }

    #[test]
    fn named_broadcast_resolution_isolated_and_skips_offline_members() {
        let ssh_one = oxideterm_settings::TerminalBroadcastTargetRef {
            kind: oxideterm_settings::TerminalBroadcastTargetKind::Ssh,
            saved_connection_id: "ssh-one".to_string(),
        };
        let ssh_two = oxideterm_settings::TerminalBroadcastTargetRef {
            kind: oxideterm_settings::TerminalBroadcastTargetKind::Ssh,
            saved_connection_id: "ssh-two".to_string(),
        };
        let offline = oxideterm_settings::TerminalBroadcastTargetRef {
            kind: oxideterm_settings::TerminalBroadcastTargetKind::Mosh,
            saved_connection_id: "offline".to_string(),
        };
        let entries = vec![
            TerminalBroadcastEntry {
                pane_id: PaneId(1),
                label: "one".to_string(),
                kind: TabKind::SshTerminal,
                saved_connection: Some(ssh_one.clone()),
            },
            TerminalBroadcastEntry {
                pane_id: PaneId(2),
                label: "two".to_string(),
                kind: TabKind::SshTerminal,
                saved_connection: Some(ssh_two.clone()),
            },
            TerminalBroadcastEntry {
                pane_id: PaneId(3),
                label: "temporary".to_string(),
                kind: TabKind::SshTerminal,
                saved_connection: None,
            },
        ];

        assert_eq!(
            resolve_terminal_broadcast_entries(
                &[ssh_one.clone(), ssh_one, offline],
                entries.clone(),
            ),
            vec![PaneId(1)]
        );
        assert_eq!(
            resolve_terminal_broadcast_entries(&[ssh_two], entries),
            vec![PaneId(2)]
        );
    }
}

fn terminal_recording_default_name_label(session_label: &str) -> String {
    // Tauri uses sessionId.slice(0, 8) in the suggested asciicast file name.
    session_label.chars().take(8).collect()
}

pub(super) fn classify_command_risk(command: &str) -> Option<&'static str> {
    // Completion suggestions still store presentation labels as strings, so
    // adapt the domain result at the existing app boundary.
    match classify_quick_command_risk(command) {
        Some(QuickCommandRisk::High) => Some("high"),
        Some(QuickCommandRisk::Medium) => Some("medium"),
        None => None,
    }
}
