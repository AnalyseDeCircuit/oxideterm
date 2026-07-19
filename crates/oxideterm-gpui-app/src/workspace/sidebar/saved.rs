use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_saved_connections_sidebar_content(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let query = self
            .session_manager
            .saved_search_query
            .trim()
            .to_lowercase();
        let mut connections = self.connection_store.connection_infos();
        if !query.is_empty() {
            connections.retain(|conn| conn.matches_search_query(&query));
        }
        let empty_label = if query.is_empty() {
            self.i18n.t("sidebar.panels.no_saved_connections")
        } else {
            self.i18n.t("sessionManager.table.no_search_results")
        };
        let row_count = connections.len();
        let virtual_connections = Arc::new(connections.clone());
        let workspace = cx.entity();
        let saved_scroll = self.session_manager.saved_sidebar_scroll_handle.clone();
        let virtual_spec = TauriVirtualListSpec::new(
            px(SAVED_CONNECTION_VIRTUAL_ROW_HEIGHT),
            SAVED_CONNECTION_VIRTUAL_OVERSCAN,
        );

        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(div().px_2().child(self.render_session_text_input(
                SessionManagerInput::SavedSearch,
                &self.session_manager.saved_search_query,
                self.i18n.t("sessionManager.toolbar.search_placeholder"),
                cx,
            )))
            .child(
                div()
                    .id("saved-connections-sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .px_1()
                    // Tauri renders an empty quick-connect list immediately
                    // below search instead of centering a page-level state.
                    .when(connections.is_empty(), |scroll| {
                        scroll.child(
                            div()
                                .px_2()
                                .py_4()
                                .text_center()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(empty_label),
                        )
                    })
                    .when(!connections.is_empty(), |scroll| {
                        scroll.child(tauri_virtual_uniform_list(
                            "saved-connections-sidebar-virtual",
                            row_count,
                            saved_scroll,
                            virtual_spec,
                            move |range, _window, app| {
                                let mut rendered = Vec::new();
                                let connections = virtual_connections.clone();
                                let _ = workspace.update(app, |this, cx| {
                                    for index in range {
                                        let Some(conn) = connections.get(index).cloned() else {
                                            continue;
                                        };
                                        rendered.push(
                                            this.render_saved_connection_sidebar_row(conn, cx),
                                        );
                                    }
                                });
                                rendered
                            },
                        ))
                    }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_saved_connection_sidebar_row(
        &self,
        conn: oxideterm_connections::ConnectionInfo,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let id = conn.id.clone();
        let detail = format!("{}@{}:{}", conn.username, conn.host, conn.port);
        let selection_group_id =
            crate::workspace::selectable_text::selectable_text_id("saved-sidebar-row", &conn.id);
        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(6.0))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(self.tokens.ui.bg_hover)))
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                12.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.render_row_safe_selectable_display_text_in_group(
                                selection_group_id,
                                "saved-sidebar-cell",
                                ("name", conn.id.as_str()),
                                0,
                                conn.name,
                                theme.text,
                                None,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_row_safe_selectable_display_text_in_group(
                                selection_group_id,
                                "saved-sidebar-cell",
                                ("detail", conn.id.as_str()),
                                1,
                                detail,
                                theme.text_muted,
                                None,
                                cx,
                            )),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.open_saved_connection(&id, window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}
