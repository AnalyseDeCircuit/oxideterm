impl Focusable for WorkspaceApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WorkspaceApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_tab_titles(cx);
        self.poll_forwarding_worker_results(cx);
        let title = self
            .active_tab()
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_else(|| "OxideTerm".to_string());
        window.set_window_title(&SharedString::from(title));
        let vibrancy_mode =
            effective_vibrancy_mode(self.settings_store.settings(), &self.render_policy);
        if self.applied_vibrancy_mode != vibrancy_mode {
            let _ = apply_window_vibrancy(window, vibrancy_mode);
            self.applied_vibrancy_mode = vibrancy_mode;
        }
        if self.needs_active_pane_focus
            && self
                .active_tab()
                .is_some_and(|tab| !matches!(tab.kind, TabKind::Settings | TabKind::SessionManager))
            && !self.search.visible
            && self.new_connection_form.is_none()
            && let Some(pane) = self.active_pane()
        {
            self.needs_active_pane_focus = false;
            window.on_next_frame(move |window, cx| {
                pane.read(cx).focus(window);
            });
        }

        let content = if let Some(tab) = self.active_tab() {
            match (&tab.kind, &tab.root_pane) {
                (TabKind::Settings, _) => self.render_settings_surface(cx),
                (TabKind::Sftp, _) => self.render_sftp_surface(window, cx),
                (TabKind::Forwards, _) => self.render_forwards_surface(window, cx),
                (TabKind::SessionManager, _) => self.render_session_manager_surface(window, cx),
                (_, Some(root_pane)) => self.render_pane_tree(root_pane, cx),
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

        div()
            .id("workspace-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(workspace_background(&self.tokens, vibrancy_mode))
            .text_color(rgb(self.tokens.ui.text))
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.keyboard_interactive_challenge.is_some() {
                    if keystroke_commits_platform_text(&event.keystroke) {
                        return;
                    }
                    let _ = this.handle_keyboard_interactive_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.host_key_challenge.is_some() {
                    if event.keystroke.key.as_str() == "escape" {
                        this.cancel_host_key_challenge(cx);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.new_connection_form.is_some() {
                    if this.active_ime_target().is_some()
                        && keystroke_commits_platform_text(&event.keystroke)
                    {
                        return;
                    }
                    let _ = this.handle_new_connection_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::SessionManager)
                    && this.session_manager.focused_input.is_some()
                {
                    if keystroke_commits_platform_text(&event.keystroke) {
                        return;
                    }
                    let _ = this.handle_session_manager_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::Forwards)
                    && this.forwarding_view.focused_input.is_some()
                {
                    if keystroke_commits_platform_text(&event.keystroke) {
                        return;
                    }
                    let _ = this.handle_forwards_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::Sftp)
                {
                    let sftp_quick_look_space = event.keystroke.key.as_str() == "space"
                        && this.sftp_view.focused_input.is_none()
                        && this.sftp_view.dialog.is_none();
                    let sftp_markdown_preview_toggle = event.keystroke.key.as_str() == "u"
                        && this.sftp_view.focused_input.is_none()
                        && matches!(
                            this.sftp_view.dialog.as_ref(),
                            Some(sftp::SftpDialog::Preview { .. })
                        );
                    if keystroke_commits_platform_text(&event.keystroke)
                        && !sftp_quick_look_space
                        && !sftp_markdown_preview_toggle
                    {
                        return;
                    }
                    let _ = this.handle_sftp_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.active_surface == ActiveSurface::Settings
                    && this.focused_settings_input.is_some()
                {
                    if keystroke_commits_platform_text(&event.keystroke) {
                        return;
                    }
                    let _ = this.handle_settings_input_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_workspace_key(event, window, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_sidebar_resize(event, cx);
                this.update_split_drag(event, window, cx);
                this.update_settings_slider_drag(event, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                    this.blur_text_inputs(cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.finish_sidebar_resize(cx);
                    this.finish_split_drag(cx);
                    this.finish_settings_slider_drag(cx);
                }),
            )
            .on_action(cx.listener(|this, _: &NewTerminal, window, cx| {
                let _ = this.create_local_terminal_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                this.close_active_tab(window, cx);
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
            .on_action(cx.listener(|this, _: &Copy, _window, cx| {
                if this.new_connection_form.is_none() {
                    this.copy(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Paste, _window, cx| {
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
                    .child(self.render_activity_bar(cx))
                    .when(!self.sidebar_collapsed, |layout| {
                        layout.child(self.render_sidebar_region(cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .min_w(px(self.tokens.metrics.min_main_width))
                            .overflow_hidden()
                            .child(self.render_tab_bar(cx))
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
                    ),
            )
            .when(self.new_connection_form.is_some(), |root| {
                root.child(self.render_new_connection_modal(window, cx))
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
            .child(WorkspaceImeElement::new(
                cx.entity(),
                self.focus_handle.clone(),
            ))
    }
}
