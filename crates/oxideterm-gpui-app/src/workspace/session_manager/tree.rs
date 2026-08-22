use super::*;

impl WorkspaceApp {
    pub(super) fn render_session_manager_toolbar(
        &self,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (selected_count, search_query) = {
            let manager = self.session_manager.read(cx);
            (manager.selected_items.len(), manager.search_query.clone())
        };
        let viewport_width = f32::from(window.viewport_size().width);
        let show_primary_labels = viewport_width >= MANAGER_RESPONSIVE_SM;
        let show_secondary_labels = viewport_width >= MANAGER_RESPONSIVE_MD;
        let workspace = cx.entity();
        div()
            .min_h(px(48.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(theme_bg(theme.bg, has_background))
            .child(
                div()
                    .flex_1()
                    .min_w(px(160.0))
                    .max_w(px(MANAGER_TOOLBAR_SEARCH_WIDTH))
                    .child(self.render_session_text_input(
                        SessionManagerInput::Search,
                        &search_query,
                        self.i18n.t("sessionManager.toolbar.search_placeholder"),
                        cx,
                    )),
            )
            .child(
                self.render_toolbar_button(
                    LucideIcon::Plus,
                    self.i18n.t("sessionManager.toolbar.new_connection"),
                    ButtonVariant::Default,
                    has_background,
                    show_primary_labels,
                    cx.listener(|this, _event, window, cx| {
                        this.open_new_connection_form(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .flex_none(),
            )
            .child(
                div()
                    .flex_none()
                    .child(self.render_session_manager_sort_trigger(
                        has_background,
                        show_secondary_labels,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .child(self.render_session_manager_view_mode_trigger(
                        has_background,
                        show_secondary_labels,
                        cx,
                    )),
            )
            .when(selected_count > 0, |toolbar| {
                toolbar.child(
                    // Match Tauri's selected-action island: it must not live
                    // inside the spacer, or GPUI can shrink the spacer to zero
                    // and let selected buttons paint over later toolbar items.
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_none()
                                .px_1()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(selected_count_label(&self.i18n, selected_count)),
                        )
                        .child(select_anchor_probe(
                            SelectAnchorId::SessionManagerBatchMove,
                            self.render_session_manager_button(
                                LucideIcon::FolderInput,
                                self.i18n.t("sessionManager.batch.move_to_group"),
                                ButtonVariant::Outline,
                                cx.listener(|this, _event, _window, cx| {
                                    this.session_manager.update(cx, |manager, cx| {
                                        manager.show_batch_move = !manager.show_batch_move;
                                        cx.notify();
                                    });
                                    cx.stop_propagation();
                                }),
                            )
                            .flex_none(),
                            move |anchor, _window, cx| {
                                let _ = workspace.update(cx, |this, cx| {
                                    this.update_select_anchor(anchor, cx);
                                });
                            },
                        ))
                        .child(
                            self.render_session_manager_button(
                                LucideIcon::Trash2,
                                self.i18n.t("sessionManager.batch.delete"),
                                ButtonVariant::Outline,
                                cx.listener(|this, _event, _window, cx| {
                                    this.request_delete_selected_connections(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .flex_none(),
                        ),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_session_manager_view_mode_trigger(
        &self,
        has_background: bool,
        show_label: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (current_mode, menu_open) = {
            let manager = self.session_manager.read(cx);
            (manager.view_mode, manager.view_mode_menu_open)
        };
        let workspace = cx.entity();
        let trigger = self.render_toolbar_button(
            current_mode.icon(),
            self.i18n.t(current_mode.label_key()),
            if menu_open {
                ButtonVariant::Default
            } else {
                ButtonVariant::Outline
            },
            has_background,
            show_label,
            cx.listener(move |this, _event, _window, cx| {
                this.toggle_session_view_mode_menu(cx);
                cx.notify();
                cx.stop_propagation();
            }),
        );

        // Keep the trigger rect warm while closed, matching Radix's immediate
        // trigger measurement and avoiding pointer-coordinate drift.
        select_anchor_probe(
            SelectAnchorId::SessionManagerViewMode,
            trigger,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn render_session_manager_sort_trigger(
        &self,
        has_background: bool,
        show_label: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (direction, sort_field, menu_open) = {
            let manager = self.session_manager.read(cx);
            (
                manager.sort_direction,
                manager.sort_field,
                manager.sort_menu_open,
            )
        };
        let label = self.i18n.t(sort_field.label_key());
        let workspace = cx.entity();
        let trigger = self.render_toolbar_button(
            direction.icon(),
            label,
            if menu_open {
                ButtonVariant::Default
            } else {
                ButtonVariant::Outline
            },
            has_background,
            show_label,
            cx.listener(move |this, _event, _window, cx| {
                this.toggle_session_sort_menu(cx);
                cx.notify();
                cx.stop_propagation();
            }),
        );

        select_anchor_probe(
            SelectAnchorId::SessionManagerSort,
            trigger,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn render_session_manager_view_mode_menu(
        &self,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(anchor) = self
            .select_anchors
            .get(&SelectAnchorId::SessionManagerViewMode)
            .copied()
        else {
            return div().into_any_element();
        };
        let viewport = window.viewport_size();
        let placement = browser_behavior::clamp_context_menu_position(
            f32::from(anchor.bounds.right()) - MANAGER_VIEW_MODE_MENU_WIDTH,
            f32::from(anchor.bounds.bottom()) + 4.0,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MANAGER_VIEW_MODE_MENU_WIDTH,
            MANAGER_VIEW_MODE_MENU_HEIGHT,
            8.0,
        );
        let modes = [
            SessionManagerViewMode::Tree,
            SessionManagerViewMode::Grid,
            SessionManagerViewMode::List,
        ];
        let mut menu = context_menu_event_boundary(
            dropdown_menu_content(&self.tokens).w(px(MANAGER_VIEW_MODE_MENU_WIDTH)),
        );
        let current_mode = self.session_manager.read(cx).view_mode;
        for mode in modes {
            let active = current_mode == mode;
            let item = dropdown_menu_item(
                &self.tokens,
                self.i18n.t(mode.label_key()),
                DropdownMenuItemKind::Radio(active),
                false,
                false,
            );
            menu = menu.child(self.render_session_manager_menu_action(
                item,
                false,
                false,
                has_background,
                move |this, _event, _window, cx| {
                    this.session_manager.update(cx, |manager, cx| {
                        manager.view_mode = mode;
                        cx.notify();
                    });
                    this.close_session_row_menus(cx);
                    cx.stop_propagation();
                },
                cx,
            ));
        }
        // The trigger probe reports window coordinates. Mount this like the
        // settings/AI popovers instead of as a surface-local absolute child.
        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(placement.x), px(placement.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(overlay_content_boundary(menu)),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }

    pub(super) fn render_session_manager_sort_menu(
        &self,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(anchor) = self
            .select_anchors
            .get(&SelectAnchorId::SessionManagerSort)
            .copied()
        else {
            return div().into_any_element();
        };
        let viewport = window.viewport_size();
        let placement = browser_behavior::clamp_context_menu_position(
            f32::from(anchor.bounds.right()) - MANAGER_SORT_MENU_WIDTH,
            f32::from(anchor.bounds.bottom()) + 4.0,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MANAGER_SORT_MENU_WIDTH,
            MANAGER_SORT_MENU_HEIGHT,
            8.0,
        );
        let fields = [
            SessionSortField::Name,
            SessionSortField::Host,
            SessionSortField::Port,
            SessionSortField::Username,
            SessionSortField::AuthType,
            SessionSortField::Group,
            SessionSortField::LastUsed,
        ];
        let mut menu = context_menu_event_boundary(
            dropdown_menu_content(&self.tokens).w(px(MANAGER_SORT_MENU_WIDTH)),
        );
        let (current_field, current_direction) = {
            let manager = self.session_manager.read(cx);
            (manager.sort_field, manager.sort_direction)
        };
        for field in fields {
            let active = current_field == field;
            let mut label = self.i18n.t(field.label_key());
            if active {
                label.push_str(match current_direction {
                    SortDirection::Asc => " ↑",
                    SortDirection::Desc => " ↓",
                });
            }
            let item = dropdown_menu_item(
                &self.tokens,
                label,
                DropdownMenuItemKind::Radio(active),
                false,
                false,
            );
            menu = menu.child(self.render_session_manager_menu_action(
                item,
                false,
                false,
                has_background,
                move |this, _event, _window, cx| {
                    this.set_session_sort_field(field, cx);
                    this.close_session_row_menus(cx);
                    cx.notify();
                    cx.stop_propagation();
                },
                cx,
            ));
        }
        // The sort trigger is inside the tab content, while the popup floats
        // at window level just like Radix DropdownMenuContent in Tauri.
        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(placement.x), px(placement.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(overlay_content_boundary(menu)),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }
}
