use super::super::*;

impl Focusable for WorkspaceApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl WorkspaceApp {
    /// Renders modal overlays owned by the tab displayed in the current window.
    pub(in crate::workspace) fn render_tab_window_modals(
        &mut self,
        tab_id: TabId,
        tab_kind: &TabKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut modals = Vec::new();
        match tab_kind {
            TabKind::Settings => {
                if let Some(modal) = self.render_settings_navigation_editor(cx) {
                    modals.push(modal);
                }
                if let Some(modal) = self.render_ai_mcp_add_server_dialog(cx) {
                    modals.push(modal);
                }
                if let Some(modal) = self.render_knowledge_create_collection_dialog(cx) {
                    modals.push(modal);
                }
                if let Some(modal) = self.render_knowledge_new_document_dialog(cx) {
                    modals.push(modal);
                }
                if let Some(modal) = self.render_knowledge_delete_confirm_dialog(cx) {
                    modals.push(modal);
                }
                if self
                    .settings_workspace
                    .read(cx)
                    .keybinding_reset_confirm_snapshot()
                    .is_some()
                {
                    modals.push(self.render_keybinding_reset_all_confirm_dialog(cx));
                }
                if let Some(modal) = self.render_settings_managed_key_dialog(cx) {
                    modals.push(modal);
                }
                if let Some(modal) = self.render_portable_password_change_dialog(cx) {
                    modals.push(modal);
                }
            }
            TabKind::SessionManager => {
                let (show_group_manager, show_delete_confirm) = {
                    let session_manager = self.session_manager.read(cx);
                    (
                        session_manager.show_group_manager,
                        session_manager.delete_confirm.is_some(),
                    )
                };
                if show_group_manager {
                    modals.push(self.render_group_manager_dialog(cx));
                }
                if show_delete_confirm {
                    modals.push(self.render_session_manager_delete_confirm(cx));
                }
            }
            TabKind::Forwards => {
                let Some(node_id) = self.forwarding.read(cx).node_for_tab(tab_id) else {
                    return modals;
                };
                let has_background = self.background_surface_active("forwards");
                // The renderers preserve the forwarding module's private state boundary
                // and return an empty element when no corresponding modal is active.
                modals.push(self.render_forward_edit_modal(
                    node_id.clone(),
                    tab_id,
                    has_background,
                    cx,
                ));
                modals.push(self.render_forward_delete_confirm(
                    node_id,
                    tab_id,
                    has_background,
                    cx,
                ));
            }
            TabKind::Sftp => {
                if let Some(dialog) = self.sftp_view.read(cx).dialog() {
                    let has_background = self.terminal_background_preferences("sftp").is_some();
                    modals.push(self.render_sftp_dialog(dialog, has_background, cx));
                }
            }
            TabKind::FileManager => {
                if self.file_manager.read(cx).dialog.is_some() {
                    let has_background = self
                        .terminal_background_preferences("file_manager")
                        .is_some();
                    modals.push(self.render_file_manager_dialog(window, has_background, cx));
                }
            }
            _ => {}
        }
        if *tab_kind != TabKind::Sftp
            && !self.sidebar_collapsed
            && self.effective_sidebar_panel_section() == SidebarSection::Sessions
            && self.embedded_sftp_node_id.is_some()
            && self.sftp_view.read(cx).current_surface_id == Some(sftp::SftpSurfaceId::Sidebar)
            && let Some(dialog) = self.sftp_view.read(cx).dialog()
        {
            // Sidebar-owned dialogs stay at the window root so previews and
            // confirmations are never clipped by the narrow sidebar region.
            modals.push(self.render_sftp_dialog(dialog, false, cx));
        }
        modals
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_main_window(
        &mut self,
        window_background: &Entity<window_shell::WorkspaceWindowBackgroundEntity>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_ime_target = self.active_ime_target(cx);
        self.workspace_input.update(cx, |input, cx| {
            input.sync_active_target(active_ime_target, cx);
        });
        self.begin_selectable_text_frame();
        self.schedule_pending_auto_close_terminal_sessions(window, cx);
        self.sync_ai_workspace_visibility(cx);
        let cloud_sync_confirm_open = self.cloud_sync.read(cx).view.confirm.is_some();
        if self.app_lock.locked {
            window.set_window_title(&SharedString::from(
                self.i18n.t("settings_view.general.app_lock_window_title"),
            ));
            return self.render_app_lock_screen(window, cx);
        }
        // Confirmation snapshots are immutable frame inputs. Sampling each
        // owner once avoids repeatedly cloning typed payloads during render.
        let ai_chat_confirm_snapshot = self.ai_entity.read(cx).chat_confirm_snapshot();
        let overlay_confirm_snapshot = self.overlay.read(cx).confirm_snapshot();
        let tab_close_confirm_open = self.tab_host.read(cx).close_confirm().is_some();
        let title = self
            .active_tab(cx)
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_else(|| "OxideTerm".to_string());
        // Keep Entity borrows out of window rendering callbacks.
        let active_tab_projection = self
            .active_tab(cx)
            .map(|tab| (tab.id, tab.kind.clone(), tab.root_pane.clone()));
        window.set_window_title(&SharedString::from(title));
        let vibrancy_mode =
            effective_vibrancy_mode(self.settings_store.settings(), &self.render_policy);
        // Modal/command-palette backdrop blur follows the active render profile
        // just like Tauri's linuxBackdropBlurClass compatibility gate.
        set_tauri_backdrop_blur_allowed(self.render_policy.allow_background_blur);
        if self.needs_active_pane_focus
            && active_tab_projection.as_ref().is_some_and(|(_, kind, _)| {
                !matches!(
                    kind,
                    TabKind::Settings
                        | TabKind::SessionManager
                        | TabKind::FileManager
                        | TabKind::Graphics
                        | TabKind::Runtime
                        | TabKind::ConnectionPool
                        | TabKind::Topology
                        | TabKind::NotificationCenter
                        | TabKind::PluginManager
                        | TabKind::Plugin { .. }
                        | TabKind::CloudSync
                        | TabKind::RemoteDesktop
                )
            })
            && !self.search.visible
            && self.connection_form_state(cx).form.is_none()
            && let Some(pane) = self.active_pane(cx)
        {
            self.needs_active_pane_focus = false;
            self.clear_ai_sidebar_keyboard_focus(cx);
            window.on_next_frame(move |window, cx| {
                pane.update(cx, |pane, cx| pane.focus(window, cx));
            });
        }

        let content = if let Some((tab_id, tab_kind, root_pane)) = &active_tab_projection {
            match (tab_kind, root_pane) {
                (TabKind::Settings, _) => self.render_settings_surface(cx),
                (TabKind::FileManager, _) => self.render_file_manager_surface(window, cx),
                (TabKind::Graphics, _) => self.render_graphics_surface(window, cx),
                (TabKind::Runtime, _) => self.render_connection_runtime_surface(cx),
                (TabKind::ConnectionPool, _) => {
                    // Old workspaces may restore the retired connection-pool tab.
                    // Keep it readable by showing the runtime overview instead.
                    self.host_tools.update(cx, |host_tools, _cx| {
                        host_tools.reset_runtime_section();
                    });
                    self.render_connection_runtime_surface(cx)
                }
                (TabKind::Topology, _) => self.render_topology_surface(cx),
                (TabKind::NotificationCenter, _) => self.render_notification_center_surface(cx),
                (TabKind::Sftp, _) => self.render_sftp_surface(window, cx),
                (TabKind::Ide, _) => self.render_ide_surface(cx),
                (TabKind::Forwards, _) => self.render_forwards_surface(window, cx),
                (TabKind::SessionManager, _) => self.render_session_manager_surface(window, cx),
                (TabKind::PluginManager, _) => self.render_plugin_manager_surface(cx),
                (TabKind::Plugin { plugin_id, tab_id }, _) => {
                    let plugin_id = plugin_id.clone();
                    let tab_id = tab_id.clone();
                    self.render_native_plugin_tab_surface(&plugin_id, &tab_id, cx)
                }
                (TabKind::CloudSync, _) => self.render_cloud_sync_surface(cx),
                (TabKind::RemoteDesktop, _) => {
                    self.render_remote_desktop_surface(*tab_id, window, cx)
                }
                (_, Some(root_pane)) => self.render_terminal_surface(root_pane, window, cx),
                _ => {
                    let available_width = self.welcome_main_content_width(window, cx);
                    self.render_empty_workspace(available_width, cx)
                }
            }
        } else {
            let available_width = self.welcome_main_content_width(window, cx);
            self.render_empty_workspace(available_width, cx)
        };
        let content = self.wrap_content_background(
            window_background,
            content,
            active_tab_projection
                .as_ref()
                .map(|(_, kind, _)| tab_background_key(kind)),
            window,
            cx,
        );
        let active_tab_window_modals = active_tab_projection
            .as_ref()
            .map(|(tab_id, kind, _)| self.render_tab_window_modals(*tab_id, kind, window, cx))
            .unwrap_or_default();
        let window_background_layer =
            self.render_workspace_window_background(window_background, window, cx);
        let has_window_background = window_background_layer.is_some();
        let native_update_notification = self.render_native_update_notification(cx);
        let overlay_layers = {
            let tokens = self.tokens;
            let i18n = &self.i18n;
            let mono_font_family = settings_mono_font_family(self.settings_store.settings());
            let control_exit_duration = oxideterm_gpui_ui::motion::duration(
                &tokens,
                oxideterm_gpui_ui::motion::MotionDuration::Control,
            );
            self.overlay.update(cx, |overlay, cx| {
                overlay.set_control_exit_duration(control_exit_duration, cx);
                overlay.render_layers(
                    &tokens,
                    i18n,
                    mono_font_family,
                    native_update_notification,
                    cx,
                )
            })
        };
        let zen_mode = self.settings_store.settings().sidebar_ui.zen_mode;
        let titlebar_visible = self.window_titlebar_visible(window);
        let effective_titlebar_height = self.window_titlebar_height(window);
        let resize_hotzone_visible =
            !zen_mode && (!self.sidebar_collapsed || self.context_sidebar_visible());
        let sidebar_resize_cursor_active = (resize_hotzone_visible
            && self.sidebar_resize_hotzone_hovered)
            || self.sidebar_resizing
            || self.ai_entity.read(cx).chat_ui().sidebar_resizing;
        let embedded_sftp_resize_cursor_active = self.embedded_sftp_sidebar_resizing;
        self.update_main_window_tabbar_drop_bounds(window, titlebar_visible, zen_mode, cx);

        div()
            .id("workspace-root")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(workspace_background(
                &self.tokens,
                vibrancy_mode,
                has_window_background,
            ))
            .text_color(rgb(self.tokens.ui.text))
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .when(sidebar_resize_cursor_active, |root| {
                root.cursor(CursorStyle::ResizeColumn)
            })
            .when(embedded_sftp_resize_cursor_active, |root| {
                root.cursor(CursorStyle::ResizeRow)
            })
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            // Host Tools actions bubble here from both sidebar pages and portaled dialogs.
            .on_action(
                cx.listener(|this, request: &HostToolsWindowRequest, window, cx| {
                    this.handle_host_tools_window_request(request, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                    // Popovers inside the command bar stop propagation. Any
                    // remaining workspace click is outside those overlays and
                    // should dismiss them without stealing the original click.
                    this.close_terminal_command_overlays(cx);
                }),
            )
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                // The top rendered blocking portal owns every key before
                // background IME, terminal, and shortcut routing can observe it.
                if this.capture_active_window_modal_key(event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if this.active_sftp_editor_owns_key(event.keystroke.key.as_str(), cx) {
                    // Windows emits committed characters only after an unhandled
                    // keydown. Do not let pane-level capture override the modal
                    // route that gives document keys to the focused editor.
                    return;
                }
                let active_ime_should_receive_key =
                    this.defer_active_ime_key(&event.keystroke, window, cx);
                if active_ime_should_receive_key {
                    // Tauri DOM inputs let printable keydown bubble while the
                    // browser performs the actual text mutation through the
                    // input/composition pipeline. GPUI follows the same shape:
                    // if we stop propagation here, the platform input handler
                    // may not receive the character or IME candidate control.
                    return;
                }
                if this.handle_active_text_input_edit_shortcut(&event.keystroke, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_active_text_input_delete_selection(&event.keystroke, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_active_text_input_newline(&event.keystroke, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_active_text_input_transpose(&event.keystroke, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_terminal_git_branch_picker_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_compact_terminal_command_sender_key(event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_active_text_input_navigation(&event.keystroke, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_host_process_search_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_host_docker_search_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_host_service_search_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_host_log_search_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_host_tmux_search_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_host_port_search_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_host_schedule_search_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_host_filesystem_search_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_host_package_search_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_cloud_sync_select_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if !this.command_palette.read(cx).is_open()
                    && this
                        .settings_workspace
                        .read(cx)
                        .keybinding_recording_action_id()
                        .is_none()
                    && crate::keybindings::keystroke_matches_action(
                        &event.keystroke,
                        "app.commandPalette",
                        &this.settings_store.settings().keybindings.overrides,
                    )
                {
                    this.open_command_palette(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .settings_workspace
                    .read(cx)
                    .keybinding_recording_action_id()
                    .is_some()
                    && this.active_surface == ActiveSurface::Settings
                    && this.settings_workspace.read(cx).route_snapshot().active_tab
                        == SettingsTab::Keybindings
                {
                    this.handle_keybinding_recording_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if {
                    let quick_commands = &this.terminal.read(cx).quick_commands;
                    quick_commands.is_open() && quick_commands.focused_input().is_some()
                } {
                    this.handle_quick_commands_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_terminal_git_branch_picker_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_terminal_command_overlay_escape(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_ai_inline_panel_key(event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.sftp_presentation_request.is_some() {
                    if event.keystroke.key.eq_ignore_ascii_case("escape") {
                        this.sftp_presentation_request = None;
                        cx.notify();
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_transient_workspace_overlay_escape(event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_privilege_prompt_helper_key(event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.terminal_command_sender_editor_focused(window, cx) {
                    // The editor owns its complete key model, including Tab and
                    // navigation keys that otherwise fall through to the pane.
                } else if this.forward_remote_desktop_key_from_capture(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.dispatch_registered_keybinding(event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if !this.registered_keybinding_matches(event)
                    && this.dispatch_runtime_plugin_keybinding(event, cx)
                {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.forward_terminal_tab_from_capture(event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.active_session_manager_input(cx).is_some() {
                    let _ = this.handle_session_manager_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab(cx)
                    .is_some_and(|tab| tab.kind == TabKind::Forwards)
                    && this.forwarding.read(cx).view().focused_input.is_some()
                {
                    let _ = this.handle_forwards_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab(cx)
                    .is_some_and(|tab| tab.kind == TabKind::Graphics)
                    && this.graphics.read(cx).focused_input().is_some()
                {
                    let _ = this.handle_graphics_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.sftp_view.read(cx).focused_input().is_some()
                    || this
                        .active_tab(cx)
                        .is_some_and(|tab| tab.kind == TabKind::Sftp)
                {
                    // Embedded SFTP inputs keep their keyboard model while a terminal tab is active.
                    let _ = this.handle_sftp_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab(cx)
                    .is_some_and(|tab| tab.kind == TabKind::FileManager)
                {
                    let _ = this.handle_file_manager_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.focused_settings_input.is_some()
                    || this
                        .settings_workspace
                        .read(cx)
                        .settings_entity_focused_input()
                        .is_some()
                    || this.ai_entity.read(cx).focused_settings_input().is_some()
                {
                    let _ = this.handle_settings_input_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.ai_sidebar_visible()
                    && this.ai_entity.read(cx).sidebar_keyboard_target_focused()
                {
                    let _ = this.handle_ai_sidebar_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_workspace_key(event, window, cx);
            }))
            .on_key_up(cx.listener(|this, event: &KeyUpEvent, _window, cx| {
                if this.forward_remote_desktop_key_up(event, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_modifiers_changed(cx.listener(
                |this, event: &ModifiersChangedEvent, _window, cx| {
                    let _ = this.forward_remote_desktop_modifiers_changed(event, cx);
                },
            ))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_sidebar_resize(event, window, cx);
                this.update_embedded_sftp_sidebar_resize(event, window, cx);
                this.update_ai_sidebar_resize(event, window, cx);
                this.update_sftp_pane_resize(event, window, cx);
                this.update_sftp_queue_resize(event, window, cx);
                this.update_terminal_command_sender_resize(event, window, cx);
                this.update_split_drag(event, window, cx);
                this.update_settings_slider_drag(event, cx);
                this.update_terminal_cast_seek_drag(event, cx);
                // Continue scrollbar dragging after the pointer leaves its thin hit target.
                this.update_host_tools_tab_scrollbar_drag(event, cx);
                this.update_tabbar_scrollbar_drag(event, window, cx);
                this.update_ime_selection_drag(event.position, window, cx);
                if this.read_only_selection_drag_active() {
                    this.update_selectable_text_autoscroll(event.position, cx);
                    cx.stop_propagation();
                }
                this.update_sftp_drag_capture(event.position, cx);
                this.update_tab_drag(event, window, cx);
                if this.browser_pointer_capture_owner(cx).is_some() {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    this.release_active_remote_desktop_inputs(cx);
                    this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                    this.blur_text_inputs(cx);
                    this.clear_read_only_ime_selection(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    this.release_active_remote_desktop_inputs(cx);
                    // Browser context menus and Radix popovers both treat
                    // outside pointer activity as a transient-layer dismiss.
                    // Right-click keeps input focus alone but must not leave an
                    // old menu/select open behind the next context action.
                    let _ =
                        this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                }),
            )
            .on_scroll_wheel(cx.listener(|this, _event: &ScrollWheelEvent, _window, cx| {
                // Portal backdrops already occlude wheel input. This catches
                // inline select/popover states that have no full-window layer,
                // closing them before the same wheel event scrolls the page or
                // terminal underneath.
                if this.dismiss_transient_workspace_overlays(cx) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, window, cx| {
                if event.button == MouseButton::Left
                    && this.browser_pointer_capture_owner(cx).is_some()
                {
                    this.finish_workspace_pointer_captures(event, window, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.finish_workspace_pointer_captures(event, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &NewTerminal, window, cx| {
                let _ = this.create_local_terminal_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShellLauncher, _window, cx| {
                this.open_local_shell_launcher(cx);
            }))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                this.request_close_active_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseOtherTabs, window, cx| {
                this.request_close_other_tabs_or_active_pane(window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewConnection, window, cx| {
                this.open_new_connection_form(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, cx| {
                this.toggle_sidebar(cx);
            }))
            .on_action(cx.listener(|this, _: &CommandPalette, _window, cx| {
                this.open_command_palette(cx);
            }))
            .on_action(cx.listener(|this, _: &ZenMode, _window, cx| {
                this.toggle_zen_mode(cx);
            }))
            .on_action(cx.listener(|_this, _: &ToggleFullscreen, window, cx| {
                // Full-screen transitions stay owned by the native window and
                // must not fall through as terminal input.
                window.toggle_fullscreen();
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                this.next_tab(true, window, cx);
                // GPUI dispatches the action before the raw key event. Stop here so the
                // workspace keybinding capture does not advance the tab a second time.
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &PrevTab, window, cx| {
                this.next_tab(false, window, cx);
                // Keep previous-tab navigation on the same single-dispatch path.
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &SplitHorizontal, window, cx| {
                this.split_active_pane(SplitDirection::Horizontal, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitVertical, window, cx| {
                this.split_active_pane(SplitDirection::Vertical, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ClosePane, window, cx| {
                this.close_active_pane(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitNavLeft, window, cx| {
                this.focus_adjacent_pane(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitNavRight, window, cx| {
                this.focus_adjacent_pane(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Copy, _window, cx| {
                if this.copy_active_text_input(cx) {
                    return;
                }
                if this.copy_active_ide_selection(cx) {
                    return;
                }
                if this.connection_form_state(cx).form.is_none() {
                    this.copy(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Cut, _window, cx| {
                if this.cut_active_text_input(cx) {
                    return;
                }
                if this.cut_active_ide_selection(cx) {
                    return;
                }
                if this.connection_form_state(cx).form.is_none() {
                    let _ = this.cut(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Paste, _window, cx| {
                if this.paste_active_text_input(cx) {
                    return;
                }
                if this.paste_into_active_ide_editor(cx) {
                    return;
                }
                if this.connection_form_state(cx).form.is_some() {
                    this.paste_into_new_connection_field(cx);
                } else {
                    this.paste(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Find, window, cx| {
                if this.open_active_ide_search(cx) {
                    return;
                }
                this.open_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FindNext, _window, cx| {
                if this.select_next_active_ide_search_match(cx) {
                    return;
                }
                this.search_next(true, cx);
            }))
            .on_action(cx.listener(|this, _: &FindPrev, _window, cx| {
                if this.select_previous_active_ide_search_match(cx) {
                    return;
                }
                this.search_next(false, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseSearch, window, cx| {
                this.close_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.open_settings(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FontIncrease, _window, cx| {
                this.adjust_terminal_font_size(1, cx);
            }))
            .on_action(cx.listener(|this, _: &FontDecrease, _window, cx| {
                this.adjust_terminal_font_size(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &FontReset, _window, cx| {
                this.reset_terminal_font_size(cx);
            }))
            .on_action(cx.listener(|this, _: &ShowShortcuts, _window, cx| {
                this.open_shortcuts_modal(cx);
            }))
            .on_action(cx.listener(|this, _: &TerminalAiPanel, _window, cx| {
                this.toggle_terminal_ai_inline_panel(_window, cx);
            }))
            .on_action(cx.listener(|this, _: &TerminalClearScreen, _window, cx| {
                this.clear_active_terminal_screen(cx);
            }))
            .on_action(cx.listener(|this, _: &TerminalRecording, _window, cx| {
                this.toggle_active_terminal_recording(cx);
            }))
            .on_action(cx.listener(|this, _: &TerminalFreeTypeMode, _window, cx| {
                this.toggle_free_type_mode(cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteEventLog, window, cx| {
                this.open_notification_center_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteAiSidebar, _window, cx| {
                let _ = this.toggle_ai_sidebar(cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteBroadcast, _window, cx| {
                this.toggle_terminal_broadcast(cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteDisconnectAll, window, cx| {
                this.disconnect_all_ssh_nodes_from_palette(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteReconnectAll, _window, cx| {
                this.reconnect_all_link_down_nodes_from_palette(cx);
            }))
            .on_action(
                cx.listener(|this, _: &PaletteCancelReconnect, _window, cx| {
                    this.cancel_all_reconnects_from_palette(cx);
                }),
            )
            .on_action(cx.listener(|this, _: &PaletteHealthCheck, _window, cx| {
                this.run_connection_health_check_from_palette(cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteResetPanes, window, cx| {
                this.reset_active_tab_to_single_pane(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteDetachTerminal, window, cx| {
                this.detach_active_local_terminal_from_palette(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteCleanupDead, _window, cx| {
                this.cleanup_dead_local_terminal_sessions_from_palette(cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleEnglish, window, cx| {
                this.switch_locale(Locale::En, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleChinese, window, cx| {
                this.switch_locale(Locale::ZhCn, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SwitchLocaleTraditionalChinese, window, cx| {
                    this.switch_locale(Locale::ZhTw, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SwitchLocaleGerman, window, cx| {
                this.switch_locale(Locale::De, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleSpanish, window, cx| {
                this.switch_locale(Locale::EsEs, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleFrench, window, cx| {
                this.switch_locale(Locale::FrFr, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleItalian, window, cx| {
                this.switch_locale(Locale::It, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleJapanese, window, cx| {
                this.switch_locale(Locale::Ja, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleKorean, window, cx| {
                this.switch_locale(Locale::Ko, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SwitchLocalePortugueseBrazil, window, cx| {
                    this.switch_locale(Locale::PtBr, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SwitchLocaleVietnamese, window, cx| {
                this.switch_locale(Locale::Vi, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab1, window, cx| {
                this.go_to_tab(0, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab2, window, cx| {
                this.go_to_tab(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab3, window, cx| {
                this.go_to_tab(2, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab4, window, cx| {
                this.go_to_tab(3, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab5, window, cx| {
                this.go_to_tab(4, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab6, window, cx| {
                this.go_to_tab(5, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab7, window, cx| {
                this.go_to_tab(6, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab8, window, cx| {
                this.go_to_tab(7, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab9, window, cx| {
                this.go_to_tab(8, window, cx);
            }))
            .when_some(window_background_layer, |root, background| {
                // Window scope paints exactly one absolute image behind all persistent chrome.
                root.child(background)
            })
            .when(titlebar_visible, |root| {
                root.child(self.render_title_bar(window, cx))
            })
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .when(!zen_mode, |layout| {
                        layout.child(self.render_activity_bar(cx))
                    })
                    .when(!zen_mode && self.sidebar_rendered, |layout| {
                        layout.child(self.render_animated_sidebar_region(window, cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .min_w(px(self.tokens.metrics.min_main_width))
                            .overflow_hidden()
                            .when(!zen_mode, |main| {
                                main.child(self.render_tab_bar(window, cx))
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .relative()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .left_0()
                                            .right_0()
                                            .bottom_0()
                                            .child(content),
                                    )
                                    .when(
                                        self.search.visible && self.active_pane(cx).is_some(),
                                        |main_content| {
                                            main_content.child(self.render_search_bar(cx))
                                        },
                                    ),
                            ),
                    )
                    .when(!zen_mode && self.context_sidebar_rendered, |layout| {
                        // Keep the right sidebar as one root child. Splitting
                        // the gutter and content here makes resize hit-testing
                        // easy to regress on scroll-heavy Host Tools pages.
                        layout.child(self.render_animated_context_sidebar_frame(cx))
                    }),
            )
            .when(!zen_mode && !self.sidebar_collapsed, |root| {
                root.child(self.render_left_sidebar_resize_hotzone(effective_titlebar_height, cx))
            })
            .when(!zen_mode && self.context_sidebar_visible(), |root| {
                root.child(
                    self.render_context_right_sidebar_resize_hotzone(effective_titlebar_height, cx),
                )
            })
            .when(sidebar_resize_cursor_active, |root| {
                root.child(
                    canvas(
                        |_, _, _| (),
                        |_, _, window, _| {
                            // A window-level override keeps the resize cursor active above
                            // blocking terminal and virtual-list hitboxes.
                            window.set_window_cursor_style(CursorStyle::ResizeColumn);
                        },
                    )
                    .absolute(),
                )
            })
            .when(embedded_sftp_resize_cursor_active, |root| {
                root.child(
                    canvas(
                        |_, _, _| (),
                        |_, _, window, _| {
                            // Keep the row-resize cursor stable while the
                            // pointer crosses either virtualized sidebar list.
                            window.set_window_cursor_style(CursorStyle::ResizeRow);
                        },
                    )
                    .absolute(),
                )
            })
            .when(
                self.browser_pointer_capture_owner(cx)
                    .is_some_and(browser_behavior::pointer_capture_needs_workspace_overlay),
                |root| root.child(self.render_workspace_pointer_capture_overlay(cx)),
            )
            .when(self.connection_form_state(cx).form.is_some(), |root| {
                root.child(self.render_new_connection_modal(window, cx))
            })
            .when_some(self.render_sftp_presentation_dialog(cx), |root, dialog| {
                root.child(dialog)
            })
            .when(self.local_shell_launcher_open, |root| {
                root.child(self.render_local_shell_launcher(window, cx))
            })
            .when(
                self.connection_form_state(cx)
                    .form
                    .as_ref()
                    .is_some_and(|form| form.jump_server_form.is_some()),
                |root| root.child(self.render_add_jump_server_modal(window, cx)),
            )
            .when_some(
                self.render_new_connection_select_overlay(window, cx),
                |root, overlay| root.child(overlay),
            )
            .when(
                self.connection_flow.read(cx).has_host_key_challenge(),
                |root| root.child(self.render_host_key_dialog(cx)),
            )
            .when(
                self.connection_flow
                    .read(cx)
                    .has_keyboard_interactive_challenge(),
                |root| root.child(self.render_keyboard_interactive_dialog(cx)),
            )
            .when(
                self.ai_entity.read(cx).settings_confirm_is_enable(),
                |root| root.child(self.render_ai_enable_confirm_dialog(cx)),
            )
            .when(
                self.ai_entity
                    .read(cx)
                    .settings_confirm_is_provider_key_remove(),
                |root| root.child(self.render_ai_provider_key_remove_confirm_dialog(cx)),
            )
            .when(
                self.ai_entity
                    .read(cx)
                    .settings_confirm_provider_name()
                    .is_some(),
                |root| root.child(self.render_ai_provider_remove_confirm_dialog(cx)),
            )
            .when(
                self.ai_entity.read(cx).chat_ui().safety_confirm_open,
                |root| root.child(self.render_ai_safety_confirm_dialog(cx)),
            )
            .when(
                self.ai_entity.read(cx).chat_ui().summarize_confirm_open,
                |root| root.child(self.render_ai_summarize_confirm_dialog(cx)),
            )
            .when(
                ai_chat_confirm_snapshot.as_ref().is_some_and(|snapshot| {
                    matches!(&snapshot.kind, ai_state::AiChatConfirmKind::ClearAll)
                }),
                |root| root.child(self.render_ai_clear_all_confirm_dialog(cx)),
            )
            .when(
                ai_chat_confirm_snapshot.as_ref().is_some_and(|snapshot| {
                    matches!(
                        &snapshot.kind,
                        ai_state::AiChatConfirmKind::DeleteMessage { .. }
                    )
                }),
                |root| root.child(self.render_ai_delete_message_confirm_dialog(cx)),
            )
            .when(
                overlay_confirm_snapshot.as_ref().is_some_and(|snapshot| {
                    matches!(&snapshot.kind, WorkspaceOverlayConfirmKind::SettingsReset)
                }),
                |root| root.child(self.render_settings_reset_confirm_dialog(cx)),
            )
            .when_some(
                self.render_settings_data_directory_confirm_dialog(cx),
                |root, dialog| root.child(dialog),
            )
            .when_some(
                self.render_remote_shell_integration_confirm(cx),
                |root, dialog| root.child(dialog),
            )
            .when_some(
                self.render_terminal_trigger_quick_command_confirm(cx),
                |root, dialog| root.child(dialog),
            )
            .when(cloud_sync_confirm_open, |root| {
                root.child(self.render_cloud_sync_confirm_dialog(cx))
            })
            .when(
                matches!(
                    overlay_confirm_snapshot
                        .as_ref()
                        .map(|snapshot| &snapshot.kind),
                    Some(WorkspaceOverlayConfirmKind::NodeDisconnect { .. })
                ),
                |root| root.child(self.render_node_disconnect_confirm_dialog(cx)),
            )
            .when(tab_close_confirm_open, |root| {
                root.child(self.render_tab_close_confirm_dialog(cx))
            })
            .when_some(
                self.render_host_process_confirm_dialog(cx),
                |root, dialog| root.child(dialog),
            )
            .when_some(
                self.render_host_docker_confirm_dialog(cx),
                |root, dialog| root.child(dialog),
            )
            .when_some(self.render_host_docker_logs_dialog(cx), |root, dialog| {
                root.child(dialog)
            })
            .when_some(
                self.render_host_service_confirm_dialog(cx),
                |root, dialog| root.child(dialog),
            )
            .when_some(self.render_host_service_logs_dialog(cx), |root, dialog| {
                root.child(dialog)
            })
            .when_some(self.render_host_tmux_confirm_dialog(cx), |root, dialog| {
                root.child(dialog)
            })
            .when_some(self.render_host_tmux_input_dialog(cx), |root, dialog| {
                root.child(dialog)
            })
            .when_some(
                self.render_host_schedule_confirm_dialog(cx),
                |root, dialog| root.child(dialog),
            )
            .when_some(self.render_host_schedule_logs_dialog(cx), |root, dialog| {
                root.child(dialog)
            })
            .when_some(
                self.render_native_plugin_confirm_dialog(cx),
                |root, dialog| root.child(dialog),
            )
            // Tab renaming is a main-window portal and never remounts the terminal tab.
            .when_some(self.render_tab_rename_dialog(cx), |root, dialog| {
                root.child(dialog)
            })
            // Tab-owned dialogs are portaled here so their backdrops cover all window chrome.
            .children(active_tab_window_modals)
            .when_some(
                self.render_ai_sidebar_floating_overlay(window, cx),
                |root, overlay| root.child(overlay),
            )
            .when(self.terminal.read(cx).broadcast_menu_open(), |root| {
                let placement = if self.settings_store.settings().terminal.command_bar.enabled {
                    actions::TerminalBroadcastMenuPlacement::Bottom(62.0)
                } else {
                    actions::TerminalBroadcastMenuPlacement::Top(
                        effective_titlebar_height + self.tokens.metrics.tabbar_height + 6.0,
                    )
                };
                root.child(self.workspace_context_menu_backdrop(
                    self.render_terminal_broadcast_menu(placement, cx),
                    cx,
                ))
            })
            .when_some(
                self.render_detached_tab_return_handoff(window, cx),
                |root, handoff| root.child(handoff),
            )
            .when_some(
                self.render_tab_detach_drag_preview(window, cx),
                |root, preview| root.child(preview),
            )
            .when_some(self.render_tab_context_menu(window, cx), |root, menu| {
                root.child(menu)
            })
            .when_some(self.render_terminal_cast_player(cx), |root, player| {
                root.child(player)
            })
            .when_some(self.render_theme_editor_modal(cx), |root, modal| {
                // Theme editing is a workspace modal, not a settings-pane overlay.
                // Mount it here so the backdrop covers every persistent chrome region.
                root.child(modal)
            })
            .when_some(
                self.render_settings_ssh_config_import_dialog(cx),
                |root, modal| {
                    // Both Settings and Connections launch the same workspace
                    // import flow, so its modal cannot be owned by either page.
                    root.child(modal)
                },
            )
            .when(self.terminal_command_specs_editor_open, |root| {
                // Structured command specs use the same workspace-wide modal
                // ownership so the settings list never contains a nested editor.
                root.child(self.render_terminal_command_specs_editor_modal(cx))
            })
            .when(
                self.terminal.read(cx).quick_commands.manager_open(),
                |root| {
                    // Quick command editing is independent from the compact
                    // command-bar launcher and must cover all workspace chrome.
                    root.child(self.render_quick_commands_manager_modal(cx))
                },
            )
            .when(self.ai_text_editor_dialog.is_some(), |root| {
                // Long AI documents use workspace-wide modal ownership so the
                // settings list keeps compact, independently measured cards.
                root.child(self.render_ai_text_editor_modal(cx))
            })
            .when(
                self.session_manager.read(cx).oxide_import_dialog.is_some(),
                |root| {
                    // .oxide dialogs are application-level import flows. Portal
                    // them beside the command palette so their backdrop covers
                    // activity, session, tab, content, and companion sidebars.
                    root.child(self.render_oxide_import_dialog(cx))
                },
            )
            .when(
                self.session_manager.read(cx).oxide_export_dialog.is_some(),
                |root| {
                    // Export uses the same workspace-wide modal ownership as import.
                    root.child(self.render_oxide_export_dialog(cx))
                },
            )
            .when(self.command_palette.read(cx).is_open(), |root| {
                root.child(self.render_command_palette(cx))
            })
            .when(self.version_migration.open, |root| {
                root.child(self.render_version_migration_modal(window, cx))
            })
            .when(
                self.onboarding.open && !self.version_migration.open,
                |root| root.child(self.render_onboarding_modal(window, cx)),
            )
            .when(
                overlay_confirm_snapshot.as_ref().is_some_and(|snapshot| {
                    matches!(&snapshot.kind, WorkspaceOverlayConfirmKind::LegalNotice)
                }),
                |root| root.child(self.render_help_legal_notice_dialog(cx)),
            )
            .when(
                overlay_confirm_snapshot.as_ref().is_some_and(|snapshot| {
                    matches!(
                        &snapshot.kind,
                        WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes
                    )
                }),
                |root| root.child(self.render_native_update_release_notes_dialog(cx)),
            )
            .when(self.shortcuts_modal.open, |root| {
                root.child(self.render_shortcuts_modal(cx))
            })
            .when_some(self.render_app_lock_dialog(cx), |root, dialog| {
                root.child(dialog)
            })
            .when(self.mermaid_zoom.is_some(), |root| {
                root.child(self.render_mermaid_zoom_modal(window, cx))
            })
            .children(overlay_layers)
            .child(WorkspaceImeElement::new(
                cx.entity(),
                self.focus_handle.clone(),
            ))
            .into_any_element()
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_workspace_pointer_capture_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cursor = match self.browser_pointer_capture_owner(cx) {
            Some(browser_behavior::BrowserPointerCaptureOwner::HostToolsTabScrollbar) => {
                CursorStyle::ClosedHand
            }
            Some(
                browser_behavior::BrowserPointerCaptureOwner::EmbeddedSftpSidebarResize
                | browser_behavior::BrowserPointerCaptureOwner::SftpQueueResize
                | browser_behavior::BrowserPointerCaptureOwner::TerminalCommandSenderResize,
            ) => CursorStyle::ResizeRow,
            _ => CursorStyle::ResizeColumn,
        };
        div()
            .id("workspace-pointer-capture-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .cursor(cursor)
            // Scroll-heavy surfaces can stop bubbling after data arrives, so
            // captured resizes and scrollbar drags move through this top layer.
            .occlude()
            .bg(rgba(0x00000000))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_sidebar_resize(event, window, cx);
                this.update_embedded_sftp_sidebar_resize(event, window, cx);
                this.update_ai_sidebar_resize(event, window, cx);
                this.update_sftp_pane_resize(event, window, cx);
                this.update_sftp_queue_resize(event, window, cx);
                this.update_terminal_command_sender_resize(event, window, cx);
                this.update_host_tools_tab_scrollbar_drag(event, cx);
                cx.stop_propagation();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.finish_workspace_pointer_captures(event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn finish_workspace_pointer_captures(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let capture_owner = self.browser_pointer_capture_owner(cx);
        let was_read_only_dragging = self.read_only_selection_drag_active();
        self.finish_sidebar_resize(cx);
        self.finish_embedded_sftp_sidebar_resize(cx);
        self.finish_ai_sidebar_resize(cx);
        self.finish_sftp_pane_resize(cx);
        self.finish_sftp_queue_resize(cx);
        self.finish_terminal_command_sender_resize(cx);
        self.finish_split_drag(cx);
        self.finish_settings_slider_drag(cx);
        self.finish_terminal_cast_seek_drag(cx);
        self.finish_host_tools_tab_scrollbar_drag(cx);
        self.finish_tabbar_scrollbar_drag(cx);
        self.finish_ime_selection_drag(cx);
        self.stop_selectable_text_autoscroll();
        self.finish_tab_drag(event, window, cx);
        let cancelled_sftp_drag = self.cancel_sftp_drag_capture(cx);
        if cancelled_sftp_drag {
            cx.notify();
        }
        if capture_owner.is_some() || was_read_only_dragging || cancelled_sftp_drag {
            cx.stop_propagation();
        }
    }

    pub(in crate::workspace) fn render_mermaid_zoom_modal(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(state) = self.mermaid_zoom.as_ref() else {
            return div().into_any_element();
        };
        let viewport = window.viewport_size();
        let max_width = f32::from(viewport.width) * 0.92;
        let max_height = f32::from(viewport.height) * 0.86;
        let title = self.i18n.t("markdown.mermaid_expand");
        let subtitle = state
            .source
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_string())
            .unwrap_or_else(|| "Mermaid".to_string());

        deferred(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.mermaid_zoom = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    oxideterm_gpui_ui::modal_container(&self.tokens)
                        .w(px(max_width.min(state.width + 48.0).max(360.0)))
                        .max_h(px(max_height))
                        .flex()
                        .flex_col()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .child(oxideterm_gpui_ui::modal_header(
                            &self.tokens,
                            title,
                            subtitle,
                        ))
                        .child(
                            oxideterm_gpui_ui::modal_body(&self.tokens)
                                .id("mermaid-zoom-modal-body-scroll")
                                .flex_1()
                                .min_h(px(0.0))
                                .selectable_overflow_y_scroll(&self.selectable_text_scroll_handle(
                                    "mermaid-zoom-modal-body-scroll",
                                ))
                                .child(
                                    div().w_full().overflow_x_scrollbar().child(
                                        div()
                                            .w(px(state.width.max(1.0)))
                                            .h(px(state.height.max(1.0)))
                                            .child(
                                                gpui::img(state.image.clone())
                                                    .w(px(state.width.max(1.0)))
                                                    .h(px(state.height.max(1.0))),
                                            ),
                                    ),
                                ),
                        ),
                ),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn queue_workspace_tooltip(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.apply_workspace_overlay_intent(
            WorkspaceOverlayIntent::QueueTooltip {
                id: id.into(),
                label: label.into(),
                x,
                y,
            },
            cx,
        );
    }

    pub(in crate::workspace) fn clear_workspace_tooltip(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        self.apply_workspace_overlay_intent(
            WorkspaceOverlayIntent::ClearTooltip { id: id.to_string() },
            cx,
        );
    }

    pub(in crate::workspace) fn clear_all_workspace_tooltips(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.overlay.update(cx, |overlay, cx| {
            overlay.apply_intent(WorkspaceOverlayIntent::ClearAllTooltips, cx)
        })
    }

    pub(in crate::workspace) fn apply_workspace_overlay_intent(
        &self,
        intent: WorkspaceOverlayIntent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.overlay
            .update(cx, |overlay, cx| overlay.apply_intent(intent, cx))
    }

    pub(in crate::workspace) fn push_workspace_notice(&self, notice: TerminalNotice, cx: &App) {
        // Producers submit through the Entity-owned channel so the root never
        // retains a second sender or owns delivery lifetime.
        let _ = self.overlay.read(cx).notice_sender().send(notice);
    }

    pub(in crate::workspace) fn handle_workspace_terminal_event(
        &mut self,
        event: &WorkspaceTerminalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorkspaceTerminalEvent::GitMetadataChanged => {
                cx.notify();
            }
            WorkspaceTerminalEvent::ProjectMetadataChanged => {
                if let Some(key) = self.active_terminal_project_key(cx) {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal.ensure_project_task_highlight(&key);
                    });
                }
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn apply_workspace_runtime_connection_trace_event(
        &mut self,
        event: ConnectionTraceEvent,
        cx: &mut Context<Self>,
    ) {
        self.apply_workspace_overlay_intent(
            WorkspaceOverlayIntent::ConnectionTraceEvents(vec![event]),
            cx,
        );
    }
}
