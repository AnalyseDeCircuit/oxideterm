impl WorkspaceApp {
    pub(in crate::workspace) fn ai_web_provider_id(
        &self,
    ) -> Result<String, oxideterm_ai::web::WebError> {
        let settings = self.settings_store.settings();
        if !settings.ai.web_search.enabled {
            return Err(oxideterm_ai::web::WebError::Disabled);
        }
        let id = settings
            .ai
            .web_search
            .provider_id
            .as_deref()
            .ok_or(oxideterm_ai::web::WebError::Configuration)?;
        oxideterm_ai::provider_views(&settings.ai.providers)
            .into_iter()
            .find(|provider| {
                provider.id == id && oxideterm_ai::web::official_deepseek_provider(provider)
            })
            .map(|provider| provider.id)
            .ok_or(oxideterm_ai::web::WebError::Configuration)
    }

    fn start_ai_web_execution(
        &mut self,
        owner: AiToolRunContext,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        mut sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
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
        let key_store = self.ai_entity.read(cx).key_store().clone();
        let audit = oxideterm_audit::AuditContext::current_request();
        // The pending tool call owns this future. Dropping its receiver cancels HTTP and releases the key.
        self.forwarding_runtime.spawn(oxideterm_audit::AuditContext::scope_optional(audit, async move {
            let started = std::time::Instant::now();
            let operation = async {
                oxideterm_ai::web::validate_arguments(&tool_name, &args)?;
                let key = tokio::task::spawn_blocking(move || key_store.get_provider_key(&provider_id))
                    .await.map_err(|_| oxideterm_ai::web::WebError::Authentication)?
                    .map_err(|_| oxideterm_ai::web::WebError::Authentication)?
                    .ok_or(oxideterm_ai::web::WebError::Authentication)?;
                oxideterm_ai::web::execute(&tool_name, &args, &key).await
            };
            let result = tokio::select! {
                _ = sender.closed() => return,
                result = ai_pending_dispatch(owner.dispatch.as_ref(), operation) => result,
            };
            let snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
            let action = match result {
                Ok(Ok(result)) => {
                    let data = serde_json::json!({"sources":result.sources,"truncated":result.truncated});
                    snapshot.ok("Web request completed.", result.output, data, "read").with_verified(false)
                }
                Ok(Err(error)) => snapshot.fail(error.to_string(), error.code(), error.to_string(), "read"),
                Err(_) => snapshot.fail("Web request cancelled.", "operation_cancelled", "Web request cancelled.", "read"),
            };
            let _ = sender.send(snapshot.to_executed_tool_result(tool_call_id, tool_name, action, started.elapsed().as_millis()));
        }));
    }
}
