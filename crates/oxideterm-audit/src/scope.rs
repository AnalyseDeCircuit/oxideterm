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
