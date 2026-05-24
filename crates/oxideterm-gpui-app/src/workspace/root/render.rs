impl Focusable for WorkspaceApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WorkspaceApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.begin_selectable_text_frame();
        self.sync_tab_titles(cx);
        self.poll_forwarding_worker_results(cx);
        self.poll_graphics_worker_results(window, cx);
        self.poll_connection_monitor_updates(cx);
        self.maybe_refresh_connection_monitor(cx);
        self.poll_connection_trace_events(cx);
        self.poll_terminal_notices(cx);
        self.poll_ai_chat_stream_events(Some(window), cx);
        self.poll_ai_compaction_results(cx);
        self.poll_ai_model_selector_probe_results(cx);
        self.poll_ai_model_refresh_results(cx);
        self.observe_active_tab_for_history();
        let title = self
            .active_tab()
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_else(|| "OxideTerm".to_string());
        window.set_window_title(&SharedString::from(title));
        let vibrancy_mode =
            effective_vibrancy_mode(self.settings_store.settings(), &self.render_policy);
        // Modal/command-palette backdrop blur follows the active render profile
        // just like Tauri's linuxBackdropBlurClass compatibility gate.
        set_tauri_backdrop_blur_allowed(self.render_policy.allow_background_blur);
        if self.applied_vibrancy_mode != vibrancy_mode {
            let _ = apply_window_vibrancy(window, vibrancy_mode);
            self.applied_vibrancy_mode = vibrancy_mode;
        }
        if self.needs_active_pane_focus
            && self
                .active_tab()
                .is_some_and(|tab| {
                    !matches!(
                        tab.kind,
                        TabKind::Settings
                            | TabKind::SessionManager
                            | TabKind::FileManager
                            | TabKind::Launcher
                            | TabKind::Graphics
                            | TabKind::ConnectionPool
                            | TabKind::ConnectionMonitor
                            | TabKind::Topology
                            | TabKind::NotificationCenter
                            | TabKind::PluginManager
                            | TabKind::CloudSync
                    )
                })
            && !self.search.visible
            && self.new_connection_form.is_none()
            && let Some(pane) = self.active_pane()
        {
            self.needs_active_pane_focus = false;
            self.clear_ai_sidebar_keyboard_focus();
            window.on_next_frame(move |window, cx| {
                pane.read(cx).focus(window);
            });
        }

        let content = if let Some(tab) = self.active_tab() {
            match (&tab.kind, &tab.root_pane) {
                (TabKind::Settings, _) => {
                    self.render_settings_surface(cx)
                }
                (TabKind::FileManager, _) => self.render_file_manager_surface(window, cx),
                (TabKind::Launcher, _) => self.render_launcher_surface(window, cx),
                (TabKind::Graphics, _) => self.render_graphics_surface(window, cx),
                (TabKind::ConnectionPool, _) => self.render_connection_pool_surface(cx),
                (TabKind::ConnectionMonitor, _) => self.render_connection_monitor_surface(cx),
                (TabKind::Topology, _) => self.render_topology_surface(cx),
                (TabKind::NotificationCenter, _) => self.render_notification_center_surface(cx),
                (TabKind::Sftp, _) => self.render_sftp_surface(window, cx),
                (TabKind::Ide, _) => self.render_ide_surface(cx),
                (TabKind::Forwards, _) => self.render_forwards_surface(window, cx),
                (TabKind::SessionManager, _) => self.render_session_manager_surface(window, cx),
                (TabKind::PluginManager, _) => self.render_plugin_manager_surface(cx),
                (TabKind::CloudSync, _) => self.render_cloud_sync_surface(cx),
                (_, Some(root_pane)) => self.render_terminal_surface(root_pane, cx),
                _ => self.render_empty_workspace(cx),
            }
        } else {
            self.render_empty_workspace(cx)
        };
        let content = self.wrap_content_background(
            content,
            self.active_tab().map(|tab| tab_background_key(&tab.kind)),
            cx,
        );
        let toast_layer = self.render_workspace_toasts();
        let zen_mode = self.settings_store.settings().sidebar_ui.zen_mode;

        div()
            .id("workspace-root")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(workspace_background(&self.tokens, vibrancy_mode))
            .text_color(rgb(self.tokens.ui.text))
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .when(self.sidebar_resizing || self.ai_sidebar_resizing, |root| {
                root.cursor(CursorStyle::ResizeColumn)
            })
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let active_ime_commits_printable_key =
                    this.defer_active_ime_printable_key(&event.keystroke, window, cx);
                if active_ime_commits_printable_key {
                    // Tauri DOM inputs let printable keydown bubble while the
                    // browser performs the actual text mutation through the
                    // input/composition pipeline. GPUI follows the same shape:
                    // if we stop propagation here, the platform input handler
                    // may not receive the character and page-level fallbacks can
                    // become a second writer. Record the owner for dedupe, then
                    // leave the key unhandled so Window dispatches text once.
                    return;
                }
                if this.keyboard_interactive_challenge.is_some() {
                    let _ = this.handle_keyboard_interactive_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.host_key_challenge.is_some() {
                    if event.keystroke.key.as_str() == "escape" {
                        this.cancel_host_key_challenge(cx);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_active_text_input_edit_shortcut(&event.keystroke, cx) {
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
                } else if this.handle_active_text_input_navigation(&event.keystroke, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_cloud_sync_confirm_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_ai_settings_confirm_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_ai_sidebar_confirm_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_settings_confirm_key(event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_ai_mcp_add_dialog_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_oxide_dialog_footer_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_cloud_sync_select_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_session_manager_basic_dialog_footer_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if !this.command_palette.open
                    && this.keybinding_recording_action_id.is_none()
                    && crate::keybindings::keystroke_matches_action(
                        &event.keystroke,
                        "app.commandPalette",
                        &this.settings_store.settings().keybindings.overrides,
                    )
                {
                    this.open_command_palette(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.auto_route_modal.open {
                    let _ = this.handle_auto_route_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.new_connection_form.is_some() {
                    let _ = this.handle_new_connection_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.command_palette.open {
                    this.handle_command_palette_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.shortcuts_modal.open {
                    this.handle_shortcuts_modal_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.keybinding_recording_action_id.is_some()
                    && this.active_surface == ActiveSurface::Settings
                    && this.active_settings_tab == SettingsTab::Keybindings
                {
                    this.handle_keybinding_recording_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.terminal_quick_commands_open
                    && this.quick_commands.focused_input.is_some()
                {
                    this.handle_quick_commands_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_terminal_command_overlay_escape(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.handle_transient_workspace_overlay_escape(event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.terminal_command_bar_focused {
                    this.handle_terminal_command_bar_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.dispatch_registered_keybinding(event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::SessionManager)
                    && this.session_manager.focused_input.is_some()
                {
                    let _ = this.handle_session_manager_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::Forwards)
                    && this.forwarding_view.focused_input.is_some()
                {
                    let _ = this.handle_forwards_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::Launcher)
                    && this.launcher.focused_input.is_some()
                {
                    let _ = this.handle_launcher_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::Graphics)
                    && this.graphics.focused_input.is_some()
                {
                    let _ = this.handle_graphics_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::Sftp)
                {
                    let _ = this.handle_sftp_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::FileManager)
                {
                    let _ = this.handle_file_manager_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.focused_settings_input.is_some() {
                    let _ = this.handle_settings_input_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.ai_sidebar_visible()
                    && (this.ai_chat_input_focused
                        || this.ai_chat_footer_focus.is_some()
                        || this.ai_model_selector_search_focused)
                {
                    let _ = this.handle_ai_sidebar_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .terminal_cast_player
                    .as_ref()
                    .is_some_and(|player| player.search_focused)
                {
                    this.handle_terminal_cast_search_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_workspace_key(event, window, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_sidebar_resize(event, cx);
                this.update_ai_sidebar_resize(event, window, cx);
                this.update_split_drag(event, window, cx);
                this.update_settings_slider_drag(event, cx);
                this.update_terminal_cast_seek_drag(event, cx);
                this.update_ime_selection_drag(event.position, window, cx);
                if this.read_only_selection_drag_active() {
                    this.update_selectable_text_autoscroll(event.position, cx);
                    cx.stop_propagation();
                }
                this.update_sftp_drag_capture(event.position, cx);
                this.update_tab_drag(event, window, cx);
                if this.browser_pointer_capture_owner().is_some() {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                    this.blur_text_inputs(cx);
                    this.clear_read_only_ime_selection(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
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
                if this.dismiss_transient_workspace_overlays() {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    let capture_owner = this.browser_pointer_capture_owner();
                    let was_read_only_dragging = this.read_only_selection_drag_active();
                    this.finish_sidebar_resize(cx);
                    this.finish_ai_sidebar_resize(cx);
                    this.finish_split_drag(cx);
                    this.finish_settings_slider_drag(cx);
                    this.finish_terminal_cast_seek_drag(cx);
                    this.finish_ime_selection_drag(cx);
                    this.stop_selectable_text_autoscroll();
                    this.finish_tab_drag(event, window, cx);
                    let cancelled_sftp_drag = this.cancel_sftp_drag_capture();
                    if this.launcher.pressed_app_path.take().is_some() {
                        cx.notify();
                    }
                    if cancelled_sftp_drag {
                        cx.notify();
                    }
                    if capture_owner.is_some() || was_read_only_dragging || cancelled_sftp_drag {
                        cx.stop_propagation();
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &NewTerminal, window, cx| {
                let _ = this.create_local_terminal_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShellLauncher, window, cx| {
                this.open_launcher_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                this.close_active_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseOtherTabs, window, cx| {
                this.close_other_tabs_or_active_pane(window, cx);
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
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                this.next_tab(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PrevTab, window, cx| {
                this.next_tab(false, window, cx);
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
                if this.new_connection_form.is_none() {
                    this.copy(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Paste, _window, cx| {
                if this.paste_active_text_input(cx) {
                    return;
                }
                if this.new_connection_form.is_some() {
                    this.paste_into_new_connection_field(cx);
                } else {
                    this.paste(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Find, window, cx| {
                this.open_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FindNext, _window, cx| {
                this.search_next(true, cx);
            }))
            .on_action(cx.listener(|this, _: &FindPrev, _window, cx| {
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
                let _ = this.toggle_ai_sidebar(cx);
            }))
            .on_action(cx.listener(|this, _: &TerminalRecording, _window, cx| {
                this.toggle_active_terminal_recording(cx);
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
            .on_action(cx.listener(|this, _: &PaletteCancelReconnect, _window, cx| {
                this.cancel_all_reconnects_from_palette(cx);
            }))
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
            .child(self.render_title_bar())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .when(!zen_mode, |layout| layout.child(self.render_activity_bar(cx)))
                    .when(!zen_mode && !self.sidebar_collapsed, |layout| {
                        layout.child(self.render_sidebar_region(cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .min_w(px(self.tokens.metrics.min_main_width))
                            .overflow_hidden()
                            .when(!zen_mode, |main| main.child(self.render_tab_bar(window, cx)))
                            .when(self.search.visible, |main| {
                                main.child(self.render_search_bar(cx))
                            })
                            .child(
                                div().flex_1().relative().overflow_hidden().child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .right_0()
                                        .bottom_0()
                                    .child(content),
                                ),
                            ),
                    )
                    .when(self.ai_sidebar_visible(), |layout| {
                        layout.child(self.render_ai_right_sidebar_region(cx))
                    }),
            )
            .when(self.new_connection_form.is_some(), |root| {
                root.child(self.render_new_connection_modal(window, cx))
            })
            .when(self.auto_route_modal.open, |root| {
                root.child(self.render_auto_route_modal(window, cx))
            })
            .when(
                self.new_connection_form
                    .as_ref()
                    .is_some_and(|form| form.jump_server_form.is_some()),
                |root| root.child(self.render_add_jump_server_modal(cx)),
            )
            .when_some(
                self.render_new_connection_select_overlay(window, cx),
                |root, overlay| root.child(overlay),
            )
            .when(self.host_key_challenge.is_some(), |root| {
                root.child(self.render_host_key_dialog(cx))
            })
            .when(self.keyboard_interactive_challenge.is_some(), |root| {
                root.child(self.render_keyboard_interactive_dialog(cx))
            })
            .when(self.show_ai_enable_confirm, |root| {
                root.child(self.render_ai_enable_confirm_dialog(cx))
            })
            .when(self.ai_provider_key_remove_confirm.is_some(), |root| {
                root.child(self.render_ai_provider_key_remove_confirm_dialog(cx))
            })
            .when(self.ai_provider_remove_confirm.is_some(), |root| {
                root.child(self.render_ai_provider_remove_confirm_dialog(cx))
            })
            .when(self.ai_safety_confirm_open, |root| {
                root.child(self.render_ai_safety_confirm_dialog(cx))
            })
            .when(self.ai_summarize_confirm_open, |root| {
                root.child(self.render_ai_summarize_confirm_dialog(cx))
            })
            .when(self.ai_clear_all_confirm_open, |root| {
                root.child(self.render_ai_clear_all_confirm_dialog(cx))
            })
            .when(self.ai_delete_message_confirm.is_some(), |root| {
                root.child(self.render_ai_delete_message_confirm_dialog(cx))
            })
            .when(self.settings_reset_confirm_open, |root| {
                root.child(self.render_settings_reset_confirm_dialog(cx))
            })
            .when(self.cloud_sync_confirm.is_some(), |root| {
                root.child(self.render_cloud_sync_confirm_dialog(cx))
            })
            .when_some(self.render_ai_sidebar_floating_overlay(window, cx), |root, overlay| {
                root.child(overlay)
            })
            .when(
                self.active_tab()
                    .is_some_and(|tab| matches!(tab.kind, TabKind::Sftp)),
                |root| {
                    if let Some(dialog) = self.sftp_view.dialog.as_ref() {
                        // Tauri's Radix Dialog portals a modal overlay at the window root.
                        // Keep GPUI SFTP dialogs outside the SFTP pane tree so hit-testing and
                        // scroll input cannot leak to file rows behind the preview.
                        let has_background = self.terminal_background_preferences("sftp").is_some();
                        root.child(self.render_sftp_dialog(dialog.clone(), has_background, cx))
                    } else {
                        root
                    }
                },
            )
            .when(
                self.active_tab()
                    .is_some_and(|tab| matches!(tab.kind, TabKind::FileManager)),
                |root| {
                    if self.file_manager.dialog.is_some() {
                        // Tauri QuickLook and file manager dialogs are portaled to
                        // document.body, so native must not center them inside only
                        // the file-manager pane.
                        let has_background = self
                            .terminal_background_preferences("file_manager")
                            .is_some();
                        root.child(self.render_file_manager_dialog(window, has_background, cx))
                    } else {
                        root
                    }
                },
            )
            .when(
                self.terminal_broadcast_menu_open,
                |root| {
                    let placement = if self.settings_store.settings().terminal.command_bar.enabled {
                        actions::TerminalBroadcastMenuPlacement::Bottom(62.0)
                    } else {
                        actions::TerminalBroadcastMenuPlacement::Top(
                            self.tokens.metrics.titlebar_height
                                + self.tokens.metrics.tabbar_height
                                + 6.0,
                        )
                    };
                    root.child(
                        self.workspace_context_menu_backdrop(
                            self.render_terminal_broadcast_menu(placement, cx),
                            cx,
                        ),
                    )
                },
            )
            .when(
                self.settings_store
                    .settings()
                    .terminal
                    .command_bar
                    .quick_commands_enabled
                    && self.terminal_quick_commands_open,
                |root| {
                    root.child(
                        popover_backdrop()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, window, cx| {
                                    this.dismiss_transient_workspace_overlays_from_outside_pointer(
                                        window, cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(|this, _event, window, cx| {
                                    this.dismiss_transient_workspace_overlays_from_outside_pointer(
                                        window, cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .child(self.render_terminal_quick_commands_popover(cx)),
                    )
                },
            )
            .when_some(self.render_terminal_cast_player(cx), |root, player| {
                root.child(player)
            })
            .when(self.command_palette.open, |root| {
                root.child(self.render_command_palette(cx))
            })
            .when(self.shortcuts_modal.open, |root| {
                root.child(self.render_shortcuts_modal(cx))
            })
            .when_some(self.workspace_tooltip.clone(), |root, tooltip| {
                root.child(self.render_workspace_tooltip(tooltip))
            })
            .when(
                self.zen_hint_expires_at
                    .is_some_and(|expires_at| expires_at > Instant::now()),
                |root| root.child(self.render_zen_mode_hint()),
            )
            .when_some(toast_layer, |root, layer| root.child(layer))
            .child(WorkspaceImeElement::new(
                cx.entity(),
                self.focus_handle.clone(),
            ))
    }
}

impl WorkspaceApp {
    fn render_zen_mode_hint(&self) -> AnyElement {
        let key = if cfg!(target_os = "macos") {
            "zen_mode.hint"
        } else {
            "zen_mode.hint_other"
        };

        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(24.0))
            .flex()
            .justify_center()
            .child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgba((self.tokens.ui.bg_elevated << 8) | 0xe6))
                    .px(px(16.0))
                    .py(px(8.0))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .shadow_lg()
                    .child(self.i18n.t(key)),
            )
            .into_any_element()
    }
}

impl WorkspaceApp {
    pub(super) fn queue_workspace_tooltip(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        const TOOLTIP_DELAY: Duration = Duration::from_millis(300); // Tauri TooltipProvider delayDuration.

        let id = id.into();
        let label = label.into();
        if let Some(tooltip) = self.workspace_tooltip.as_mut()
            && tooltip.id == id
        {
            // Once visible, a tooltip tracks pointer movement immediately like
            // Tauri/Radix TooltipContent. Re-queueing it would restart the
            // delay on every mouse move and leave stale text behind.
            tooltip.label = label;
            tooltip.x = x;
            tooltip.y = y;
            return;
        }
        if let Some(pending) = self.workspace_tooltip_pending.as_mut()
            && pending.id == id
        {
            pending.x = x;
            pending.y = y;
            return;
        }

        self.workspace_tooltip = None;
        self.workspace_tooltip_generation = self.workspace_tooltip_generation.wrapping_add(1);
        let generation = self.workspace_tooltip_generation;
        self.workspace_tooltip_pending = Some(WorkspaceTooltipPending {
            id,
            label,
            x,
            y,
            generation,
        });
        cx.spawn(async move |weak, cx| {
            Timer::after(TOOLTIP_DELAY).await;
            let _ = weak.update(cx, move |workspace, cx| {
                let Some(pending) = workspace.workspace_tooltip_pending.as_ref() else {
                    return;
                };
                if pending.generation != generation {
                    return;
                }
                workspace.workspace_tooltip = Some(WorkspaceTooltip {
                    id: pending.id.clone(),
                    label: pending.label.clone(),
                    x: pending.x,
                    y: pending.y,
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn clear_workspace_tooltip(&mut self, id: &str, cx: &mut Context<Self>) {
        let mut changed = false;
        if self
            .workspace_tooltip_pending
            .as_ref()
            .is_some_and(|pending| pending.id == id)
        {
            self.workspace_tooltip_pending = None;
            self.workspace_tooltip_generation = self.workspace_tooltip_generation.wrapping_add(1);
            changed = true;
        }
        if self
            .workspace_tooltip
            .as_ref()
            .is_some_and(|tooltip| tooltip.id == id)
        {
            self.workspace_tooltip = None;
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    fn render_workspace_tooltip(&self, tooltip: WorkspaceTooltip) -> AnyElement {
        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(tooltip.x), px(tooltip.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(tooltip_content(&self.tokens, tooltip.label, None)),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_TOOLTIP_LAYER_PRIORITY)
        .into_any_element()
    }

    fn poll_terminal_notices(&mut self, cx: &mut Context<Self>) {
        const WORKSPACE_TOAST_TTL: Duration = Duration::from_secs(4);

        let now = Instant::now();
        self.workspace_toasts
            .retain(|toast| toast.expires_at > now);
        self.connection_trace_toasts
            .retain(|_, trace| trace.expires_at.map_or(true, |expires_at| expires_at > now));

        let mut added = false;
        while let Ok(notice) = self.terminal_notice_rx.try_recv() {
            self.workspace_toasts.push(WorkspaceToast {
                notice,
                expires_at: now + WORKSPACE_TOAST_TTL,
            });
            added = true;
        }

        if added {
            cx.spawn(async move |weak, cx| {
                Timer::after(WORKSPACE_TOAST_TTL).await;
                let _ = weak.update(cx, |workspace, cx| {
                    let now = Instant::now();
                    workspace
                        .workspace_toasts
                        .retain(|toast| toast.expires_at > now);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn render_workspace_toasts(&self) -> Option<AnyElement> {
        if self.workspace_toasts.is_empty()
            && !self
                .connection_trace_toasts
                .values()
                .any(|trace| trace.displayed.is_some())
        {
            return None;
        }

        let standard_toasts = self.workspace_toasts.iter().map(|toast| ToastView {
            title: toast.notice.title.clone(),
            description: toast.notice.description.clone(),
            status_text: toast.notice.status_text.clone(),
            progress: toast.notice.progress,
            variant: toast_variant_from_terminal(toast.notice.variant),
        });
        let trace_toasts = self
            .connection_trace_toasts
            .values()
            .filter_map(|trace| trace.displayed.as_ref())
            .map(|event| ToastView {
                title: self.connection_trace_title(event),
                description: None,
                status_text: Some(self.connection_trace_status_text(event)),
                progress: Some(event.progress),
                variant: match event.status {
                    ConnectionTraceStatus::Ready => ToastVariant::Success,
                    _ => ToastVariant::Default,
                },
            });
        let toasts = standard_toasts.chain(trace_toasts);
        Some(toaster(&self.tokens, toasts).into_any_element())
    }

    fn poll_connection_trace_events(&mut self, cx: &mut Context<Self>) {
        const DISPLAY_DELAY: Duration = Duration::from_millis(1200);
        const UPDATE_COALESCE: Duration = Duration::from_millis(300);
        const SUCCESS_DISMISS: Duration = Duration::from_millis(1800);

        let mut changed = false;
        while let Ok(event) = self.connection_trace_rx.try_recv() {
            let now = Instant::now();
            let attempt_id = event.attempt_id.clone();
            let trace = self
                .connection_trace_toasts
                .entry(attempt_id.clone())
                .or_insert_with(|| ActiveConnectionTrace {
                    visible: false,
                    latest: event.clone(),
                    displayed: None,
                    started_at: now,
                    show_generation: 0,
                    flush_generation: 0,
                    expires_at: None,
                });
            trace.latest = event.clone();
            trace.expires_at = None;

            match event.status {
                ConnectionTraceStatus::Running => {
                    if !trace.visible && trace.show_generation == 0 {
                        trace.show_generation = trace.show_generation.wrapping_add(1);
                        let generation = trace.show_generation;
                        let attempt_id = attempt_id.clone();
                        cx.spawn(async move |weak, cx| {
                            Timer::after(DISPLAY_DELAY).await;
                            let _ = weak.update(cx, |workspace, cx| {
                                workspace.show_connection_trace(&attempt_id, generation);
                                cx.notify();
                            });
                        })
                        .detach();
                    } else {
                        trace.flush_generation = trace.flush_generation.wrapping_add(1);
                        let generation = trace.flush_generation;
                        let attempt_id = attempt_id.clone();
                        cx.spawn(async move |weak, cx| {
                            Timer::after(UPDATE_COALESCE).await;
                            let _ = weak.update(cx, |workspace, cx| {
                                workspace.flush_connection_trace(&attempt_id, generation);
                                cx.notify();
                            });
                        })
                        .detach();
                    }
                }
                ConnectionTraceStatus::Ready => {
                    let elapsed_ms = trace
                        .started_at
                        .elapsed()
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64;
                    if trace.visible {
                        let mut success = event;
                        success.elapsed_ms = elapsed_ms;
                        trace.latest = success.clone();
                        trace.displayed = Some(success);
                        trace.expires_at = Some(now + SUCCESS_DISMISS);
                        let attempt_id = attempt_id.clone();
                        cx.spawn(async move |weak, cx| {
                            Timer::after(SUCCESS_DISMISS).await;
                            let _ = weak.update(cx, |workspace, cx| {
                                workspace.connection_trace_toasts.remove(&attempt_id);
                                cx.notify();
                            });
                        })
                        .detach();
                    } else {
                        self.connection_trace_toasts.remove(&attempt_id);
                    }
                    changed = true;
                }
                ConnectionTraceStatus::Failed | ConnectionTraceStatus::Cancelled => {
                    self.connection_trace_toasts.remove(&attempt_id);
                    changed = true;
                }
            }
        }

        if changed {
            cx.notify();
        }
    }

    fn show_connection_trace(&mut self, attempt_id: &str, generation: u64) {
        let Some(trace) = self.connection_trace_toasts.get_mut(attempt_id) else {
            return;
        };
        if trace.visible
            || trace.show_generation != generation
            || trace.latest.status != ConnectionTraceStatus::Running
        {
            return;
        }
        trace.visible = true;
        trace.displayed = Some(trace.latest.clone());
    }

    fn flush_connection_trace(&mut self, attempt_id: &str, generation: u64) {
        let Some(trace) = self.connection_trace_toasts.get_mut(attempt_id) else {
            return;
        };
        if !trace.visible
            || trace.flush_generation != generation
            || trace.latest.status != ConnectionTraceStatus::Running
        {
            return;
        }
        trace.displayed = Some(trace.latest.clone());
    }

    fn connection_trace_title(&self, event: &ConnectionTraceEvent) -> String {
        let label = event
            .label
            .clone()
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| {
                if event.node_id.0.is_empty() {
                    self.i18n.t("connections.trace.target_unknown")
                } else {
                    event.node_id.0.clone()
                }
            });
        let chain_title = event
            .step_index
            .zip(event.total_steps)
            .filter(|(_, total)| *total > 1);
        match (event.mode, chain_title) {
            (ConnectionTraceMode::Reconnect, Some((current, total))) => self
                .i18n
                .t("connections.trace.reconnecting_chain")
                .replace("{{current}}", &current.to_string())
                .replace("{{total}}", &total.to_string())
                .replace("{{label}}", &label),
            (ConnectionTraceMode::Connect, Some((current, total))) => self
                .i18n
                .t("connections.trace.connecting_chain")
                .replace("{{current}}", &current.to_string())
                .replace("{{total}}", &total.to_string())
                .replace("{{label}}", &label),
            (ConnectionTraceMode::Reconnect, None) => self
                .i18n
                .t("connections.trace.reconnecting")
                .replace("{{label}}", &label),
            (ConnectionTraceMode::Connect, None) => self
                .i18n
                .t("connections.trace.connecting")
                .replace("{{label}}", &label),
        }
    }

    fn connection_trace_status_text(&self, event: &ConnectionTraceEvent) -> String {
        if event.status == ConnectionTraceStatus::Ready {
            return self
                .i18n
                .t("connections.trace.connected")
                .replace("{{elapsed}}", &format_connection_trace_elapsed(event.elapsed_ms));
        }
        event
            .detail
            .clone()
            .unwrap_or_else(|| self.i18n.t(connection_trace_stage_key(event.stage)))
    }
}

fn toast_variant_from_terminal(variant: TerminalNoticeVariant) -> ToastVariant {
    match variant {
        TerminalNoticeVariant::Default => ToastVariant::Default,
        TerminalNoticeVariant::Success => ToastVariant::Success,
        TerminalNoticeVariant::Error => ToastVariant::Error,
        TerminalNoticeVariant::Warning => ToastVariant::Warning,
    }
}

fn connection_trace_stage_key(stage: ConnectionTraceStage) -> &'static str {
    match stage {
        ConnectionTraceStage::Queued => "connections.trace.stage.queued",
        ConnectionTraceStage::Preparing => "connections.trace.stage.preparing",
        ConnectionTraceStage::OpeningTransport => "connections.trace.stage.opening_transport",
        ConnectionTraceStage::SshHandshake => "connections.trace.stage.ssh_handshake",
        ConnectionTraceStage::HostKey => "connections.trace.stage.host_key",
        ConnectionTraceStage::Authentication => "connections.trace.stage.authentication",
        ConnectionTraceStage::Pty => "connections.trace.stage.pty",
        ConnectionTraceStage::ShellReady => "connections.trace.stage.shell_ready",
        ConnectionTraceStage::Ready => "connections.trace.stage.ready",
    }
}

fn format_connection_trace_elapsed(ms: u64) -> String {
    if ms < 10_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}s", (ms + 500) / 1000)
    }
}
