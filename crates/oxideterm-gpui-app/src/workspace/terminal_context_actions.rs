use super::*;

fn terminal_selection_command_bar_text(selection: &str) -> Option<String> {
    let command = selection.trim_matches(|ch| matches!(ch, '\r' | '\n'));
    (!command.trim().is_empty()).then(|| command.to_string())
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_active_terminal_context_action_request(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active_pane) = self.active_pane() else {
            return false;
        };
        let Some(action) = active_pane.update(cx, |pane, _cx| pane.take_context_action_request())
        else {
            return false;
        };

        match action {
            TerminalContextAction::OpenSearch => {
                self.open_search(window, cx);
                true
            }
            TerminalContextAction::SendSelectionToAi => {
                let Some(_selection) = active_pane.read(cx).selected_text_snapshot() else {
                    return false;
                };
                // The inline panel owns AI context sanitization and truncation.
                self.open_terminal_ai_inline_panel(window, cx);
                true
            }
            TerminalContextAction::FillCommandBarFromSelection => {
                let Some(selection) = active_pane.read(cx).selected_text_snapshot() else {
                    return false;
                };
                let Some(command) = terminal_selection_command_bar_text(&selection) else {
                    return false;
                };
                self.search.visible = false;
                if self.ai.chat.inline_panel.open {
                    self.close_terminal_ai_inline_panel(window, cx);
                }
                self.close_terminal_command_overlays(cx);
                self.terminal_command_bar_draft = command;
                self.terminal_command_bar_focused = true;
                self.ime_marked_text = None;
                window.focus(&self.focus_handle, cx);
                cx.notify();
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::terminal_selection_command_bar_text;

    #[test]
    fn command_bar_text_trims_terminal_line_edges_only() {
        assert_eq!(
            terminal_selection_command_bar_text("\n  printf 'ok'  \r\n").as_deref(),
            Some("  printf 'ok'  ")
        );
    }

    #[test]
    fn command_bar_text_rejects_blank_selection() {
        assert_eq!(terminal_selection_command_bar_text("\n \t\r\n"), None);
    }
}
