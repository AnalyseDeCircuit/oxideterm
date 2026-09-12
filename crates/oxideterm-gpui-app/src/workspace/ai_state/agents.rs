use super::*;
use oxideterm_ai::AiChatStreamConfig;
/// Provider-side services that are safe for the background model loop to own.
/// Application runtime owners deliberately remain on the GPUI broker side.
#[derive(Clone)]
pub(in crate::workspace) struct AiModelBackendServices {
    pub(in crate::workspace) rag_store: std::sync::Arc<oxideterm_ai::RagStore>,
    pub(in crate::workspace) ai_mcp_registry: oxideterm_ai::McpRegistry,
    pub(in crate::workspace) ai_key_store: oxideterm_ai::AiProviderKeyStore,
    pub(in crate::workspace) ai_providers: Vec<serde_json::Value>,
    pub(in crate::workspace) ai_embedding_config: Option<serde_json::Value>,
    pub(in crate::workspace) agent_model_limits:
        HashMap<(String, String), (usize, Option<i64>, String)>,
}
use oxideterm_ai::agent::{
    AgentConversationOptions, AgentGroupId, AgentRecord, AgentResourceCoordinator, AgentRunId,
    AgentRunRef, AgentRuntime,
};

#[derive(Clone)]
pub(in crate::workspace) struct AiAgentServices {
    pub runtime: AgentRuntime,
    pub resources: AgentResourceCoordinator,
}
impl Default for AiAgentServices {
    fn default() -> Self {
        Self {
            runtime: AgentRuntime::new(oxideterm_ai::agent::DEFAULT_AGENT_CONCURRENCY),
            resources: AgentResourceCoordinator::default(),
        }
    }
}
impl gpui::Global for AiAgentServices {}

pub(in crate::workspace) struct AiAgentGroup {
    pub parent: AgentRunRef,
    pub parent_message_id: String,
    pub config: AiChatStreamConfig,
    pub services: AiModelBackendServices,
    pub context_window: usize,
}

pub(in crate::workspace) struct AiAgentWorkspace {
    pub tool_leases:
        HashMap<(oxideterm_ai::ToolSessionId, String), Vec<oxideterm_ai::agent::AgentToolLease>>,
    pub local_commands: HashMap<(String, u64, String), Task<()>>,
    pub command_monitors: HashMap<oxideterm_ai::RuntimeOwnerKey, Task<()>>,
    pub services: AiAgentServices,
    pub groups: HashMap<AgentGroupId, AiAgentGroup>,
    pub records: HashMap<AgentRunId, AgentRecord>,
    pub message_runs: HashMap<String, AgentRunId>,
    pub detail: Option<AgentRunId>,
    pub details_loaded: HashSet<AgentRunId>,
    pub details_loading: HashSet<AgentRunId>,
    pub detail_errors: HashSet<AgentRunId>,
    pub dirty: std::cell::RefCell<HashSet<AgentRunId>>,
    pub settings_model_picker_open: bool,
    pub model_picker_open: bool,
    pub expanded_groups: HashMap<AgentGroupId, bool>,
    pub parent_revisions: HashMap<String, u64>,
    pub detail_scroll: HashMap<AgentRunId, gpui::ScrollHandle>,
    watch: Option<Task<()>>,
}

impl AiAgentWorkspace {
    pub fn new(services: AiAgentServices) -> Self {
        Self {
            services,
            tool_leases: HashMap::new(),
            command_monitors: HashMap::new(),
            local_commands: HashMap::new(),
            groups: HashMap::new(),
            records: HashMap::new(),
            message_runs: HashMap::new(),
            detail: None,
            details_loaded: HashSet::new(),
            details_loading: HashSet::new(),
            detail_errors: HashSet::new(),
            dirty: std::cell::RefCell::new(HashSet::new()),
            settings_model_picker_open: false,
            model_picker_open: false,
            expanded_groups: HashMap::new(),
            parent_revisions: HashMap::new(),
            detail_scroll: HashMap::new(),
            watch: None,
        }
    }
}

impl AiWorkspaceEntity {
    pub(in crate::workspace) fn chat_stream_runs_for_agent(&self, run: &AgentRunRef) -> Vec<u64> {
        self.chat_stream_runs
            .iter()
            .filter(|(_, stream)| stream.agent.as_ref() == Some(run))
            .map(|(generation, _)| *generation)
            .collect()
    }
    pub(in crate::workspace) fn can_supplement_agent(&self) -> bool {
        self.chat_is_loading()
            && self.chat_stream_runs.values().any(|stream| {
                !stream.child
                    && stream
                        .agent
                        .as_ref()
                        .is_some_and(|run| self.agents.services.runtime.accepts_messages(run))
                    && Some(&stream.conversation_id)
                        == self.conversation_state.active_conversation_id.as_ref()
            })
    }

    pub(in crate::workspace) fn supplement_agent(
        &mut self,
        content: &str,
    ) -> Result<(), oxideterm_ai::agent::AgentError> {
        if !self.can_supplement_agent() {
            return Err(oxideterm_ai::agent::AgentError::StaleRun);
        }
        let run = self
            .agent_run(self.chat_stream_generation())
            .ok_or(oxideterm_ai::agent::AgentError::StaleRun)?;
        self.agents.services.runtime.send(
            &run,
            &run,
            oxideterm_ai::agent::AgentMessageKind::UserSupplement,
            oxideterm_ai::agent::AgentText::new(content),
        )?;
        Ok(())
    }
    pub(in crate::workspace) fn run_accepts_tools(&self, generation: u64) -> bool {
        let Some(stream) = self.chat_stream_runs.get(&generation) else {
            return false;
        };
        stream
            .agent
            .as_ref()
            .is_none_or(|run| self.agents.services.runtime.accepts_messages(run))
    }
    pub(in crate::workspace) fn agent_options(
        &self,
        conversation_id: &str,
    ) -> AgentConversationOptions {
        self.conversation_state
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(|conversation| conversation.session_metadata.as_ref()?.get("subagents"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default()
    }

    pub(in crate::workspace) fn set_agent_options(
        &mut self,
        conversation_id: &str,
        options: AgentConversationOptions,
    ) {
        if let Some(conversation) = self
            .conversation_state
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        {
            let metadata = conversation
                .session_metadata
                .get_or_insert_with(|| serde_json::json!({}));
            metadata["subagents"] =
                serde_json::to_value(options).expect("agent options are serializable");
        }
        self.persist_chat_state();
    }

    pub(in crate::workspace) fn agent_run(&self, generation: u64) -> Option<AgentRunRef> {
        self.chat_stream_runs.get(&generation)?.agent.clone()
    }

    pub(in crate::workspace) fn bind_agent_run(
        &mut self,
        generation: u64,
        run: AgentRunRef,
        child: bool,
    ) {
        if let Some(stream) = self.chat_stream_runs.get_mut(&generation) {
            stream.agent = Some(run);
            stream.child = child;
        }
    }

    pub(in crate::workspace) fn is_child_stream(&self, generation: u64) -> bool {
        self.chat_stream_runs
            .get(&generation)
            .is_some_and(|stream| stream.child)
    }

    pub(in crate::workspace) fn is_agent_message(&self, message_id: &str) -> bool {
        self.agents.message_runs.contains_key(message_id)
    }

    pub(in crate::workspace) fn add_agent_record(&mut self, record: AgentRecord) {
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
        self.agents.details_loaded.insert(id.clone());
        self.agents.dirty.borrow_mut().insert(id.clone());
        self.agents.records.insert(id, record);
    }

    pub(super) fn schedule_agent_updates(&mut self, cx: &mut Context<Self>) {
        let mut updates = self.agents.services.runtime.subscribe();
        self.agents.watch = Some(cx.spawn(async move |weak, cx| {
            while updates.changed().await.is_ok() {
                Timer::after(Duration::from_millis(50)).await;
                if weak
                    .update(cx, |ai, cx| {
                        ai.refresh_agent_records();
                        ai.persist_agent_records();
                        // Use the existing window-routed delivery wake; no hidden detail is rendered here.
                        cx.emit(AiWorkspaceEvent::ChatStreamDeliveryReady);
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(in crate::workspace) fn refresh_agent_records(&mut self) {
        let conversation_ids: HashSet<_> = self
            .agents
            .records
            .values()
            .map(|record| record.snapshot.conversation_id.clone())
            .collect();
        for conversation_id in conversation_ids {
            let snapshots = self.agents.services.runtime.snapshots(&conversation_id);
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
                    let communication = self.agents.services.runtime.communication(&snapshot.run);
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
                            message.model = Some(snapshot.model.model.clone());
                        }
                        record.parent_usage = parent_usage;
                        record.communication = communication;
                        record.revision =
                            oxideterm_ai::AiChatPersistenceStore::next_projection_persist_at();
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
    }

    pub(in crate::workspace) fn persist_agent_records(&self) {
        let Some(store) = self.persistence_store.clone() else {
            return;
        };
        let ids: Vec<_> = self.agents.dirty.borrow_mut().drain().collect();
        let records: Vec<_> = ids
            .iter()
            .filter_map(|id| self.agents.records.get(id).cloned())
            .collect();
        if records.is_empty() {
            return;
        }
        self.task_runtime.spawn_blocking(move || {
            if store.save_agent_records(records).is_err() {
                eprintln!("[AiChatStore] Failed to persist agent records");
            }
        });
    }

    pub(in crate::workspace) fn load_agent_summaries(&mut self, conversation_id: &str) {
        if let Some(store) = self.persistence_store.as_ref() {
            match store.load_agent_summaries(conversation_id) {
                Ok(records) => {
                    for record in records {
                        self.agents
                            .records
                            .entry(record.snapshot.run.run_id.clone())
                            .or_insert(record);
                    }
                }
                Err(_) => eprintln!("[AiChatStore] Failed to load agent summaries"),
            }
        }
    }

    pub(in crate::workspace) fn open_agent_detail(
        &mut self,
        id: AgentRunId,
        cx: &mut Context<Self>,
    ) {
        self.agents.detail = Some(id.clone());
        self.agents.model_picker_open = false;
        self.agents.detail_scroll.entry(id.clone()).or_default();
        if self.agents.details_loaded.contains(&id) || self.agents.details_loading.contains(&id) {
            return;
        }
        let Some(record) = self.agents.records.get(&id) else {
            return;
        };
        let Some(store) = self.persistence_store.clone() else {
            self.agents.detail_errors.insert(id);
            return;
        };
        let conversation_id = record.snapshot.conversation_id.clone();
        let revision = record.revision;
        self.agents.details_loading.insert(id.clone());
        self.agents.detail_errors.remove(&id);
        let load_id = id.clone();
        let task = self
            .task_runtime
            .spawn_blocking(move || store.load_agent_record(&conversation_id, &load_id));
        cx.spawn(async move |weak, cx| {
            let loaded = task.await;
            let _ = weak.update(cx, |ai, cx| {
                ai.agents.details_loading.remove(&id);
                let Some(current) = ai.agents.records.get(&id) else {
                    return;
                };
                // A disk read must not replace newer live delivery.
                if current.revision != revision || ai.agents.details_loaded.contains(&id) {
                    return;
                }
                match loaded {
                    Ok(Ok(Some(record)))
                        if ai
                            .conversation_state
                            .conversations
                            .iter()
                            .any(|conversation| {
                                conversation.id == record.snapshot.conversation_id
                            }) =>
                    {
                        ai.agents.records.insert(id.clone(), record);
                        ai.agents.details_loaded.insert(id);
                    }
                    _ => {
                        ai.agents.detail_errors.insert(id);
                    }
                }
                cx.emit(AiWorkspaceEvent::ChatStreamDeliveryReady);
            });
        })
        .detach();
    }
}
