use super::*;

/// Platform-resolved font metrics supplied by the terminal renderer.
pub struct TerminalPreviewFont {
    pub font: gpui::Font,
    pub unavailable_families: Vec<String>,
    pub font_size: gpui::Pixels,
    pub line_height: gpui::Pixels,
    pub letter_spacing: Option<gpui::Pixels>,
}

impl SettingsComponents<'_> {
    pub fn terminal_preview(&self, resolved: TerminalPreviewFont) -> AnyElement {
        let unavailable_message = (!resolved.unavailable_families.is_empty()).then(|| {
            self.i18n
                .t("settings_view.terminal.font_unavailable")
                .replace("{{fonts}}", &resolved.unavailable_families.join(", "))
                .replace("{{font}}", &resolved.font.family)
        });
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
                    .text_size(resolved.font_size)
                    .when_some(resolved.letter_spacing, |style, spacing| {
                        style.letter_spacing(spacing)
                    })
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        div()
                            .line_height(resolved.line_height)
                            .child("ABCDEFG abcdefg 0123456789"),
                    )
                    .child(
                        div()
                            .line_height(resolved.line_height)
                            .child("Thực thi lệnh chậm - lưu, tổ chức, chạy"),
                    )
                    .child(
                        div()
                            .line_height(resolved.line_height)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child("-> => == != <= >= {}"),
                    )
                    .child(
                        div()
                            .line_height(resolved.line_height)
                            .text_color(rgb(self.tokens.ui.success))
                            .child("天地玄黄 The quick brown fox"),
                    )
                    .child(
                        div()
                            .line_height(resolved.line_height)
                            .text_color(rgb(self.tokens.ui.warning))
                            .child("       󰊤  "),
                    ),
            )
            .into_any_element()
    }
}
