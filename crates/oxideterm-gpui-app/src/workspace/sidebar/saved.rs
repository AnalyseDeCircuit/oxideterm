impl WorkspaceApp {
    fn render_saved_connections_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let query = self
            .session_manager
            .saved_search_query
            .trim()
            .to_lowercase();
        let mut connections = self.connection_store.connection_infos();
        if !query.is_empty() {
            connections.retain(|conn| {
                conn.name.to_lowercase().contains(&query)
                    || conn.host.to_lowercase().contains(&query)
                    || conn.username.to_lowercase().contains(&query)
            });
        }

        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
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
                    .overflow_y_scroll()
                    .px_1()
                    .children(
                        connections
                            .into_iter()
                            .map(|conn| self.render_saved_connection_sidebar_row(conn, cx)),
                    ),
            )
            .when(self.connection_store.connections().is_empty(), |content| {
                content.child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(8.0))
                        .text_color(rgb(theme.text_muted))
                        .child(Self::render_lucide_icon(
                            LucideIcon::Server,
                            self.tokens.metrics.empty_sidebar_icon_size,
                            rgba((theme.text_muted << 8) | 0x4d),
                        ))
                        .child(
                            div()
                                .text_center()
                                .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                                .child(self.i18n.t("sessionManager.table.no_connections")),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_saved_connection_sidebar_row(
        &self,
        conn: oxideterm_connections::ConnectionInfo,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let id = conn.id.clone();
        let detail = format!("{}@{}:{}", conn.username, conn.host, conn.port);
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
                            .child(conn.name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(detail),
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
