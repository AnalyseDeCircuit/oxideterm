//! File work runs on the SSH runtime on desktop and on the host event loop on the web.
use super::{IdeFileError, IdeSurface};
use std::future::Future;

#[cfg(not(target_family = "wasm"))]
pub(super) use oxideterm_audit::AuditSource as FileOrigin;
#[cfg(target_family = "wasm")]
#[derive(Clone, Copy)]
pub(super) enum FileOrigin {
    User,
    Plugin,
    System,
}

pub(super) struct FileJobs {
    #[cfg(not(target_family = "wasm"))]
    runtime: std::sync::Arc<tokio::runtime::Runtime>,
}

impl IdeSurface {
    pub(super) fn file_jobs(&self) -> FileJobs {
        FileJobs {
            #[cfg(not(target_family = "wasm"))]
            runtime: self.backend_runtime.clone(),
        }
    }
}

impl FileJobs {
    #[cfg(not(target_family = "wasm"))]
    pub(super) async fn run<T: Send + 'static>(
        &self,
        job: impl Future<Output = Result<T, IdeFileError>> + Send + 'static,
    ) -> Result<T, IdeFileError> {
        super::await_ide_backend(self.runtime.spawn(job)).await
    }

    #[cfg(target_family = "wasm")]
    pub(super) async fn run<T>(
        &self,
        job: impl Future<Output = Result<T, IdeFileError>>,
    ) -> Result<T, IdeFileError> {
        job.await
    }
}

/// A queued save keeps the origin of the request, not of the previous save's completion.
#[derive(Clone)]
pub(super) struct FileRequest {
    #[cfg(not(target_family = "wasm"))]
    context: Option<oxideterm_audit::AuditContext>,
}

impl FileRequest {
    pub(super) fn capture(_origin: Option<FileOrigin>) -> Self {
        Self {
            #[cfg(not(target_family = "wasm"))]
            context: _origin.and_then(|source| {
                oxideterm_audit::AuditContext::current_request().or_else(|| {
                    oxideterm_audit::AuditContext::current().map(|mut context| {
                        context.source = source;
                        context
                    })
                })
            }),
        }
    }

    pub(super) async fn run<F: Future>(self, job: F) -> F::Output {
        #[cfg(not(target_family = "wasm"))]
        if let Some(context) = self.context {
            return context.scope(job).await;
        }
        job.await
    }
}

pub(super) struct ConflictRequest {
    #[cfg(not(target_family = "wasm"))]
    operation: oxideterm_audit::AuditOperation,
    request: FileRequest,
}

impl ConflictRequest {
    pub(super) fn new(_action: &'static str, _location: &oxideterm_ide_core::IdeLocation) -> Self {
        let request = FileRequest::capture(Some(FileOrigin::User));
        #[cfg(not(target_family = "wasm"))]
        {
            let mut request = request;
            let operation = oxideterm_audit::AuditOperation::in_context(
                request.context.as_ref(),
                oxideterm_audit::AuditCategory::File,
                _action,
                Some(&_location.stable_key()),
            );
            if let Some(context) = &mut request.context {
                context.parent_id = operation.id().map(str::to_owned);
            }
            Self { operation, request }
        }
        #[cfg(target_family = "wasm")]
        Self { request }
    }

    pub(super) async fn run<T>(
        self,
        job: impl Future<Output = Result<T, IdeFileError>>,
        _size: impl FnOnce(&T) -> Option<u64>,
    ) -> Result<T, IdeFileError> {
        let result = self.request.run(job).await;
        #[cfg(not(target_family = "wasm"))]
        self.operation.finish(
            if result.is_ok() {
                oxideterm_audit::AuditOutcome::Succeeded
            } else {
                oxideterm_audit::AuditOutcome::Failed
            },
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            result.as_ref().ok().and_then(_size),
        );
        result
    }
}
