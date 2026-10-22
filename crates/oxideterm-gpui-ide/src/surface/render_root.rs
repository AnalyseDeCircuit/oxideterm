impl Render for IdeSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.fs.terminal_session_id().is_some()
            && !self.fs.status_for_node(self.node_id.as_deref()).is_ready()
            && matches!(self.load_state, IdeLoadState::Ready)
        {
            self.load_state = IdeLoadState::Disconnected;
        }
        if self.has_modal() {
            // Floating menus and pointer drags belong to the covered surface, not the dialog.
            self.tab_context_menu = None;
            self.tree_context_menu = None;
            self.file_format_menu = None;
            self.agent_status_menu = None;
            self.tree_resize = None;
            self.tree_resize_activation = None;
            self.tab_drag = None;
        }
        let theme = self.tokens.ui;
        let font_family = tauri_ui_font_family(self.tokens.metrics.font_family);
        let mut root = div()
            .id("oxideterm-gpui-ide")
            .relative()
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .font_family(font_family)
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
                    let closed_format_menu = this.file_format_menu.take().is_some();
                    let closed_agent_menu = this.agent_status_menu.take().is_some();
                    if closed_format_menu
                        || closed_tab_menu
                        || closed_tree_menu
                        || closed_agent_menu
                    {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.finish_tab_drag(cx);
                    this.finish_tree_resize(cx);
                    this.finish_folder_picker_path_selection_drag();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.finish_tree_resize(cx)),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_tree_resize(event, cx);
                this.update_folder_picker_path_selection_drag(event, window, cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.has_modal() {
                    this.handle_modal_key(event, window, cx);
                    return;
                }
                if event.keystroke.key == "escape" && this.file_format_menu.take().is_some() {
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.search.open {
                    // Project search owns the IDE text-input channel while its
                    // overlay is open; editor find must not consume the same key.
                    this.handle_project_search_key(event, window, cx);
                    return;
                }
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
        if self.is_shell_ide() {
            root = root
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(px(IDE_WORKSPACE_HEADER_HEIGHT))
                        .flex_none()
                        .px_2()
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .child(self.root_path.clone().unwrap_or_default()),
                        )
                        .child(
                            button_with(
                                &self.tokens,
                                self.labels.return_to_terminal.clone(),
                                ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.request_shell_return(cx);
                                    cx.stop_propagation();
                                }),
                            ),
                        ),
                )
                .child(div().flex_1().min_h_0().relative().child(body));
        } else {
            root = root.child(body);
        }

        if matches!(self.load_state, IdeLoadState::Disconnected) {
            let overlay = self.render_disconnected_overlay();
            root = if self.is_shell_ide() {
                root.child(
                    div()
                        .absolute()
                        .top(px(IDE_WORKSPACE_HEADER_HEIGHT))
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .child(overlay),
                )
            } else {
                root.child(overlay)
            };
        }
        if self.search.open {
            root = root.child(self.render_project_search_panel(cx));
        }
        if let Some(menu) = self.tab_context_menu {
            root = root.child(self.render_tab_context_menu(menu, _window, cx));
        }
        if let Some(menu) = self.tree_context_menu.clone() {
            root = root.child(self.render_tree_context_menu(menu, _window, cx));
        }
        if let Some(menu) = self.file_format_menu {
            root = root.child(self.render_file_format_menu(menu, _window, cx));
        }
        if let Some(menu) = self.agent_status_menu {
            root = root.child(self.render_agent_status_menu(menu, _window, cx));
        }
        if self.tree_resize.is_some() {
            // Match sidebar capture: editor and scrollable children must not
            // consume movement while the pointer is away from the divider.
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .cursor_col_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        this.update_tree_resize(event, cx);
                        cx.stop_propagation();
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.finish_tree_resize(cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        root
    }
}

impl Focusable for IdeSurface {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        if self.has_modal() {
            self.modal_focus_handle.clone()
        } else {
            self.focus_handle.clone()
        }
    }
}

impl EventEmitter<IdeSurfaceEvent> for IdeSurface {}
