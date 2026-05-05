impl WorkspaceApp {
    fn terminal_preview(&self, settings: &PersistedSettings) -> AnyElement {
        let family = settings
            .terminal
            .font_family
            .terminal_family_name(&settings.terminal.custom_font_family);
        let preview_line_height =
            px(settings.terminal.font_size as f32 * settings.terminal.line_height as f32);
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .p(px(self.tokens.metrics.settings_font_preview_padding))
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(self
                        .tokens
                        .metrics
                        .settings_font_preview_label_margin_bottom))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.terminal.font_preview")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .font_family(&family)
                    .text_size(px(settings.terminal.font_size as f32))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        div()
                            .line_height(preview_line_height)
                            .child("ABCDEFG abcdefg 0123456789"),
                    )
                    .child(
                        div()
                            .line_height(preview_line_height)
                            .child("Thực thi lệnh chậm - lưu, tổ chức, chạy"),
                    )
                    .child(
                        div()
                            .line_height(preview_line_height)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child("-> => == != <= >= {}"),
                    )
                    .child(
                        div()
                            .line_height(preview_line_height)
                            .text_color(rgb(self.tokens.ui.success))
                            .child("天地玄黄 The quick brown fox"),
                    )
                    .child(
                        div()
                            .line_height(preview_line_height)
                            .text_color(rgb(self.tokens.ui.warning))
                            .child("       󰊤  "),
                    ),
            )
            .into_any_element()
    }
}
