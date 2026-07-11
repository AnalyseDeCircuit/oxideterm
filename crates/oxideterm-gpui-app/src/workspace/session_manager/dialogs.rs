use super::*;

impl WorkspaceApp {
    pub(super) fn session_manager_basic_footer_action(
        &self,
        label: String,
        variant: ButtonVariant,
        action: SessionManagerBasicDialogFooterAction,
        disabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        self.session_manager_dialog_footer_action(
            label,
            variant,
            action,
            disabled,
            ButtonSize::Sm,
            None,
            listener,
            cx,
        )
    }

    pub(super) fn session_manager_dialog_footer_action(
        &self,
        label: String,
        variant: ButtonVariant,
        action: SessionManagerBasicDialogFooterAction,
        disabled: bool,
        size: ButtonSize,
        icon: Option<AnyElement>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        // Mouse activation uses the same disabled/focus-visible ownership as
        // the keyboard FocusCycle path. Keep it centralized so import, group,
        // and auto-route dialogs do not each compose DialogFooter buttons.
        self.workspace_toolbar_action_button(
            label,
            icon,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                focus_visible: self.session_manager.focused_basic_dialog_footer_action
                    == Some(action),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(listener),
        )
    }

    pub(super) fn render_new_group_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let can_create_group = !self.session_manager.new_group_name.trim().is_empty();
        modal_backdrop(rgba(
            (0x000000 << 8) | SESSION_MANAGER_LIGHT_DIALOG_BACKDROP_ALPHA,
        ))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.session_manager.show_new_group = false;
                this.session_manager.focused_input = None;
                this.session_manager.focused_basic_dialog_footer_action = None;
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .child(overlay_content_boundary(
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
                        .child(self.session_manager_basic_footer_action(
                            self.i18n.t("sessionManager.edit_properties.cancel"),
                            ButtonVariant::Secondary,
                            SessionManagerBasicDialogFooterAction::Cancel,
                            false,
                            |this, _event, _window, cx| {
                                this.session_manager.show_new_group = false;
                                this.session_manager.focused_input = None;
                                this.session_manager.focused_basic_dialog_footer_action = None;
                                cx.notify();
                            },
                            cx,
                        ))
                        .child(self.session_manager_basic_footer_action(
                            self.i18n.t("sessionManager.edit_properties.save"),
                            ButtonVariant::Default,
                            SessionManagerBasicDialogFooterAction::Primary,
                            !can_create_group,
                            |this, _event, _window, cx| {
                                this.session_manager.focused_basic_dialog_footer_action = None;
                                this.create_session_group(cx);
                            },
                            cx,
                        )),
                ),
        ))
        .into_any_element()
    }

    pub(super) fn render_ssh_config_import_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.session_manager.show_import = false;
                    this.session_manager.selected_import_aliases.clear();
                    this.session_manager.focused_basic_dialog_footer_action = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(overlay_content_boundary(
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
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "ssh-config-import",
                                "title",
                                self.i18n.t("settings_view.connections.ssh_config.title"),
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .id("session-manager-import-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .selectable_overflow_y_scroll(
                                &self
                                    .selectable_text_scroll_handle("session-manager-import-scroll"),
                            )
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
                            .child(self.session_manager_basic_footer_action(
                                self.i18n.t("sessionManager.edit_properties.cancel"),
                                ButtonVariant::Secondary,
                                SessionManagerBasicDialogFooterAction::Cancel,
                                false,
                                |this, _event, _window, cx| {
                                    this.session_manager.show_import = false;
                                    this.session_manager.selected_import_aliases.clear();
                                    this.session_manager.focused_basic_dialog_footer_action = None;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(self.session_manager_basic_footer_action(
                                self.i18n.t("sessionManager.toolbar.import"),
                                ButtonVariant::Default,
                                SessionManagerBasicDialogFooterAction::Primary,
                                false,
                                |this, _event, _window, cx| {
                                    this.session_manager.focused_basic_dialog_footer_action = None;
                                    this.import_selected_ssh_hosts(cx);
                                },
                                cx,
                            )),
                    ),
            ))
            .into_any_element()
    }

    pub(super) fn render_import_host_row(
        &self,
        host: SshConfigHost,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let checked = self
            .session_manager
            .selected_import_aliases
            .contains(&host.alias);
        let alias = host.alias.clone();
        let endpoint = format!(
            "{}@{}:{}",
            host.user.unwrap_or_else(|| current_username()),
            host.hostname.unwrap_or_else(|| "-".to_string()),
            host.port.unwrap_or(22)
        );
        div()
            .min_h(px(64.0))
            .flex()
            .items_start()
            .gap(px(10.0))
            .px_4()
            .py_3()
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
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .flex_wrap()
                            // SSH aliases are the primary identity in the import dialog, so they
                            // must wrap before lower-priority metadata squeezes them into ellipses.
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(20.0))
                                    .child(host.alias),
                            )
                            .when(host.already_imported, |row| {
                                row.child(
                                    div()
                                        .flex_shrink_0()
                                        .px_2()
                                        .py(px(2.0))
                                        .rounded(px(self.tokens.radii.md))
                                        .bg(rgba((theme.success << 8) | 0x2a))
                                        .text_color(rgb(theme.success))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .child(self.render_display_text_with_role(
                                            SelectableTextRole::PlainDocument,
                                            "ssh-config-import",
                                            "imported",
                                            self.i18n.t(
                                                "settings_view.connections.ssh_config.already_imported",
                                            ),
                                            theme.success,
                                            cx,
                                        )),
                                )
                            }),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .text_color(rgb(theme.text_muted))
                            .line_height(px(18.0))
                            .child(endpoint),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_batch_move_popover(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let groups = self.connection_store.groups().to_vec();
        let Some(anchor) = self
            .select_anchors
            .get(&SelectAnchorId::SessionManagerBatchMove)
            .copied()
        else {
            return div().into_any_element();
        };
        let viewport = window.viewport_size();
        let placement = browser_behavior::clamp_context_menu_position(
            f32::from(anchor.bounds.left()),
            f32::from(anchor.bounds.bottom()) + 4.0,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MANAGER_BATCH_MOVE_MENU_WIDTH,
            MANAGER_BATCH_MOVE_MENU_HEIGHT,
            8.0,
        );
        let popup = div()
            .id("session-manager-batch-move-scroll")
            .w(px(MANAGER_BATCH_MOVE_MENU_WIDTH))
            .max_h(px(MANAGER_BATCH_MOVE_MENU_HEIGHT))
            .selectable_overflow_y_scroll(
                &self.selectable_text_scroll_handle("session-manager-batch-move-scroll"),
            )
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
            );

        // Batch move is a Radix dropdown in Tauri; keep it anchored to the
        // actual trigger instead of the old toolbar-relative hard-coded corner.
        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(placement.x), px(placement.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(oxideterm_gpui_ui::motion::fade_in(
                    &self.tokens,
                    "session-manager-batch-move-enter",
                    overlay_content_boundary(popup),
                    oxideterm_gpui_ui::motion::MotionDuration::Micro,
                )),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }

    pub(super) fn render_batch_move_item(
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
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "batch-move-item",
                label.clone(),
                label,
                theme.text,
                cx,
            ))
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
