// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime},
};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const GRACE_PERIOD: Duration = Duration::from_secs(30);
pub const PROACTIVE_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(5);
pub const WEBSOCKET_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
pub const WEBSOCKET_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(300);
pub const SSH_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);
pub const MAX_RETAINED_RECONNECT_JOBS: usize = 200;

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
    pub forward_rules: Vec<ReconnectForwardRuleSnapshot>,
    /// Legacy summary retained for existing diagnostics. Reconnect restore uses
    /// `forward_rules`, matching Tauri's rule snapshot semantics.
    pub active_port_forward_ids: Vec<String>,
    pub inflight_sftp_transfer_ids: Vec<String>,
    pub incomplete_sftp_transfers_by_node: Vec<ReconnectNodeTransferSnapshot>,
    pub ide_snapshot: Option<ReconnectIdeSnapshot>,
    /// Tauri keeps oldConnectionIds as a nodeId -> connectionId map. Native
    /// retains the legacy flat list below for diagnostics, but reconnect
    /// grace-period recovery uses this node-scoped map.
    pub old_connections_by_node: Vec<ReconnectNodeConnectionSnapshot>,
    pub old_connection_ids: Vec<String>,
    pub snapshot_at: Option<SystemTime>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectIdeSnapshot {
    pub project_path: String,
    pub tab_paths: Vec<String>,
    /// Tauri stores nodeId in the ideSnapshot.connectionId slot during
    /// reconnect; keep the field name for parity with its restore phase.
    pub connection_id: String,
    pub dirty_contents: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectNodeTerminalSnapshot {
    pub node_id: String,
    pub old_terminal_session_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectForwardRuleSnapshot {
    pub node_id: String,
    pub rules: Vec<ReconnectForwardRule>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectForwardRule {
    pub id: String,
    pub forward_type: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub status: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectNodeTransferSnapshot {
    pub node_id: String,
    pub transfer_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectNodeConnectionSnapshot {
    pub node_id: String,
    pub old_connection_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconnectTiming {
    pub grace_period: Duration,
    pub retry_base_delay: Duration,
    pub retry_max_delay: Duration,
    pub proactive_keepalive_timeout: Duration,
    pub websocket_heartbeat_interval: Duration,
    pub websocket_heartbeat_timeout: Duration,
    pub ssh_keepalive_interval: Duration,
}

impl Default for ReconnectTiming {
    fn default() -> Self {
        Self {
            grace_period: GRACE_PERIOD,
            retry_base_delay: Duration::from_secs(1),
            retry_max_delay: Duration::from_secs(15),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconnectRetry {
    pub attempt: u32,
    pub max_attempts: u32,
    pub delay: Duration,
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
            max_attempts: max_attempts.max(1),
        }
    }

    pub fn configure(&mut self, timing: ReconnectTiming, max_attempts: u32) {
        self.timing = timing;
        self.max_attempts = max_attempts.max(1);
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
            attempt: 0,
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

    pub fn cleanup_terminal_job(&self, node_id: &str, started_at: SystemTime) -> bool {
        let should_remove = self
            .jobs
            .get(node_id)
            .is_some_and(|job| is_terminal_phase(&job.status) && job.started_at == started_at);
        if should_remove {
            self.jobs.remove(node_id);
        }
        should_remove
    }

    pub fn enforce_terminal_job_cap(&self, max_retained: usize) {
        let mut terminal_jobs = self
            .jobs
            .iter()
            .filter(|entry| is_terminal_phase(&entry.status))
            .map(|entry| {
                (
                    entry.key().clone(),
                    entry.ended_at.unwrap_or(entry.started_at),
                    entry.started_at,
                )
            })
            .collect::<Vec<_>>();
        if terminal_jobs.len() <= max_retained {
            return;
        }
        terminal_jobs.sort_by_key(|(_, ended_at, started_at)| (*ended_at, *started_at));
        let to_remove = terminal_jobs.len() - max_retained;
        for (node_id, _, _) in terminal_jobs.into_iter().take(to_remove) {
            self.jobs.remove(&node_id);
        }
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

    pub fn retry_delay_for_attempt(&self, attempt: u32) -> Duration {
        retry_delay(self.timing, attempt)
    }

    pub fn begin_ssh_attempt(&self, node_id: &str) -> Option<ReconnectJob> {
        let mut job = self.jobs.get_mut(node_id)?;
        if job.ended_at.is_some() {
            return None;
        }
        job.attempt = job.attempt.saturating_add(1).min(job.max_attempts);
        Some(job.clone())
    }

    pub fn schedule_retry(&self, node_id: &str) -> Option<ReconnectRetry> {
        let mut job = self.jobs.get_mut(node_id)?;
        if job.ended_at.is_some() || job.attempt >= job.max_attempts {
            return None;
        }
        let next_attempt = job.attempt + 1;
        let max_attempts = job.max_attempts;
        let delay = retry_delay(self.timing, job.attempt);
        job.status = ReconnectPhase::Queued;
        Self::push_phase(
            &mut job,
            ReconnectPhase::Queued,
            PhaseResult::Running,
            Some(format!(
                "retry {next_attempt}/{max_attempts} in {:?}",
                delay
            )),
        );
        Some(ReconnectRetry {
            attempt: next_attempt,
            max_attempts,
            delay,
        })
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

fn is_terminal_phase(phase: &ReconnectPhase) -> bool {
    matches!(
        phase,
        ReconnectPhase::Done | ReconnectPhase::Failed | ReconnectPhase::Cancelled
    )
}

fn retry_delay(timing: ReconnectTiming, attempt: u32) -> Duration {
    retry_delay_with_jitter(timing, attempt, retry_jitter_factor())
}

fn retry_delay_with_jitter(timing: ReconnectTiming, attempt: u32, jitter_factor: f64) -> Duration {
    const BACKOFF_MULTIPLIER: f64 = 1.5;
    let exponent = attempt.saturating_sub(1) as i32;
    let base_ms = timing.retry_base_delay.as_millis() as f64;
    let max_ms = timing.retry_max_delay.as_millis() as f64;
    let delay_ms = (base_ms * BACKOFF_MULTIPLIER.powi(exponent)).min(max_ms);
    Duration::from_millis((delay_ms * jitter_factor).round().max(1.0) as u64)
}

fn retry_jitter_factor() -> f64 {
    let sample = (Uuid::new_v4().as_u128() & 0xffff) as f64 / 65_535.0;
    0.8 + sample * 0.4
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

    #[test]
    fn retry_policy_uses_configured_attempts_and_delays() {
        let store = ReconnectOrchestratorStore::new(
            ReconnectTiming {
                retry_base_delay: Duration::from_millis(500),
                retry_max_delay: Duration::from_secs(2),
                ..ReconnectTiming::default()
            },
            4,
        );
        store.schedule("node-a", "Node A", ReconnectSnapshot::default());

        let attempt_1 = store.begin_ssh_attempt("node-a").unwrap();
        assert_eq!(attempt_1.attempt, 1);

        let retry_2 = store.schedule_retry("node-a").unwrap();
        assert_eq!(retry_2.attempt, 2);
        assert_eq!(retry_2.max_attempts, 4);
        assert!(retry_2.delay >= Duration::from_millis(400));
        assert!(retry_2.delay <= Duration::from_millis(600));

        let attempt_2 = store.begin_ssh_attempt("node-a").unwrap();
        assert_eq!(attempt_2.attempt, 2);
        let retry_3 = store.schedule_retry("node-a").unwrap();
        assert!(retry_3.delay >= Duration::from_millis(600));
        assert!(retry_3.delay <= Duration::from_millis(900));

        let attempt_3 = store.begin_ssh_attempt("node-a").unwrap();
        assert_eq!(attempt_3.attempt, 3);
        let retry_4 = store.schedule_retry("node-a").unwrap();
        assert!(retry_4.delay >= Duration::from_millis(900));
        assert!(retry_4.delay <= Duration::from_millis(1350));

        let attempt_4 = store.begin_ssh_attempt("node-a").unwrap();
        assert_eq!(attempt_4.attempt, 4);

        assert!(store.schedule_retry("node-a").is_none());
    }

    #[test]
    fn retry_delay_matches_tauri_backoff_without_jitter() {
        let timing = ReconnectTiming {
            retry_base_delay: Duration::from_millis(1_000),
            retry_max_delay: Duration::from_millis(15_000),
            ..ReconnectTiming::default()
        };

        assert_eq!(
            retry_delay_with_jitter(timing, 1, 1.0),
            Duration::from_millis(1_000)
        );
        assert_eq!(
            retry_delay_with_jitter(timing, 2, 1.0),
            Duration::from_millis(1_500)
        );
        assert_eq!(
            retry_delay_with_jitter(timing, 3, 1.0),
            Duration::from_millis(2_250)
        );
    }

    #[test]
    fn cleanup_terminal_job_removes_only_matching_finished_job() {
        let store = ReconnectOrchestratorStore::default();
        let job = store.schedule("node-a", "Node A", ReconnectSnapshot::default());
        assert!(!store.cleanup_terminal_job("node-a", job.started_at));

        store.finish("node-a", Ok(0)).unwrap();
        assert!(!store.cleanup_terminal_job("node-a", job.started_at + Duration::from_secs(1)));
        assert_eq!(store.jobs().len(), 1);
        assert!(store.cleanup_terminal_job("node-a", job.started_at));
        assert!(store.jobs().is_empty());
    }

    #[test]
    fn reconnect_snapshot_carries_ide_dirty_contents() {
        let mut dirty_contents = BTreeMap::new();
        dirty_contents.insert("/home/demo/main.rs".to_string(), "dirty".to_string());
        let snapshot = ReconnectSnapshot {
            ide_snapshot: Some(ReconnectIdeSnapshot {
                project_path: "/home/demo".to_string(),
                tab_paths: vec!["/home/demo/main.rs".to_string()],
                connection_id: "node-a".to_string(),
                dirty_contents,
            }),
            ..ReconnectSnapshot::default()
        };

        let ide_snapshot = snapshot.ide_snapshot.expect("IDE snapshot should exist");
        assert_eq!(ide_snapshot.connection_id, "node-a");
        assert_eq!(
            ide_snapshot.dirty_contents.get("/home/demo/main.rs"),
            Some(&"dirty".to_string())
        );
    }

    #[test]
    fn reconnect_snapshot_carries_forward_rules_like_tauri() {
        let snapshot = ReconnectSnapshot {
            forward_rules: vec![ReconnectForwardRuleSnapshot {
                node_id: "node-a".to_string(),
                rules: vec![ReconnectForwardRule {
                    id: "forward-1".to_string(),
                    forward_type: "local".to_string(),
                    bind_address: "localhost".to_string(),
                    bind_port: 8080,
                    target_host: "localhost".to_string(),
                    target_port: 8080,
                    status: "active".to_string(),
                    description: "web".to_string(),
                }],
            }],
            ..ReconnectSnapshot::default()
        };

        assert_eq!(snapshot.forward_rules[0].node_id, "node-a");
        assert_eq!(snapshot.forward_rules[0].rules[0].forward_type, "local");
        assert_eq!(snapshot.forward_rules[0].rules[0].status, "active");
    }
}
