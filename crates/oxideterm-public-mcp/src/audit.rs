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
