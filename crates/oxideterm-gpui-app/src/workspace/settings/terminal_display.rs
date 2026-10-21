use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn terminal_preview(
        &self,
        settings: &PersistedSettings,
        cx: &App,
    ) -> AnyElement {
        let family = settings
            .terminal
            .font_family
            .terminal_family_name(&settings.terminal.custom_font_family);
        let cjk_family = terminal_cjk_font_family_preference(&settings.terminal.cjk_font_family);
        let resolved = oxideterm_gpui_terminal::terminal_ui::resolve_terminal_font(
            cx.text_system(),
            &family,
            cjk_family.as_deref(),
            settings.terminal.font_ligatures,
            settings.terminal.font_weight as f32,
        );
        let unavailable_message = (!resolved.unavailable_families.is_empty()).then(|| {
            self.i18n
                .t("settings_view.terminal.font_unavailable")
                .replace("{{fonts}}", &resolved.unavailable_families.join(", "))
                .replace("{{font}}", &resolved.font.family)
        });
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
            .when_some(unavailable_message, |preview, message| {
                preview.child(
                    div()
                        .mb(px(self.tokens.spacing.two))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.warning))
                        .child(message),
                )
            })
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
                    .font(resolved.font)
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
