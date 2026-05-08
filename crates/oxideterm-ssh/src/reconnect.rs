// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, SystemTime};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const GRACE_PERIOD: Duration = Duration::from_secs(30);
pub const PROACTIVE_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(2);
pub const WEBSOCKET_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
pub const WEBSOCKET_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(300);
pub const SSH_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReconnectPhase {
    Queued,
    Snapshot,
    GracePeriod,
    SshConnect,
    AwaitTerminal,
    RestoreForwards,
    ResumeTransfers,
    RestoreIde,
    Verify,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseResult {
    Running,
    Ok,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseEvent {
    pub phase: ReconnectPhase,
    pub started_at: SystemTime,
    pub ended_at: Option<SystemTime>,
    pub result: PhaseResult,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectSnapshot {
    pub node_id: String,
    pub terminal_pane_ids: Vec<String>,
    pub old_terminal_session_ids: Vec<String>,
    pub terminal_sessions_by_node: Vec<ReconnectNodeTerminalSnapshot>,
    pub active_port_forward_ids: Vec<String>,
    pub inflight_sftp_transfer_ids: Vec<String>,
    pub incomplete_sftp_transfers_by_node: Vec<ReconnectNodeTransferSnapshot>,
    pub open_ide_file_paths: Vec<String>,
    pub old_connection_ids: Vec<String>,
    pub snapshot_at: Option<SystemTime>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectNodeTerminalSnapshot {
    pub node_id: String,
    pub old_terminal_session_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectNodeTransferSnapshot {
    pub node_id: String,
    pub transfer_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconnectTiming {
    pub grace_period: Duration,
    pub proactive_keepalive_timeout: Duration,
    pub websocket_heartbeat_interval: Duration,
    pub websocket_heartbeat_timeout: Duration,
    pub ssh_keepalive_interval: Duration,
}

impl Default for ReconnectTiming {
    fn default() -> Self {
        Self {
            grace_period: GRACE_PERIOD,
            proactive_keepalive_timeout: PROACTIVE_KEEPALIVE_TIMEOUT,
            websocket_heartbeat_interval: WEBSOCKET_HEARTBEAT_INTERVAL,
            websocket_heartbeat_timeout: WEBSOCKET_HEARTBEAT_TIMEOUT,
            ssh_keepalive_interval: SSH_KEEPALIVE_INTERVAL,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectJob {
    pub job_id: String,
    pub node_id: String,
    pub node_name: String,
    pub status: ReconnectPhase,
    pub attempt: u32,
    pub max_attempts: u32,
    pub started_at: SystemTime,
    pub ended_at: Option<SystemTime>,
    pub error: Option<String>,
    pub snapshot: ReconnectSnapshot,
    pub restored_count: u32,
    pub phase_history: Vec<PhaseEvent>,
}

#[derive(Debug)]
pub struct ReconnectOrchestratorStore {
    jobs: DashMap<String, ReconnectJob>,
    timing: ReconnectTiming,
    max_attempts: u32,
}

impl ReconnectOrchestratorStore {
    pub fn new(timing: ReconnectTiming, max_attempts: u32) -> Self {
        Self {
            jobs: DashMap::new(),
            timing,
            max_attempts,
        }
    }

    pub fn schedule(
        &self,
        node_id: impl Into<String>,
        node_name: impl Into<String>,
        mut snapshot: ReconnectSnapshot,
    ) -> ReconnectJob {
        let node_id = node_id.into();
        snapshot.node_id = node_id.clone();
        snapshot.snapshot_at = Some(SystemTime::now());

        if let Some(job) = self.jobs.get(&node_id) {
            if job.ended_at.is_none() {
                return job.clone();
            }
            drop(job);
            self.jobs.remove(&node_id);
        }

        let mut job = ReconnectJob {
            job_id: Uuid::new_v4().to_string(),
            node_id: node_id.clone(),
            node_name: node_name.into(),
            status: ReconnectPhase::Queued,
            attempt: 1,
            max_attempts: self.max_attempts,
            started_at: SystemTime::now(),
            ended_at: None,
            error: None,
            snapshot,
            restored_count: 0,
            phase_history: Vec::new(),
        };
        Self::push_phase(&mut job, ReconnectPhase::Queued, PhaseResult::Running, None);
        self.jobs.insert(node_id, job.clone());
        job
    }

    pub fn advance(&self, node_id: &str, phase: ReconnectPhase) -> Option<ReconnectJob> {
        let mut job = self.jobs.get_mut(node_id)?;
        job.status = phase.clone();
        Self::push_phase(&mut job, phase, PhaseResult::Running, None);
        Some(job.clone())
    }

    pub fn complete_phase(
        &self,
        node_id: &str,
        result: PhaseResult,
        detail: Option<String>,
    ) -> Option<ReconnectJob> {
        let mut job = self.jobs.get_mut(node_id)?;
        if let Some(event) = job.phase_history.last_mut() {
            event.ended_at = Some(SystemTime::now());
            event.result = result;
            event.detail = detail;
        }
        Some(job.clone())
    }

    pub fn finish(&self, node_id: &str, result: Result<u32, String>) -> Option<ReconnectJob> {
        let mut job = self.jobs.get_mut(node_id)?;
        job.ended_at = Some(SystemTime::now());
        match result {
            Ok(restored_count) => {
                job.status = ReconnectPhase::Done;
                job.restored_count = restored_count;
                Self::push_phase(&mut job, ReconnectPhase::Done, PhaseResult::Ok, None);
            }
            Err(error) => {
                job.status = ReconnectPhase::Failed;
                job.error = Some(error.clone());
                Self::push_phase(
                    &mut job,
                    ReconnectPhase::Failed,
                    PhaseResult::Failed,
                    Some(error),
                );
            }
        }
        Some(job.clone())
    }

    pub fn cancel(&self, node_id: &str) -> Option<ReconnectJob> {
        let mut job = self.jobs.get_mut(node_id)?;
        job.status = ReconnectPhase::Cancelled;
        job.ended_at = Some(SystemTime::now());
        Self::push_phase(&mut job, ReconnectPhase::Cancelled, PhaseResult::Ok, None);
        Some(job.clone())
    }

    pub fn job(&self, node_id: &str) -> Option<ReconnectJob> {
        self.jobs.get(node_id).map(|job| job.clone())
    }

    pub fn update_snapshot<F>(&self, node_id: &str, update: F) -> Option<ReconnectJob>
    where
        F: FnOnce(&mut ReconnectSnapshot),
    {
        let mut job = self.jobs.get_mut(node_id)?;
        update(&mut job.snapshot);
        Some(job.clone())
    }

    pub fn jobs(&self) -> Vec<ReconnectJob> {
        self.jobs.iter().map(|job| job.clone()).collect()
    }

    pub fn timing(&self) -> ReconnectTiming {
        self.timing
    }

    pub fn pipeline() -> [ReconnectPhase; 9] {
        [
            ReconnectPhase::Snapshot,
            ReconnectPhase::GracePeriod,
            ReconnectPhase::SshConnect,
            ReconnectPhase::AwaitTerminal,
            ReconnectPhase::RestoreForwards,
            ReconnectPhase::ResumeTransfers,
            ReconnectPhase::RestoreIde,
            ReconnectPhase::Verify,
            ReconnectPhase::Done,
        ]
    }

    fn push_phase(
        job: &mut ReconnectJob,
        phase: ReconnectPhase,
        result: PhaseResult,
        detail: Option<String>,
    ) {
        job.phase_history.push(PhaseEvent {
            phase,
            started_at: SystemTime::now(),
            ended_at: (result != PhaseResult::Running).then(SystemTime::now),
            result,
            detail,
        });
        if job.phase_history.len() > 64 {
            job.phase_history.remove(0);
        }
    }
}

impl Default for ReconnectOrchestratorStore {
    fn default() -> Self {
        Self::new(ReconnectTiming::default(), 5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_tauri_reconnect_pipeline_order() {
        assert_eq!(
            ReconnectOrchestratorStore::pipeline(),
            [
                ReconnectPhase::Snapshot,
                ReconnectPhase::GracePeriod,
                ReconnectPhase::SshConnect,
                ReconnectPhase::AwaitTerminal,
                ReconnectPhase::RestoreForwards,
                ReconnectPhase::ResumeTransfers,
                ReconnectPhase::RestoreIde,
                ReconnectPhase::Verify,
                ReconnectPhase::Done,
            ]
        );
    }

    #[test]
    fn schedule_is_idempotent_per_node() {
        let store = ReconnectOrchestratorStore::default();
        let first = store.schedule("node-a", "Node A", ReconnectSnapshot::default());
        let second = store.schedule("node-a", "Node A", ReconnectSnapshot::default());

        assert_eq!(first.job_id, second.job_id);
        assert_eq!(store.jobs().len(), 1);
    }
}
