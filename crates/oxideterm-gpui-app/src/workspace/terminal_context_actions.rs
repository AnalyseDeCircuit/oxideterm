use super::*;

fn terminal_selection_command_bar_text(selection: &str) -> Option<String> {
    let command = selection.trim_matches(|ch| matches!(ch, '\r' | '\n'));
    (!command.trim().is_empty()).then(|| command.to_string())
}

impl WorkspaceApp {
    pub(in crate::workspace) fn request_ssh_terminal_reconnect(
        &mut self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
            return;
        };
        if !pane.read(cx).can_reconnect_ssh() {
            return;
        }
        let Some(session_id) = self
            .tabs(cx)
            .iter()
            .find_map(|tab| tab.root_pane.as_ref()?.session_id_for_pane(pane_id))
        else {
            return;
        };
        let Some(node_id) = self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_node_id(session_id)
        else {
            return;
        };
        if self.node_is_ready_for_terminal(&node_id) {
            self.reconnect_ssh_terminal_session(session_id, &node_id, cx);
            return;
        }
        pane.update(cx, |pane, cx| pane.set_ssh_reconnect_pending(true, cx));
        self.workspace_runtime.update(cx, |runtime, cx| {
            runtime.queue_ssh_terminal_open(
                runtime_entity::PendingSshTerminalOpen {
                    node_id: node_id.clone(),
                    reconnect_session_id: Some(session_id),
                    post_connect_command: None,
                    mark_used_connection_id: None,
                    save_after_open: None,
                    cleanup_node_id: None,
                    title: String::new(),
                },
                cx,
            )
        });
        if !self.ensure_node_connection_started(&node_id, cx) {
            pane.update(cx, |pane, cx| pane.set_ssh_reconnect_pending(false, cx));
        }
    }

    pub(in crate::workspace) fn reconnect_ssh_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_session_belongs_to_node(session_id, node_id)
        {
            return false;
        }
        // Resolve this exact consumer; reconnect never elects another pane as the node owner.
        let Some(location) = self.tab_host.read(cx).terminal_location(session_id) else {
            return false;
        };
        let Some(pane) = self
            .tab_host
            .read(cx)
            .terminal_pane_for_target(location.pane_id, session_id)
        else {
            return false;
        };
        let reuse_connection_id = pane
            .read(cx)
            .ssh_connection_handle()
            .filter(|connection| {
                matches!(
                    connection.state(),
                    ConnectionState::Active | ConnectionState::Idle
                )
            })
            .map(|connection| connection.connection_id().to_string());
        if reuse_connection_id.is_some() && pane.read(cx).lifecycle().is_running() {
            return false;
        }
        let result = self
            .ssh_terminal_session_config(node_id, session_id, reuse_connection_id, None, true, cx)
            .and_then(|config| pane.update(cx, |pane, cx| pane.reconnect_ssh(config, cx)));
        if let Err(error) = result {
            pane.update(cx, |pane, cx| pane.set_ssh_reconnect_pending(false, cx));
            self.push_command_palette_toast(
                self.i18n.t("sessions.actions.reconnect"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
                cx,
            );
            return false;
        }
        self.terminal.update(cx, |terminal, _| {
            terminal
                .sync_groups_mut()
                .remount(location.pane_id, location.pane_id)
        });
        self.stop_public_mcp_recordings_for_session(session_id, cx);
        true
    }

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
            TerminalContextAction::Reconnect => {
                self.request_ssh_terminal_reconnect(pane_id, cx);
                true
            }
            TerminalContextAction::PluginTextTool => {
                let Some((action, text)) =
                    source_pane.update(cx, |pane, _| pane.take_plugin_text_request())
                else {
                    return false;
                };
                self.open_native_plugin_text_workbench(
                    &action.plugin_id,
                    &action.tab_id,
                    &action.control_id,
                    text,
                    cx,
                );
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
            TerminalContextAction::PreviewArchiveCommand => {
                let path = source_pane.update(cx, |pane, _| pane.take_archive_path_request());
                if let Some(path) = path {
                    self.open_archive_command(pane_id, source_pane, path, window, cx);
                }
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
