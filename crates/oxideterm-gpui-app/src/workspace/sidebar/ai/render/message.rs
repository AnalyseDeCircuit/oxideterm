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

    pub(in crate::workspace) fn resolve_ai_cursor_request(
        &mut self,
        generation: u64,
        id: &str,
        accepted: bool,
        cx: &mut Context<Self>,
    ) {
        let route = self
            .ai.state
            .read(cx)
            .pending_cursor_requests
            .get(&(generation, id.into()))
            .map(|pending| {
                (
                    pending.conversation_id.clone(),
                    pending.assistant_id.clone(),
                    pending.request.method(),
                )
            });
        let Some((conversation, assistant, method)) = route else {
            return;
        };
        if self.ai.state.update(cx, |ai, _| {
            ai.resolve_cursor_request(generation, id, accepted)
        }) {
            self.apply_ai_tool_status(
                generation,
                &conversation,
                &assistant,
                id,
                method,
                "{}",
                if accepted { "completed" } else { "rejected" },
                None,
                Some("read".into()),
                None,
                false,
                None,
                None,
                None,
                cx,
            );
            cx.notify();
        }
    }

    pub(in crate::workspace) fn ai_tool_display_name(&self, name: &str) -> String {
        oxideterm_gpui_ai::sidebar::transcript::tool_display_name(&self.i18n, name)
    }
}
