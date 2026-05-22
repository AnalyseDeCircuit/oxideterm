impl WorkspaceApp {
    fn render_sftp_diff_body(
        &self,
        local_path: &str,
        local_content: &str,
        remote_path: &str,
        remote_content: &str,
        _has_background: bool,
        cx: &mut Context<Self>,
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
            .flex_1()
            .min_h(px(0.0))
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
                                    .child(self.render_selectable_text_scoped(
                                        "sftp-diff-local-label",
                                        (),
                                        format!("{}:", self.i18n.t("sftp.diff.local")),
                                        0xfca5a5,
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.render_selectable_text_scoped(
                                        "sftp-diff-local-path",
                                        local_path,
                                        sftp_file_name(local_path),
                                        theme.text_muted,
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .ml_auto()
                                    .text_color(rgb(SFTP_RED))
                                    .child(self.render_selectable_text_scoped(
                                        "sftp-diff-local-removed",
                                        (),
                                        format!("-{}", stats.removed),
                                        SFTP_RED,
                                        cx,
                                    )),
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
                                    .child(self.render_selectable_text_scoped(
                                        "sftp-diff-remote-label",
                                        (),
                                        format!("{}:", self.i18n.t("sftp.diff.remote")),
                                        0x86efac,
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.render_selectable_text_scoped(
                                        "sftp-diff-remote-path",
                                        remote_path,
                                        sftp_file_name(remote_path),
                                        theme.text_muted,
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .ml_auto()
                                    .text_color(rgb(SFTP_GREEN))
                                    .child(self.render_selectable_text_scoped(
                                        "sftp-diff-remote-added",
                                        (),
                                        format!("+{}", stats.added),
                                        SFTP_GREEN,
                                        cx,
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .id("sftp-diff-scroll")
                    .w_full()
                    .flex_1()
                    .selectable_overflow_y_scroll(
                        &self.selectable_text_scroll_handle("sftp-diff-scroll"),
                    )
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
                            tauri_virtual_uniform_list(
                                "sftp-diff-virtual-list",
                                line_count,
                                diff_scroll,
                                TauriVirtualListSpec::new(
                                    px(SFTP_DIFF_ROW_HEIGHT),
                                    SFTP_DIFF_VIRTUAL_OVERSCAN,
                                ),
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
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
                        )
                    }),
            )
            .into_any_element()
    }
}
