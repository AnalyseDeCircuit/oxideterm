use std::{collections::VecDeque, time::SystemTime};

use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    calls::{PublicToolCall, ToolOutcome},
    handles::{AuditRef, ClientRef},
};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAuthorization {
    NotRequired,
    AppApproval,
    Unattended,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditRecord {
    pub audit_ref: AuditRef,
    pub client_ref: ClientRef,
    pub tool_name: String,
    pub target_digest: String,
    pub authorization: AuditAuthorization,
    pub outcome: ToolOutcome,
    pub created_at_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditProjection {
    pub audit_ref: AuditRef,
    pub tool_name: String,
    pub target_digest: String,
    pub authorization: AuditAuthorization,
    pub outcome: ToolOutcome,
    pub created_at_ms: u128,
}

#[derive(Debug, Clone, Default)]
pub struct AuditQuery<'a> {
    pub after_ms: Option<u128>,
    pub before_ms: Option<u128>,
    pub tool_name: Option<&'a str>,
    pub target: Option<&'a str>,
    pub cursor: Option<&'a AuditRef>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditPage {
    pub records: Vec<AuditProjection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<AuditRef>,
}

pub struct AuditStore {
    capacity: usize,
    records: Mutex<VecDeque<AuditRecord>>,
}

impl AuditStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            records: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
        }
    }

    /// Records only the target digest, never command text or secret-bearing arguments.
    pub fn record(
        &self,
        client_ref: ClientRef,
        call: &PublicToolCall,
        authorization: AuditAuthorization,
        outcome: ToolOutcome,
    ) -> AuditRecord {
        self.record_fields(
            client_ref,
            call.tool_name(),
            &call.target_summary(),
            authorization,
            outcome,
        )
    }

    pub fn record_fields(
        &self,
        client_ref: ClientRef,
        tool_name: impl Into<String>,
        target: &str,
        authorization: AuditAuthorization,
        outcome: ToolOutcome,
    ) -> AuditRecord {
        let target_digest = hex_digest(target.as_bytes());
        let record = AuditRecord {
            audit_ref: AuditRef::new(),
            client_ref,
            tool_name: tool_name.into(),
            target_digest,
            authorization,
            outcome,
            created_at_ms: unix_time_ms(),
        };
        let mut records = self.records.lock();
        if records.len() == self.capacity {
            records.pop_front();
        }
        records.push_back(record.clone());
        record
    }

    pub fn list(&self) -> Vec<AuditRecord> {
        self.records.lock().iter().cloned().collect()
    }

    /// Returns only records owned by the authenticated client and projects away its identity.
    pub fn search(&self, client_ref: &ClientRef, query: AuditQuery<'_>) -> AuditPage {
        let target_digest = query.target.map(|target| hex_digest(target.as_bytes()));
        let limit = query.limit.clamp(1, 200);
        let records = self.records.lock();
        let mut passed_cursor = query.cursor.is_none();
        let mut matching = Vec::with_capacity(limit.saturating_add(1));
        for record in records.iter().rev() {
            if !passed_cursor {
                if query.cursor == Some(&record.audit_ref) {
                    passed_cursor = true;
                }
                continue;
            }
            if &record.client_ref != client_ref
                || query
                    .after_ms
                    .is_some_and(|after| record.created_at_ms < after)
                || query
                    .before_ms
                    .is_some_and(|before| record.created_at_ms > before)
                || query
                    .tool_name
                    .is_some_and(|tool_name| record.tool_name != tool_name)
                || target_digest
                    .as_ref()
                    .is_some_and(|digest| &record.target_digest != digest)
            {
                continue;
            }
            matching.push(AuditProjection {
                audit_ref: record.audit_ref.clone(),
                tool_name: record.tool_name.clone(),
                target_digest: record.target_digest.clone(),
                authorization: record.authorization,
                outcome: record.outcome.clone(),
                created_at_ms: record.created_at_ms,
            });
            if matching.len() > limit {
                break;
            }
        }
        let has_more = matching.len() > limit;
        matching.truncate(limit);
        let next_cursor = has_more
            .then(|| matching.last().map(|record| record.audit_ref.clone()))
            .flatten();
        AuditPage {
            records: matching,
            next_cursor,
        }
    }
}

fn hex_digest(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn unix_time_ms() -> u128 {
    SystemTime::UNIX_EPOCH
        .elapsed()
        .map_or(0, |elapsed| elapsed.as_millis())
}
