#[cfg(test)]
use oxideterm_gpui_ai::sidebar::transcript::ai_message_is_awaiting_tool_summary;

impl WorkspaceApp {
    pub(in crate::workspace) fn insert_ai_code_block_command(
        &mut self,
        command: String,
        cx: &mut Context<Self>,
    ) {
        if command.trim().is_empty() {
            return;
        }
        if let Some(pane) = self.active_pane(cx) {
            let _ = pane.update(cx, |pane, cx| {
                // Tauri's ai-insert-command writes programmatic terminal input without submitting it.
                pane.send_ai_input_bytes(command.as_bytes(), cx);
            });
        }
    }



    pub(in crate::workspace) fn ai_tool_display_name(&self, name: &str) -> String {
        oxideterm_gpui_ai::sidebar::transcript::tool_display_name(&self.i18n, name)
    }
}
