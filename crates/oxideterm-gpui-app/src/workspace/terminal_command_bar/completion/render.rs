use super::*;
use oxideterm_gpui_ui::modal::rounded_shell_child_radius;

fn terminal_command_suggestion_risk_tone(risk: &str) -> StatusTone {
    if risk == "high" {
        StatusTone::Error
    } else {
        StatusTone::Warning
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_terminal_command_sender_suggestions(
        &self,
        suggestions: &[TerminalCommandSuggestion],
        highlighted: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const SUGGESTIONS_BG_ALPHA: u32 = 0xf2;
        const SUGGESTIONS_HEADER_BG_ALPHA: u32 = 0x99;
        const SUGGESTIONS_ROW_HOVER_ALPHA: u32 = 0x99;

        let theme = self.tokens.ui;
        let mut menu = div()
            .absolute()
            .bottom_full()
            .mb(px(4.0))
            .left(px(12.0))
            .w(px(720.0))
            .max_w(relative(0.96))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg_elevated << 8) | SUGGESTIONS_BG_ALPHA))
            .shadow_lg()
            .occlude()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .font_family(settings_mono_font_family(self.settings_store.settings()));

        let mut group_cursor = None;
        for (index, suggestion) in suggestions.iter().enumerate() {
            if group_cursor != Some(suggestion.group_label_key) {
                group_cursor = Some(suggestion.group_label_key);
                menu = menu.child(
                    div()
                        .when(index == 0, |header| {
                            header.rounded_t(px(rounded_shell_child_radius(self.tokens.radii.lg)))
                        })
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
                            this.accept_terminal_command_sender_suggestion(
                                &suggestion_for_click,
                                cx,
                            );
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
                    .when_some(suggestion.risk, |row, risk| {
                        row.child(status_pill(
                            &self.tokens,
                            risk.to_uppercase(),
                            StatusPillOptions::new(terminal_command_suggestion_risk_tone(risk))
                                .compact()
                                .strong(),
                        ))
                    })
                    .child(status_pill(
                        &self.tokens,
                        self.i18n.t(suggestion.source_label_key),
                        StatusPillOptions::new(StatusTone::Neutral).compact(),
                    )),
            );
        }
        menu.into_any_element()
    }
}
