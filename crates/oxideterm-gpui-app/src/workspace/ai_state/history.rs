use super::*;
use oxideterm_ai::{
    AiChatMessage, AiConversation, ConversationStore, HistoryCursor, HistoryMutation,
    HistoryWriteState, HistoryWriter,
};
use std::cell::RefCell;
mod archives;
mod pages;
pub(in crate::workspace) use pages::HistoryViewOwner;
static NEXT_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
type StreamBases = Vec<(
    (String, String),
    Option<oxideterm_ai::HistoryStreamSnapshot>,
)>;

struct JournalBatch {
    revision: u64,
    operations: Vec<HistoryMutation>,
    streams: StreamBases,
    pending: Vec<PendingMessage>,
}

struct PendingMessage {
    conversation: String,
    branch: String,
    message: Option<AiChatMessage>,
    revision: u64,
    base: Option<oxideterm_ai::HistoryStreamSnapshot>,
    structural: bool,
}

impl Drop for PendingMessage {
    fn drop(&mut self) {
        if let Some(message) = &mut self.message {
            clear_history_source(message);
        }
    }
}

fn clear_history_source(message: &mut AiChatMessage) {
    use zeroize::Zeroize;
    fn clear_json(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => text.zeroize(),
            serde_json::Value::Array(values) => values.iter_mut().for_each(clear_json),
            serde_json::Value::Object(values) => {
                for (mut key, mut value) in std::mem::take(values) {
                    key.zeroize();
                    clear_json(&mut value);
                }
            }
            _ => {}
        }
    }
    message.content.zeroize();
    for text in [
        &mut message.context,
        &mut message.thinking_content,
        &mut message.model,
    ]
    .into_iter()
    .flatten()
    {
        text.zeroize();
    }
    for value in &mut message.tool_calls {
        clear_json(value);
    }
    for value in [
        &mut message.turn,
        &mut message.transcript_ref,
        &mut message.summary_ref,
    ]
    .into_iter()
    .flatten()
    {
        clear_json(value);
    }
    for suggestion in &mut message.suggestions {
        suggestion.text.zeroize();
    }
    if let Some(messages) = message
        .metadata
        .as_mut()
        .and_then(|metadata| metadata.original_messages.as_mut())
    {
        messages.iter_mut().for_each(clear_history_source);
    }
    if let Some(branches) = &mut message.branches {
        for messages in branches.tails.values_mut() {
            messages.iter_mut().for_each(clear_history_source);
        }
    }
}

impl JournalBatch {
    /// Sanitization, hashing and delta construction run off the GPUI thread.
    fn prepare(mut self) -> Self {
        for mut pending in std::mem::take(&mut self.pending) {
            let message = pending.message.as_ref().unwrap();
            let key = (pending.conversation.clone(), message.id.clone());
            if let Some((delta, snapshot)) = pending.base.as_ref().and_then(|base| {
                if pending.structural {
                    base.message_delta(message, pending.revision)
                } else {
                    base.delta(message, pending.revision)
                }
            }) {
                self.operations.push(HistoryMutation::StreamText {
                    conversation_id: pending.conversation.clone(),
                    branch_id: pending.branch.clone(),
                    message_id: message.id.clone(),
                    delta,
                });
                self.streams.push((key, Some(snapshot)));
                continue;
            }
            let snapshot = (message.is_streaming
                && message.role == oxideterm_ai::AiChatRole::Assistant)
                .then(|| oxideterm_ai::HistoryStreamSnapshot::capture(message, pending.revision));
            self.streams.push((key, snapshot));
            self.operations.push(HistoryMutation::PutMessage {
                conversation_id: pending.conversation.clone(),
                branch_id: pending.branch.clone(),
                message: pending.message.take().unwrap(),
                revision: pending.revision,
            });
        }
        self
    }
}

#[derive(Default)]
struct Changes {
    revision: u64,
    messages: HashMap<(String, String), u64>,
    full_messages: HashMap<(String, String), u64>,
    stream_bases: HashMap<(String, String), oxideterm_ai::HistoryStreamSnapshot>,
    metadata: HashMap<String, u64>,
    created: HashMap<String, u64>,
    deleted: HashMap<String, u64>,
    extra: Vec<(u64, HistoryMutation)>,
    events: HashMap<(String, String, String), (u64, HistoryEvent)>,
}

pub(super) enum HistoryEvent {
    Transcript(oxideterm_ai::PersistedTranscriptEntry),
    Diagnostic(oxideterm_ai::PersistedDiagnosticEvent),
    Agent(oxideterm_ai::agent::AgentRecord),
    AgentCommunication(oxideterm_ai::agent::AgentMessage),
}

impl HistoryEvent {
    fn value(&self) -> serde_json::Result<serde_json::Value> {
        match self {
            Self::Transcript(entry) => serde_json::to_value(entry),
            Self::Diagnostic(event) => serde_json::to_value(event),
            Self::Agent(record) => {
                let mut value = serde_json::to_value(record)?;
                value["messageBranch"] =
                    oxideterm_ai::agent_history_branch(&record.snapshot.run.run_id).into();
                Ok(value)
            }
            Self::AgentCommunication(message) => serde_json::to_value(message),
        }
    }
}
impl Changes {
    fn next(&mut self) -> u64 {
        self.revision = NEXT_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        self.revision
    }
    fn acknowledged(&mut self, revision: u64) {
        self.messages.retain(|_, value| *value > revision);
        self.full_messages.retain(|_, value| *value > revision);
        self.metadata.retain(|_, value| *value > revision);
        self.created.retain(|_, value| *value > revision);
        self.deleted.retain(|_, value| *value > revision);
        self.extra.retain(|(value, _)| *value > revision);
        self.events.retain(|_, (value, _)| *value > revision);
    }
}

pub(in crate::workspace) enum HistoryStatus {
    Idle,
    Loading,
    Ready,
    Failed,
}

pub(in crate::workspace) struct AiHistoryState {
    pub store: Option<ConversationStore>,
    pub writer: Option<HistoryWriter>,
    pub status: HistoryStatus,
    pub quitting: bool,
    pub migration_cancel: Arc<std::sync::atomic::AtomicBool>,
    retry: Option<tokio::task::JoinHandle<()>>,
    pub progress: Option<(usize, usize)>,
    pub archives: HashMap<(String, String), archives::ArchiveView>,
    archive_tasks: HashMap<(String, String), tokio::task::JoinHandle<()>>,
    pub answers: HashMap<(u64, String), tokio::task::JoinHandle<()>>,
    pub model_contexts: HashMap<String, AiConversation>,
    pub compaction_key_loads: HashMap<String, Task<()>>,
    pub compaction_runs: HashMap<String, (u64, Option<tokio::task::JoinHandle<()>>)>,
    pub compaction_generation: u64,
    pub compaction_sources: HashMap<String, (String, AiConversation)>,
    pub compaction_loads: HashMap<String, Task<()>>,
    pub launches: HashMap<String, Task<()>>,
    pub edit_load: Option<Task<()>>,
    pub copy_load: Option<Task<()>>,
    pub pages: HashMap<String, pages::HistoryPageState>,
    pub auxiliary_pages: HashMap<HistoryViewOwner, pages::HistoryPageState>,
    pub branches: HashMap<String, String>,
    changes: RefCell<Changes>,
    sender: std::sync::mpsc::Sender<Delivery>,
    receiver: std::sync::mpsc::Receiver<Delivery>,
    wake: Arc<tokio::sync::Notify>,
    watch: Option<Task<()>>,
    write: Option<tokio::task::JoinHandle<()>>,
    load: Option<tokio::task::JoinHandle<()>>,
    status_watch: Option<tokio::task::JoinHandle<()>>,
    agents_load: Option<tokio::task::JoinHandle<()>>,
    save_timer: RefCell<Option<tokio::task::JoinHandle<()>>>,
    generation: u64,
    page_generation: u64,
    committed: u64,
    waiters: Vec<(u64, tokio::sync::oneshot::Sender<bool>)>,
}

enum Delivery {
    Initialized(
        u64,
        Result<(ConversationStore, Vec<oxideterm_ai::ConversationHead>), String>,
    ),
    Progress(u64, usize, usize),
    Written(u64, StreamBases),
    WriteFailed,
    WriteState(HistoryWriteState),
    Archive(archives::ArchiveDelivery),
    Save,
    StreamSave,
    Page(pages::PageDelivery),
    Body(pages::BodyDelivery),
    Agents(
        u64,
        String,
        Result<Vec<oxideterm_ai::agent::AgentRecord>, ()>,
    ),
}

impl Default for AiHistoryState {
    fn default() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        Self {
            store: None,
            writer: None,
            status: HistoryStatus::Idle,
            quitting: false,
            migration_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            retry: None,
            progress: None,
            archives: HashMap::new(),
            archive_tasks: HashMap::new(),
            answers: HashMap::new(),
            model_contexts: HashMap::new(),
            compaction_key_loads: HashMap::new(),
            compaction_runs: HashMap::new(),
            compaction_generation: 0,
            compaction_sources: HashMap::new(),
            compaction_loads: HashMap::new(),
            launches: HashMap::new(),
            edit_load: None,
            copy_load: None,
            pages: HashMap::new(),
            auxiliary_pages: HashMap::new(),
            branches: HashMap::new(),
            changes: RefCell::new(Changes::default()),
            sender,
            receiver,
            wake: Arc::new(tokio::sync::Notify::new()),
            watch: None,
            write: None,
            load: None,
            status_watch: None,
            agents_load: None,
            save_timer: RefCell::new(None),
            generation: 0,
            page_generation: 0,
            committed: 0,
            waiters: Vec::new(),
        }
    }
}
impl Drop for AiHistoryState {
    fn drop(&mut self) {
        self.migration_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        if let Some(task) = self.retry.take() {
            task.abort();
        }
        for (_, (_, task)) in self.compaction_runs.drain() {
            if let Some(task) = task {
                task.abort();
            }
        }
        for (_, task) in self.archive_tasks.drain() {
            task.abort();
        }
        for (_, task) in self.answers.drain() {
            task.abort();
        }
        if let Some(task) = self.agents_load.take() {
            task.abort();
        }
        if let Some(task) = self.save_timer.get_mut().take() {
            task.abort();
        }
        if let Some(task) = self.load.take() {
            task.abort();
        }
        if let Some(task) = self.status_watch.take() {
            task.abort();
        }
        if let Some(task) = self.write.take() {
            task.abort();
        }
    }
}

fn metadata(conversation: &AiConversation) -> AiConversation {
    AiConversation {
        id: conversation.id.clone(),
        title: conversation.title.clone(),
        messages: Vec::new(),
        created_at_ms: conversation.created_at_ms,
        updated_at_ms: conversation.updated_at_ms,
        origin: conversation.origin.clone(),
        profile_id: conversation.profile_id.clone(),
        message_count: conversation.message_count,
        turn_count: conversation.turn_count,
        session_id: conversation.session_id.clone(),
        session_metadata: conversation.session_metadata.clone(),
        messages_loaded: false,
    }
}

impl AiWorkspaceEntity {
    pub(in crate::workspace) fn schedule_history(&mut self, cx: &mut Context<Self>) {
        cx.on_app_quit(|this, _| {
            this.persist_agent_records();
            let operations = this.history_operations();
            let writer = this.history.writer.clone();
            let pending = this.history.write.take();
            async move {
                if let Some(pending) = pending {
                    let _ = pending.await;
                }
                if let Some(writer) = writer {
                    let Ok(batch) = operations else {
                        return;
                    };
                    let Ok(batch) = tokio::task::spawn_blocking(move || batch.prepare()).await
                    else {
                        return;
                    };
                    if !batch.operations.is_empty() {
                        let _ = writer.submit(batch.operations).await;
                    }
                    let _ = writer.flush().await;
                }
            }
        })
        .detach();
        let wake = self.history.wake.clone();
        self.history.watch = Some(cx.spawn(async move |weak, cx| {
            loop {
                wake.notified().await;
                if weak.update(cx, |this, cx| this.drain_history(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    pub(in crate::workspace) fn initialize_history(
        &mut self,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.history
            .migration_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        self.history.migration_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelled = self.history.migration_cancel.clone();
        self.history.generation += 1;
        let generation = self.history.generation;
        if let Some(load) = self.history.load.take() {
            load.abort();
        }
        self.history.status = HistoryStatus::Loading;
        self.history.progress = None;
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        self.history.load = Some(self.task_runtime.spawn(async move {
            let progress_sender = sender.clone();
            let progress_wake = wake.clone();
            let result = tokio::task::spawn_blocking(move || {
                let store = ConversationStore::open_or_migrate_cancellable(
                    &path,
                    |done, total| {
                        let _ = progress_sender.send(Delivery::Progress(generation, done, total));
                        progress_wake.notify_one();
                    },
                    &cancelled,
                )?;
                let mut heads = Vec::new();
                let mut cursor = None;
                loop {
                    let page = store.list_heads(cursor, 100)?;
                    cursor = page.last().map(|head| {
                        (
                            head.conversation.updated_at_ms,
                            head.conversation.id.clone(),
                        )
                    });
                    let done = page.len() < 100;
                    heads.extend(page);
                    if done {
                        break;
                    }
                }
                Ok::<_, anyhow::Error>((store, heads))
            })
            .await;
            let result = result
                .map_err(|_| "History initialization interrupted".to_owned())
                .and_then(|result| result.map_err(|_| "History could not be opened".to_owned()));
            let _ = sender.send(Delivery::Initialized(generation, result));
            wake.notify_one();
        }));
        cx.notify();
    }

    fn drain_history(&mut self, cx: &mut Context<Self>) {
        while let Ok(delivery) = self.history.receiver.try_recv() {
            match delivery {
                Delivery::Archive(delivery) => self.apply_history_archive(delivery),
                Delivery::Progress(generation, done, total)
                    if generation == self.history.generation =>
                {
                    self.history.progress = Some((done, total))
                }
                Delivery::Initialized(generation, result)
                    if generation == self.history.generation =>
                {
                    self.history.load.take();
                    match result {
                        Ok((store, heads)) => {
                            self.history.changes.borrow_mut().revision =
                                heads.iter().map(|head| head.revision).max().unwrap_or(0);
                            self.history.committed = self.history.changes.borrow().revision;
                            NEXT_REVISION.fetch_max(
                                self.history.committed,
                                std::sync::atomic::Ordering::Relaxed,
                            );
                            self.history.branches = heads
                                .iter()
                                .map(|head| {
                                    (head.conversation.id.clone(), head.active_branch.clone())
                                })
                                .collect();
                            self.conversation_state.conversations =
                                heads.into_iter().map(|head| head.conversation).collect();
                            self.conversation_state.active_conversation_id = self
                                .conversation_state
                                .conversations
                                .first()
                                .map(|conversation| conversation.id.clone());
                            match HistoryWriter::new(store.clone()) {
                                Ok(writer) => {
                                    let mut status = writer.subscribe();
                                    let sender = self.history.sender.clone();
                                    let wake = self.history.wake.clone();
                                    self.history.status_watch =
                                        Some(self.task_runtime.spawn(async move {
                                            while status.changed().await.is_ok() {
                                                let _ = sender.send(Delivery::WriteState(
                                                    status.borrow_and_update().clone(),
                                                ));
                                                wake.notify_one();
                                            }
                                        }));
                                    self.history.writer = Some(writer);
                                    self.history.store = Some(store);
                                    self.history.status = HistoryStatus::Ready;
                                    self.chat_initialization_error = None;
                                    if let Some(id) =
                                        self.conversation_state.active_conversation_id.clone()
                                    {
                                        self.request_history_conversation(id);
                                    }
                                }
                                Err(_) => {
                                    self.history.status = HistoryStatus::Failed;
                                    self.chat_initialization_error =
                                        Some(AiChatInitializationError {
                                            message_key: "ai.chat.load_failed_generic",
                                            can_retry: true,
                                        });
                                }
                            }
                        }
                        Err(_) => {
                            self.history.status = HistoryStatus::Failed;
                            self.chat_initialization_error = Some(AiChatInitializationError {
                                message_key: "ai.chat.load_failed_generic",
                                can_retry: true,
                            });
                        }
                    }
                }
                Delivery::Page(delivery) => self.apply_history_page(delivery),
                Delivery::Body(delivery) => self.apply_history_body(delivery),
                Delivery::Agents(generation, id, result)
                    if generation == self.history.generation =>
                {
                    self.history.agents_load.take();
                    if self
                        .conversation_state
                        .conversations
                        .iter()
                        .any(|conversation| conversation.id == id)
                    {
                        match result {
                            Ok(records) => {
                                for record in records {
                                    self.agents
                                        .records
                                        .entry(record.snapshot.run.run_id.clone())
                                        .or_insert(record);
                                }
                            }
                            Err(()) => {
                                self.chat_initialization_error = Some(AiChatInitializationError {
                                    message_key: "ai.chat.load_failed_generic",
                                    can_retry: true,
                                })
                            }
                        }
                    }
                }
                Delivery::StreamSave => {
                    self.history.save_timer.borrow_mut().take();
                }

                Delivery::Written(revision, streams) => {
                    self.history.write.take();
                    self.history.committed = self.history.committed.max(revision);
                    let active = self.conversation_state.active_conversation_id.clone();
                    let changed = active.as_ref().is_some_and(|active| {
                        self.history.changes.borrow().messages.iter().any(
                            |((id, message), version)| {
                                id == active
                                    && !self.agents.message_runs.contains_key(message)
                                    && *version <= revision
                            },
                        )
                    });
                    let agent_changed = self.agents.detail.clone().filter(|run| {
                        self.history.changes.borrow().messages.iter().any(
                            |((_, message), version)| {
                                *version <= revision
                                    && self.agents.message_runs.get(message) == Some(run)
                            },
                        )
                    });
                    self.history.changes.borrow_mut().acknowledged(revision);
                    {
                        let mut changes = self.history.changes.borrow_mut();
                        for (key, snapshot) in streams {
                            if let Some(snapshot) = snapshot {
                                changes.stream_bases.insert(key, snapshot);
                            } else {
                                changes.stream_bases.remove(&key);
                            }
                        }
                        changes.stream_bases.retain(|(conversation, message), _| {
                            self.history_message(conversation, message)
                                .is_some_and(|message| message.is_streaming)
                        });
                    }
                    self.release_committed_agent_messages();
                    if let Some(run) = agent_changed {
                        if self
                            .agents
                            .records
                            .get(&run)
                            .is_some_and(|record| record.snapshot.state.is_terminal())
                        {
                            self.load_agent_message_page(run, None, cx);
                        }
                    }
                    if let Some(id) = active.filter(|id| {
                        !self.loading_conversations.contains(id)
                            && !self.history_has_unsaved_messages(id)
                    }) {
                        let structural = self.history.pages.get(&id).is_none_or(|page| {
                            self.history
                                .branches
                                .get(&id)
                                .is_some_and(|branch| branch != &page.branch)
                                || self.conversation_state.active_conversation().is_some_and(
                                    |conversation| {
                                        page.descriptions.keys().any(|id| {
                                            !conversation
                                                .messages
                                                .iter()
                                                .any(|message| &message.id == id)
                                        })
                                    },
                                )
                        });
                        if changed || structural {
                            self.refresh_history_page(id);
                        }
                    }
                }
                Delivery::WriteFailed => {
                    self.history.write.take();
                    self.history.status = HistoryStatus::Failed;
                }
                Delivery::WriteState(state) => {
                    self.history.status = if state == HistoryWriteState::Ready {
                        HistoryStatus::Ready
                    } else {
                        HistoryStatus::Failed
                    }
                }
                _ => {}
            }
        }
        if matches!(self.history.status, HistoryStatus::Ready)
            && self
                .history
                .writer
                .as_ref()
                .is_some_and(|writer| *writer.subscribe().borrow() == HistoryWriteState::Ready)
        {
            let mut pending = Vec::new();
            for (version, sender) in self.history.waiters.drain(..) {
                if version <= self.history.committed {
                    let _ = sender.send(true);
                } else if !sender.is_closed() {
                    pending.push((version, sender));
                }
            }
            self.history.waiters = pending;
        }
        self.history.answers.retain(|_, task| !task.is_finished());
        self.start_history_write();
        cx.notify();
    }

    pub(in crate::workspace) fn request_history_conversation(&mut self, id: String) {
        self.load_history_page(id, pages::PageDirection::Initial);
    }

    pub(in crate::workspace) fn request_history_agents(&mut self, id: &str) {
        let Some(store) = self.history.store.clone() else {
            return;
        };
        let id = id.to_owned();
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        if let Some(task) = self.history.agents_load.take() {
            task.abort();
        }
        let generation = self.history.generation;
        self.history.agents_load = Some(self.task_runtime.spawn(async move {
            let target = id.clone();
            let result = tokio::task::spawn_blocking(move || store.load_agent_summaries(&target))
                .await
                .map_err(|_| ())
                .and_then(|result| result.map_err(|_| ()));
            let _ = sender.send(Delivery::Agents(generation, id, result));
            wake.notify_one();
        }));
    }

    pub(in crate::workspace) fn history_has_unsaved_messages(&self, conversation: &str) -> bool {
        self.history
            .changes
            .borrow()
            .messages
            .keys()
            .any(|(id, message)| {
                id == conversation && !self.agents.message_runs.contains_key(message)
            })
    }

    pub(in crate::workspace) fn history_message_is_pending(
        &self,
        conversation: &str,
        message: &str,
    ) -> bool {
        self.history
            .changes
            .borrow()
            .messages
            .contains_key(&(conversation.into(), message.into()))
    }

    pub(in crate::workspace) fn create_conversation(
        &mut self,
        id: String,
        title: Option<String>,
        now: i64,
        profile: Option<String>,
    ) -> String {
        if let Some(previous) = self.conversation_state.active_conversation_id.clone() {
            if !self.loading_conversations.contains(&previous)
                && !self.history_has_unsaved_messages(&previous)
            {
                self.history.pages.remove(&previous);
                if let Some(conversation) = self
                    .conversation_state
                    .conversations
                    .iter_mut()
                    .find(|conversation| conversation.id == previous)
                {
                    conversation.messages.clear();
                    conversation.messages_loaded = false;
                }
            }
        }
        let id = self
            .conversation_state
            .create_conversation(id, title, now, profile);
        self.history_created(&id);
        id
    }
    pub(in crate::workspace) fn ensure_conversation(
        &mut self,
        id: String,
        title: Option<String>,
        now: i64,
        profile: Option<String>,
    ) -> String {
        self.conversation_state
            .active_conversation_id
            .clone()
            .unwrap_or_else(|| self.create_conversation(id, title, now, profile))
    }
    pub(in crate::workspace) fn add_message(&mut self, conversation: &str, message: AiChatMessage) {
        let id = message.id.clone();
        let counts = self
            .conversation_state
            .conversations
            .iter()
            .find(|item| item.id == conversation)
            .map(|item| (item.message_count, item.turn_count));
        let turns = oxideterm_ai::ai_conversation_turn_count(std::slice::from_ref(&message));
        self.conversation_state.add_message(conversation, message);
        if let (Some((messages, previous_turns)), Some(item)) = (
            counts,
            self.conversation_state
                .conversations
                .iter_mut()
                .find(|item| item.id == conversation),
        ) {
            item.message_count = messages.saturating_add(1);
            item.turn_count = previous_turns.saturating_add(turns);
        }
        self.history_message_changed(conversation, &id);
        self.history_metadata_changed(conversation);
    }

    fn history_message(&self, conversation: &str, id: &str) -> Option<&AiChatMessage> {
        if let Some(run) = self.agents.message_runs.get(id) {
            return self
                .agents
                .records
                .get(run)
                .filter(|record| record.snapshot.conversation_id == conversation)
                .and_then(|record| record.messages.iter().find(|message| message.id == id));
        }
        self.conversation_state
            .conversations
            .iter()
            .find(|item| item.id == conversation)
            .and_then(|item| item.messages.iter().find(|message| message.id == id))
    }

    fn history_message_branch(&self, conversation: &str, message: &str) -> String {
        self.agents
            .message_runs
            .get(message)
            .map(oxideterm_ai::agent_history_branch)
            .unwrap_or_else(|| {
                self.history
                    .branches
                    .get(conversation)
                    .cloned()
                    .unwrap_or_else(|| "main".into())
            })
    }

    pub(super) fn history_agent_created(
        &self,
        conversation: &str,
        run: oxideterm_ai::agent::AgentRunId,
    ) {
        let mut changes = self.history.changes.borrow_mut();
        let revision = changes.next();
        changes.extra.push((
            revision,
            HistoryMutation::CreateAgentHistory {
                conversation_id: conversation.into(),
                run_id: run,
                revision,
            },
        ));
    }

    pub(in crate::workspace) fn history_message_deleted(&self, conversation: &str, id: &str) {
        let branch = self.history_message_branch(conversation, id);
        let mut changes = self.history.changes.borrow_mut();
        changes.messages.remove(&(conversation.into(), id.into()));
        changes
            .full_messages
            .remove(&(conversation.into(), id.into()));
        changes
            .stream_bases
            .remove(&(conversation.into(), id.into()));
        let revision = changes.next();
        changes.extra.push((
            revision,
            HistoryMutation::DeleteMessage {
                conversation_id: conversation.into(),
                branch_id: branch,
                message_id: id.into(),
                revision,
            },
        ));
    }
    pub(in crate::workspace) fn history_message_changed(&self, conversation: &str, message: &str) {
        if self.history_message(conversation, message).is_none() {
            return;
        }
        let mut changes = self.history.changes.borrow_mut();
        let version = changes.next();
        changes
            .messages
            .insert((conversation.into(), message.into()), version);
        changes
            .full_messages
            .insert((conversation.into(), message.into()), version);
    }

    pub(in crate::workspace) fn history_stream_text_changed(
        &self,
        conversation: &str,
        message: &str,
    ) {
        if self.history_message(conversation, message).is_none() {
            return;
        }
        let mut changes = self.history.changes.borrow_mut();
        let revision = changes.next();
        changes
            .messages
            .insert((conversation.into(), message.into()), revision);
    }
    pub(in crate::workspace) fn history_metadata_changed(&self, conversation: &str) {
        if !self
            .conversation_state
            .conversations
            .iter()
            .any(|item| item.id == conversation)
        {
            return;
        }
        let mut changes = self.history.changes.borrow_mut();
        let version = changes.next();
        changes.metadata.insert(conversation.into(), version);
    }

    pub(in crate::workspace) fn history_title_changed(&self, conversation: &str) {
        let Some(item) = self
            .conversation_state
            .conversations
            .iter()
            .find(|item| item.id == conversation)
        else {
            return;
        };
        let mut changes = self.history.changes.borrow_mut();
        let revision = changes.next();
        changes.extra.push((
            revision,
            HistoryMutation::Rename {
                conversation_id: conversation.into(),
                title: item.title.clone(),
                updated_at: item.updated_at_ms,
                revision,
            },
        ));
    }
    pub(in crate::workspace) fn history_created(&self, conversation: &str) {
        let mut changes = self.history.changes.borrow_mut();
        let version = changes.next();
        changes.created.insert(conversation.into(), version);
    }
    pub(in crate::workspace) fn history_deleted(&self, conversation: &str) {
        let mut changes = self.history.changes.borrow_mut();
        let version = changes.next();
        changes.deleted.insert(conversation.into(), version);
        changes.messages.retain(|(id, _), _| id != conversation);
        changes
            .full_messages
            .retain(|(id, _), _| id != conversation);
        changes.stream_bases.retain(|(id, _), _| id != conversation);
        changes.metadata.remove(conversation);
        changes.created.remove(conversation);
        changes.events.retain(|(id, _, _), _| id != conversation);
    }
    pub(in crate::workspace) fn history_load_failed(&mut self) {
        self.chat_initialization_error = Some(AiChatInitializationError {
            message_key: "ai.chat.load_failed_generic",
            can_retry: true,
        });
    }
    pub(in crate::workspace) fn history_ready(&self) -> bool {
        !self.history.quitting
            && matches!(self.history.status, HistoryStatus::Ready)
            && self.chat_initialization_error.is_none()
            && self
                .history
                .writer
                .as_ref()
                .is_some_and(|writer| *writer.subscribe().borrow() == HistoryWriteState::Ready)
            && self
                .conversation_state
                .active_conversation()
                .is_none_or(|conversation| conversation.messages_loaded)
    }
    pub(in crate::workspace) fn retry_history_write(&mut self) {
        let Some(writer) = self.history.writer.clone() else {
            return;
        };
        if self
            .history
            .retry
            .as_ref()
            .is_some_and(|task| !task.is_finished())
        {
            return;
        }
        self.history.retry = Some(self.task_runtime.spawn(async move {
            let _ = writer.retry().await;
        }));
    }
    pub(in crate::workspace) fn schedule_history_stream_save(&self) {
        if self.history.save_timer.borrow().is_some() {
            return;
        }
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        *self.history.save_timer.borrow_mut() = Some(self.task_runtime.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let _ = sender.send(Delivery::StreamSave);
            wake.notify_one();
        }));
    }
    pub(in crate::workspace) fn request_history_save(&self) {
        let _ = self.history.sender.send(Delivery::Save);
        self.history.wake.notify_one();
    }
    pub(in crate::workspace) fn history_barrier(
        &mut self,
        sender: tokio::sync::oneshot::Sender<bool>,
    ) {
        let revision = self.history.changes.borrow().revision;
        if self.history.quitting && self.history.writer.is_none() && revision == 0 {
            // Initialization has admitted no work; cancellation preserves the source database.
            let _ = sender.send(true);
            return;
        }
        if matches!(self.history.status, HistoryStatus::Idle) {
            let _ = sender.send(true);
            return;
        }
        if self.history.committed >= revision
            && matches!(self.history.status, HistoryStatus::Ready)
            && self
                .history
                .writer
                .as_ref()
                .is_some_and(|writer| *writer.subscribe().borrow() == HistoryWriteState::Ready)
        {
            let _ = sender.send(true);
        } else {
            self.history.waiters.push((revision, sender));
            self.request_history_save();
        }
    }

    pub(in crate::workspace) fn history_event(
        &self,
        conversation: &str,
        family: &str,
        id: &str,
        value: serde_json::Value,
    ) {
        let mut changes = self.history.changes.borrow_mut();
        let revision = changes.next();
        let update = HistoryMutation::PutEvent {
            conversation_id: conversation.into(),
            family: family.into(),
            id: id.into(),
            value,
            revision,
        };
        if let Some(entry) = changes.extra.iter_mut().find(|(_,operation)| matches!(operation,HistoryMutation::PutEvent { conversation_id,family:event_family,id:event_id,.. } if conversation_id == conversation && event_family == family && event_id == id)) {
            *entry = (revision,update);
        } else { changes.extra.push((revision,update)); }
        drop(changes);
        self.request_history_save();
    }

    pub(super) fn queue_history_event(
        &self,
        conversation: &str,
        family: &str,
        id: &str,
        event: HistoryEvent,
    ) {
        let mut changes = self.history.changes.borrow_mut();
        let revision = changes.next();
        changes.events.insert(
            (conversation.into(), family.into(), id.into()),
            (revision, event),
        );
        drop(changes);
        self.request_history_save();
    }

    pub(in crate::workspace) fn capture_history_before_edit(&self, id: &str) {
        let Some(conversation) = self
            .conversation_state
            .conversations
            .iter()
            .find(|item| item.id == id)
        else {
            return;
        };
        let branch = self
            .history
            .branches
            .get(id)
            .cloned()
            .unwrap_or_else(|| "main".into());
        let mut changes = self.history.changes.borrow_mut();
        for message in &conversation.messages {
            if let Some(revision) = changes
                .messages
                .remove(&(id.to_owned(), message.id.clone()))
            {
                changes.extra.push((
                    revision,
                    HistoryMutation::PutMessage {
                        conversation_id: id.into(),
                        branch_id: branch.clone(),
                        message: message.clone(),
                        revision,
                    },
                ));
            }
        }
    }
    pub(in crate::workspace) fn compact_history(&mut self, id: &str, through_message: String) {
        let Some(anchor) = self
            .conversation_state
            .conversations
            .iter()
            .find(|item| item.id == id)
            .and_then(|conversation| conversation.messages.first())
            .cloned()
        else {
            return;
        };
        let source_branch = self
            .history
            .branches
            .get(id)
            .cloned()
            .unwrap_or_else(|| "main".into());
        let branch_id = uuid::Uuid::new_v4().to_string();
        let mut changes = self.history.changes.borrow_mut();
        let revision = changes.next();
        changes.extra.push((
            revision,
            HistoryMutation::Compact {
                conversation_id: id.into(),
                source_branch,
                branch_id: branch_id.clone(),
                through_message,
                anchor,
                revision,
            },
        ));
        changes.messages.retain(|(conversation, message), _| {
            conversation != id || self.agents.message_runs.contains_key(message)
        });
        self.history.branches.insert(id.into(), branch_id);
        if let Some(page) = self.history.pages.get_mut(id) {
            page.before = None;
            page.after = None;
        }
        drop(changes);
        self.history_metadata_changed(id);
        self.request_history_save();
    }
    pub(in crate::workspace) fn replace_history_tail(
        &mut self,
        id: &str,
        first: usize,
        replaced_message: Option<&str>,
    ) {
        let Some(conversation) = self
            .conversation_state
            .conversations
            .iter()
            .find(|item| item.id == id)
        else {
            return;
        };
        let after_message = first
            .checked_sub(1)
            .and_then(|index| conversation.messages.get(index))
            .map(|message| message.id.clone());
        let messages = conversation.messages[first..].to_vec();
        let source_branch = self
            .history
            .branches
            .get(id)
            .cloned()
            .unwrap_or_else(|| "main".into());
        let branch_id = uuid::Uuid::new_v4().to_string();
        let mut changes = self.history.changes.borrow_mut();
        let revision = changes.next();
        let before_message = (first == 0)
            .then_some(replaced_message)
            .flatten()
            .map(str::to_owned);
        changes.extra.push((
            revision,
            HistoryMutation::ReplaceTail {
                conversation_id: id.into(),
                source_branch,
                branch_id: branch_id.clone(),
                after_message,
                before_message,
                messages,
                revision,
            },
        ));
        changes.messages.retain(|(conversation, message), _| {
            conversation != id || self.agents.message_runs.contains_key(message)
        });
        self.history.branches.insert(id.into(), branch_id);
        if let Some(page) = self.history.pages.get_mut(id) {
            page.before = None;
            page.after = None;
        }
        drop(changes);
        self.history_metadata_changed(id);
        self.request_history_save();
    }

    fn history_operations(&self) -> serde_json::Result<JournalBatch> {
        let changes = self.history.changes.borrow();
        let revision = changes.revision;
        let mut operations = Vec::new();
        let streams = Vec::new();
        for (id, version) in &changes.created {
            if let Some(conversation) = self
                .conversation_state
                .conversations
                .iter()
                .find(|conversation| &conversation.id == id)
            {
                operations.push(HistoryMutation::Create {
                    conversation: metadata(conversation),
                    revision: *version,
                });
            }
        }
        for (id, version) in &changes.metadata {
            if let Some(conversation) = self
                .conversation_state
                .conversations
                .iter()
                .find(|conversation| &conversation.id == id)
            {
                operations.push(HistoryMutation::Metadata {
                    conversation: metadata(conversation),
                    revision: *version,
                });
            }
        }
        operations.extend(changes.extra.iter().map(|(_, operation)| operation.clone()));
        let mut events: Vec<_> = changes.events.iter().collect();
        events.sort_unstable_by_key(|(_, (revision, _))| *revision);
        for ((conversation_id, family, id), (revision, event)) in events {
            operations.push(HistoryMutation::PutEvent {
                conversation_id: conversation_id.clone(),
                family: family.clone(),
                id: id.clone(),
                value: event.value()?,
                revision: *revision,
            });
        }
        let mut pending = Vec::with_capacity(changes.messages.len());
        for ((conversation_id, message_id), revision) in &changes.messages {
            let branch = self.history_message_branch(conversation_id, message_id);
            let messages = self
                .agents
                .message_runs
                .get(message_id)
                .and_then(|run| self.agents.records.get(run))
                .filter(|record| &record.snapshot.conversation_id == conversation_id)
                .map(|record| &record.messages)
                .or_else(|| {
                    self.conversation_state
                        .conversations
                        .iter()
                        .find(|conversation| &conversation.id == conversation_id)
                        .map(|conversation| &conversation.messages)
                });
            let Some((index, message)) = messages.and_then(|messages| {
                messages
                    .iter()
                    .enumerate()
                    .find(|(_, message)| &message.id == message_id)
            }) else {
                continue;
            };
            pending.push((conversation_id, branch, index, message, *revision));
        }
        // New identities retain their order within the branch even after an earlier reply changes.
        pending.sort_unstable_by(|left, right| {
            (left.0, &left.1, left.2).cmp(&(right.0, &right.1, right.2))
        });
        let pending = pending
            .into_iter()
            .map(|(conversation_id, branch_id, _, message, revision)| {
                let key = (conversation_id.clone(), message.id.clone());
                PendingMessage {
                    conversation: conversation_id.clone(),
                    branch: branch_id,
                    message: Some(message.clone()),
                    revision,
                    base: self
                        .history
                        .write
                        .is_none()
                        .then(|| changes.stream_bases.get(&key).cloned())
                        .flatten(),
                    structural: changes.full_messages.contains_key(&key),
                }
            })
            .collect();
        for (id, version) in &changes.deleted {
            operations.push(HistoryMutation::DeleteConversation {
                conversation_id: id.clone(),
                revision: *version,
            });
        }
        drop(changes);
        Ok(JournalBatch {
            revision,
            operations,
            streams,
            pending,
        })
    }

    fn start_history_write(&mut self) {
        if self.history.write.is_some() || !matches!(self.history.status, HistoryStatus::Ready) {
            return;
        }
        let Some(writer) = self.history.writer.clone() else {
            return;
        };
        let Ok(batch) = self.history_operations() else {
            self.history.status = HistoryStatus::Failed;
            return;
        };
        if batch.operations.is_empty() && batch.pending.is_empty() {
            return;
        }
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        self.history.write = Some(self.task_runtime.spawn(async move {
            let prepared = tokio::task::spawn_blocking(move || batch.prepare()).await;
            match prepared {
                Ok(batch) => {
                    if writer.submit(batch.operations).await.is_ok() {
                        let _ = sender.send(Delivery::Written(batch.revision, batch.streams));
                    } else {
                        let _ = sender.send(Delivery::WriteFailed);
                    }
                }
                Err(_) => {
                    let _ = sender.send(Delivery::WriteFailed);
                }
            }
            wake.notify_one();
        }));
    }
}
