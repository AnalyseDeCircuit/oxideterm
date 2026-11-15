use super::*;

impl WorkspaceApp {
    /// Renders the session manager as a tree-only panel inside the sidebar body.
    /// Reuses the tab's tree row renderer, context menu, and sort menu, but keeps
    /// its own virtual list state so the tab and sidebar don't fight over scroll.
    pub(in crate::workspace) fn render_session_manager_sidebar_surface(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let items = self.session_manager_display_items(cx);
        let body = self.render_session_manager_sidebar_tree(items, cx);
        let row_action_menu = self.session_manager.read(cx).row_action_menu.clone();
        let sort_menu_open = self.session_manager_sidebar_sort_open;
        // Operation output shares the tab's status field; the sidebar has no
        // bottom chrome, so mirror the tab's status strip here.
        let status = self.session_manager.read(cx).status.clone();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(div().flex_1().min_h(px(0.0)).child(body))
            .when_some(status, |surface, status| {
                surface.child(
                    div()
                        .h(px(32.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .px_2()
                        .border_t_1()
                        .border_color(theme_border(theme.border, false))
                        .bg(theme_bg(theme.bg, false))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.accent))
                        .child(status),
                )
            })
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .overflow_hidden()
                    .when(sort_menu_open, |this| {
                        this.child(self.workspace_context_menu_backdrop(
                            self.render_session_manager_sort_menu(
                                window,
                                false,
                                SelectAnchorId::SessionManagerSidebarSort,
                                true,
                                cx,
                            ),
                            cx,
                        ))
                    })
                    .when_some(row_action_menu, |this, menu| {
                        this.child(self.workspace_context_menu_backdrop(
                            self.render_session_manager_row_action_menu(menu, window, false, cx),
                            cx,
                        ))
                    }),
            )
            .into_any_element()
    }

    fn render_session_manager_sidebar_tree(
        &mut self,
        items: Vec<SessionManagerDisplayItem>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (roots, children) = self.session_group_tree();
        let expanded_groups = self.session_manager.read(cx).expanded_groups.clone();
        let rows = Arc::<[SessionManagerTreeRow]>::from(session_manager_tree_rows(
            &items,
            &roots,
            &children,
            &expanded_groups,
        ));
        let has_rows = !rows.is_empty();
        self.set_session_manager_visible_selection_targets(
            session_manager_tree_visible_targets(&rows, &items),
            cx,
        );
        self.sync_session_manager_sidebar_tree_list_state(&rows, &items, cx);
        let state = self
            .session_manager
            .read(cx)
            .sidebar_tree_list_state
            .clone();
        let workspace = cx.entity();
        let items = Arc::<[SessionManagerDisplayItem]>::from(items);
        let list = tauri_virtual_list(
            state,
            Self::session_manager_tree_list_spec(),
            move |index, _window, cx| {
                workspace.update(cx, |this, cx| {
                    this.render_session_manager_tree_row(rows.get(index), &items, false, true, cx)
                })
            },
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .child(
                div()
                    .id("session-manager-sidebar-tree-drop-target")
                    .flex_1()
                    .min_h(px(0.0))
                    .drag_over::<SessionManagerDrag>(move |target, _drag, _window, _cx| {
                        target
                            .border_1()
                            .border_dashed()
                            .border_color(rgb(theme.accent))
                            .bg(rgba((theme.accent << 8) | MANAGER_DRAG_ROOT_BG_ALPHA))
                    })
                    .can_drop(|drag, _window, _cx| drag.is::<SessionManagerDrag>())
                    .on_drop(cx.listener(|this, drag: &SessionManagerDrag, _window, cx| {
                        this.move_dragged_sessions_to_group(&drag.targets, None, cx);
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.open_session_manager_context_menu(
                                SessionManagerRowActionTarget::GroupRoot,
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            // Clicking empty sidebar tree space clears the
                            // selection; rows and group headers stop
                            // propagation so their own handlers keep the press.
                            this.clear_session_manager_selection(cx);
                        }),
                    )
                    // GPUI renamed the wheel listener from on_scroll_wheel.
                    .on_scroll(cx.listener(|this, _event, _window, cx| {
                        this.close_session_row_menus(cx);
                    }))
                    .child(if has_rows {
                        list.into_any_element()
                    } else {
                        self.render_session_manager_empty_view(false, cx)
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }
}
