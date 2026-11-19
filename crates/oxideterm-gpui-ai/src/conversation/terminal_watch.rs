use std::time::{Duration, Instant};

use gpui::{AsyncApp, Timer};
use oxideterm_ai::{
    AiExecutedToolResult,
    agent::{AgentDispatch, AgentToolLease},
    terminal_observation::{AiTerminalCommandWait, ai_terminal_command_output_ready},
    tool_resources::{
        AiOrchestratorRuntimeSnapshot, AiOrchestratorTarget, AiTerminalCommand,
        AiTerminalCommandStatus, AiTerminalOutputWait, AiTerminalRunEnd, AiTerminalWaitFailure,
        AiTerminalWaitProgress, AiTerminalWaitSample,
    },
};
use tokio::sync::{oneshot, watch};
use zeroize::Zeroizing;

use crate::state::ai_now_ms;

const SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

pub struct AiTerminalWatchRecord {
    pub id: String,
    pub command: Zeroizing<String>,
    pub status: AiTerminalCommandStatus,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub exit_code: Option<i32>,
}

impl AiTerminalWatchRecord {
    fn as_command(&self) -> AiTerminalCommand<'_> {
        AiTerminalCommand {
            id: &self.id,
            command: &self.command,
            status: self.status,
            started_at: self.started_at,
            finished_at: self.finished_at,
            exit_code: self.exit_code,
        }
    }
}

pub enum AiTerminalWatchSample {
    HostClosed,
    OwnerClosed,
    Recovering,
    Current {
        buffer: Zeroizing<String>,
        alternate_screen: bool,
        waiting_for_secret: bool,
        prompt_returned: bool,
        records: Vec<AiTerminalWatchRecord>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AiTerminalWatchStatus {
    Connection,
    User,
    Condition,
}

impl AiTerminalWatchStatus {
    pub fn protocol_status(self) -> &'static str {
        match self {
            Self::Connection => "waiting_connection",
            Self::User => "waiting_user",
            Self::Condition => "waiting_condition",
        }
    }

    pub fn agent_state(self) -> oxideterm_ai::agent::AgentState {
        use oxideterm_ai::agent::AgentState;
        match self {
            Self::Connection => AgentState::AwaitingConnection,
            Self::User => AgentState::AwaitingUser,
            Self::Condition => AgentState::AwaitingCondition,
        }
    }
}

pub struct AiTerminalWatch {
    pub call_id: String,
    pub tool_name: String,
    pub started: Instant,
    pub sender: oneshot::Sender<AiExecutedToolResult>,
    pub activity: watch::Receiver<u64>,
    pub direction: Option<AgentDispatch>,
}

pub struct AiTerminalCommandObservation {
    pub command_id: Option<String>,
    pub before: Zeroizing<String>,
    pub timeout: Duration,
    pub leases: Vec<AgentToolLease>,
    pub target: Option<AiOrchestratorTarget>,
    pub post_user_approval: bool,
}

impl AiTerminalWatch {
    pub async fn observe_command(
        mut self,
        observation: AiTerminalCommandObservation,
        mut sample: impl FnMut(bool, Option<&str>, &mut AsyncApp) -> AiTerminalWatchSample,
        final_snapshot: impl FnOnce(&mut AsyncApp) -> Option<AiOrchestratorRuntimeSnapshot>,
        mut status_changed: impl FnMut(AiTerminalWatchStatus, i64, &mut AsyncApp),
        cx: &mut AsyncApp,
    ) {
        let AiTerminalCommandObservation {
            command_id,
            before,
            timeout,
            leases,
            target,
            post_user_approval,
        } = observation;
        let wait = AiTerminalCommandWait::with_timeout(Instant::now(), timeout);
        let mut previous_status = None;
        let mut last = before.clone();
        let mut changed_at = Instant::now();
        let mut connection_lost = false;
        let mut last_waiting_for_secret = false;
        let end = loop {
            if self.sender.is_closed() {
                return;
            }
            if self
                .direction
                .as_ref()
                .is_some_and(|guard| guard.check().is_err())
            {
                break AiTerminalRunEnd::DirectionChanged;
            }
            if leases.iter().any(|lease| !lease.is_current()) {
                break AiTerminalRunEnd::TakenOver;
            }
            let (current, waiting_for_secret, prompt_returned, record, recovering) =
                match sample(connection_lost, command_id.as_deref(), cx) {
                    AiTerminalWatchSample::HostClosed => break AiTerminalRunEnd::Unconfirmed,
                    AiTerminalWatchSample::OwnerClosed => break AiTerminalRunEnd::OwnerClosed,
                    AiTerminalWatchSample::Recovering => {
                        connection_lost = true;
                        (last.clone(), false, false, None, true)
                    }
                    AiTerminalWatchSample::Current {
                        buffer,
                        waiting_for_secret,
                        prompt_returned,
                        records,
                        ..
                    } => (
                        buffer,
                        waiting_for_secret,
                        prompt_returned,
                        records.into_iter().next(),
                        false,
                    ),
                };
            if *current != *last {
                last = current.clone();
                changed_at = Instant::now();
            }
            let completed = record
                .as_ref()
                .is_some_and(|record| record.status == AiTerminalCommandStatus::Completed);
            if !recovering {
                if (connection_lost && record.is_none())
                    || record
                        .as_ref()
                        .is_some_and(|record| record.status == AiTerminalCommandStatus::Stale)
                {
                    break AiTerminalRunEnd::OutcomeUnknown;
                }
                connection_lost = false;
            }
            last_waiting_for_secret = waiting_for_secret;
            let expired = wait.expired(Instant::now());
            if recovering && expired {
                break AiTerminalRunEnd::OutcomeUnknown;
            }
            if expired
                || ai_terminal_command_output_ready(
                    completed,
                    prompt_returned,
                    &before,
                    &current,
                    changed_at.elapsed(),
                    waiting_for_secret,
                    recovering,
                )
            {
                if let Some(snapshot) = final_snapshot(cx) {
                    let action = snapshot.terminal_run_observed(
                        command_id.as_deref(),
                        &before,
                        &current,
                        completed,
                        prompt_returned,
                        waiting_for_secret,
                        record.and_then(|record| record.exit_code),
                        expired,
                        target,
                    );
                    self.send(&snapshot, action);
                }
                return;
            }
            let status = if recovering {
                AiTerminalWatchStatus::Connection
            } else if waiting_for_secret {
                AiTerminalWatchStatus::User
            } else {
                AiTerminalWatchStatus::Condition
            };
            if previous_status != Some(status) {
                status_changed(status, deadline_ms(wait.deadline()), cx);
                previous_status = Some(status);
            }
            if !self.next_sample().await {
                return;
            }
        };
        if let Some(snapshot) = final_snapshot(cx) {
            let result = snapshot.terminal_run_finished(
                self.call_id,
                self.tool_name,
                self.started.elapsed().as_millis(),
                end,
                &before,
                &last,
                command_id,
                last_waiting_for_secret,
                target,
                post_user_approval,
            );
            let _ = self.sender.send(result);
        }
    }

    pub async fn observe_output(
        mut self,
        mut wait: AiTerminalOutputWait<'_>,
        snapshot: AiOrchestratorRuntimeSnapshot,
        mut sample: impl FnMut(bool, Option<&str>, &mut AsyncApp) -> AiTerminalWatchSample,
        final_snapshot: impl FnOnce(&mut AsyncApp) -> Option<AiOrchestratorRuntimeSnapshot>,
        mut status_changed: impl FnMut(AiTerminalWatchStatus, i64, &mut AsyncApp),
        cx: &mut AsyncApp,
    ) {
        let mut previous_status = None;
        let failure = loop {
            if self
                .direction
                .as_ref()
                .is_some_and(|guard| guard.check().is_err())
            {
                break AiTerminalWaitFailure::DirectionChanged;
            }
            if self.sender.is_closed() {
                return;
            }
            let observation = sample(wait.connection_lost(), wait.requested_command(), cx);
            let status = match observation {
                AiTerminalWatchSample::HostClosed | AiTerminalWatchSample::OwnerClosed => {
                    break AiTerminalWaitFailure::OwnerClosed;
                }
                AiTerminalWatchSample::Recovering => AiTerminalWatchStatus::Connection,
                AiTerminalWatchSample::Current { .. } => AiTerminalWatchStatus::Condition,
            };
            if previous_status != Some(status) {
                status_changed(status, deadline_ms(wait.deadline()), cx);
                previous_status = Some(status);
            }
            let AiTerminalWatchSample::Current {
                buffer,
                alternate_screen,
                waiting_for_secret,
                prompt_returned,
                records,
            } = observation
            else {
                if !wait.recovering(Instant::now()) {
                    break AiTerminalWaitFailure::TimedOut;
                }
                Timer::after(SAMPLE_INTERVAL).await;
                continue;
            };
            let records = records
                .iter()
                .map(AiTerminalWatchRecord::as_command)
                .collect::<Vec<_>>();
            match wait.observe(
                &snapshot,
                AiTerminalWaitSample {
                    buffer: &buffer,
                    alternate_screen,
                    waiting_for_secret,
                    prompt_returned,
                },
                &records,
                Instant::now(),
            ) {
                AiTerminalWaitProgress::Pending => {}
                AiTerminalWaitProgress::Failed(failure) => break failure,
                AiTerminalWaitProgress::Observed(action) => {
                    if let Some(snapshot) = final_snapshot(cx) {
                        self.send(&snapshot, action);
                    }
                    return;
                }
            }
            if !self.next_sample().await {
                return;
            }
        };
        if let Some(snapshot) = final_snapshot(cx) {
            let action = wait.failure(&snapshot, failure);
            self.send(&snapshot, action);
        }
    }

    async fn next_sample(&mut self) -> bool {
        tokio::select! {
            _ = self.activity.changed() => true,
            _ = Timer::after(SAMPLE_INTERVAL) => true,
            _ = self.sender.closed() => false,
        }
    }

    fn send(
        self,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        result: oxideterm_ai::tool_resources::AiActionResultLite,
    ) {
        let result = snapshot.to_executed_tool_result(
            self.call_id,
            self.tool_name,
            result,
            self.started.elapsed().as_millis(),
        );
        let _ = self.sender.send(result);
    }
}

fn deadline_ms(deadline: Instant) -> i64 {
    ai_now_ms()
        + deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as i64
}

/// Waits for the saved connection without owning its transport or workspace.
pub fn wait_for_connected_target(
    base: AiExecutedToolResult,
    initial: Option<AiExecutedToolResult>,
    sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
    started: std::time::Instant,
    cx: &mut gpui::App,
    mut sample: impl FnMut(
        Option<&AiExecutedToolResult>,
        u128,
        &mut gpui::AsyncApp,
    ) -> Option<Option<AiExecutedToolResult>>
    + 'static,
) {
    const MAX_SAMPLES: usize = 900;
    const SAMPLE_INTERVAL: Duration = Duration::from_millis(100);

    if !connection_needs_wait(&base) {
        let _ = sender.send(base);
        return;
    }
    if let Some(ready) = initial {
        let _ = sender.send(ready);
        return;
    }
    cx.spawn(async move |cx| {
        for _ in 0..MAX_SAMPLES {
            gpui::Timer::after(SAMPLE_INTERVAL).await;
            match sample(None, started.elapsed().as_millis(), cx) {
                Some(Some(result)) => {
                    let _ = sender.send(result);
                    return;
                }
                Some(None) => {}
                None => break,
            }
        }
        let result = sample(Some(&base), started.elapsed().as_millis(), cx)
            .flatten()
            .unwrap_or(base);
        let _ = sender.send(result);
    })
    .detach();
}

impl crate::state::AiWorkspaceEntity {
    pub fn terminal_watch_status(
        &self,
        generation: u64,
        command_id: Option<&str>,
        status: AiTerminalWatchStatus,
        deadline_ms: i64,
        run: bool,
    ) -> (serde_json::Value, &'static str) {
        let mut data = serde_json::json!({"waiting": true, "waitDeadline": deadline_ms});
        if run {
            if let Some(agent) = self.agent_run(generation) {
                let _ = self
                    .agents
                    .services
                    .runtime
                    .set_state(&agent, status.agent_state());
            }
            data["commandId"] = serde_json::json!(command_id);
        }
        (data, if run { "interactive" } else { "read" })
    }
}

pub enum AiCommandMonitorSample {
    Recovering,
    Unavailable,
    Current(Option<AiTerminalCommandStatus>),
}

pub struct AiCommandEvidence {
    pub buffer: Zeroizing<String>,
    pub exit_code: Option<i32>,
}

impl crate::state::AiWorkspaceEntity {
    pub fn monitor_terminal_command(
        &mut self,
        before: &str,
        leases: Vec<AgentToolLease>,
        record: Option<oxideterm_ai::agent::AgentResourceRecord>,
        sample: impl Fn(&mut gpui::App) -> AiCommandMonitorSample + Clone + 'static,
        evidence: impl Fn(bool, &mut gpui::App) -> Option<AiCommandEvidence> + Clone + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        use oxideterm_ai::agent::OwnedResourceState;
        for lease in leases {
            let before = Zeroizing::new(before.to_owned());
            let record = record.clone();
            let key = lease.lease().resource.clone();
            let monitor_key = key.clone();
            let resources = self.agents.services.resources.clone();
            let sample = sample.clone();
            let evidence = evidence.clone();
            let task = cx.spawn(async move |weak, cx| {
                let finish_record = |require_closed, cx: &mut gpui::App| {
                    let Some(record) = &record else { return };
                    if let Some(evidence) = evidence(require_closed, cx) {
                        let output = oxideterm_ai::terminal_observation::terminal_delta_output(
                            &before,
                            &evidence.buffer,
                        );
                        record.outcome(
                            &serde_json::json!({"exitCode":evidence.exit_code,"output":output})
                                .to_string(),
                        );
                        record.finish(OwnedResourceState::Completed);
                    } else {
                        record.finish(OwnedResourceState::OutcomeUnknown);
                    }
                };
                loop {
                    Timer::after(SAMPLE_INTERVAL).await;
                    if weak.upgrade().is_none() {
                        break;
                    }
                    let finished = cx.update(|cx| {
                        if !lease.is_current() {
                            // Preserve confirmed evidence without changing the next request's lease.
                            finish_record(true, cx);
                            return true;
                        }
                        match sample(cx) {
                            AiCommandMonitorSample::Recovering => false,
                            AiCommandMonitorSample::Unavailable => {
                                resources.invalidate(&monitor_key);
                                true
                            }
                            AiCommandMonitorSample::Current(status)
                                if lease.response_finished() =>
                            {
                                match status {
                                    Some(AiTerminalCommandStatus::Completed) => {
                                        lease.command_finished();
                                        finish_record(false, cx);
                                        true
                                    }
                                    None | Some(AiTerminalCommandStatus::Stale) => {
                                        if let Some(record) = &record {
                                            record.finish(OwnedResourceState::OutcomeUnknown);
                                        }
                                        resources.invalidate(&monitor_key);
                                        true
                                    }
                                    _ => false,
                                }
                            }
                            AiCommandMonitorSample::Current(_) => false,
                        }
                    });
                    if finished {
                        break;
                    }
                }
                let _ = weak.update(cx, |ai, _| {
                    ai.agents.command_monitors.remove(&monitor_key);
                });
            });
            self.agents.command_monitors.insert(key, task);
        }
    }
}

pub fn connection_needs_wait(result: &AiExecutedToolResult) -> bool {
    result.success
        && result
            .envelope
            .get("summary")
            .and_then(serde_json::Value::as_str)
            != Some("Target is already live.")
}
