use super::*;

use oxideterm_gpui_ui::{StatusPillOptions, StatusTone, status_pill};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandSuggestionRowTextRole {
    Muted,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandSuggestionRowBackgroundRole {
    Transparent,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalCommandSuggestionRowUiState {
    text: TerminalCommandSuggestionRowTextRole,
    background: TerminalCommandSuggestionRowBackgroundRole,
}

fn terminal_command_suggestion_row_ui_state(
    highlighted: bool,
) -> TerminalCommandSuggestionRowUiState {
    if highlighted {
        TerminalCommandSuggestionRowUiState {
            text: TerminalCommandSuggestionRowTextRole::Active,
            background: TerminalCommandSuggestionRowBackgroundRole::Active,
        }
    } else {
        TerminalCommandSuggestionRowUiState {
            text: TerminalCommandSuggestionRowTextRole::Muted,
            background: TerminalCommandSuggestionRowBackgroundRole::Transparent,
        }
    }
}

fn terminal_command_suggestion_risk_tone(risk: &str) -> StatusTone {
    // Completion providers expose the same risk labels as quick commands; keep
    // rendering semantic so the actual colors stay centralized.
    if risk == "high" {
        StatusTone::Error
    } else {
        StatusTone::Warning
    }
}

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
            .on_scroll_wheel(|_, _, cx| {
                // Completion popovers are their own wheel boundary; otherwise
                // root-level overlay dismissal would close suggestions while
                // the user is trying to inspect them.
                cx.stop_propagation();
            })
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
                        // The first group header paints directly against the
                        // rounded suggestion popover edge; match the shell's
                        // inner curve so GPUI cannot expose square pixels.
                        .when(index == 0, |header| {
                            header.rounded_t(px(rounded_shell_child_radius(self.tokens.radii.lg)))
                        })
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
            let row_state = terminal_command_suggestion_row_ui_state(active);
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
                    .text_color(match row_state.text {
                        TerminalCommandSuggestionRowTextRole::Active => rgb(theme.text),
                        TerminalCommandSuggestionRowTextRole::Muted => rgb(theme.text_muted),
                    })
                    .bg(match row_state.background {
                        TerminalCommandSuggestionRowBackgroundRole::Active => rgb(theme.bg_hover),
                        TerminalCommandSuggestionRowBackgroundRole::Transparent => rgba(0x00000000),
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
        // Suggestion refreshes keep one timeline while the popup remains mounted.
        oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            "terminal-command-suggestions-enter",
            menu,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        )
    }
}

#[cfg(test)]
mod tests {
    use oxideterm_gpui_ui::StatusTone;

    use super::{
        TerminalCommandSuggestionRowBackgroundRole, TerminalCommandSuggestionRowTextRole,
        TerminalCommandSuggestionRowUiState, terminal_command_suggestion_risk_tone,
        terminal_command_suggestion_row_ui_state,
    };

    #[test]
    fn highlighted_suggestion_row_uses_active_visual_state() {
        assert_eq!(
            terminal_command_suggestion_row_ui_state(true),
            TerminalCommandSuggestionRowUiState {
                text: TerminalCommandSuggestionRowTextRole::Active,
                background: TerminalCommandSuggestionRowBackgroundRole::Active,
            }
        );
    }

    #[test]
    fn unhighlighted_suggestion_row_stays_muted_and_transparent() {
        assert_eq!(
            terminal_command_suggestion_row_ui_state(false),
            TerminalCommandSuggestionRowUiState {
                text: TerminalCommandSuggestionRowTextRole::Muted,
                background: TerminalCommandSuggestionRowBackgroundRole::Transparent,
            }
        );
    }

    #[test]
    fn suggestion_risk_tone_maps_classifier_labels_to_shared_status_tones() {
        assert_eq!(
            terminal_command_suggestion_risk_tone("high"),
            StatusTone::Error
        );
        assert_eq!(
            terminal_command_suggestion_risk_tone("medium"),
            StatusTone::Warning
        );
    }
}
