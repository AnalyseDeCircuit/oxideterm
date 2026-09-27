use super::AuditHealth;
use crate::{
    AuditCapture, AuditCategory, AuditContext, AuditError, AuditEvidence, AuditOutcome, AuditPhase,
    AuditSeverity, AuditStore,
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(super) struct HealthState {
    pub(super) status: AuditHealth,
    pending: Option<CaptureGap>,
}

struct CaptureGap {
    first_ms: i64,
    last_ms: i64,
    count: u64,
    first_error: AuditError,
    last_error: AuditError,
}

impl HealthState {
    pub(super) fn missed(&mut self, error: AuditError, count: u64) {
        self.status.error = Some(error);
        self.status.unrecorded = self.status.unrecorded.saturating_add(count);
        if count == 0 {
            return;
        }
        let now = crate::model::now_ms();
        let gap = CaptureGap {
            first_ms: now,
            last_ms: now,
            count,
            first_error: error,
            last_error: error,
        };
        self.merge(gap);
    }

    pub(super) fn captured(&mut self) {
        self.status.revision += 1;
        if self.pending.is_none() {
            self.status.error = None;
        }
    }

    fn merge(&mut self, gap: CaptureGap) {
        match &mut self.pending {
            None => self.pending = Some(gap),
            Some(pending) => {
                if gap.first_ms < pending.first_ms {
                    pending.first_ms = gap.first_ms;
                    pending.first_error = gap.first_error;
                }
                if gap.last_ms >= pending.last_ms {
                    pending.last_ms = gap.last_ms;
                    pending.last_error = gap.last_error;
                }
                pending.count = pending.count.saturating_add(gap.count);
            }
        }
    }
}

pub(super) fn flush_gap(
    store: &mut Result<AuditStore, AuditError>,
    health: &Arc<Mutex<HealthState>>,
    context: &AuditContext,
) {
    let Ok(store) = store else {
        return;
    };
    let Some(gap) = health
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .pending
        .take()
    else {
        return;
    };
    let detail = serde_json::json!({
        "first_failure_ms": gap.first_ms, "last_failure_ms": gap.last_ms,
        "recovered_at_ms": crate::model::now_ms(), "unrecorded_records": gap.count,
        "first_error": gap.first_error.to_string(), "last_error": gap.last_error.to_string(),
    })
    .to_string();
    let mut record = context.event_record(AuditCategory::Audit, "audit_capture_gap", Some(&detail));
    record.severity = AuditSeverity::Warning;
    if let Some(operation) = &mut record.details.operation {
        operation.phase = Some(AuditPhase::Observation);
        operation.outcome = AuditOutcome::Partial;
        operation.evidence = AuditEvidence::Lifecycle;
        operation.capture = Some(AuditCapture::Partial);
    }
    let result = store.append(&record);
    let mut health = health.lock().unwrap_or_else(|e| e.into_inner());
    match result {
        Ok(true) => health.captured(),
        Ok(false) => health.merge(gap),
        Err(error) => {
            // Retrying the summary must not recursively count itself as another lost event.
            health.status.error = Some(error);
            health.merge(gap);
        }
    }
}
