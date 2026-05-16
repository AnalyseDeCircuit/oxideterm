impl WorkspaceApp {
    pub(in crate::workspace) fn render_terminal_command_suggestions(
        &self,
        suggestions: &[TerminalCommandSuggestion],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const SUGGESTIONS_BG_ALPHA: u32 = 0xf2; // Tauri bg-theme-bg-elevated/95
        const SUGGESTIONS_HEADER_BG_ALPHA: u32 = 0x99; // Tauri bg-theme-bg/60
        const SUGGESTIONS_BORDER_ALPHA: u32 = 0xff;
        const SUGGESTIONS_ROW_HOVER_ALPHA: u32 = 0x99; // Tauri hover:bg-theme-bg-hover/60
        const SUGGESTIONS_SOURCE_BG_ALPHA: u32 = 0xff;
        let theme = self.tokens.ui;
        let highlighted = self.terminal_command_suggestion_highlighted;
        let mut menu = div()
            .absolute()
            .bottom(px(56.0))
            .left(px(12.0))
            .w(px(720.0))
            .max_w(relative(0.96))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((theme.border << 8) | SUGGESTIONS_BORDER_ALPHA))
            .bg(rgba((theme.bg_elevated << 8) | SUGGESTIONS_BG_ALPHA))
            .shadow_lg()
            .font_family(settings_mono_font_family(self.settings_store.settings()));

        let mut index = 0usize;
        let mut group_cursor: Option<&'static str> = None;
        for suggestion in suggestions {
            if group_cursor != Some(suggestion.group_label_key) {
                group_cursor = Some(suggestion.group_label_key);
                menu = menu.child(
                    div()
                        .border_b_1()
                        .border_color(rgba((theme.border << 8) | 0x80))
                        .bg(rgba((theme.bg << 8) | SUGGESTIONS_HEADER_BG_ALPHA))
                        .px(px(12.0))
                        .py(px(4.0))
                        .text_size(px(10.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t(suggestion.group_label_key).to_uppercase()),
                );
            }
            let suggestion_for_click = suggestion.clone();
            let active = highlighted == Some(index);
            let row_index = index;
            index += 1;
            menu = menu.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .text_size(px(13.0))
                    .text_color(if active {
                        rgb(theme.text)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .bg(if active {
                        rgb(theme.bg_hover)
                    } else {
                        rgba(0x00000000)
                    })
                    .hover(move |style| {
                        style
                            .bg(rgba((theme.bg_hover << 8) | SUGGESTIONS_ROW_HOVER_ALPHA))
                            .text_color(rgb(theme.text))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.terminal_command_suggestion_highlighted = Some(row_index);
                            this.accept_terminal_command_suggestion(&suggestion_for_click, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .child(suggestion.label.clone()),
                    )
                    .when_some(suggestion.description.as_ref(), |row, description| {
                        row.child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(rgba((theme.text_muted << 8) | 0xb3))
                                .child(description.clone()),
                        )
                    })
                    .when_some(suggestion.risk, |row, risk| {
                        row.child(
                            div()
                                .rounded(px(self.tokens.radii.sm))
                                .px(px(6.0))
                                .py(px(2.0))
                                .text_size(px(10.0))
                                .text_color(if risk == "high" {
                                    rgba(0xfca5a5ff)
                                } else {
                                    rgba(0xfcd34dff)
                                })
                                .bg(if risk == "high" {
                                    rgba(0xef444426)
                                } else {
                                    rgba(0xf59e0b26)
                                })
                                .child(risk.to_uppercase()),
                        )
                    })
                    .child(
                        div()
                            .rounded(px(self.tokens.radii.sm))
                            .px(px(6.0))
                            .py(px(2.0))
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .bg(rgba((theme.bg_panel << 8) | SUGGESTIONS_SOURCE_BG_ALPHA))
                            .child(self.i18n.t(suggestion.source_label_key)),
                    ),
            );
        }
        menu.into_any_element()
    }
}
