use super::*;
use oxideterm_ai::AiChatStreamConfig;
#[cfg(not(target_family = "wasm"))]
pub use oxideterm_ai::tool_loop::AiModelBackendServices;

use oxideterm_ai::agent::{AgentGroupId, AgentRecord, AgentRunId, AgentRunRef, AgentText};

#[cfg(not(target_family = "wasm"))]
use oxideterm_ai::agent::{AgentResourceCoordinator, AgentRuntime};

#[cfg(not(target_family = "wasm"))]
#[derive(Clone)]
pub struct AiAgentServices {
    pub runtime: AgentRuntime,
    pub resources: AgentResourceCoordinator,
}
#[cfg(not(target_family = "wasm"))]
impl Default for AiAgentServices {
    fn default() -> Self {
        Self {
            runtime: AgentRuntime::new(oxideterm_ai::agent::DEFAULT_AGENT_CONCURRENCY),
            resources: AgentResourceCoordinator::default(),
        }
    }
}
#[cfg(not(target_family = "wasm"))]
impl gpui::Global for AiAgentServices {}

pub struct AiAgentGroup {
    pub parent: AgentRunRef,
    pub parent_message_id: String,
    pub parent_base_system_prompt: Option<String>,
    pub config: AiChatStreamConfig,
    #[cfg(not(target_family = "wasm"))]
    pub services: AiModelBackendServices,
    pub context_window: usize,
}

#[derive(Default)]
pub struct AgentCommunicationView {
    pub page: Option<Arc<oxideterm_ai::AgentCommunicationPage>>,
    pub cursors: Vec<Option<u64>>,
    pub loading: bool,
    pub failed: bool,
    task: Option<Task<()>>,
}

pub struct AiAgentWorkspace {
    pub host: Rc<dyn crate::bridge::agents::AiAgentHost>,
    #[cfg(not(target_family = "wasm"))]
    pub tool_leases:
        HashMap<(oxideterm_ai::ToolSessionId, String), Vec<oxideterm_ai::agent::AgentToolLease>>,
    pub local_commands: HashMap<(String, u64, String), Task<()>>,
    pub command_monitors: HashMap<oxideterm_ai::RuntimeOwnerKey, Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    pub services: AiAgentServices,
    pub groups: HashMap<AgentGroupId, AiAgentGroup>,
    pub records: HashMap<AgentRunId, AgentRecord>,
    pub message_runs: HashMap<String, AgentRunId>,
    pub detail: Option<AgentRunId>,
    pub details_loading: HashSet<AgentRunId>,
    pub detail_errors: HashSet<AgentRunId>,
    detail_tasks: HashMap<AgentRunId, Task<()>>,
    pub dirty: std::cell::RefCell<HashSet<AgentRunId>>,
    communication_versions: std::cell::RefCell<HashMap<(AgentRunId, u64), bool>>,
    pub model_picker_open: bool,
    pub expanded_groups: HashMap<AgentGroupId, bool>,
    pub parent_revisions: HashMap<String, u64>,
    pub communication_pages: HashMap<AgentRunId, AgentCommunicationView>,
    pub detail_lists: HashMap<AgentRunId, gpui::ListState>,
    pub detail_scroll: HashMap<AgentRunId, gpui::ScrollHandle>,
    watch: Option<Task<()>>,
}

impl AiAgentWorkspace {
    #[cfg(not(target_family = "wasm"))]
    pub fn new(services: AiAgentServices) -> Self {
        Self::from_host(Rc::new(services.runtime.clone()), services)
    }

    pub fn with_bridge(host: Rc<dyn crate::bridge::agents::AiAgentHost>) -> Self {
        Self::from_host(
            host,
            #[cfg(not(target_family = "wasm"))]
            AiAgentServices::default(),
        )
    }

    fn from_host(
        host: Rc<dyn crate::bridge::agents::AiAgentHost>,
        #[cfg(not(target_family = "wasm"))] services: AiAgentServices,
    ) -> Self {
        Self {
            host,
            #[cfg(not(target_family = "wasm"))]
            services,
            #[cfg(not(target_family = "wasm"))]
            tool_leases: HashMap::new(),
            command_monitors: HashMap::new(),
            local_commands: HashMap::new(),
            groups: HashMap::new(),
            records: HashMap::new(),
            message_runs: HashMap::new(),
            detail: None,
            details_loading: HashSet::new(),
            detail_errors: HashSet::new(),
            detail_tasks: HashMap::new(),
            dirty: std::cell::RefCell::new(HashSet::new()),
            communication_versions: std::cell::RefCell::new(HashMap::new()),
            model_picker_open: false,
            expanded_groups: HashMap::new(),
            parent_revisions: HashMap::new(),
            detail_scroll: HashMap::new(),
            detail_lists: HashMap::new(),
            communication_pages: HashMap::new(),
            watch: None,
        }
    }
}

impl AiWorkspaceEntity {
    pub fn chat_stream_runs_for_agent(&self, run: &AgentRunRef) -> Vec<u64> {
        self.chat_stream_runs
            .iter()
            .filter(|(_, stream)| stream.agent.as_ref() == Some(run))
            .map(|(generation, _)| *generation)
            .collect()
    }
    pub fn can_supplement_agent(&self) -> bool {
        self.chat_is_loading()
            && self.chat_stream_runs.values().any(|stream| {
                !stream.child
                    && stream
                        .agent
                        .as_ref()
                        .is_some_and(|run| self.agents.host.accepts_messages(run))
                    && Some(&stream.conversation_id)
                        == self.conversation_state.active_conversation_id.as_ref()
            })
    }

    pub fn supplement_agent(
        &mut self,
        content: &str,
    ) -> Result<(), oxideterm_ai::agent::AgentError> {
        if !self.can_supplement_agent() {
            return Err(oxideterm_ai::agent::AgentError::StaleRun);
        }
        let run = self
            .agent_run(self.chat_stream_generation())
            .ok_or(oxideterm_ai::agent::AgentError::StaleRun)?;
        self.agents
            .host
            .supplement(&run, oxideterm_ai::agent::AgentText::new(content))
    }
    pub fn run_accepts_tools(&self, generation: u64) -> bool {
        let Some(stream) = self.chat_stream_runs.get(&generation) else {
            return false;
        };
        stream
            .agent
            .as_ref()
            .is_none_or(|run| self.agents.host.accepts_messages(run))
    }
    pub fn agent_run(&self, generation: u64) -> Option<AgentRunRef> {
        self.chat_stream_runs.get(&generation)?.agent.clone()
    }

    pub fn bind_agent_run(&mut self, generation: u64, run: AgentRunRef, child: bool) {
        if let Some(stream) = self.chat_stream_runs.get_mut(&generation) {
            stream.agent = Some(run);
            stream.child = child;
        }
    }

    pub fn is_child_stream(&self, generation: u64) -> bool {
        self.chat_stream_runs
            .get(&generation)
            .is_some_and(|stream| stream.child)
    }

    pub fn is_agent_message(&self, message_id: &str) -> bool {
        self.agents.message_runs.contains_key(message_id)
    }

    pub fn add_agent_record(&mut self, record: AgentRecord) {
        *self
            .agents
            .parent_revisions
            .entry(record.parent_message_id.clone())
            .or_default() += 1;
        let id = record.snapshot.run.run_id.clone();
        for message in &record.messages {
            self.agents
                .message_runs
                .insert(message.id.clone(), id.clone());
        }
        self.agents.dirty.borrow_mut().insert(id.clone());
        let conversation = record.snapshot.conversation_id.clone();
        let messages: Vec<_> = record
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect();
        self.history_agent_created(&conversation, id.clone());
        self.agents.records.insert(id, record);
        for message in messages {
            self.history_message_changed(&conversation, &message);
        }
    }

    pub(super) fn schedule_agent_updates(&mut self, cx: &mut Context<Self>) {
        let mut updates = self.agents.host.subscribe();
        self.agents.watch = Some(cx.spawn(async move |weak, cx| {
            while updates.changed().await {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                if weak
                    .update(cx, |ai, cx| {
                        ai.refresh_agent_records();
                        ai.persist_agent_records();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub fn refresh_agent_records(&mut self) {
        let conversation_ids: HashSet<_> = self
            .agents
            .records
            .values()
            .map(|record| record.snapshot.conversation_id.clone())
            .collect();
        let mut changed_messages = Vec::new();
        for conversation_id in conversation_ids {
            let snapshots = self.agents.host.snapshots(&conversation_id);
            for snapshot in &snapshots {
                if let Some(record) = self.agents.records.get_mut(&snapshot.run.run_id) {
                    let parent_usage = snapshots
                        .iter()
                        .find(|parent| {
                            parent.run.group_id == snapshot.run.group_id
                                && parent.parent_id.is_none()
                        })
                        .map(|parent| parent.usage)
                        .unwrap_or(record.parent_usage);
                    let communication = self.agents.host.communication(&snapshot.run);
                    if record.parent_usage != parent_usage
                        || record.snapshot.usage != snapshot.usage
                        || record.snapshot.state != snapshot.state
                        || record.snapshot.progress.as_str() != snapshot.progress.as_str()
                        || record.communication.len() != communication.len()
                        || record.snapshot.model != snapshot.model
                        || record
                            .communication
                            .iter()
                            .zip(&communication)
                            .any(|(old, new)| old.consumed != new.consumed)
                    {
                        record.snapshot = snapshot.clone();
                        for message in &mut record.messages {
                            if message.model.as_deref() != Some(snapshot.model.model.as_str()) {
                                message.model = Some(snapshot.model.model.clone());
                                changed_messages
                                    .push((conversation_id.clone(), message.id.clone()));
                            }
                        }
                        record.parent_usage = parent_usage;
                        record.communication = communication;
                        record.revision = next_history_timestamp();
                        self.agents
                            .dirty
                            .borrow_mut()
                            .insert(record.snapshot.run.run_id.clone());
                        *self
                            .agents
                            .parent_revisions
                            .entry(record.parent_message_id.clone())
                            .or_default() += 1;
                    }
                }
            }
        }
        for (conversation, message) in changed_messages {
            self.history_message_changed(&conversation, &message);
        }
    }

    pub fn persist_agent_records(&self) {
        let ids: Vec<_> = self.agents.dirty.borrow_mut().drain().collect();
        for id in ids {
            if let Some(record) = self.agents.records.get(&id) {
                let metadata = record.metadata_projection();
                self.queue_history_event(
                    &record.snapshot.conversation_id,
                    "agent",
                    &id.to_string(),
                    super::history::HistoryEvent::Agent(metadata),
                );
                let family = format!("agent-communication:{id}");
                let mut versions = self.agents.communication_versions.borrow_mut();
                for message in &record.communication {
                    let key = (id.clone(), message.sequence);
                    if versions.get(&key) == Some(&message.consumed) {
                        continue;
                    }
                    self.queue_history_event(
                        &record.snapshot.conversation_id,
                        &family,
                        &message.sequence.to_string(),
                        super::history::HistoryEvent::AgentCommunication(message.clone()),
                    );
                    // The journal owns this version until the writer acknowledges it.
                    versions.insert(key, message.consumed);
                }
            }
        }
    }

    pub fn load_agent_summaries(&mut self, conversation_id: &str) {
        self.request_history_agents(conversation_id);
    }

    pub fn open_agent_detail(&mut self, id: AgentRunId, cx: &mut Context<Self>) {
        if self.agents.detail.as_ref() != Some(&id) {
            self.close_agent_detail();
        }
        self.agents.detail = Some(id.clone());
        self.agents
            .detail_lists
            .entry(id.clone())
            .or_insert_with(|| gpui::ListState::new(0, gpui::ListAlignment::Top, gpui::px(500.0)));
        self.agents.model_picker_open = false;
        self.agents.detail_scroll.entry(id.clone()).or_default();
        self.load_agent_communication(id.clone(), None, cx);
        self.load_agent_message_page(id, None, cx);
    }

    pub(super) fn release_committed_agent_messages(&mut self) {
        let ready: Vec<_> = self
            .agents
            .records
            .iter()
            .filter(|(_, record)| record.snapshot.state.is_terminal())
            .filter(|(_, record)| {
                record.messages.iter().all(|message| {
                    !self.history_message_is_pending(&record.snapshot.conversation_id, &message.id)
                })
            })
            .map(|(run, _)| run.clone())
            .collect();
        for run in ready {
            if let Some(record) = self.agents.records.get_mut(&run) {
                record.messages.clear();
            }
        }
    }

    pub(super) fn remove_agent_history_views(&mut self, removed: &HashSet<AgentRunId>) {
        self.agents
            .detail_tasks
            .retain(|run, _| !removed.contains(run));
        self.agents
            .detail_lists
            .retain(|run, _| !removed.contains(run));
        self.agents
            .communication_pages
            .retain(|run, _| !removed.contains(run));
        self.agents
            .communication_versions
            .borrow_mut()
            .retain(|(run, _), _| !removed.contains(run));
        self.history.auxiliary_pages.retain(|owner, _| !matches!(owner, super::history::HistoryViewOwner::Agent(_, run) if removed.contains(run)));
    }

    pub fn close_agent_detail(&mut self) {
        if let Some(id) = self.agents.detail.take() {
            self.agents.detail_tasks.remove(&id);
            self.agents.details_loading.remove(&id);
            self.agents.detail_lists.remove(&id);
            self.agents.communication_pages.remove(&id);
            self.history.auxiliary_pages.retain(|owner, _| !matches!(owner, super::history::HistoryViewOwner::Agent(_, run) if run == &id));
        }
    }

    pub fn load_agent_communication(
        &mut self,
        id: AgentRunId,
        older: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let Some(record) = self.agents.records.get(&id) else {
            return;
        };
        let conversation = record.snapshot.conversation_id.clone();
        let Some(store) = self.history_reader() else {
            return;
        };
        let view = self
            .agents
            .communication_pages
            .entry(id.clone())
            .or_default();
        if view.loading {
            return;
        }
        match older {
            Some(true) => {
                let Some(before) = view.page.as_ref().and_then(|page| page.before) else {
                    return;
                };
                view.cursors.push(Some(before));
            }
            Some(false) if view.cursors.len() > 1 => {
                view.cursors.pop();
            }
            Some(false) => return,
            None => {
                view.cursors = vec![None];
            }
        }
        let before = view.cursors.last().copied().flatten();
        view.loading = true;
        view.failed = false;
        let committed = self.committed_history();
        let run = id.clone();
        let key = id.clone();
        let task = cx.spawn(async move |weak, cx| {
            let loaded = if committed.await == Ok(true) {
                crate::bridge::history::read_history(|reply| {
                    store.communication(conversation, run, before, reply)
                })
                .await
            } else {
                Err(crate::bridge::history::HistoryReadFailed)
            };
            let _ = weak.update(cx, |ai, cx| {
                if ai.agents.detail.as_ref() != Some(&id) {
                    return;
                }
                let Some(view) = ai.agents.communication_pages.get_mut(&id) else {
                    return;
                };
                view.loading = false;
                view.task.take();
                match loaded {
                    Ok(page) => view.page = page.map(Arc::new),
                    _ => view.failed = true,
                }
                cx.notify();
            });
        });
        self.agents.communication_pages.get_mut(&key).unwrap().task = Some(task);
    }

    pub fn load_agent_message_page(
        &mut self,
        id: AgentRunId,
        older: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let Some(record) = self.agents.records.get(&id) else {
            return;
        };
        let conversation = record.snapshot.conversation_id.clone();
        let owner = super::history::HistoryViewOwner::Agent(conversation.clone(), id.clone());
        if self.agents.details_loading.contains(&id) {
            return;
        }
        let cursor = older.and_then(|older| {
            self.history.auxiliary_pages.get(&owner).and_then(|page| {
                if older {
                    page.before.clone()
                } else {
                    page.after.clone()
                }
            })
        });
        if older.is_some() && cursor.is_none() {
            return;
        }
        let Some(store) = self.history_reader() else {
            self.agents.detail_errors.insert(id);
            return;
        };
        self.agents.details_loading.insert(id.clone());
        self.agents.detail_errors.remove(&id);
        let branch = oxideterm_ai::agent_history_branch(&id);
        let record_revision = record.revision;
        let committed = self.committed_history();
        let key = id.clone();
        let task = cx.spawn(async move |weak, cx| {
            let loaded = if committed.await == Ok(true) {
                crate::bridge::history::read_history(|reply| {
                    store.branch_descriptions(
                        conversation,
                        branch,
                        cursor,
                        older == Some(false),
                        reply,
                    )
                })
                .await
            } else {
                Err(crate::bridge::history::HistoryReadFailed)
            };
            let _ = weak.update(cx, |ai, cx| {
                ai.agents.detail_tasks.remove(&id);
                ai.agents.details_loading.remove(&id);
                if ai.agents.detail.as_ref() != Some(&id) {
                    return;
                }
                match loaded {
                    Ok(page) => {
                        if ai
                            .agents
                            .records
                            .get(&id)
                            .is_some_and(|record| record.revision != record_revision)
                        {
                            ai.load_agent_message_page(id, None, cx);
                            return;
                        }
                        if let Some(list) = ai.agents.detail_lists.get(&id) {
                            list.reset(page.messages.len());
                        }
                        let state = ai.history.auxiliary_pages.entry(owner).or_default();
                        state.before = page.before;
                        state.after = page.after;
                        state.descriptions = page
                            .messages
                            .into_iter()
                            .map(|message| (message.id.clone(), message))
                            .collect();
                        state
                            .bodies
                            .retain(|id, _| state.descriptions.contains_key(id));
                        if let Some(record) = ai.agents.records.get_mut(&id) {
                            if record.snapshot.state.is_terminal() {
                                record.messages.clear();
                            }
                        }
                    }
                    _ => {
                        ai.agents.detail_errors.insert(id);
                    }
                }
                cx.notify();
            });
        });
        self.agents.detail_tasks.insert(key, task);
    }
}

impl AiWorkspaceEntity {
    pub fn agent_attention_description(
        &self,
        conversation_id: &str,
        message_id: &str,
    ) -> Option<String> {
        let record = self
            .agents
            .message_runs
            .get(message_id)
            .and_then(|id| self.agents.records.get(id));
        if self.conversation_state().active_conversation_id.as_deref() == Some(conversation_id)
            && record.is_none_or(|record| {
                self.agents.detail.as_ref() == Some(&record.snapshot.run.run_id)
            })
        {
            return None;
        }
        let conversation = self
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| conversation.title.clone())
            .unwrap_or_default();
        let task = record
            .map(|record| {
                format!(
                    "{} · {}",
                    record.snapshot.title.as_str(),
                    record
                        .target_labels
                        .iter()
                        .map(AgentText::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .unwrap_or_default();

        Some(oxideterm_ai::sanitize_for_persistence(&format!(
            "{conversation} · {task}"
        )))
    }
}
