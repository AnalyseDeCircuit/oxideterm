use super::*;

fn terminal_selection_command_bar_text(selection: &str) -> Option<String> {
    let command = selection.trim_matches(|ch| matches!(ch, '\r' | '\n'));
    (!command.trim().is_empty()).then(|| command.to_string())
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_terminal_context_action_request_for_pane(
        &mut self,
        pane_id: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(source_pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
            return false;
        };
        let Some(action) = source_pane.update(cx, |pane, _cx| pane.take_context_action_request())
        else {
            return false;
        };

        match action {
            TerminalContextAction::OpenPasteEditor => {
                source_pane.update(cx, |pane, cx| pane.open_paste_editor(true, window, cx));
                true
            }
            TerminalContextAction::InspectText | TerminalContextAction::ExtractArchive => {
                source_pane.update(cx, |pane, cx| pane.inspect_selected_text(window, cx));
                true
            }

            TerminalContextAction::SaveTemporaryMarker => {
                let Some(draft) = source_pane.update(cx, |pane, _| pane.take_marker_rule_draft())
                else {
                    return false;
                };
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.pending_highlight_marker = Some(draft);
                    settings.set_active_tab(SettingsTab::Terminal, cx);
                    settings.set_terminal_page(
                        oxideterm_settings_model::TerminalSettingsPage::Highlight,
                        cx,
                    );
                });
                if let Some(main) = self
                    .window_registry
                    .handle_for_role(window_registry::WindowRole::Main)
                    && main.window_id() != window.window_handle().window_id()
                {
                    cx.spawn(async move |workspace, cx| {
                        let _ = cx.update_window(main, |_, window, cx| {
                            let _ = workspace.update(cx, |workspace, cx| {
                                window.activate_window();
                                workspace.open_settings(window, cx);
                            });
                        });
                    })
                    .detach();
                } else {
                    self.open_settings(window, cx);
                }
                true
            }
            TerminalContextAction::OpenSenderDraft => {
                let Some(text) = source_pane.update(cx, |pane, _| pane.take_sender_draft()) else {
                    return false;
                };
                let editor = self.terminal_command_sender.update(cx, |sender, cx| {
                    sender.add_source_document(
                        pane_id,
                        window.window_handle().window_id(),
                        text,
                        cx,
                    )
                });
                window.focus(&editor.focus_handle(cx), cx);
                cx.notify();
                true
            }
            TerminalContextAction::OpenSearch => {
                self.open_search(window, cx);
                true
            }
            TerminalContextAction::SendSelectionToAi => {
                let Some(_selection) = source_pane.read(cx).selected_text_snapshot() else {
                    return false;
                };
                // The inline panel owns AI context sanitization and truncation.
                self.open_terminal_ai_inline_panel(window, cx);
                true
            }
            TerminalContextAction::FillCommandBarFromSelection => {
                let Some(selection) = source_pane.read(cx).selected_text_snapshot() else {
                    return false;
                };
                let Some(command) = terminal_selection_command_bar_text(&selection) else {
                    return false;
                };
                if let Some(id) = self.active_pane_id(cx) {
                    self.hide_search(id, cx);
                }
                if self.ai_entity.read(cx).terminal_inline_panel().open {
                    self.close_terminal_ai_inline_panel(window, cx);
                }
                self.close_terminal_command_overlays(cx);
                let sender_id = self.replace_terminal_command_sender_text(command, cx);
                self.ime_marked_text = None;
                self.focus_terminal_command_sender_input(sender_id, window, cx);
                cx.notify();
                true
            }
            TerminalContextAction::OpenSessionTriggers => {
                self.open_terminal_trigger_settings_for_pane(pane_id, window, cx);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::terminal_selection_command_bar_text;

    #[test]
    fn command_bar_selection_preserves_command_spaces_and_rejects_blank_text() {
        for (input, expected) in [
            ("\n  printf 'ok'  \r\n", Some("  printf 'ok'  ")),
            ("\n \t\r\n", None),
        ] {
            assert_eq!(
                terminal_selection_command_bar_text(input).as_deref(),
                expected
            );
        }
    }
}
