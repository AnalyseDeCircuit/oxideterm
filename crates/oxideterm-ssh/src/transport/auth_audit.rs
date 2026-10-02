struct AuthenticationAudit {
    context: Option<oxideterm_audit::AuditContext>,
    sequence: u32,
}

impl AuthenticationAudit {
    async fn password_prompt(
        &self,
        handler: &dyn SshPromptHandler,
        prompt: SshPasswordPrompt,
    ) -> Result<SshPasswordResponse, SshPromptError> {
        use oxideterm_audit::{AuditEvidence, AuditOutcome};
        let operation = oxideterm_audit::AuditOperation::in_context(
            self.context.as_ref(),
            oxideterm_audit::AuditCategory::Security,
            "ssh_auth_prompt",
            Some(r#"{"prompt_count":1,"chained":false}"#),
        );
        let result = handler.password(prompt).await;
        let outcome = match &result {
            Ok(_) => AuditOutcome::Sent,
            Err(SshPromptError::Cancelled) => AuditOutcome::Cancelled,
            Err(_) => AuditOutcome::Failed,
        };
        operation.finish(outcome, AuditEvidence::Lifecycle, None, None);
        result
    }

    fn new(context: Option<&oxideterm_audit::AuditContext>, parent: Option<&str>) -> Self {
        Self {
            context: context.cloned().map(|mut context| {
                context.parent_id = parent.map(str::to_owned);
                context
            }),
            sequence: 0,
        }
    }

    fn attempt(
        &mut self,
        method: &str,
        request: &str,
        algorithm: Option<&str>,
    ) -> oxideterm_audit::AuditOperation {
        self.sequence += 1;
        let detail = serde_json::json!({"method": method, "request": request,
            "sequence": self.sequence, "algorithm": algorithm})
        .to_string();
        oxideterm_audit::AuditOperation::in_context(
            self.context.as_ref(),
            oxideterm_audit::AuditCategory::Security,
            "ssh_auth_attempt",
            Some(&detail),
        )
    }

    fn finish(operation: oxideterm_audit::AuditOperation, result: Option<&client::AuthResult>) {
        use oxideterm_audit::{AuditEvidence, AuditOutcome};
        let outcome = match result {
            Some(client::AuthResult::Success) => AuditOutcome::Succeeded,
            Some(client::AuthResult::Failure {
                partial_success: true,
                ..
            }) => AuditOutcome::Partial,
            Some(client::AuthResult::Failure { .. }) => AuditOutcome::Denied,
            None => AuditOutcome::Failed,
        };
        operation.finish(outcome, AuditEvidence::Protocol, None, None);
    }

    async fn authenticate<E>(
        &mut self,
        method: &str,
        algorithm: Option<&str>,
        request: impl Future<Output = Result<client::AuthResult, E>>,
    ) -> Result<client::AuthResult, E> {
        let operation = self.attempt(method, "authenticate", algorithm);
        let result = request.await;
        Self::finish(operation, result.as_ref().ok());
        result
    }

    async fn interactive<E>(
        &mut self,
        step: &str,
        request: impl Future<Output = Result<client::KeyboardInteractiveAuthResponse, E>>,
    ) -> Result<client::KeyboardInteractiveAuthResponse, E> {
        use oxideterm_audit::{AuditEvidence, AuditOutcome};
        let operation = self.attempt("keyboard-interactive", step, None);
        let result = request.await;
        let outcome = match &result {
            Ok(client::KeyboardInteractiveAuthResponse::Success) => AuditOutcome::Succeeded,
            Ok(client::KeyboardInteractiveAuthResponse::Failure {
                partial_success: true,
                ..
            }) => AuditOutcome::Partial,
            Ok(client::KeyboardInteractiveAuthResponse::Failure { .. }) => AuditOutcome::Denied,
            Ok(client::KeyboardInteractiveAuthResponse::InfoRequest { .. }) => AuditOutcome::Sent,
            Err(_) => AuditOutcome::Failed,
        };
        operation.finish(outcome, AuditEvidence::Protocol, None, None);
        result
    }

    async fn prompt(
        &self,
        handler: &dyn SshPromptHandler,
        request: KeyboardInteractivePromptRequest,
        timeout: Option<Duration>,
    ) -> Result<KeyboardInteractiveResponses, SshPromptError> {
        use oxideterm_audit::{AuditEvidence, AuditOutcome};
        // Only counts and flow type cross the audit boundary; server prompts and all replies remain with the authentication flow.
        let detail =
            serde_json::json!({"prompt_count": request.prompts.len(), "chained": request.chained})
                .to_string();
        let mut operation = oxideterm_audit::AuditOperation::in_context(
            self.context.as_ref(),
            oxideterm_audit::AuditCategory::Security,
            "ssh_auth_prompt",
            Some(&detail),
        );
        let response = handler.keyboard_interactive(request);
        let result = match timeout {
            Some(timeout) => tokio::time::timeout(timeout, response)
                .await
                .unwrap_or(Err(SshPromptError::Timeout)),
            None => response.await,
        };
        let outcome = match &result {
            Ok(_) => AuditOutcome::Sent,
            Err(SshPromptError::Cancelled) => AuditOutcome::Cancelled,
            Err(SshPromptError::Timeout) => {
                operation.summary(&format!("{detail}; reason=timeout"));
                AuditOutcome::Failed
            }
            Err(SshPromptError::Failed(_)) => AuditOutcome::Failed,
        };
        operation.finish(outcome, AuditEvidence::Lifecycle, None, None);
        result
    }
}
