use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_gpui_ui::text_input::{
    text_caret, text_input_anchor_probe, text_input_value_segments_with_color,
};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub(super) enum TerminalBroadcastMenuPlacement {
    Bottom(f32),
    Top(f32),
}

#[derive(Default)]
pub(super) struct SearchBarState {
    pub(super) visible: bool,
    pub(super) query: String,
    pub(super) active_match: Option<usize>,
    pub(super) match_count: usize,
}

impl SearchBarState {
    fn sync_from_terminal(&mut self, status: TerminalSearchStatus) {
        self.active_match = status.active_match;
        self.match_count = status.match_count;
    }

    fn clear_match_state(&mut self) {
        self.active_match = None;
        self.match_count = 0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandEnterAction {
    SubmitDraft,
    SubmitSuggestion(usize),
    AcceptSuggestion(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandSuggestionDirection {
    Up,
    Down,
}

impl WorkspaceApp {
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.visible = true;
        self.terminal_command_bar_focused = false;
        self.terminal_command_suggestions_open = false;
        self.terminal_command_suggestion_highlighted = None;
        self.close_terminal_quick_commands_popover();
        window.focus(&self.focus_handle);
        if let Some(pane) = self.active_pane() {
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
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.set_search_query(None, None, cx));
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn update_search_query(&mut self, cx: &mut Context<Self>) {
        let query = (!self.search.query.is_empty()).then(|| self.search.query.clone());
        self.search.active_match = query.as_ref().map(|_| 0);
        if let Some(pane) = self.active_pane() {
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
        if let Some(pane) = self.active_pane() {
            let status = pane.update(cx, |pane, cx| pane.select_next_search_result(forward, cx));
            self.search.sync_from_terminal(status);
            cx.notify();
        }
    }

    pub(super) fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.copy_to_clipboard(cx));
        }
    }

    pub(super) fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.paste_from_clipboard(cx));
        }
    }

    pub(super) fn toggle_zen_mode(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings_store.settings_mut();
        let entering = !settings.sidebar_ui.zen_mode;
        settings.sidebar_ui.zen_mode = entering;
        if entering {
            self.sidebar_collapsed = true;
            settings.sidebar_ui.collapsed = true;
            settings.sidebar_ui.ai_sidebar_collapsed = true;
            self.clear_ai_sidebar_keyboard_focus();
            const ZEN_HINT_TTL: Duration = Duration::from_millis(2500);
            self.zen_hint_expires_at = Some(Instant::now() + ZEN_HINT_TTL);
            cx.spawn(async move |weak, cx| {
                Timer::after(ZEN_HINT_TTL).await;
                let _ = weak.update(cx, |this, cx| {
                    this.zen_hint_expires_at = None;
                    cx.notify();
                });
            })
            .detach();
        } else {
            self.sidebar_collapsed = false;
            settings.sidebar_ui.collapsed = false;
            self.zen_hint_expires_at = None;
        }
        cx.notify();
    }

    pub(super) fn adjust_terminal_font_size(&mut self, delta: i64, cx: &mut Context<Self>) {
        self.edit_settings(
            |settings| {
                settings.terminal.font_size = (settings.terminal.font_size + delta).clamp(8, 32);
            },
            cx,
        );
    }

    pub(super) fn reset_terminal_font_size(&mut self, cx: &mut Context<Self>) {
        self.edit_settings(|settings| settings.terminal.font_size = 14, cx);
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

        let terminal_active = self
            .active_tab()
            .is_some_and(|tab| matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal));
        if matches!(
            definition.scope,
            crate::keybindings::ActionScope::Terminal | crate::keybindings::ActionScope::Split
        ) && !terminal_active
        {
            return false;
        }

        let terminal_panel_open =
            self.search.visible || self.ai_inline_panel.open || self.context_sidebar_visible();
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
            "app.shellLauncher" => self.open_launcher_tab(window, cx),
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
            "terminal.paste" => self.paste(cx),
            "terminal.aiPanel" => {
                self.toggle_terminal_ai_inline_panel(window, cx);
            }
            "terminal.recording" => self.toggle_active_terminal_recording(cx),
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

    fn close_terminal_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_terminal_command_overlays(cx) {
            return;
        }
        if self.search.visible {
            self.close_search(window, cx);
            return;
        }
        if self.ai_inline_panel.open {
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
        if self.dismiss_terminal_broadcast_menu() {
            cx.notify();
            return true;
        }

        if self.terminal_quick_commands_open {
            self.close_terminal_quick_commands_popover();
            cx.notify();
            return true;
        }

        if self.close_terminal_cwd_picker() {
            cx.notify();
            return true;
        }

        if self.close_terminal_git_branch_picker() {
            cx.notify();
            return true;
        }

        if self.close_terminal_project_panel() {
            cx.notify();
            return true;
        }

        if self.terminal_command_suggestions_open {
            self.terminal_command_suggestions_open = false;
            self.terminal_command_suggestion_highlighted = None;
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
        self.terminal_broadcast_enabled = !self.terminal_broadcast_enabled;
        self.dismiss_terminal_broadcast_menu();
        if !self.terminal_broadcast_enabled {
            self.terminal_broadcast_targets.clear();
        }
        cx.notify();
    }

    pub(in crate::workspace) fn dismiss_terminal_broadcast_menu(&mut self) -> bool {
        // Broadcast target selection is rendered as a Radix-style context menu.
        // Keep Esc, outside click, command overlay close, and toolbar toggles
        // on the same owner path instead of mutating the open flag ad hoc.
        let was_open = self.terminal_broadcast_menu_open;
        self.terminal_broadcast_menu_open = false;
        was_open
    }

    pub(in crate::workspace) fn toggle_terminal_broadcast_menu(&mut self) {
        // Opening the broadcast target menu replaces sibling terminal command
        // popovers, matching browser overlay ownership where only one floating
        // command surface receives pointer/wheel events at a time.
        let should_open = !self.terminal_broadcast_menu_open;
        self.dismiss_terminal_broadcast_menu();
        if should_open {
            self.close_terminal_quick_commands_popover();
            self.close_terminal_cwd_picker();
            self.close_terminal_git_branch_picker();
            self.close_terminal_project_panel();
            self.terminal_command_suggestions_open = false;
            self.terminal_command_suggestion_highlighted = None;
            self.terminal_broadcast_menu_open = true;
        }
    }

    pub(in crate::workspace) fn keep_terminal_broadcast_menu_open(&mut self) {
        // Broadcast target rows are persistent checkbox-style menu items; their
        // shared action guard runs without closing the menu, so keep ownership
        // explicit after selection changes.
        self.terminal_broadcast_menu_open = true;
    }

    pub(super) fn handle_workspace_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if active_ime_should_defer_input_key(
            self.active_ime_target().is_some(),
            self.ime_marked_text.is_some(),
            &event.keystroke,
        ) {
            // The capture handler deliberately lets platform text input own text
            // and IME composition keys; the bubble fallback must follow the same
            // rule so inputs do not append or activate once per key path.
            return;
        }

        if self.new_connection_form.is_some() {
            let _ = self.handle_new_connection_key(event, window, cx);
            return;
        }

        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if self.handle_native_plugin_confirm_key(event, cx) {
            return;
        }

        if self.handle_tab_close_confirm_key(event, window, cx) {
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

        let connection_monitor_keys_visible = self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::ConnectionMonitor)
            || (self.context_sidebar_visible()
                && self.active_context_sidebar_panel == ContextSidebarPanel::HostTools
                && matches!(
                    self.active_context_sidebar_tool,
                    ContextSidebarTool::Monitor
                        | ContextSidebarTool::Processes
                        | ContextSidebarTool::Services
                        | ContextSidebarTool::Logs
                        | ContextSidebarTool::Tmux
                        | ContextSidebarTool::Docker
                        | ContextSidebarTool::Ports
                        | ContextSidebarTool::Schedules
                        | ContextSidebarTool::Filesystems
                        | ContextSidebarTool::Packages
                ));
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

        if self.focused_settings_input.is_some() {
            let _ = self.handle_settings_input_key(event, cx);
            return;
        }

        if self.terminal_quick_commands_open && self.quick_commands.focused_input.is_some() {
            self.handle_quick_commands_key(event, cx);
            return;
        }

        if self.handle_terminal_cwd_picker_key(event, cx) {
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
            && (self.ai_chat_input_focused || self.ai_model_selector_search_focused)
        {
            let _ = self.handle_ai_sidebar_key(event, cx);
            return;
        }

        if self
            .terminal_cast_player
            .as_ref()
            .is_some_and(|player| player.search_focused)
        {
            self.handle_terminal_cast_search_key(event, cx);
            return;
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::SessionManager)
            && self.session_manager.focused_input.is_some()
        {
            let _ = self.handle_session_manager_key(event, window, cx);
            return;
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Sftp)
        {
            let _ = self.handle_sftp_key(event, cx);
            return;
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Launcher)
            && self.launcher.focused_input.is_some()
        {
            let _ = self.handle_launcher_key(event, cx);
            return;
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Graphics)
            && self.graphics.focused_input.is_some()
        {
            let _ = self.handle_graphics_key(event, cx);
            return;
        }

        if self.terminal_command_bar_focused {
            self.handle_terminal_command_bar_key(event, window, cx);
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
        if self.settings_page.settings_reset_confirm_open {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.settings_page.set_settings_reset_confirm_open(false);
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.settings_page.set_settings_reset_confirm_open(false);
                    self.edit_settings(|settings| *settings = PersistedSettings::default(), cx);
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.settings_page.keybinding_reset_all_confirm_open {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.settings_page
                        .set_keybinding_reset_all_confirm_open(false);
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.settings_page
                        .set_keybinding_reset_all_confirm_open(false);
                    self.reset_all_keybindings(window, cx);
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.settings_data_directory_confirm.is_some() {
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
        } else if self.settings_page.knowledge_create_dialog_open {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.settings_page.close_knowledge_create_dialog();
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    if self
                        .settings_page
                        .knowledge_new_collection_name
                        .trim()
                        .is_empty()
                    {
                        // Disabled primary buttons keep focus in the dialog;
                        // restore the shared footer owner after the key guard.
                        self.reset_standard_confirm_focus();
                        cx.notify();
                    } else {
                        self.knowledge_create_collection(cx);
                        self.settings_page.hide_knowledge_create_dialog();
                    }
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.settings_page.knowledge_new_document_dialog_open {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.settings_page.close_knowledge_new_document_dialog();
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    if self
                        .settings_page
                        .knowledge_new_document_title
                        .trim()
                        .is_empty()
                    {
                        // Keep disabled-submit behavior aligned with the
                        // shared two-action footer instead of adding a local
                        // key path for this dialog only.
                        self.reset_standard_confirm_focus();
                        cx.notify();
                    } else {
                        self.knowledge_create_blank_document(cx);
                        self.settings_page.hide_knowledge_new_document_dialog();
                    }
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.settings_page.knowledge_delete_confirm.is_some() {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.settings_page.clear_knowledge_delete_confirm();
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.knowledge_confirm_delete(cx);
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else {
            false
        }
    }

    pub(super) fn handle_tab_close_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.tab_close_confirm.is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.cancel_tab_close_confirm(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_tab_close_confirm(window, cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_node_disconnect_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.node_disconnect_confirm.is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.cancel_node_disconnect_confirm(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_node_disconnect_confirm(window, cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_ai_sidebar_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.ai_safety_confirm_open {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.ai_safety_confirm_open = false;
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.confirm_ai_safety_bypass(cx);
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.ai_summarize_confirm_open {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.ai_summarize_confirm_open = false;
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.ai_summarize_confirm_open = false;
                    self.start_ai_summarize_conversation(cx);
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.ai_clear_all_confirm_open {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.ai_clear_all_confirm_open = false;
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.clear_ai_conversations();
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.ai_delete_message_confirm.is_some() {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.ai_delete_message_confirm = None;
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    if let Some(message_id) = self.ai_delete_message_confirm.take() {
                        self.delete_ai_message(&message_id, cx);
                    } else {
                        cx.notify();
                    }
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else {
            false
        }
    }

    pub(super) fn handle_keybinding_recording_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key.as_str() == "escape"
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.shift
        {
            self.cancel_keybinding_recording(cx);
            return;
        }

        if self.keybinding_recording_combo.is_some()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
        {
            match browser_behavior::modal_footer_key_action(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                &KEYBINDING_RECORDING_FOOTER_ACTIONS,
                self.keybinding_recording_footer_focus,
                KeybindingRecordingFooterAction::Confirm,
            ) {
                Some(browser_behavior::ModalFooterKeyAction::Cancel) => {
                    self.cancel_keybinding_recording(cx);
                    return;
                }
                Some(browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                    // Tauri renders real footer buttons once a combo exists.
                    // Native captures keydown globally, so route recorder
                    // footer navigation through the shared browser footer
                    // contract instead of recording Tab/Home/End again.
                    self.keybinding_recording_footer_focus = Some(action);
                    cx.notify();
                    return;
                }
                Some(browser_behavior::ModalFooterKeyAction::Activate(action)) => {
                    self.activate_keybinding_recording_footer_action(action, window, cx);
                    return;
                }
                None => {}
            }
        }

        let Some(action_id) = self.settings_page.keybinding_recording_action_id.clone() else {
            return;
        };
        let Some(combo) = crate::keybindings::combo_from_keystroke(&event.keystroke) else {
            return;
        };

        let side = crate::keybindings::KeybindingSide::current();
        let conflicts = crate::keybindings::conflicts_for_combo(
            &action_id,
            &combo,
            &self.settings_store.settings().keybindings.overrides,
            side,
        )
        .into_iter()
        .map(|definition| definition.id.to_string())
        .collect::<Vec<_>>();

        self.keybinding_recording_combo = Some(combo);
        self.keybinding_recording_footer_focus = None;
        self.settings_page.set_keybinding_conflicts(conflicts);
        cx.notify();
    }

    pub(super) fn activate_keybinding_recording_footer_action(
        &mut self,
        action: KeybindingRecordingFooterAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Tauri RecordingCell buttons stop propagation and then run exactly one
        // footer action. Native shares this entry for keyboard and pointer
        // activation so focus cleanup and confirm/cancel branching cannot drift.
        self.keybinding_recording_footer_focus = None;
        match action {
            KeybindingRecordingFooterAction::Confirm => {
                self.confirm_keybinding_recording(window, cx);
            }
            KeybindingRecordingFooterAction::Cancel => {
                self.cancel_keybinding_recording(cx);
            }
        }
    }

    pub(super) fn confirm_keybinding_recording(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action_id) = self.settings_page.keybinding_recording_action_id.clone() else {
            return;
        };
        let Some(combo) = self.keybinding_recording_combo.clone() else {
            return;
        };
        let Some(definition) = crate::keybindings::action_definition(&action_id) else {
            self.cancel_keybinding_recording(cx);
            return;
        };

        let side = crate::keybindings::KeybindingSide::current();
        let previous = crate::keybindings::effective_combo(
            definition,
            &self.settings_store.settings().keybindings.overrides,
            side,
        );
        let runtime_bindings =
            crate::keybindings::runtime_rebind_key_bindings(&action_id, &previous, &combo);

        self.edit_settings(
            |settings| {
                crate::keybindings::set_override(
                    &mut settings.keybindings.overrides,
                    &action_id,
                    side,
                    combo,
                );
            },
            cx,
        );
        self.cancel_keybinding_recording(cx);
        self.apply_runtime_key_bindings(runtime_bindings, window, cx);
    }

    pub(super) fn cancel_keybinding_recording(&mut self, cx: &mut Context<Self>) {
        self.settings_page.stop_keybinding_recording();
        self.keybinding_recording_combo = None;
        self.keybinding_recording_footer_focus = None;
        cx.notify();
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
        let next = definition.default_combo(side).clone();
        let runtime_bindings =
            crate::keybindings::runtime_rebind_key_bindings(action_id, &previous, &next);
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
        self.settings_page.stop_keybinding_recording();
        self.keybinding_recording_combo = None;
        self.keybinding_recording_footer_focus = None;
        self.apply_runtime_key_bindings(runtime_bindings, window, cx);
    }

    pub(super) fn reset_all_keybindings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = crate::keybindings::KeybindingSide::current();
        let overrides = self.settings_store.settings().keybindings.overrides.clone();
        let runtime_bindings = crate::keybindings::ACTION_DEFINITIONS
            .iter()
            .flat_map(|definition| {
                let previous = crate::keybindings::effective_combo(definition, &overrides, side);
                let next = definition.default_combo(side).clone();
                crate::keybindings::runtime_rebind_key_bindings(definition.id, &previous, &next)
            })
            .collect::<Vec<_>>();
        self.edit_settings(|settings| settings.keybindings.overrides.clear(), cx);
        self.settings_page.stop_keybinding_recording();
        self.keybinding_recording_combo = None;
        self.keybinding_recording_footer_focus = None;
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
        let overrides = self.settings_store.settings().keybindings.overrides.clone();
        let success = self.i18n.t("settings_view.keybindings.export_success");
        let error = self.i18n.t("settings_view.keybindings.export_error");
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(directory) = paths.into_iter().next() else {
                return;
            };
            let path = directory.join("oxideterm-keybindings.json");
            let result = serde_json::to_string_pretty(&overrides)
                .map_err(|err| err.to_string())
                .and_then(|json| fs::write(path, json).map_err(|err| err.to_string()));
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(()) => this.push_ai_settings_toast(success, TerminalNoticeVariant::Success),
                    Err(_) => this.push_ai_settings_toast(error, TerminalNoticeVariant::Error),
                }
                cx.notify();
            });
        })
        .detach();
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
        let window_handle = window.window_handle();
        let side = crate::keybindings::KeybindingSide::current();
        let previous_overrides = self.settings_store.settings().keybindings.overrides.clone();
        let success = self.i18n.t("settings_view.keybindings.import_success");
        let invalid = self.i18n.t("settings_view.keybindings.import_invalid");
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let result = fs::read_to_string(path)
                .map_err(|err| err.to_string())
                .and_then(|content| {
                    serde_json::from_str::<serde_json::Value>(&content)
                        .map_err(|err| err.to_string())
                })
                .and_then(crate::keybindings::sanitize_imported_overrides);
            let _ = cx.update_window(window_handle, |_root, window, cx| {
                let _ = weak.update(cx, |this, cx| match result {
                    Ok(next_overrides) => {
                        let runtime_bindings = crate::keybindings::ACTION_DEFINITIONS
                            .iter()
                            .flat_map(|definition| {
                                let previous = crate::keybindings::effective_combo(
                                    definition,
                                    &previous_overrides,
                                    side,
                                );
                                let next = crate::keybindings::effective_combo(
                                    definition,
                                    &next_overrides,
                                    side,
                                );
                                crate::keybindings::runtime_rebind_key_bindings(
                                    definition.id,
                                    &previous,
                                    &next,
                                )
                            })
                            .collect::<Vec<_>>();
                        this.edit_settings(
                            |settings| settings.keybindings.overrides = next_overrides,
                            cx,
                        );
                        this.settings_page.stop_keybinding_recording();
                        this.keybinding_recording_combo = None;
                        this.keybinding_recording_footer_focus = None;
                        this.apply_runtime_key_bindings(runtime_bindings, window, cx);
                        this.push_ai_settings_toast(success, TerminalNoticeVariant::Success);
                    }
                    Err(_) => {
                        this.push_ai_settings_toast(invalid, TerminalNoticeVariant::Error);
                        cx.notify();
                    }
                });
            });
        })
        .detach();
    }

    fn apply_runtime_key_bindings(
        &self,
        bindings: Vec<gpui::KeyBinding>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if bindings.is_empty() {
            return;
        }
        let _ = cx.update_window(window.window_handle(), move |_root, _window, app| {
            app.bind_keys(bindings);
        });
    }

    pub(super) fn handle_terminal_command_bar_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform {
            return;
        }

        match key {
            "escape" => {
                if self.close_terminal_command_overlays(cx) {
                    return;
                }
                self.terminal_command_bar_focused = false;
                self.close_terminal_quick_commands_popover();
                self.ime_marked_text = None;
                self.focus_active_pane(window, cx);
                cx.notify();
            }
            "tab" => {
                if self.terminal_command_suggestions_open {
                    let suggestions = self.terminal_command_bar_visible_suggestions(cx);
                    let index = self.terminal_command_suggestion_highlighted.unwrap_or(0);
                    if let Some(suggestion) = suggestions.get(index) {
                        self.accept_terminal_command_suggestion(suggestion, cx);
                    }
                }
            }
            "right" => {
                let suggestions = self.terminal_command_bar_visible_suggestions(cx);
                if let Some(suggestion) =
                    self.terminal_command_inline_suggestion_for_accept(&suggestions)
                {
                    self.accept_terminal_command_suggestion(&suggestion, cx);
                }
            }
            "down" => {
                let mut suggestions = self.terminal_command_bar_suggestions(false, cx);
                if suggestions.is_empty() {
                    suggestions = self.terminal_command_bar_suggestions(true, cx);
                }
                if !suggestions.is_empty() {
                    self.terminal_command_suggestions_open = true;
                    self.terminal_command_suggestion_highlighted =
                        terminal_command_next_suggestion_index(
                            suggestions.len(),
                            true,
                            self.terminal_command_suggestion_highlighted,
                            TerminalCommandSuggestionDirection::Down,
                        );
                    cx.notify();
                }
            }
            "up" => {
                let mut suggestions = self.terminal_command_bar_suggestions(false, cx);
                if suggestions.is_empty() {
                    suggestions = self.terminal_command_bar_suggestions(true, cx);
                }
                if !suggestions.is_empty() {
                    self.terminal_command_suggestions_open = true;
                    self.terminal_command_suggestion_highlighted =
                        terminal_command_next_suggestion_index(
                            suggestions.len(),
                            true,
                            self.terminal_command_suggestion_highlighted,
                            TerminalCommandSuggestionDirection::Up,
                        );
                    cx.notify();
                }
            }
            "enter" if modifiers.shift || modifiers.alt => {
                self.terminal_command_bar_draft.push('\n');
                self.terminal_command_suggestions_open = false;
                self.terminal_command_suggestion_highlighted = None;
                self.ime_marked_text = None;
                cx.notify();
            }
            "enter" => {
                let suggestions = self.terminal_command_bar_visible_suggestions(cx);
                match terminal_command_enter_action(
                    self.terminal_command_suggestions_open,
                    self.terminal_command_suggestion_highlighted,
                    &suggestions,
                ) {
                    TerminalCommandEnterAction::AcceptSuggestion(index) => {
                        if let Some(suggestion) = suggestions.get(index) {
                            self.accept_terminal_command_suggestion(suggestion, cx);
                            return;
                        }
                    }
                    TerminalCommandEnterAction::SubmitSuggestion(index) => {
                        if let Some(suggestion) = suggestions.get(index) {
                            self.accept_terminal_command_suggestion(suggestion, cx);
                        }
                    }
                    TerminalCommandEnterAction::SubmitDraft => {
                        self.terminal_command_suggestions_open = false;
                        self.terminal_command_suggestion_highlighted = None;
                    }
                }
                self.terminal_command_suggestions_open = false;
                self.terminal_command_suggestion_highlighted = None;
                self.submit_terminal_command_bar(window, cx)
            }
            "space" | " "
                if terminal_command_bar_space_inserts_literal(
                    modifiers.platform,
                    modifiers.control,
                    modifiers.alt,
                ) =>
            {
                // Some GPUI platforms deliver Space without key_char, so the
                // platform text path cannot mutate the textarea-like command
                // draft. Preserve Tauri textarea semantics by inserting the
                // literal space through the shared IME replacement path.
                let target = WorkspaceImeTarget::TerminalCommandBar;
                let replacement_range = self.ime_selection_range_for_target(target);
                let caret = replacement_range
                    .as_ref()
                    .map(|range| range.start + " ".encode_utf16().count());
                self.clear_ime_selection();
                self.replace_ime_target_text(target, replacement_range, " ", cx);
                if let Some(caret) = caret {
                    self.set_ime_selection_from_anchor(target, caret, caret);
                }
            }
            "backspace" => {
                let changed = self.terminal_command_bar_draft.pop().is_some()
                    || self.terminal_command_suggestions_open
                    || self
                        .terminal_command_suggestion_highlighted
                        .take()
                        .is_some()
                    || self.ime_marked_text.take().is_some();
                self.terminal_command_suggestions_open = false;
                if changed {
                    // Backspace with an empty command and no open suggestions
                    // leaves the command bar visually unchanged.
                    cx.notify();
                }
            }
            _ => {}
        }
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
                if let Some(player) = self.terminal_cast_player.as_mut() {
                    player.search_focused = false;
                }
                self.ime_marked_text = None;
                cx.notify();
            }
            "backspace" => {
                if let Some(player) = self.terminal_cast_player.as_mut() {
                    if player.search_query.pop().is_some() {
                        self.update_terminal_cast_search(cx);
                        cx.notify();
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn submit_terminal_command_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = self.terminal_command_bar_draft.trim().to_string();
        if command.is_empty() {
            return;
        }

        self.submit_terminal_command_line(&command, window, cx);
        self.terminal_command_bar_draft.clear();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn submit_terminal_command_line(
        &mut self,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(source_pane_id) = self.active_pane_id() {
            self.send_terminal_command_to_pane(
                source_pane_id,
                command,
                TerminalCommandMarkDetectionSource::CommandBar,
                cx,
            );
            self.broadcast_terminal_command(source_pane_id, command, cx);
        } else {
            return false;
        }

        if self.terminal_command_should_handoff_focus(command) {
            self.terminal_command_bar_focused = false;
            self.focus_active_pane(window, cx);
        }
        true
    }

    pub(super) fn run_quick_command(
        &mut self,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = &self.settings_store.settings().terminal.command_bar;
        let risk = classify_command_risk(command);
        if settings.quick_commands_confirm_before_run || risk.is_some() {
            self.terminal_quick_command_pending = Some(command.to_string());
            self.terminal_quick_commands_open = true;
            cx.notify();
            return;
        }
        self.execute_quick_command(command, window, cx);
    }

    fn execute_quick_command(
        &mut self,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.submit_terminal_command_line(command, window, cx)
            && self
                .settings_store
                .settings()
                .terminal
                .command_bar
                .quick_commands_show_toast
        {
            let _ = self.terminal_notice_tx.send(TerminalNotice {
                title: self.i18n.t("terminal.quick_commands.toast_executed"),
                description: Some(command.to_string()),
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Success,
            });
        }
        self.finish_terminal_quick_command_execution();
        self.terminal_command_bar_draft.clear();
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn active_terminal_recording_status(
        &self,
        cx: &mut Context<Self>,
    ) -> TerminalRecordingStatus {
        self.active_pane()
            .map(|pane| pane.read(cx).recording_status())
            .unwrap_or_default()
    }

    pub(super) fn any_terminal_recording_active(&self, cx: &mut Context<Self>) -> bool {
        self.panes
            .values()
            .any(|pane| pane.read(cx).recording_status().state != TerminalRecordingState::Idle)
    }

    pub(super) fn active_terminal_timestamps_enabled(&self, cx: &mut Context<Self>) -> bool {
        self.active_pane()
            .is_some_and(|pane| pane.read(cx).terminal_timestamps_enabled())
    }

    pub(super) fn toggle_active_terminal_timestamps(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.toggle_terminal_timestamps(cx));
        }
        cx.notify();
    }

    pub(super) fn start_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        let title = self.active_tab().map(|tab| tab.title.clone());
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.start_recording(title, cx));
            let _ = self.terminal_notice_tx.send(TerminalNotice {
                title: self.i18n.t("terminal.recording.started"),
                description: None,
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Success,
            });
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
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.pause_recording(cx));
        }
        cx.notify();
    }

    pub(super) fn resume_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.resume_recording(cx));
        }
        cx.notify();
    }

    pub(super) fn discard_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.discard_recording(cx));
        }
        cx.notify();
    }

    pub(super) fn stop_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        let Some(pane_id) = self.active_pane_id() else {
            return;
        };
        let Some(pane) = self.panes.get(&pane_id).cloned() else {
            return;
        };
        let session_label = self
            .active_terminal_session_id()
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
                        let _ = this.terminal_notice_tx.send(TerminalNotice {
                            title: this.i18n.t("terminal.recording.saved"),
                            description: Some(path.to_string_lossy().to_string()),
                            status_text: None,
                            progress: None,
                            variant: TerminalNoticeVariant::Success,
                        });
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = this.terminal_notice_tx.send(TerminalNotice {
                            title: this.i18n.t("terminal.recording.save_failed"),
                            description: Some(error),
                            status_text: None,
                            progress: None,
                            variant: TerminalNoticeVariant::Error,
                        });
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
        if let Some(pane) = self.panes.get(&pane_id).cloned() {
            let _ = pane.update(cx, |pane, cx| {
                pane.begin_command_mark(command, mark_source, cx);
                pane.send_command_line(command, cx);
            });
        }
    }

    fn broadcast_terminal_command(
        &mut self,
        source_pane_id: PaneId,
        command: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.terminal_broadcast_enabled {
            return;
        }

        self.retain_live_terminal_broadcast_targets();
        let targets = self.terminal_broadcast_target_panes(source_pane_id);
        for pane_id in targets {
            self.send_terminal_command_to_pane(
                pane_id,
                command,
                TerminalCommandMarkDetectionSource::Broadcast,
                cx,
            );
        }
    }

    pub(super) fn terminal_broadcast_target_panes(&self, source_pane_id: PaneId) -> Vec<PaneId> {
        let mut candidates = Vec::new();
        for tab in &self.tabs {
            if let Some(root) = tab.root_pane.as_ref() {
                root.collect_pane_ids(&mut candidates);
            }
        }
        candidates.retain(|pane_id| *pane_id != source_pane_id && self.panes.contains_key(pane_id));

        if self.terminal_broadcast_targets.is_empty() {
            candidates
        } else {
            candidates
                .into_iter()
                .filter(|pane_id| self.terminal_broadcast_targets.contains(pane_id))
                .collect()
        }
    }

    fn retain_live_terminal_broadcast_targets(&mut self) {
        let panes = &self.panes;
        self.terminal_broadcast_targets
            .retain(|pane_id| panes.contains_key(pane_id));
    }

    pub(in crate::workspace) fn terminal_broadcast_entries(
        &self,
    ) -> Vec<(PaneId, String, TabKind)> {
        let mut entries = Vec::new();
        for tab in &self.tabs {
            let Some(root) = tab.root_pane.as_ref() else {
                continue;
            };
            let mut pane_ids = Vec::new();
            root.collect_pane_ids(&mut pane_ids);
            for pane_id in pane_ids {
                if !self.panes.contains_key(&pane_id) {
                    continue;
                }
                let label = if root.pane_count() > 1 {
                    format!("{} · {}", tab.title, pane_id)
                } else {
                    tab.title.clone()
                };
                entries.push((pane_id, label, tab.kind.clone()));
            }
        }
        entries
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

    pub(super) fn sync_tab_titles(&mut self, _cx: &App) {
        for tab in &mut self.tabs {
            if let TabTitleSource::I18nKey(key) = tab.title_source {
                tab.title = self.i18n.t(key);
            }
        }
    }

    pub(super) fn render_search_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        const SEARCH_PANEL_BG_ALPHA: u32 = 0xf5; // Tauri bg-theme-bg-elevated translated to native opacity.
        const SEARCH_PANEL_BORDER_ALPHA: u32 = 0xcc; // Tauri border-theme-border.

        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::Search;
        let workspace = cx.entity();
        let has_query = !self.search.query.is_empty();
        let marked_text = self.marked_text_for_target(target);
        let selected_range = self.ime_selected_range_for_target(target);
        let input_range = selected_range
            .clone()
            .filter(|_| has_query && marked_text.is_none());
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
                    .child(text_input_anchor_probe(
                        target.anchor_id(),
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
                                input.child(text_caret(
                                    &self.tokens,
                                    self.new_connection_caret_visible,
                                ))
                            })
                            .child(if has_query {
                                text_input_value_segments_with_color(
                                    &self.tokens,
                                    &query,
                                    false,
                                    selection_range,
                                    caret_offset,
                                    self.new_connection_caret_visible,
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
                                        self.new_connection_caret_visible,
                                    ))
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event, window, cx| {
                                    this.terminal_command_bar_focused = false;
                                    this.ime_marked_text = None;
                                    window.focus(&this.focus_handle);
                                    this.begin_ime_selection_from_mouse_down(
                                        WorkspaceImeTarget::Search,
                                        event,
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_move(cx.listener(|this, event, window, cx| {
                                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                            })),
                        move |anchor, _window, cx| {
                            let _ = workspace.update(cx, |this, cx| {
                                this.update_text_input_anchor(anchor, cx);
                            });
                        },
                    ))
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

fn terminal_command_enter_action(
    suggestions_open: bool,
    highlighted: Option<usize>,
    suggestions: &[TerminalCommandSuggestion],
) -> TerminalCommandEnterAction {
    let Some(index) = highlighted else {
        return TerminalCommandEnterAction::SubmitDraft;
    };
    if !suggestions_open {
        return TerminalCommandEnterAction::SubmitDraft;
    }
    let Some(suggestion) = suggestions.get(index) else {
        return TerminalCommandEnterAction::SubmitDraft;
    };
    if suggestion.executable {
        TerminalCommandEnterAction::SubmitSuggestion(index)
    } else {
        TerminalCommandEnterAction::AcceptSuggestion(index)
    }
}

fn terminal_command_next_suggestion_index(
    suggestions_len: usize,
    suggestions_open: bool,
    highlighted: Option<usize>,
    direction: TerminalCommandSuggestionDirection,
) -> Option<usize> {
    if suggestions_len == 0 {
        return None;
    }
    let last = suggestions_len.saturating_sub(1);
    Some(match (direction, suggestions_open, highlighted) {
        (TerminalCommandSuggestionDirection::Down, true, Some(index)) => {
            index.saturating_add(1).min(last)
        }
        (TerminalCommandSuggestionDirection::Down, _, _) => 0,
        (TerminalCommandSuggestionDirection::Up, true, Some(index)) => index.saturating_sub(1),
        (TerminalCommandSuggestionDirection::Up, _, _) => last,
    })
}

fn terminal_command_bar_space_inserts_literal(platform: bool, control: bool, alt: bool) -> bool {
    !platform && !control && !alt
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

    fn suggestion(executable: bool) -> TerminalCommandSuggestion {
        TerminalCommandSuggestion {
            kind: TerminalCommandSuggestionKind::History,
            label: "ls -la".to_string(),
            insert_text: "ls -la".to_string(),
            description: None,
            executable,
            replacement: 0..2,
            group_label_key: "terminal.command_bar.group_history",
            source_label_key: "terminal.command_bar.source_history",
            score: 1.0,
            risk: None,
            inline_safe: true,
        }
    }

    #[test]
    fn command_bar_enter_matches_tauri_unselected_popup_semantics() {
        let suggestions = vec![suggestion(true)];

        assert_eq!(
            terminal_command_enter_action(true, None, &suggestions),
            TerminalCommandEnterAction::SubmitDraft
        );
        assert_eq!(
            terminal_command_enter_action(false, Some(0), &suggestions),
            TerminalCommandEnterAction::SubmitDraft
        );
    }

    #[test]
    fn command_bar_enter_submits_only_highlighted_executable_suggestion() {
        assert_eq!(
            terminal_command_enter_action(true, Some(0), &[suggestion(true)]),
            TerminalCommandEnterAction::SubmitSuggestion(0)
        );
        assert_eq!(
            terminal_command_enter_action(true, Some(0), &[suggestion(false)]),
            TerminalCommandEnterAction::AcceptSuggestion(0)
        );
    }

    #[test]
    fn command_bar_arrow_navigation_matches_tauri_highlight_rules() {
        assert_eq!(
            terminal_command_next_suggestion_index(
                2,
                false,
                None,
                TerminalCommandSuggestionDirection::Down
            ),
            Some(0)
        );
        assert_eq!(
            terminal_command_next_suggestion_index(
                2,
                true,
                Some(0),
                TerminalCommandSuggestionDirection::Down
            ),
            Some(1)
        );
        assert_eq!(
            terminal_command_next_suggestion_index(
                2,
                false,
                None,
                TerminalCommandSuggestionDirection::Up
            ),
            Some(1)
        );
        assert_eq!(
            terminal_command_next_suggestion_index(
                2,
                true,
                Some(1),
                TerminalCommandSuggestionDirection::Up
            ),
            Some(0)
        );
    }

    #[test]
    fn command_bar_plain_space_is_literal_text() {
        assert!(terminal_command_bar_space_inserts_literal(
            false, false, false
        ));
        assert!(!terminal_command_bar_space_inserts_literal(
            true, false, false
        ));
        assert!(!terminal_command_bar_space_inserts_literal(
            false, true, false
        ));
        assert!(!terminal_command_bar_space_inserts_literal(
            false, false, true
        ));
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
    fn quick_command_risk_patterns_match_tauri_regex_boundaries() {
        assert_eq!(classify_command_risk("rm -rf /tmp/example"), Some("high"));
        assert_eq!(classify_command_risk("mkfs.ext4 /dev/sdb"), Some("high"));
        assert_eq!(
            classify_command_risk("sudo systemctl status nginx"),
            Some("medium")
        );
        assert_eq!(classify_command_risk("echo shutdowns"), None);
        assert_eq!(classify_command_risk("docker image rm unused"), None);
    }

    #[test]
    fn terminal_recording_default_name_label_matches_tauri_prefix() {
        assert_eq!(
            terminal_recording_default_name_label("1234567890abcdef"),
            "12345678"
        );
        assert_eq!(terminal_recording_default_name_label("1234"), "1234");
    }
}

pub(super) fn classify_command_risk(command: &str) -> Option<&'static str> {
    if command_matches_patterns(command, high_risk_command_patterns()) {
        return Some("high");
    }
    if command_matches_patterns(command, medium_risk_command_patterns()) {
        return Some("medium");
    }
    None
}

fn terminal_recording_default_name_label(session_label: &str) -> String {
    // Tauri uses sessionId.slice(0, 8) in the suggested asciicast file name.
    session_label.chars().take(8).collect()
}

fn command_matches_patterns(command: &str, patterns: &[Regex]) -> bool {
    patterns.iter().any(|pattern| pattern.is_match(command))
}

fn high_risk_command_patterns() -> &'static [Regex] {
    // Keep these patterns in semantic lockstep with Tauri's
    // lib/terminal/completion/risk.ts classifier.
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"(?i)\brm\s+-(?:[^\s]*r[^\s]*f|[^\s]*f[^\s]*r)\b",
            r"(?i)\bkubectl\s+delete\b",
            r"(?i)\bsystemctl\s+(?:stop|restart|disable|kill)\b",
            r"(?i)\bdocker\s+(?:rm|rmi|system\s+prune|container\s+prune|volume\s+prune|network\s+prune)\b",
            r"(?i)\b(?:shutdown|reboot|halt|poweroff)\b",
            r"(?i)\bkill(?:all)?\s+-9\b",
            r"(?i)\bmkfs(?:\.[^\s]+)?\b",
            r"(?i)\bdd\s+.*\bof=",
            r"(?i)\bchmod\s+-R\b",
            r"(?i)\bchown\s+-R\b",
        ]
        .into_iter()
        .map(|pattern| Regex::new(pattern).expect("quick command risk pattern must compile"))
        .collect()
    })
}

fn medium_risk_command_patterns() -> &'static [Regex] {
    // Keep these patterns in semantic lockstep with Tauri's
    // lib/terminal/completion/risk.ts classifier.
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [r"(?i)\bsudo\b", r"(?i)\bchmod\s+(?:-R\s+)?777\b"]
            .into_iter()
            .map(|pattern| Regex::new(pattern).expect("quick command risk pattern must compile"))
            .collect()
    })
}
