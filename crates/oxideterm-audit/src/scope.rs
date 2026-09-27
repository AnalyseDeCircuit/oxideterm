use crate::{AuditContext, AuditOperation};
use std::future::Future;

tokio::task_local! {
    static REQUEST: AuditContext;
}

impl AuditContext {
    /// The scope follows this future's polls, including executor thread changes.
    /// Spawned tasks must explicitly capture and scope their request context.
    pub async fn scope<F: Future>(&self, future: F) -> F::Output {
        REQUEST.scope(self.clone(), future).await
    }

    pub async fn scope_optional<F: Future>(context: Option<Self>, future: F) -> F::Output {
        match context {
            Some(context) => context.scope(future).await,
            None => future.await,
        }
    }

    pub fn current_request() -> Option<Self> {
        REQUEST.try_with(Clone::clone).ok()
    }

    /// The synchronous UI dispatch owns this scope only until the handler returns.
    pub fn with_sync_request<R>(context: Option<&Self>, run: impl FnOnce() -> R) -> R {
        let Some(context) = context else {
            return run();
        };
        REQUEST.sync_scope(context.clone(), run)
    }

    /// Request provenance augments the runtime owner; it cannot replace the
    /// actual transport, endpoint, consumer, or writer that performs the work.
    pub fn for_request(&self) -> Self {
        Self::current_request().map_or_else(|| self.clone(), |request| self.with_request(&request))
    }

    pub fn with_request(&self, request: &Self) -> Self {
        let mut context = self.clone();
        context.source = request.source;
        context.agent_id = request.agent_id.clone();
        context.parent_id = request.parent_id.clone();
        context
    }
}

impl AuditOperation {
    pub fn in_request(
        context: Option<&AuditContext>,
        category: crate::AuditCategory,
        action: &str,
        detail: Option<&str>,
    ) -> Self {
        Self::in_context(
            context.map(AuditContext::for_request).as_ref(),
            category,
            action,
            detail,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuditError, AuditKeyProvider, AuditService, AuditSource};
    use zeroize::Zeroizing;

    struct Keys;
    impl AuditKeyProvider for Keys {
        fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
            Ok(Zeroizing::new(vec![11; 32]))
        }
        fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
            self.load(id)
        }
    }

    #[test]
    fn one_cross_target_request_preserves_each_execution_session() {
        let directory = tempfile::tempdir().unwrap();
        let service =
            AuditService::with_key_provider(directory.path().join("audit.db"), Keys).unwrap();
        let base = AuditContext::new(service.client(), AuditSource::User);
        let mut request = base.session("ssh", "source@first");
        request.node_id = Some(Zeroizing::new("source-node".into()));
        request.parent_id = Some("broadcast-operation".into());
        request.agent_id = Some(Zeroizing::new("agent-one".into()));
        request.source = AuditSource::Ai;

        let mut first = base.session("ssh", "target@one");
        first.node_id = Some(Zeroizing::new("first-node".into()));
        first.transport_id = Some("first-transport".into());
        let mut second = base.session("ssh", "target@two");
        second.node_id = Some(Zeroizing::new("second-node".into()));
        second.transport_id = Some("second-transport".into());

        let first_result = first.with_request(&request);
        let second_result = second.with_request(&request);
        assert_eq!(first_result.session_id, first.session_id);
        assert_eq!(second_result.session_id, second.session_id);
        assert_ne!(first_result.session_id, second_result.session_id);
        assert_eq!(
            first_result.node_id.as_deref().map(String::as_str),
            Some("first-node")
        );
        assert_eq!(
            second_result.node_id.as_deref().map(String::as_str),
            Some("second-node")
        );
        assert_eq!(
            first_result.target.as_deref().map(String::as_str),
            Some("target@one")
        );
        assert_eq!(
            second_result.target.as_deref().map(String::as_str),
            Some("target@two")
        );
        assert_eq!(
            first_result.transport_id.as_deref(),
            Some("first-transport")
        );
        assert_eq!(
            second_result.transport_id.as_deref(),
            Some("second-transport")
        );
        assert_eq!(
            first_result.parent_id.as_deref(),
            Some("broadcast-operation")
        );
        assert_eq!(
            second_result.agent_id.as_deref().map(String::as_str),
            Some("agent-one")
        );
        assert_eq!(first_result.source, AuditSource::Ai);
    }
}
