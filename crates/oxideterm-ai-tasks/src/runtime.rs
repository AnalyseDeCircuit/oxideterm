// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use tokio::{sync::Notify, task::AbortHandle};
use zeroize::Zeroizing;

use crate::{
    BackgroundTaskCondition, BackgroundTaskEvent, BackgroundTaskExecution,
    BackgroundTaskExecutionResult, BackgroundTaskId, BackgroundTaskLimits, BackgroundTaskMode,
    BackgroundTaskSnapshot, BackgroundTaskSpec, BackgroundTaskState, BackgroundTaskValidationError,
    validate_background_task_spec,
};

const MAX_ACTIVE_TASKS_PER_OWNER: usize = 32;
const MAX_RETAINED_TASKS: usize = 128;

#[async_trait]
pub trait BackgroundTaskExecutor: Send + Sync + 'static {
    async fn execute(
        &self,
        execution: BackgroundTaskExecution,
    ) -> Result<BackgroundTaskExecutionResult, String>;
}

struct OwnedTask {
    snapshot: BackgroundTaskSnapshot,
    arguments_json: Zeroizing<String>,
    last_fingerprint: Option<String>,
    had_failure: bool,
    cancelled: Arc<AtomicBool>,
    cancellation: Arc<Notify>,
    abort_handle: Option<AbortHandle>,
}

#[derive(Clone)]
pub struct BackgroundTaskRuntime {
    inner: Arc<Mutex<HashMap<BackgroundTaskId, OwnedTask>>>,
    executor: Arc<dyn BackgroundTaskExecutor>,
    runtime: tokio::runtime::Handle,
    event_tx: tokio::sync::mpsc::UnboundedSender<BackgroundTaskEvent>,
    limits: BackgroundTaskLimits,
}

impl BackgroundTaskRuntime {
    pub fn new(
        executor: Arc<dyn BackgroundTaskExecutor>,
        runtime: tokio::runtime::Handle,
    ) -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<BackgroundTaskEvent>,
    ) {
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                inner: Arc::new(Mutex::new(HashMap::new())),
                executor,
                runtime,
                event_tx,
                limits: BackgroundTaskLimits::default(),
            },
            event_rx,
        )
    }

    pub fn create(
        &self,
        spec: BackgroundTaskSpec,
    ) -> Result<BackgroundTaskId, BackgroundTaskValidationError> {
        validate_background_task_spec(&spec, self.limits)?;
        let task_id = BackgroundTaskId::new();
        let now = Utc::now();
        let normalized_title = spec.normalized_title();
        let snapshot = BackgroundTaskSnapshot {
            id: task_id.clone(),
            owner: spec.owner,
            title: normalized_title,
            tool_name: spec.tool_name,
            mode: spec.mode,
            state: BackgroundTaskState::Queued,
            run_count: 0,
            last_summary: None,
            last_error_code: None,
            created_at: now,
            updated_at: now,
        };
        let cancellation = Arc::new(Notify::new());
        let cancelled = Arc::new(AtomicBool::new(false));
        let removed_task = {
            let mut tasks = self.inner.lock();
            let active_for_owner = tasks
                .values()
                .filter(|task| {
                    task.snapshot.owner == snapshot.owner && !is_terminal_state(task.snapshot.state)
                })
                .count();
            if active_for_owner >= MAX_ACTIVE_TASKS_PER_OWNER {
                return Err(BackgroundTaskValidationError::TooManyActiveTasks);
            }
            let removed_task = if tasks.len() >= MAX_RETAINED_TASKS {
                let oldest_terminal = tasks
                    .iter()
                    .filter(|(_, task)| is_terminal_state(task.snapshot.state))
                    .min_by_key(|(_, task)| task.snapshot.updated_at)
                    .map(|(task_id, _)| task_id.clone())
                    .ok_or(BackgroundTaskValidationError::TaskCapacityReached)?;
                tasks.remove(&oldest_terminal);
                Some(oldest_terminal)
            } else {
                None
            };
            tasks.insert(
                task_id.clone(),
                OwnedTask {
                    snapshot: snapshot.clone(),
                    arguments_json: spec.arguments_json,
                    last_fingerprint: None,
                    had_failure: false,
                    cancelled: cancelled.clone(),
                    cancellation: cancellation.clone(),
                    abort_handle: None,
                },
            );
            removed_task
        };
        if let Some(removed_task) = removed_task {
            self.emit(BackgroundTaskEvent::Removed(removed_task));
        }
        self.emit(BackgroundTaskEvent::Changed(snapshot));

        let runtime = self.clone();
        let spawned_task_id = task_id.clone();
        let join_handle = self.runtime.spawn(async move {
            runtime.run_task(spawned_task_id).await;
        });
        if let Some(owned_task) = self.inner.lock().get_mut(&task_id) {
            owned_task.abort_handle = Some(join_handle.abort_handle());
        }
        Ok(task_id)
    }

    pub fn cancel(&self, task_id: &BackgroundTaskId) -> bool {
        let snapshot = {
            let mut tasks = self.inner.lock();
            let Some(task) = tasks.get_mut(task_id) else {
                return false;
            };
            if matches!(
                task.snapshot.state,
                BackgroundTaskState::Completed
                    | BackgroundTaskState::Failed
                    | BackgroundTaskState::Cancelled
            ) {
                return false;
            }
            task.cancelled.store(true, Ordering::Release);
            task.cancellation.notify_waiters();
            task.snapshot.state = BackgroundTaskState::Cancelled;
            task.snapshot.updated_at = Utc::now();
            task.snapshot.clone()
        };
        self.emit(BackgroundTaskEvent::Changed(snapshot));
        true
    }

    pub fn cancel_owner(&self, conversation_id: &str) -> usize {
        let task_ids = self
            .inner
            .lock()
            .iter()
            .filter(|(_, task)| task.snapshot.owner.conversation_id == conversation_id)
            .map(|(task_id, _)| task_id.clone())
            .collect::<Vec<_>>();
        task_ids
            .iter()
            .filter(|task_id| self.cancel(task_id))
            .count()
    }

    pub fn cancel_all(&self) -> usize {
        let task_ids = self.inner.lock().keys().cloned().collect::<Vec<_>>();
        task_ids
            .iter()
            .filter(|task_id| self.cancel(task_id))
            .count()
    }

    pub fn shutdown(&self) {
        let task_ids = self.inner.lock().keys().cloned().collect::<Vec<_>>();
        for task_id in task_ids {
            let _ = self.cancel(&task_id);
        }
        for task in self.inner.lock().values_mut() {
            if let Some(abort_handle) = task.abort_handle.take() {
                abort_handle.abort();
            }
        }
    }

    pub fn snapshot(&self, task_id: &BackgroundTaskId) -> Option<BackgroundTaskSnapshot> {
        self.inner
            .lock()
            .get(task_id)
            .map(|task| task.snapshot.clone())
    }

    pub fn snapshots_for_owner(&self, conversation_id: &str) -> Vec<BackgroundTaskSnapshot> {
        let mut snapshots = self
            .inner
            .lock()
            .values()
            .filter(|task| task.snapshot.owner.conversation_id == conversation_id)
            .map(|task| task.snapshot.clone())
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| snapshot.created_at);
        snapshots
    }

    async fn run_task(&self, task_id: BackgroundTaskId) {
        loop {
            let Some(execution) = self.prepare_execution(&task_id) else {
                break;
            };
            let result = self.executor.execute(execution).await;
            let decision = self.apply_execution_result(&task_id, result);
            match decision {
                NextRun::Stop => break,
                NextRun::Wait {
                    interval,
                    cancellation,
                } => {
                    tokio::select! {
                        _ = tokio::time::sleep(interval) => {}
                        _ = cancellation.notified() => break,
                    }
                }
            }
        }
    }

    fn prepare_execution(&self, task_id: &BackgroundTaskId) -> Option<BackgroundTaskExecution> {
        let (execution, snapshot) = {
            let mut tasks = self.inner.lock();
            let task = tasks.get_mut(task_id)?;
            if task.cancelled.load(Ordering::Acquire) {
                return None;
            }
            task.snapshot.state = BackgroundTaskState::Running;
            task.snapshot.run_count = task.snapshot.run_count.saturating_add(1);
            task.snapshot.updated_at = Utc::now();
            (
                BackgroundTaskExecution {
                    task_id: task_id.clone(),
                    tool_name: task.snapshot.tool_name.clone(),
                    arguments_json: Zeroizing::new(task.arguments_json.to_string()),
                    run_number: task.snapshot.run_count,
                },
                task.snapshot.clone(),
            )
        };
        self.emit(BackgroundTaskEvent::Changed(snapshot));
        Some(execution)
    }

    fn apply_execution_result(
        &self,
        task_id: &BackgroundTaskId,
        result: Result<BackgroundTaskExecutionResult, String>,
    ) -> NextRun {
        let (next, snapshot) = {
            let mut tasks = self.inner.lock();
            let Some(task) = tasks.get_mut(task_id) else {
                return NextRun::Stop;
            };
            if task.cancelled.load(Ordering::Acquire) {
                return NextRun::Stop;
            }

            let condition_matched = match result {
                Ok(result) => {
                    let matched = condition_matches(
                        &task.snapshot.mode,
                        task.last_fingerprint.as_deref(),
                        task.had_failure,
                        &result,
                    );
                    task.last_fingerprint = Some(result.fingerprint.clone());
                    task.had_failure = false;
                    task.snapshot.last_summary = Some(result.summary.clone());
                    task.snapshot.last_error_code = None;
                    matched
                }
                Err(mut error) => {
                    // Executor errors may originate from remote tools and must not outlive the run.
                    use zeroize::Zeroize;
                    error.zeroize();
                    let matched = matches!(
                        task.snapshot.mode,
                        BackgroundTaskMode::Condition {
                            condition: BackgroundTaskCondition::ExecutionFails,
                            ..
                        }
                    );
                    task.had_failure = true;
                    task.snapshot.last_summary = None;
                    task.snapshot.last_error_code = Some("execution_failed".to_string());
                    matched
                }
            };

            let max_runs = maximum_runs(&task.snapshot.mode);
            let finished = matches!(task.snapshot.mode, BackgroundTaskMode::OneShot)
                || condition_matched
                || task.snapshot.run_count >= max_runs;
            if finished {
                task.snapshot.state =
                    if task.snapshot.last_error_code.is_some() && !condition_matched {
                        BackgroundTaskState::Failed
                    } else {
                        BackgroundTaskState::Completed
                    };
                task.snapshot.updated_at = Utc::now();
                (NextRun::Stop, task.snapshot.clone())
            } else {
                task.snapshot.state = BackgroundTaskState::Waiting;
                task.snapshot.updated_at = Utc::now();
                (
                    NextRun::Wait {
                        interval: Duration::from_secs(interval_seconds(&task.snapshot.mode)),
                        cancellation: task.cancellation.clone(),
                    },
                    task.snapshot.clone(),
                )
            }
        };
        self.emit(BackgroundTaskEvent::Changed(snapshot));
        next
    }

    fn emit(&self, event: BackgroundTaskEvent) {
        let _ = self.event_tx.send(event);
    }
}

impl Drop for BackgroundTaskRuntime {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) != 1 {
            return;
        }
        self.shutdown();
    }
}

enum NextRun {
    Stop,
    Wait {
        interval: Duration,
        cancellation: Arc<Notify>,
    },
}

fn maximum_runs(mode: &BackgroundTaskMode) -> u32 {
    match mode {
        BackgroundTaskMode::OneShot => 1,
        BackgroundTaskMode::Interval { max_runs, .. }
        | BackgroundTaskMode::Condition { max_runs, .. } => *max_runs,
    }
}

fn interval_seconds(mode: &BackgroundTaskMode) -> u64 {
    match mode {
        BackgroundTaskMode::OneShot => 0,
        BackgroundTaskMode::Interval {
            interval_seconds, ..
        }
        | BackgroundTaskMode::Condition {
            interval_seconds, ..
        } => *interval_seconds,
    }
}

fn is_terminal_state(state: BackgroundTaskState) -> bool {
    matches!(
        state,
        BackgroundTaskState::Completed
            | BackgroundTaskState::Failed
            | BackgroundTaskState::Cancelled
    )
}

fn condition_matches(
    mode: &BackgroundTaskMode,
    previous_fingerprint: Option<&str>,
    had_failure: bool,
    result: &BackgroundTaskExecutionResult,
) -> bool {
    let BackgroundTaskMode::Condition { condition, .. } = mode else {
        return false;
    };
    match condition {
        BackgroundTaskCondition::ResultChanged => {
            previous_fingerprint.is_some_and(|previous| previous != result.fingerprint)
        }
        BackgroundTaskCondition::ResultContains { text } => result.summary.contains(text),
        BackgroundTaskCondition::ResultFieldEquals { pointer, expected } => {
            result
                .condition_value
                .as_ref()
                .and_then(|value| value.pointer(pointer))
                == Some(expected)
        }
        BackgroundTaskCondition::ExecutionFails => false,
        BackgroundTaskCondition::ExecutionRecovers => had_failure,
    }
}

pub fn fingerprint_json(value: &serde_json::Value) -> String {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(value).unwrap_or_default());
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::BackgroundTaskOwner;

    struct CountingExecutor {
        calls: AtomicU32,
    }

    #[async_trait]
    impl BackgroundTaskExecutor for CountingExecutor {
        async fn execute(
            &self,
            _execution: BackgroundTaskExecution,
        ) -> Result<BackgroundTaskExecutionResult, String> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
            Ok(BackgroundTaskExecutionResult::sanitized(
                format!("run {call}"),
                call.to_string(),
                None,
            ))
        }
    }

    fn spec(mode: BackgroundTaskMode) -> BackgroundTaskSpec {
        BackgroundTaskSpec {
            owner: BackgroundTaskOwner {
                conversation_id: "conversation".to_string(),
            },
            title: "Task".to_string(),
            tool_name: "list_plugins".to_string(),
            arguments_json: Zeroizing::new("{}".to_string()),
            mode,
        }
    }

    #[tokio::test]
    async fn one_shot_task_completes_without_polling() {
        let executor = Arc::new(CountingExecutor {
            calls: AtomicU32::new(0),
        });
        let (runtime, mut events) =
            BackgroundTaskRuntime::new(executor.clone(), tokio::runtime::Handle::current());
        let task_id = runtime
            .create(spec(BackgroundTaskMode::OneShot))
            .expect("create task");

        for _ in 0..4 {
            let _ = tokio::time::timeout(Duration::from_secs(1), events.recv()).await;
            if runtime
                .snapshot(&task_id)
                .is_some_and(|snapshot| snapshot.state == BackgroundTaskState::Completed)
            {
                break;
            }
        }

        assert_eq!(executor.calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            runtime.snapshot(&task_id).map(|snapshot| snapshot.state),
            Some(BackgroundTaskState::Completed)
        );
    }

    #[tokio::test]
    async fn owner_cancellation_stops_waiting_task() {
        let executor = Arc::new(CountingExecutor {
            calls: AtomicU32::new(0),
        });
        let (runtime, mut events) =
            BackgroundTaskRuntime::new(executor, tokio::runtime::Handle::current());
        let task_id = runtime
            .create(spec(BackgroundTaskMode::Interval {
                interval_seconds: 5,
                max_runs: 10,
            }))
            .expect("create task");

        while runtime
            .snapshot(&task_id)
            .is_some_and(|snapshot| snapshot.state != BackgroundTaskState::Waiting)
        {
            let _ = tokio::time::timeout(Duration::from_secs(1), events.recv()).await;
        }
        assert_eq!(runtime.cancel_owner("conversation"), 1);
        assert_eq!(
            runtime.snapshot(&task_id).map(|snapshot| snapshot.state),
            Some(BackgroundTaskState::Cancelled)
        );
    }

    #[tokio::test]
    async fn one_conversation_cannot_create_unbounded_active_tasks() {
        let executor = Arc::new(CountingExecutor {
            calls: AtomicU32::new(0),
        });
        let (runtime, _events) =
            BackgroundTaskRuntime::new(executor, tokio::runtime::Handle::current());

        for _ in 0..MAX_ACTIVE_TASKS_PER_OWNER {
            runtime
                .create(spec(BackgroundTaskMode::Interval {
                    interval_seconds: 60,
                    max_runs: 10,
                }))
                .expect("task below owner limit");
        }

        assert_eq!(
            runtime.create(spec(BackgroundTaskMode::Interval {
                interval_seconds: 60,
                max_runs: 10,
            })),
            Err(BackgroundTaskValidationError::TooManyActiveTasks)
        );
        runtime.shutdown();
    }
}
