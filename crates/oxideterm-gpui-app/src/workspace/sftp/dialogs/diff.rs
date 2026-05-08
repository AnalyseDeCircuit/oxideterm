impl WorkspaceApp {
    fn render_sftp_diff_body(
        &self,
        local_path: &str,
        local_content: &str,
        remote_path: &str,
        remote_content: &str,
        _has_background: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let lines = compute_sftp_diff(local_content, remote_content);
        let stats = sftp_diff_stats(&lines);
        let visual_lines = sftp_diff_visual_lines(&lines);
        let line_count = visual_lines.len();
        let diff_lines = std::sync::Arc::new(visual_lines);
        let diff_scroll = self.sftp_view.diff_scroll.clone();
        div()
            .w_full()
            .h(px(480.0))
            .flex()
            .flex_col()
            .bg(rgb(theme.bg_sunken))
            .child(
                div()
                    .w_full()
                    .flex()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .text_size(px(SFTP_TEXT_XS))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(rgba((0x7f1d1d << 8) | SFTP_DIFF_HEADER_BG_ALPHA))
                            .child(Self::render_lucide_icon(
                                LucideIcon::File,
                                SFTP_ICON_SM,
                                rgb(SFTP_RED),
                            ))
                            .child(
                                div()
                                    .text_color(rgb(0xfca5a5))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(format!("{}:", self.i18n.t("sftp.diff.local"))),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(rgb(theme.text_muted))
                                    .child(sftp_file_name(local_path)),
                            )
                            .child(
                                div()
                                    .ml_auto()
                                    .text_color(rgb(SFTP_RED))
                                    .child(format!("-{}", stats.removed)),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(rgba((0x14532d << 8) | SFTP_DIFF_HEADER_BG_ALPHA))
                            .child(Self::render_lucide_icon(
                                LucideIcon::File,
                                SFTP_ICON_SM,
                                rgb(SFTP_GREEN),
                            ))
                            .child(
                                div()
                                    .text_color(rgb(0x86efac))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(format!("{}:", self.i18n.t("sftp.diff.remote"))),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(rgb(theme.text_muted))
                                    .child(sftp_file_name(remote_path)),
                            )
                            .child(
                                div()
                                    .ml_auto()
                                    .text_color(rgb(SFTP_GREEN))
                                    .child(format!("+{}", stats.added)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("sftp-diff-scroll")
                    .w_full()
                    .flex_1()
                    .overflow_y_scroll()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(SFTP_TEXT_XS))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .when(line_count == 0, |body| {
                        body.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t("sftp.diff.identical")),
                        )
                    })
                    .when(line_count > 0, |body| {
                        let diff_lines = diff_lines.clone();
                        body.child(
                            uniform_list(
                                "sftp-diff-virtual-list",
                                line_count,
                                move |range, _window, _cx| {
                                    range
                                        .map(|index| {
                                            let line = diff_lines[index].clone();
                                            let removed =
                                                line.kind == SftpDiffLineKind::Removed;
                                            let added = line.kind == SftpDiffLineKind::Added;
                                            div()
                                                .w_full()
                                                .h(px(SFTP_DIFF_ROW_HEIGHT))
                                                .flex()
                                                .border_b_1()
                                                .border_color(rgba((theme.border << 8) | SFTP_DIALOG_BORDER_HALF_ALPHA))
                                                .child(diff_cell(
                                                    &line.left_line_num,
                                                    &line.left_content,
                                                    removed,
                                                    theme.border,
                                                    true,
                                                ))
                                                .child(diff_cell(
                                                    &line.right_line_num,
                                                    &line.right_content,
                                                    added,
                                                    theme.border,
                                                    false,
                                                ))
                                                .into_any_element()
                                        })
                                        .collect::<Vec<_>>()
                                },
                            )
                            .track_scroll(diff_scroll)
                            .size_full()
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
                        )
                    }),
            )
            .into_any_element()
    }
}
