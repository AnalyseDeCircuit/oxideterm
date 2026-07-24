impl Render for IdeSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.tokens.ui;
        let font_family = tauri_ui_font_family(self.tokens.metrics.font_family);
        let mut root = div()
            .id("oxideterm-gpui-ide")
            .relative()
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .font_family(SharedString::from(font_family))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(theme.text))
            .bg(if self.runtime_settings.background_active {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    let closed_tab_menu = this.tab_context_menu.take().is_some();
                    let closed_tree_menu = this.tree_context_menu.take().is_some();
                    let closed_agent_menu = this.agent_status_menu.take().is_some();
                    if closed_tab_menu || closed_tree_menu || closed_agent_menu {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.finish_tab_drag(cx);
                    this.finish_folder_picker_path_selection_drag();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    this.update_folder_picker_path_selection_drag(event, window, cx);
                }),
            )
            .on_key_down(cx.listener(|this, event, window, cx| {
                if this.search.open {
                    // Project search owns the IDE text-input channel while its
                    // overlay is open; editor find must not consume the same key.
                    this.handle_project_search_key(event, window, cx);
                    return;
                }
                this.handle_editor_search_key(event, cx);
                this.handle_editor_find_shortcut(event, cx);
                this.handle_tree_name_input_key(event, cx);
                this.handle_folder_picker_key(event, window, cx);
            }));

        let body = match &self.load_state {
            IdeLoadState::Empty => self.render_empty_project(cx),
            IdeLoadState::Loading => self.render_loading_project(cx),
            IdeLoadState::Error(message) => self.render_project_error(message.clone(), cx),
            IdeLoadState::Disconnected | IdeLoadState::Ready => self.render_workspace(cx),
        };
        root = root.child(body);

        if matches!(self.load_state, IdeLoadState::Disconnected) {
            root = root.child(self.render_disconnected_overlay());
        }
        if self.workspace.pending_close().is_some() {
            root = root.child(self.render_dirty_close_dialog(cx));
        }
        if self.conflict_state.is_some() {
            root = root.child(self.render_conflict_dialog(cx));
        }
        if self.folder_switch_confirm_open {
            root = root.child(self.render_folder_switch_confirm_dialog(cx));
        }
        if self.folder_picker.open {
            root = root.child(self.render_folder_picker_dialog(cx));
        }
        if self.tree_name_input.is_some() {
            root = root.child(self.render_tree_name_input_dialog(cx));
        }
        if self.search.open {
            root = root.child(self.render_project_search_panel(cx));
        }
        if self.delete_confirm.is_some() {
            root = root.child(self.render_delete_confirm_dialog(cx));
        }
        if let Some(menu) = self.tab_context_menu {
            root = root.child(self.render_tab_context_menu(menu, _window, cx));
        }
        if let Some(menu) = self.tree_context_menu.clone() {
            root = root.child(self.render_tree_context_menu(menu, _window, cx));
        }
        if let Some(menu) = self.agent_status_menu {
            root = root.child(self.render_agent_status_menu(menu, _window, cx));
        }
        if self.agent_remove_confirm_open {
            root = root.child(self.render_agent_remove_confirm_dialog(cx));
        }
        if self.agent_opt_in_open {
            root = root.child(self.render_agent_opt_in_dialog(cx));
        }
        root
    }
}

impl Focusable for IdeSurface {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<IdeSurfaceEvent> for IdeSurface {}
