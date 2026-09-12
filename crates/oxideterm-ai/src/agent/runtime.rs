use super::scheduler::AgentScheduler;
use parking_lot::Mutex;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tokio::sync::watch;

use super::*;

pub(super) const MAX_MAILBOX_MESSAGES: usize = 64;

struct Run {
    direction: u64,
    closing: bool,
    requests: Vec<AgentUsage>,
    model_locked: bool,
    snapshot: AgentSnapshot,
    inbox: VecDeque<AgentMessage>,
    cancellation: watch::Sender<bool>,
}
struct Group {
    direction: u64,
    parent: AgentId,
    remaining_rounds: usize,
    summary_used: bool,
    finished: bool,
    runs: HashMap<AgentId, Run>,
    updates: Vec<AgentUpdate>,
    previous_runs: Vec<AgentSnapshot>,
    contexts: HashMap<AgentId, Vec<crate::AiChatMessage>>,
    communication: Vec<AgentMessage>,
}
struct State {
    groups: HashMap<AgentGroupId, Group>,
    sequence: u64,
}

/// Share one runtime per application so work in other conversations uses the same quota.
#[derive(Clone)]
pub struct AgentRuntime {
    state: Arc<Mutex<State>>,
    changes: watch::Sender<u64>,
    scheduler: AgentScheduler,
}

impl AgentRuntime {
    pub fn new(concurrency: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                groups: HashMap::new(),
                sequence: 0,
            })),
            changes: watch::channel(0).0,
            scheduler: AgentScheduler::new(concurrency),
        }
    }

    fn changed(&self) {
        self.changes
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    pub fn register_resource(
        &self,
        run: &AgentRunRef,
        kind: OwnedResourceKind,
        label: AgentText,
    ) -> Result<AgentResourceRecord, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        ensure_active(group, run)?;
        let id = uuid::Uuid::new_v4().to_string();
        group
            .runs
            .get_mut(&run.agent_id)
            .unwrap()
            .snapshot
            .resources
            .push(OwnedResource {
                id: id.clone(),
                kind,
                label,
                outcome: None,
                state: OwnedResourceState::Running,
            });
        drop(state);
        self.changed();
        Ok(AgentResourceRecord::new(
            self.clone(),
            run.clone(),
            id,
            kind,
        ))
    }

    pub(super) fn record_resource_outcome(&self, run: &AgentRunRef, id: &str, outcome: AgentText) {
        let mut state = self.state.lock();
        let Ok(group) = current_group(&mut state, run) else {
            return;
        };
        let Some(resource) = group
            .runs
            .get_mut(&run.agent_id)
            .unwrap()
            .snapshot
            .resources
            .iter_mut()
            .find(|resource| resource.id == id)
        else {
            return;
        };
        resource.outcome = Some(outcome);
        drop(state);
        self.changed();
    }

    pub(super) fn finish_resource(
        &self,
        run: &AgentRunRef,
        id: &str,
        next: OwnedResourceState,
        only_running: bool,
    ) {
        let mut state = self.state.lock();
        let Ok(group) = current_group(&mut state, run) else {
            return;
        };
        let Some(resource) = group
            .runs
            .get_mut(&run.agent_id)
            .unwrap()
            .snapshot
            .resources
            .iter_mut()
            .find(|resource| resource.id == id)
        else {
            return;
        };
        if only_running && resource.state != OwnedResourceState::Running {
            return;
        }
        resource.state = next;
        drop(state);
        self.changed();
    }

    pub fn dispatch(&self, run: &AgentRunRef) -> Result<AgentDispatch, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        ensure_active(group, run)?;
        let direction = group.runs[&run.agent_id].direction;
        if direction != group.direction {
            return Err(AgentError::DirectionChanged);
        }
        Ok(AgentDispatch {
            runtime: self.clone(),
            run: run.clone(),
            direction,
        })
    }

    pub fn create_group(
        &self,
        conversation_id: String,
        model: AgentModel,
        scope: AgentScope,
        rounds: usize,
    ) -> AgentRunRef {
        let run = AgentRunRef {
            group_id: AgentGroupId::new(),
            agent_id: AgentId::new(),
            run_id: AgentRunId::new(),
        };
        let snapshot = AgentSnapshot {
            resources: Vec::new(),
            usage: AgentUsage::default(),
            run: run.clone(),
            parent_id: None,
            conversation_id,
            title: AgentText::default(),
            task: AgentText::default(),
            model,
            scope,
            state: AgentState::Running,
            progress: AgentText::default(),
            result: None,
        };
        self.state.lock().groups.insert(
            run.group_id.clone(),
            Group {
                direction: 0,
                parent: run.agent_id.clone(),
                remaining_rounds: rounds,
                summary_used: false,
                finished: false,
                updates: Vec::new(),
                previous_runs: Vec::new(),
                contexts: HashMap::new(),
                communication: Vec::new(),
                runs: HashMap::from([(
                    run.agent_id.clone(),
                    Run {
                        direction: 0,
                        closing: false,
                        requests: Vec::new(),
                        model_locked: false,
                        snapshot,
                        inbox: VecDeque::new(),
                        cancellation: watch::channel(false).0,
                    },
                )]),
            },
        );
        self.changed();
        run
    }

    pub fn delegate(
        &self,
        parent: &AgentRunRef,
        title: AgentText,
        task: AgentText,
        scope: AgentScope,
        model: Option<AgentModel>,
    ) -> Result<AgentRunRef, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        ensure_active(group, parent)?;
        if group.remaining_rounds == 0 {
            return Err(AgentError::BudgetExhausted);
        }
        let parent_snapshot = &group.runs[&parent.agent_id].snapshot;
        if !parent_snapshot.scope.contains(&scope) {
            return Err(AgentError::ScopeDenied);
        }
        let run = AgentRunRef {
            group_id: parent.group_id.clone(),
            agent_id: AgentId::new(),
            run_id: AgentRunId::new(),
        };
        let snapshot = AgentSnapshot {
            resources: Vec::new(),
            run: run.clone(),
            parent_id: Some(parent.agent_id.clone()),
            conversation_id: parent_snapshot.conversation_id.clone(),
            title,
            task,
            model: model.unwrap_or_else(|| parent_snapshot.model.clone()),
            scope,
            state: AgentState::Queued,
            progress: AgentText::default(),
            result: None,
            usage: AgentUsage::default(),
        };
        group.runs.insert(
            run.agent_id.clone(),
            Run {
                direction: group.direction,
                closing: false,
                requests: Vec::new(),
                model_locked: false,
                snapshot,
                inbox: VecDeque::new(),
                cancellation: watch::channel(false).0,
            },
        );
        drop(state);
        self.changed();
        Ok(run)
    }

    /// Only the host supplies refreshed ownership after authoritative target discovery.
    pub fn refresh_parent_targets(
        &self,
        parent: &AgentRunRef,
        targets: std::collections::HashSet<crate::RuntimeOwnerKey>,
    ) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        ensure_active(group, parent)?;
        group
            .runs
            .get_mut(&parent.agent_id)
            .unwrap()
            .snapshot
            .scope
            .targets = targets;
        Ok(())
    }

    pub fn snapshots(&self, conversation_id: &str) -> Vec<AgentSnapshot> {
        self.state
            .lock()
            .groups
            .values()
            .flat_map(|group| group.runs.values())
            .filter(|run| run.snapshot.conversation_id == conversation_id)
            .map(|run| run.snapshot.clone())
            .collect()
    }

    pub fn unresolved_resources(
        &self,
        conversation_id: &str,
        resources: &AgentResourceCoordinator,
    ) -> Vec<crate::RuntimeOwnerKey> {
        if !resources.has_unresolved() {
            return Vec::new();
        }
        self.state
            .lock()
            .groups
            .values()
            .flat_map(|group| group.runs.values())
            .filter(|run| run.snapshot.conversation_id == conversation_id)
            .flat_map(|run| {
                resources.unresolved_owned_by(&run.snapshot.run, run.snapshot.state.is_terminal())
            })
            .collect()
    }

    pub fn snapshot(&self, run: &AgentRunRef) -> Result<AgentSnapshot, AgentError> {
        let mut state = self.state.lock();
        Ok(current_group(&mut state, run)?.runs[&run.agent_id]
            .snapshot
            .clone())
    }

    pub fn take_round(&self, run: &AgentRunRef) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        ensure_active(group, run)?;
        if group.remaining_rounds == 0 {
            return Err(AgentError::BudgetExhausted);
        }
        group.remaining_rounds -= 1;
        Ok(())
    }

    pub fn refund_empty_round(&self, run: &AgentRunRef) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        current_group(&mut state, run)?.remaining_rounds += 1;
        Ok(())
    }

    pub fn save_context(
        &self,
        run: &AgentRunRef,
        history: Vec<crate::AiChatMessage>,
    ) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        if *group.runs[&run.agent_id].cancellation.borrow() {
            return Err(AgentError::Cancelled);
        }
        group.contexts.insert(
            run.agent_id.clone(),
            crate::sanitize_api_messages_for_provider(history),
        );
        Ok(())
    }

    pub fn take_context(&self, run: &AgentRunRef) -> Result<Vec<crate::AiChatMessage>, AgentError> {
        let mut state = self.state.lock();
        Ok(current_group(&mut state, run)?
            .contexts
            .remove(&run.agent_id)
            .unwrap_or_default())
    }

    pub fn child_run(&self, parent: &AgentRunRef, id: &AgentId) -> Result<AgentRunRef, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        group
            .runs
            .get(id)
            .filter(|run| run.snapshot.parent_id.is_some())
            .map(|run| run.snapshot.run.clone())
            .ok_or(AgentError::StaleRun)
    }

    pub fn children(&self, parent: &AgentRunRef) -> Result<Vec<AgentSnapshot>, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        Ok(group
            .runs
            .values()
            .filter(|run| run.snapshot.parent_id.is_some())
            .map(|run| run.snapshot.clone())
            .collect())
    }

    pub fn parent_run(&self, child: &AgentRunRef) -> Result<AgentRunRef, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, child)?;
        Ok(group.runs[&group.parent].snapshot.run.clone())
    }

    pub fn take_final_summary(&self, parent: &AgentRunRef) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        ensure_active(group, parent)?;
        if group.summary_used {
            return Err(AgentError::BudgetExhausted);
        }
        group.summary_used = true;
        Ok(())
    }

    pub fn send(
        &self,
        from: &AgentRunRef,
        to: &AgentRunRef,
        kind: AgentMessageKind,
        text: AgentText,
    ) -> Result<u64, AgentError> {
        let mut state = self.state.lock();
        let source_group = current_group(&mut state, from)?;
        ensure_active(source_group, from)?;
        if from.group_id != to.group_id {
            return Err(AgentError::ScopeDenied);
        }
        {
            let group = current_group(&mut state, to)?;
            ensure_active(group, to)?;
            if from.agent_id != group.parent && to.agent_id != group.parent {
                return Err(AgentError::ParentOnly);
            }
            if group.runs[&to.agent_id].inbox.len() >= MAX_MAILBOX_MESSAGES {
                return Err(AgentError::MailboxFull);
            }
        }
        state.sequence += 1;
        let sequence = state.sequence;
        let message = AgentMessage {
            sequence,
            from: from.clone(),
            to: to.clone(),
            kind,
            text,
            consumed: false,
        };
        let group = state.groups.get_mut(&to.group_id).unwrap();
        if kind == AgentMessageKind::UserSupplement && from.agent_id == group.parent {
            group.direction = group
                .direction
                .checked_add(1)
                .expect("direction counter exhausted");
            for run in group
                .runs
                .values_mut()
                .filter(|run| !run.snapshot.state.is_terminal())
            {
                run.snapshot.state = AgentState::Replanning;
            }
        }
        group.communication.push(message.clone());
        group
            .runs
            .get_mut(&to.agent_id)
            .unwrap()
            .inbox
            .push_back(message.clone());
        if to.agent_id == group.parent {
            group.updates.push(AgentUpdate {
                sequence,
                source: from.clone(),
                state: group.runs[&from.agent_id].snapshot.state,
                message: Some(message),
                result: None,
            });
        }
        drop(state);
        self.changed();
        Ok(sequence)
    }

    pub fn drain_messages(&self, run: &AgentRunRef) -> Result<Vec<AgentMessage>, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        if *group.runs[&run.agent_id].cancellation.borrow() {
            return Err(AgentError::Cancelled);
        }
        let messages: Vec<_> = group
            .runs
            .get_mut(&run.agent_id)
            .unwrap()
            .inbox
            .drain(..)
            .map(|mut message| {
                message.consumed = true;
                message
            })
            .collect();
        if !messages.is_empty()
            && (run.agent_id == group.parent
                || messages.iter().any(|message| {
                    message.from.agent_id == group.parent
                        && matches!(
                            message.kind,
                            AgentMessageKind::FollowUp | AgentMessageKind::Assignment
                        )
                }))
        {
            group.runs.get_mut(&run.agent_id).unwrap().direction = group.direction;
        }
        for update in &mut group.updates {
            if let Some(message) = update.message.as_mut() {
                if messages
                    .iter()
                    .any(|consumed| consumed.sequence == message.sequence)
                {
                    message.consumed = true;
                }
            }
        }
        for stored in &mut group.communication {
            if messages
                .iter()
                .any(|message| message.sequence == stored.sequence)
            {
                stored.consumed = true;
            }
        }
        Ok(messages)
    }

    pub async fn wait_messages(&self, run: &AgentRunRef) -> Result<Vec<AgentMessage>, AgentError> {
        // Subscribe before inspecting state to avoid losing a send between checking and sleeping.
        let mut changes = self.subscribe();
        loop {
            let messages = self.drain_messages(run)?;
            if !messages.is_empty() {
                return Ok(messages);
            }
            changes.changed().await.map_err(|_| AgentError::Cancelled)?;
        }
    }

    pub fn set_state(&self, run: &AgentRunRef, next: AgentState) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        let snapshot = &mut group.runs.get_mut(&run.agent_id).unwrap().snapshot;
        if snapshot.state.is_terminal()
            || snapshot.state == AgentState::Stopping
            || next.is_terminal()
            || next == AgentState::Stopping
        {
            return Err(AgentError::InvalidState);
        }
        snapshot.state = next;
        drop(state);
        self.changed();
        Ok(())
    }

    pub fn report_progress(&self, run: &AgentRunRef, text: AgentText) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        ensure_active(group, run)?;
        group.runs.get_mut(&run.agent_id).unwrap().snapshot.progress = text;
        drop(state);
        self.changed();
        Ok(())
    }

    pub fn cancellation(&self, run: &AgentRunRef) -> Result<watch::Receiver<bool>, AgentError> {
        let mut state = self.state.lock();
        Ok(current_group(&mut state, run)?.runs[&run.agent_id]
            .cancellation
            .subscribe())
    }

    pub fn set_concurrency(&self, concurrency: usize) {
        self.scheduler.set_limit(concurrency);
    }

    pub async fn acquire(&self, run: &AgentRunRef) -> Result<AgentPermit, AgentError> {
        if self.snapshot(run)?.parent_id.is_none() {
            return Err(AgentError::InvalidState);
        }
        let permit = self
            .scheduler
            .acquire(run.clone(), self.cancellation(run)?)
            .await?;
        self.set_state(run, AgentState::Running)?;
        Ok(permit)
    }

    pub fn stop(&self, parent: &AgentRunRef, child: &AgentRunRef) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        ensure_active(group, parent)?;
        if parent.group_id != child.group_id || parent.agent_id == child.agent_id {
            return Err(AgentError::ScopeDenied);
        }
        let run = group
            .runs
            .get_mut(&child.agent_id)
            .filter(|run| run.snapshot.run == *child)
            .ok_or(AgentError::StaleRun)?;
        if !run.snapshot.state.is_terminal() {
            run.snapshot.state = AgentState::Stopping;
            run.cancellation.send_replace(true);
        }
        drop(state);
        self.changed();
        Ok(())
    }

    pub fn complete(&self, run: &AgentRunRef, result: AgentResult) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        state.sequence += 1;
        let sequence = state.sequence;
        let group = current_group(&mut state, run)?;
        if group.parent == run.agent_id
            && group.runs.values().any(|child| {
                child.snapshot.parent_id.is_some() && !child.snapshot.state.is_terminal()
            })
        {
            return Err(AgentError::InvalidState);
        }
        let owned = group.runs.get_mut(&run.agent_id).unwrap();
        if owned.snapshot.state.is_terminal() {
            return Err(AgentError::StaleRun);
        }
        owned.snapshot.state = if *owned.cancellation.borrow() {
            AgentState::Cancelled
        } else if result.error_code.is_some() {
            AgentState::Failed
        } else {
            AgentState::Completed
        };
        owned.snapshot.result = Some(result.clone());
        if run.agent_id != group.parent {
            // Completion is retained separately from the bounded conversational mailbox.
            // A full mailbox must never turn a finished remote operation into a lost result.
            group.updates.push(AgentUpdate {
                sequence,
                source: run.clone(),
                state: owned.snapshot.state,
                message: None,
                result: Some(result),
            });
        }
        drop(state);
        self.changed();
        Ok(())
    }

    pub fn accepts_messages(&self, run: &AgentRunRef) -> bool {
        let mut state = self.state.lock();
        current_group(&mut state, run).is_ok_and(|group| ensure_active(group, run).is_ok())
    }

    /// Close receipt atomically with the last mailbox check. A message is either
    /// accepted for this run or rejected so its sender can retain/retry it.
    pub fn prepare_completion(&self, run: &AgentRunRef, cursor: u64) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        if group.runs[&run.agent_id].closing {
            return Ok(());
        }
        ensure_active(group, run)?;
        if !group.runs[&run.agent_id].inbox.is_empty()
            || (group.parent == run.agent_id
                && group.updates.iter().any(|update| update.sequence > cursor))
        {
            return Err(AgentError::PendingMessages);
        }
        if group.parent == run.agent_id
            && group.runs.values().any(|child| {
                child.snapshot.parent_id.is_some() && !child.snapshot.state.is_terminal()
            })
        {
            return Err(AgentError::InvalidState);
        }
        group.runs.get_mut(&run.agent_id).unwrap().closing = true;
        drop(state);
        self.changed();
        Ok(())
    }

    pub fn resume(
        &self,
        parent: &AgentRunRef,
        child: &AgentRunRef,
    ) -> Result<AgentRunRef, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        ensure_active(group, parent)?;
        if group.remaining_rounds == 0 {
            return Err(AgentError::BudgetExhausted);
        }
        if parent.group_id != child.group_id || parent.agent_id == child.agent_id {
            return Err(AgentError::ScopeDenied);
        }
        let owned = group
            .runs
            .get_mut(&child.agent_id)
            .filter(|owned| owned.snapshot.run == *child)
            .ok_or(AgentError::StaleRun)?;
        if !owned.snapshot.state.is_terminal() {
            return Err(AgentError::InvalidState);
        }
        group.previous_runs.push(owned.snapshot.clone());
        owned.snapshot.run.run_id = AgentRunId::new();
        owned.snapshot.state = AgentState::Queued;
        owned.snapshot.result = None;
        owned.snapshot.progress = AgentText::default();
        owned.inbox.clear();
        owned.cancellation = watch::channel(false).0;
        owned.model_locked = false;
        owned.closing = false;
        owned.requests.clear();
        owned.snapshot.usage = AgentUsage::default();
        let run = owned.snapshot.run.clone();
        drop(state);
        self.changed();
        Ok(run)
    }

    pub fn finish_group(&self, parent: &AgentRunRef) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        if group
            .runs
            .values()
            .any(|run| run.snapshot.parent_id.is_some() && !run.snapshot.state.is_terminal())
        {
            return Err(AgentError::InvalidState);
        }
        group.finished = true;
        group.contexts.clear();
        let parent_state = &mut group.runs.get_mut(&parent.agent_id).unwrap().snapshot.state;
        if !parent_state.is_terminal() {
            *parent_state = AgentState::Completed;
        }
        for run in group.runs.values_mut() {
            run.inbox.clear();
        }
        drop(state);
        self.changed();
        Ok(())
    }

    /// The caller retains the last returned sequence. Progress-only changes do not return here.
    pub async fn wait_updates(
        &self,
        parent: &AgentRunRef,
        after: u64,
    ) -> Result<Vec<AgentUpdate>, AgentError> {
        let mut changes = self.subscribe();
        loop {
            {
                let mut state = self.state.lock();
                let group = current_group(&mut state, parent)?;
                if group.parent != parent.agent_id {
                    return Err(AgentError::ParentOnly);
                }
                ensure_active(group, parent)?;
                let updates: Vec<_> = group
                    .updates
                    .iter()
                    .filter(|update| update.sequence > after)
                    .cloned()
                    .collect();
                if !updates.is_empty() {
                    return Ok(updates);
                }
                if group
                    .runs
                    .values()
                    .filter(|run| run.snapshot.parent_id.is_some())
                    .all(|run| run.snapshot.state.is_terminal())
                {
                    return Ok(Vec::new());
                }
            }
            changes.changed().await.map_err(|_| AgentError::Cancelled)?;
        }
    }

    pub fn run_history(&self, parent: &AgentRunRef) -> Result<Vec<AgentSnapshot>, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        Ok(group.previous_runs.clone())
    }

    pub fn communication(&self, run: &AgentRunRef) -> Vec<AgentMessage> {
        self.state
            .lock()
            .groups
            .get(&run.group_id)
            .map(|group| {
                group
                    .communication
                    .iter()
                    .filter(|message| {
                        message.from.agent_id == run.agent_id || message.to.agent_id == run.agent_id
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn updates_since(
        &self,
        parent: &AgentRunRef,
        after: u64,
    ) -> Result<Vec<AgentUpdate>, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        Ok(group
            .updates
            .iter()
            .filter(|update| update.sequence > after)
            .cloned()
            .collect())
    }

    pub fn change_queued_model(
        &self,
        run: &AgentRunRef,
        model: AgentModel,
    ) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        let child = group.runs.get_mut(&run.agent_id).unwrap();
        if child.model_locked
            || child.snapshot.parent_id.is_none()
            || child.snapshot.state != AgentState::Queued
        {
            return Err(AgentError::InvalidState);
        }
        child.snapshot.model = model;
        drop(state);
        self.changed();
        Ok(())
    }

    pub fn lock_model(&self, run: &AgentRunRef) -> Result<AgentModel, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        ensure_active(group, run)?;
        let owned = group.runs.get_mut(&run.agent_id).unwrap();
        owned.model_locked = true;
        Ok(owned.snapshot.model.clone())
    }

    pub fn begin_request(&self, run: &AgentRunRef) -> Result<usize, AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        if *group.runs[&run.agent_id].cancellation.borrow() {
            return Err(AgentError::Cancelled);
        }
        let owned = group.runs.get_mut(&run.agent_id).unwrap();
        let index = owned.requests.len();
        owned.requests.push(AgentUsage {
            requests: 1,
            ..AgentUsage::default()
        });
        owned.snapshot.usage = AgentUsage::total(owned.requests.iter().copied());
        drop(state);
        self.changed();
        Ok(index)
    }

    pub fn record_usage(
        &self,
        run: &AgentRunRef,
        request: usize,
        input: Option<u64>,
        output: Option<u64>,
    ) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, run)?;
        let owned = group.runs.get_mut(&run.agent_id).unwrap();
        let usage = owned
            .requests
            .get_mut(request)
            .ok_or(AgentError::InvalidState)?;
        usage.input_tokens = input.or(usage.input_tokens);
        usage.output_tokens = output.or(usage.output_tokens);
        owned.snapshot.usage = AgentUsage::total(owned.requests.iter().copied());
        drop(state);
        self.changed();
        Ok(())
    }

    pub fn cancel_conversation(&self, conversation_id: &str) {
        let mut state = self.state.lock();
        for group in state.groups.values_mut() {
            if group.runs[&group.parent].snapshot.conversation_id == conversation_id {
                group.contexts.clear();
            }
            for run in group.runs.values_mut().filter(|run| {
                run.snapshot.conversation_id == conversation_id && !run.snapshot.state.is_terminal()
            }) {
                run.snapshot.state = AgentState::Stopping;
                run.cancellation.send_replace(true);
            }
        }
        drop(state);
        self.changed();
    }

    pub fn cancel_group(&self, parent: &AgentRunRef) -> Result<(), AgentError> {
        let mut state = self.state.lock();
        let group = current_group(&mut state, parent)?;
        if group.parent != parent.agent_id {
            return Err(AgentError::ParentOnly);
        }
        group.contexts.clear();
        for run in group
            .runs
            .values_mut()
            .filter(|run| !run.snapshot.state.is_terminal())
        {
            run.snapshot.state = AgentState::Stopping;
            run.cancellation.send_replace(true);
        }
        drop(state);
        self.changed();
        Ok(())
    }

    /// Cancel all owned waits before discarding history during conversation deletion.
    pub fn remove_conversation(&self, conversation_id: &str) {
        self.cancel_conversation(conversation_id);
        self.state.lock().groups.retain(|_, group| {
            group.runs[&group.parent].snapshot.conversation_id != conversation_id
        });
        self.changed();
    }
}

fn current_group<'a>(state: &'a mut State, run: &AgentRunRef) -> Result<&'a mut Group, AgentError> {
    let group = state
        .groups
        .get_mut(&run.group_id)
        .ok_or(AgentError::StaleRun)?;
    if group.finished {
        return Err(AgentError::GroupFinished);
    }
    let owned = group.runs.get(&run.agent_id).ok_or(AgentError::StaleRun)?;
    if owned.snapshot.run != *run {
        return Err(AgentError::StaleRun);
    }
    Ok(group)
}

fn ensure_active(group: &Group, run: &AgentRunRef) -> Result<(), AgentError> {
    let owned = &group.runs[&run.agent_id];
    if *owned.cancellation.borrow() {
        return Err(AgentError::Cancelled);
    }
    if owned.closing {
        return Err(AgentError::InvalidState);
    }
    if owned.snapshot.state.is_terminal() {
        return Err(AgentError::InvalidState);
    }
    Ok(())
}

/// Captured before a model round; never refreshed by an old pending tool call.
#[derive(Clone)]
pub struct AgentDispatch {
    runtime: AgentRuntime,
    run: AgentRunRef,
    direction: u64,
}

impl AgentDispatch {
    pub fn check(&self) -> Result<(), AgentError> {
        let current = self.runtime.dispatch(&self.run)?;
        if current.direction != self.direction {
            return Err(AgentError::DirectionChanged);
        }
        Ok(())
    }

    pub async fn cancelled(&self) {
        let Ok(mut cancellation) = self.runtime.cancellation(&self.run) else {
            return;
        };
        if !*cancellation.borrow() {
            let _ = cancellation.changed().await;
        }
    }

    pub async fn invalidated(&self) {
        let mut changes = self.runtime.subscribe();
        loop {
            if self.check().is_err() {
                return;
            }
            if changes.changed().await.is_err() {
                return;
            }
        }
    }
}
