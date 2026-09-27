use super::{AuditClient, HealthState, Request};
use crate::{
    AuditCapture, AuditCategory, AuditContext, AuditError, AuditEvidence, AuditExportFormat,
    AuditOutcome, AuditQuery, AuditRecord, AuditStore, redact,
};
use async_channel::Sender;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

pub(super) struct ManagementOperation {
    pub(super) record: AuditRecord,
    client: AuditClient,
    started: Instant,
}

impl ManagementOperation {
    pub(super) fn new(
        client: &AuditClient,
        action: &str,
        target: Option<&str>,
        detail: Option<&str>,
    ) -> Self {
        let mut context = AuditContext::new(client.clone(), client.source);
        context.target = target.map(redact);
        Self {
            record: context.event_record(AuditCategory::Audit, action, detail),
            client: client.clone(),
            started: Instant::now(),
        }
    }

    pub(super) fn summary(&mut self, detail: &str) {
        self.record.details.detail = Some(redact(detail));
    }

    pub(super) fn add_summary(&mut self, detail: &str) {
        let previous = self
            .record
            .details
            .detail
            .as_deref()
            .map(|s| s.as_str())
            .unwrap_or_default();
        self.summary(&format!("{previous}; {detail}"));
    }

    pub(super) fn start(
        &mut self,
        store: &mut Result<AuditStore, AuditError>,
        health: &Arc<Mutex<HealthState>>,
    ) {
        // These records belong to the accepted service request, independent of its caller's future.
        if !self.append(store, health) {
            if let Some(operation) = &mut self.record.details.operation {
                operation.capture = Some(AuditCapture::Partial);
            }
        }
    }

    pub(super) fn finish(
        mut self,
        store: &mut Result<AuditStore, AuditError>,
        health: &Arc<Mutex<HealthState>>,
        outcome: AuditOutcome,
        error: Option<AuditError>,
        bytes: Option<u64>,
    ) {
        if let Some(error) = error {
            self.add_summary(&format!("error={error}"));
        }
        self.record
            .finish_operation(self.started, outcome, AuditEvidence::Protocol, None, bytes);
        self.append(store, health);
    }

    fn append(
        &self,
        store: &mut Result<AuditStore, AuditError>,
        health: &Arc<Mutex<HealthState>>,
    ) -> bool {
        let result = store
            .as_mut()
            .map_err(|e| *e)
            .and_then(|store| store.append(&self.record));
        let mut health = health.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(true) => {
                health.captured();
                true
            }
            Ok(false) => false,
            Err(error) => {
                health.missed(error, 1);
                false
            }
        }
    }

    pub(super) fn export_completed(
        self,
        result: Result<u64, AuditError>,
        reply: Sender<Result<u64, AuditError>>,
    ) {
        let sender = self.client.sender.clone();
        // The reader can wait for queue capacity; the writer never waits on a live reader request.
        // Owner shutdown closes this sender, waking a pending completion immediately.
        let _ = sender.send_blocking(Request::ExportCompleted(self, result, reply));
    }
}

pub(super) fn export_scope(query: &AuditQuery, format: AuditExportFormat, details: bool) -> String {
    serde_json::json!({
        "format": format, "include_details": details, "category": query.category,
        "severity": query.severity, "before_sequence": query.before_sequence,
        "after_ms": query.after_ms, "until_ms": query.until_ms,
        "search": query.search.as_deref().map(|s| redact(s)), "operation_id": query.operation_id,
        "session_id": query.session_id, "source": query.source, "outcome": query.outcome,
        "protocol": query.protocol, "connection_id": query.connection_id.as_deref().map(|s| redact(s)),
        "remote_account": query.remote_account.as_deref().map(|s| redact(s)),
        "local_account": query.local_account.as_deref().map(|s| redact(s)),
        "parent_id": query.parent_id,
    })
    .to_string()
}
