use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn command_history_icon_button(
        &self,
        icon: LucideIcon,
        size: f32,
        label_key: &'static str,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_tooltip_icon_button(
            icon,
            size,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions::compact(24.0),
            self.i18n.t(label_key),
            "command-history-action",
            true,
            listener,
            cx.entity(),
        )
    }
    pub(in crate::workspace) fn open_command_history_clear_confirm(
        &mut self,
        all: bool,
        cx: &mut Context<Self>,
    ) {
        self.overlay.update(cx, |overlay, cx| {
            overlay.open_confirm(
                overlay::WorkspaceOverlayConfirmKind::CommandHistoryClear { all },
                cx,
            );
        });
    }
    pub(in crate::workspace) fn ssh_command_history(
        &self,
        node_id: &NodeId,
    ) -> Option<SharedTerminalCommandHistory> {
        let node = self.ssh_nodes.get(node_id)?;
        if let Some(saved_id) = &node.saved_connection_id {
            // Renames keep history; changing the endpoint or account selects a different scope.
            let scope = serde_json::to_string(&(
                saved_id,
                &node.endpoint.host,
                node.endpoint.port,
                &node.endpoint.username,
            ))
            .ok()?;
            Some(
                self.terminal_history_store
                    .history(scope, node.title.clone()),
            )
        } else {
            self.ssh_terminal_command_histories.get(node_id).cloned()
        }
    }

    pub(in crate::workspace) fn active_command_history(
        &self,
        cx: &App,
    ) -> Option<SharedTerminalCommandHistory> {
        self.active_pane(cx)
            .map(|pane| pane.read(cx).command_history())
    }

    pub(in crate::workspace) fn remove_history_suggestion(
        &mut self,
        command: &str,
        cx: &mut Context<Self>,
    ) {
        if self.terminal_history_all_connections {
            for (_, _, history) in self.terminal_history_store.histories() {
                history.remove(command);
            }
        }
        if let Some(history) = self.active_command_history(cx) {
            history.remove(command);
        }
        self.terminal_command_sender
            .update(cx, |sender, cx| sender.show_compact_history(cx));
        cx.notify();
    }
}
