impl WorkspaceApp {
    fn render_new_group_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(dialog_backdrop_color())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(380.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .p(px(16.0))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.i18n.t("sessionManager.folder_tree.new_group")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                self.i18n
                                    .t("sessionManager.folder_tree.new_group_description"),
                            ),
                    )
                    .child(
                        self.render_session_text_input(
                            SessionManagerInput::NewGroup,
                            &self.session_manager.new_group_name,
                            self.i18n
                                .t("sessionManager.folder_tree.new_group_placeholder"),
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("sessionManager.edit_properties.cancel"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Secondary,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.session_manager.show_new_group = false;
                                        this.session_manager.focused_input = None;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("sessionManager.edit_properties.save"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.create_session_group(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_ssh_config_import_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(dialog_backdrop_color())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(620.0))
                    .max_h(px(520.0))
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .child(
                        div()
                            .h(px(48.0))
                            .flex()
                            .items_center()
                            .px_4()
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("SSH Config"),
                    )
                    .child(
                        div()
                            .id("session-manager-import-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .children(
                                self.session_manager
                                    .ssh_config_hosts
                                    .iter()
                                    .cloned()
                                    .map(|host| self.render_import_host_row(host, cx)),
                            ),
                    )
                    .child(
                        div()
                            .h(px(54.0))
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(8.0))
                            .px_4()
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("sessionManager.edit_properties.cancel"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Secondary,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.session_manager.show_import = false;
                                        this.session_manager.selected_import_aliases.clear();
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("sessionManager.toolbar.import"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.import_selected_ssh_hosts(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_import_host_row(&self, host: SshConfigHost, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let checked = self
            .session_manager
            .selected_import_aliases
            .contains(&host.alias);
        let alias = host.alias.clone();
        div()
            .h(px(44.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px_4()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .child(
                checkbox(&self.tokens, String::new(), checked).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if this
                            .session_manager
                            .selected_import_aliases
                            .contains(&alias)
                        {
                            this.session_manager.selected_import_aliases.remove(&alias);
                        } else {
                            this.session_manager
                                .selected_import_aliases
                                .insert(alias.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                div()
                    .w(px(150.0))
                    .truncate()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(host.alias),
            )
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_color(rgb(theme.text_muted))
                    .child(format!(
                        "{}@{}:{}",
                        host.user.unwrap_or_else(|| current_username()),
                        host.hostname.unwrap_or_else(|| "-".to_string()),
                        host.port.unwrap_or(22)
                    )),
            )
            .when(host.already_imported, |row| {
                row.child(
                    div()
                        .px_2()
                        .py(px(2.0))
                        .rounded(px(self.tokens.radii.md))
                        .bg(rgba((theme.success << 8) | 0x2a))
                        .text_color(rgb(theme.success))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .child("Imported"),
                )
            })
            .into_any_element()
    }

    fn render_batch_move_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let groups = self.connection_store.groups().to_vec();
        div()
            .id("session-manager-batch-move-scroll")
            .absolute()
            .top(px(44.0))
            .right(px(104.0))
            .w(px(220.0))
            .max_h(px(260.0))
            .overflow_y_scroll()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .shadow_lg()
            .child(self.render_batch_move_item(
                None,
                self.i18n.t("sessionManager.folder_tree.ungrouped"),
                cx,
            ))
            .children(
                groups
                    .into_iter()
                    .map(|group| self.render_batch_move_item(Some(group.clone()), group, cx)),
            )
            .into_any_element()
    }

    fn render_batch_move_item(
        &self,
        group: Option<String>,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(34.0))
            .px_3()
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.move_selected_connections(group.as_deref(), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}
