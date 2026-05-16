impl WorkspaceApp {
    fn render_ai_sidebar_disabled(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .px(px(16.0))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .text_center()
            .child(
                div()
                    .size(px(48.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba((self.tokens.ui.accent << 8) | 0x0d))
                    .child(Self::render_lucide_icon(
                        LucideIcon::MessageSquare,
                        24.0,
                        rgba((self.tokens.ui.text_muted << 8) | 0x66),
                    )),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("ai.chat.title")),
            )
            .child(
                div()
                    .max_w(px(220.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .line_height(px(18.0))
                    .child(self.i18n.t("ai.chat.disabled_message")),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .rounded(px(self.tokens.radii.md))
                    .px(px(10.0))
                    .py(px(6.0))
                    .bg(rgb(self.tokens.ui.accent))
                    .text_color(rgb(self.tokens.ui.bg))
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .cursor_pointer()
                    .child(self.i18n.t("ai.chat.open_settings"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.open_ai_settings(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }



}
