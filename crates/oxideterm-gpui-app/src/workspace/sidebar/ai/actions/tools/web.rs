impl WorkspaceApp {
    pub(in crate::workspace) fn ai_web_provider_id(
        &self,
    ) -> Result<String, oxideterm_ai::web::WebError> {
        let settings = self.settings_store.settings();
        oxideterm_ai::web::configured_provider_id(&settings.ai)
    }

    fn start_ai_web_execution(
        &mut self,
        owner: AiToolRunContext,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        cx: &mut Context<Self>,
    ) {
        let settings = self.settings_store.settings();
        let provider = if !settings.ai.enabled
            || !settings.ai.tool_use.enabled
            || settings.ai.tool_use.disabled_tools.contains(&tool_name)
        {
            Err(oxideterm_ai::web::WebError::Disabled)
        } else {
            self.ai_web_provider_id()
        };
        let provider_id = match provider {
            Ok(id) => id,
            Err(error) => {
                let _ = sender.send(rejected_ai_tool_result(
                    tool_call_id,
                    tool_name,
                    error.code(),
                    error.to_string(),
                ));
                return;
            }
        };
        let key_store = self.ai.state.read(cx).key_store().clone();
        let audit = oxideterm_audit::AuditContext::current_request();
        // The pending tool call owns the HTTP request and its temporary key.
        self.forwarding_runtime.spawn(oxideterm_audit::AuditContext::scope_optional(
            audit,
            oxideterm_ai::web::complete_authorized_request(
                tool_call_id, tool_name, args, provider_id, key_store, owner.dispatch, sender,
            ),
        ));
    }
}
