use super::super::*;

impl WorkspaceApp {
    /// Handles the one-shot window work that must remain with workspace-owned nodes and tabs.
    pub(in crate::workspace) fn handle_host_tools_window_request(
        &mut self,
        request: &HostToolsWindowRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(intent) = request.take() else {
            return;
        };
        match intent {
            HostToolsWindowIntent::OpenExistingNodeTerminal {
                connection_id,
                command,
                title,
                opened_notice,
                missing_notice,
            } => {
                let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
                    self.push_host_tools_window_notice(
                        missing_notice,
                        TerminalNoticeVariant::Error,
                    );
                    cx.notify();
                    return;
                };
                if !self.ssh_nodes.contains_key(&node_id) {
                    self.push_host_tools_window_notice(
                        missing_notice,
                        TerminalNoticeVariant::Error,
                    );
                    cx.notify();
                    return;
                }
                // NodeRouter retains the physical connection; this creates only a tab consumer.
                match self.queue_ssh_terminal_tab_for_existing_node(
                    node_id,
                    Some(command),
                    title,
                    window,
                    cx,
                ) {
                    Ok(()) => self.push_host_tools_window_notice(
                        opened_notice,
                        TerminalNoticeVariant::Success,
                    ),
                    Err(_) => self.push_host_tools_window_notice(
                        missing_notice,
                        TerminalNoticeVariant::Error,
                    ),
                }
                cx.notify();
            }
        }
    }

    fn push_host_tools_window_notice(&mut self, title: String, variant: TerminalNoticeVariant) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }
}
