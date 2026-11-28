use super::*;

use crate::bridge::history::{AiHistoryHost, AiHistorySession, AiHistoryWriter};
use oxideterm_ai::{AiChatMessage, AiConversation, HistoryCursor, HistoryMutation};
#[cfg(not(target_family = "wasm"))]
use oxideterm_ai::{ConversationStore, HistoryWriteState, HistoryWriter};
use std::cell::RefCell;
mod archives;
mod conversations;
mod pages;
pub use pages::HistoryViewOwner;
static NEXT_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
type StreamBases = Vec<(
    (String, String),
    Option<oxideterm_ai::HistoryStreamSnapshot>,
)>;

#[cfg(not(target_family = "wasm"))]
impl crate::stream::StreamReply<bool> for tokio::sync::oneshot::Sender<bool> {
    fn active(&self) -> bool {
        !self.is_closed()
    }
    fn resolve(self: Box<Self>, value: bool) {
        let _ = (*self).send(value);
    }
}

impl crate::stream::StreamReply<bool> for futures::channel::oneshot::Sender<bool> {
    fn active(&self) -> bool {
        !self.is_canceled()
    }
    fn resolve(self: Box<Self>, value: bool) {
        let _ = (*self).send(value);
    }
}

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
    fn prepare(mut self, sanitize: bool, stream_deltas: bool) -> Self {
        for mut pending in std::mem::take(&mut self.pending) {
            let message = pending.message.as_ref().unwrap();
            let key = (pending.conversation.clone(), message.id.clone());
            if let Some((delta, snapshot)) = pending
                .base
                .as_ref()
                .filter(|_| stream_deltas)
                .and_then(|base| {
                    if pending.structural {
                        base.message_delta(message, pending.revision)
                    } else {
                        base.delta(message, pending.revision)
                    }
                })
            {
                self.operations.push(HistoryMutation::StreamText {
                    conversation_id: pending.conversation.clone(),
                    branch_id: pending.branch.clone(),
                    message_id: message.id.clone(),
                    delta,
                });
                self.streams.push((key, Some(snapshot)));
                continue;
            }
            let snapshot = (stream_deltas
                && message.is_streaming
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
        if sanitize {
            for operation in &mut self.operations {
                operation.sanitize();
            }
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

pub enum HistoryStatus {
    Idle,
    Loading,
    Ready,
    Failed,
}

type AnswerAction = Box<dyn FnOnce() + Send>;
type PendingAnswer = Arc<parking_lot::Mutex<Option<AnswerAction>>>;
struct AnswerCommit(std::sync::Weak<parking_lot::Mutex<Option<AnswerAction>>>);
impl crate::stream::StreamReply<bool> for AnswerCommit {
    fn active(&self) -> bool {
        self.0.strong_count() > 0
    }
    fn resolve(self: Box<Self>, value: bool) {
        if let Some(answer) = self.0.upgrade() {
            let action = answer.lock().take();
            if value && let Some(deliver) = action {
                deliver();
            }
        }
    }
}
pub struct AiHistoryState {
    session: Option<AiHistorySession>,
    foreground: Option<gpui::ForegroundExecutor>,
    background: Option<gpui::BackgroundExecutor>,
    #[cfg(not(target_family = "wasm"))]
    pub store: Option<ConversationStore>,
    conversation_lists: [conversations::ConversationListPage; 2],
    conversation_list_epoch: u64,
    pub(super) deleted_conversations: HashSet<String>,
    #[cfg(not(target_family = "wasm"))]
    pub writer: Option<HistoryWriter>,
    pub status: HistoryStatus,
    pub quitting: bool,
    pub migration_cancel: Arc<std::sync::atomic::AtomicBool>,
    retry: Option<Task<()>>,
    pub progress: Option<(usize, usize)>,
    pub archives: HashMap<(String, String), archives::ArchiveView>,
    archive_tasks: HashMap<(String, String), crate::bridge::HostTask>,
    #[cfg(not(target_family = "wasm"))]
    pub answers: HashMap<(u64, String), tokio::task::JoinHandle<()>>,
    pub(super) hosted_answers: HashMap<(u64, String), PendingAnswer>,
    pub model_contexts: HashMap<String, AiConversation>,
    pub compaction_key_loads: HashMap<String, Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    pub compaction_runs: HashMap<String, (u64, Option<tokio::task::JoinHandle<()>>)>,
    #[cfg(not(target_family = "wasm"))]
    pub(super) hosted_compaction_tasks: HashMap<String, crate::bridge::HostTask>,
    #[cfg(target_family = "wasm")]
    pub compaction_runs: HashMap<String, (u64, Option<crate::bridge::HostTask>)>,
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
    wake: oxideterm_workspace::delivery::ActiveDeliveryWake,
    watch: Option<Task<()>>,
    write: Option<Task<()>>,
    load: Option<crate::bridge::HostTask>,
    #[cfg(not(target_family = "wasm"))]
    status_watch: Option<crate::bridge::HostTask>,
    agents_load: Option<crate::bridge::HostTask>,
    save_timer: RefCell<Option<Task<()>>>,
    generation: u64,
    page_generation: u64,
    committed: u64,
    waiters: Vec<(u64, crate::stream::CommitReply)>,
}

enum Delivery {
    #[cfg(not(target_family = "wasm"))]
    Initialized(
        u64,
        Result<(ConversationStore, Vec<oxideterm_ai::ConversationHead>, u64), String>,
    ),
    Conversations(
        u64,
        u64,
        bool,
        Result<Vec<oxideterm_ai::ConversationHead>, ()>,
    ),
    #[cfg(not(target_family = "wasm"))]
    Progress(u64, usize, usize),
    Written(u64, StreamBases),
    WriteFailed,
    #[cfg(not(target_family = "wasm"))]
    WriteState(bool),
    Retried(bool),
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
            session: None,
            foreground: None,
            background: None,
            #[cfg(not(target_family = "wasm"))]
            store: None,
            conversation_lists: Default::default(),
            conversation_list_epoch: 0,
            deleted_conversations: HashSet::new(),
            #[cfg(not(target_family = "wasm"))]
            writer: None,
            status: HistoryStatus::Idle,
            quitting: false,
            migration_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            retry: None,
            progress: None,
            archives: HashMap::new(),
            archive_tasks: HashMap::new(),
            #[cfg(not(target_family = "wasm"))]
            answers: HashMap::new(),
            hosted_answers: HashMap::new(),
            model_contexts: HashMap::new(),
            compaction_key_loads: HashMap::new(),
            compaction_runs: HashMap::new(),
            #[cfg(not(target_family = "wasm"))]
            hosted_compaction_tasks: HashMap::new(),
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
            wake: Default::default(),
            watch: None,
            write: None,
            load: None,
            #[cfg(not(target_family = "wasm"))]
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
        for (_, (_, task)) in self.compaction_runs.drain() {
            if let Some(task) = task {
                task.abort();
            }
        }
        #[cfg(not(target_family = "wasm"))]
        for (_, task) in self.answers.drain() {
            task.abort();
        }
        for (_, task) in self.archive_tasks.drain() {
            task.abort();
        }
        if let Some(task) = self.agents_load.take() {
            task.abort();
        }
        if let Some(task) = self.load.take() {
            task.abort();
        }
        #[cfg(not(target_family = "wasm"))]
        if let Some(task) = self.status_watch.take() {
            task.abort();
        }
    }
}

fn metadata(conversation: &AiConversation) -> AiConversation {
    AiConversation {
        archived: conversation.archived,
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
    pub fn committed_history(&mut self) -> futures::channel::oneshot::Receiver<bool> {
        let (sender, receiver) = futures::channel::oneshot::channel();
        self.after_history_commit(Box::new(sender));
        receiver
    }
    pub fn history_reader(&self) -> Option<Rc<dyn crate::bridge::history::AiHistoryReader>> {
        if let Some(session) = &self.history.session {
            return Some(session.reader.clone());
        }
        #[cfg(not(target_family = "wasm"))]
        if let Some(store) = self.history.store.clone() {
            return Some(Rc::new(crate::bridge::history::DesktopHistoryReader {
                runtime: self.task_runtime().clone(),
                store,
            }));
        }
        None
    }
    pub(crate) fn has_history_writer(&self) -> bool {
        self.history_writer().is_some()
    }

    fn history_writer(&self) -> Option<Rc<dyn AiHistoryWriter>> {
        if let Some(session) = &self.history.session {
            return Some(session.writer.clone());
        }
        #[cfg(not(target_family = "wasm"))]
        if let Some(writer) = self.history.writer.clone() {
            return Some(Rc::new(crate::bridge::history::DesktopHistoryWriter {
                runtime: self.task_runtime().clone(),
                writer,
            }));
        }
        None
    }
    pub fn schedule_history(&mut self, cx: &mut Context<Self>) {
        self.history.foreground = Some(cx.foreground_executor().clone());
        self.history.background = Some(cx.background_executor().clone());
        cx.on_app_quit(|this, cx| {
            this.persist_agent_records();
            let operations = this.history_operations();
            let writer = this.history_writer();
            let pending = this.history.write.take();
            let background = cx.background_executor().clone();
            let hosted = this.history.session.is_some();
            async move {
                if let Some(pending) = pending {
                    pending.await;
                }
                if let (Some(writer), Ok(batch)) = (writer, operations) {
                    let deltas = writer.accepts_stream_deltas();
                    let batch = background
                        .spawn(async move { batch.prepare(hosted, deltas) })
                        .await;
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
                wake.wait().await;
                wake.take();
                if weak.update(cx, |this, cx| this.drain_history(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn initialize_history(&mut self, path: PathBuf, cx: &mut Context<Self>) {
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
        self.history.load = Some(
            self.task_runtime()
                .spawn(async move {
                    let progress_sender = sender.clone();
                    let progress_wake = wake.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let store = ConversationStore::open_or_migrate_cancellable(
                            &path,
                            |done, total| {
                                let _ = progress_sender
                                    .send(Delivery::Progress(generation, done, total));
                                progress_wake.mark();
                            },
                            &cancelled,
                        )?;
                        let heads = store.list_conversation_heads(
                            false,
                            None,
                            conversations::CONVERSATION_PAGE_SIZE + 1,
                        )?;
                        let revision = store.max_revision()?;
                        Ok::<_, anyhow::Error>((store, heads, revision))
                    })
                    .await;
                    let result = result
                        .map_err(|_| "History initialization interrupted".to_owned())
                        .and_then(|result| {
                            result.map_err(|_| "History could not be opened".to_owned())
                        });
                    let _ = sender.send(Delivery::Initialized(generation, result));
                    wake.mark();
                })
                .into(),
        );
        cx.notify();
    }

    fn install_history_heads(
        &mut self,
        mut heads: Vec<oxideterm_ai::ConversationHead>,
        revision: u64,
    ) {
        self.initialize_conversation_list(&mut heads);
        self.history.changes.borrow_mut().revision = revision;
        self.history.committed = self.history.changes.borrow().revision;
        NEXT_REVISION.fetch_max(self.history.committed, std::sync::atomic::Ordering::Relaxed);
        self.history.branches = heads
            .iter()
            .map(|head| (head.conversation.id.clone(), head.active_branch.clone()))
            .collect();
        self.conversation_state.conversations =
            heads.into_iter().map(|head| head.conversation).collect();
        self.conversation_state.active_conversation_id = self
            .conversation_state
            .conversations
            .iter()
            .find(|conversation| !conversation.archived)
            .map(|conversation| conversation.id.clone());
    }
    pub fn initialize_hosted_history(&mut self, host: &dyn AiHistoryHost, cx: &mut Context<Self>) {
        if !matches!(
            self.history.status,
            HistoryStatus::Idle | HistoryStatus::Failed
        ) || self.history.session.is_some()
        {
            return;
        }
        self.history.generation += 1;
        let generation = self.history.generation;
        self.history.status = HistoryStatus::Loading;
        let open = host.open(conversations::CONVERSATION_PAGE_SIZE + 1);
        let task = cx.spawn(async move |weak, cx| {
            let result = open.await;
            let _ = weak.update(cx, |ai, cx| {
                if ai.history.generation != generation {
                    return;
                }
                ai.history.load.take();
                match result {
                    Ok((session, heads, revision)) => {
                        ai.history.session = Some(session);
                        ai.install_history_heads(heads, revision);
                        ai.history.status = HistoryStatus::Ready;
                        ai.chat_initialization_error = None;
                        if ai.chat_ui.show_archived_conversations {
                            ai.request_conversation_list_page(true);
                        }
                        if let Some(id) = ai.conversation_state.active_conversation_id.clone() {
                            ai.request_history_conversation(id);
                        }
                    }
                    Err(_) => {
                        ai.history.status = HistoryStatus::Failed;
                        ai.history_load_failed();
                    }
                }
                cx.notify();
            });
        });
        self.history.load = Some(crate::bridge::HostTask::new(move || drop(task)));
        cx.notify();
    }
    fn drain_history(&mut self, cx: &mut Context<Self>) {
        while let Ok(delivery) = self.history.receiver.try_recv() {
            match delivery {
                Delivery::Conversations(generation, epoch, archived, result)
                    if generation == self.history.generation
                        && epoch == self.history.conversation_list_epoch =>
                {
                    self.apply_conversation_list(archived, result);
                }
                Delivery::Archive(delivery) => self.apply_history_archive(delivery),
                #[cfg(not(target_family = "wasm"))]
                Delivery::Progress(generation, done, total)
                    if generation == self.history.generation =>
                {
                    self.history.progress = Some((done, total))
                }
                #[cfg(not(target_family = "wasm"))]
                Delivery::Initialized(generation, result)
                    if generation == self.history.generation =>
                {
                    self.history.load.take();
                    match result {
                        Ok((store, heads, revision)) => {
                            self.install_history_heads(heads, revision);
                            match HistoryWriter::new(store.clone()) {
                                Ok(writer) => {
                                    let mut status = writer.subscribe();
                                    let sender = self.history.sender.clone();
                                    let wake = self.history.wake.clone();
                                    self.history.status_watch = Some(
                                        self.task_runtime()
                                            .spawn(async move {
                                                while status.changed().await.is_ok() {
                                                    let _ = sender.send(Delivery::WriteState(
                                                        *status.borrow_and_update()
                                                            == HistoryWriteState::Ready,
                                                    ));
                                                    wake.mark();
                                                }
                                            })
                                            .into(),
                                    );
                                    self.history.writer = Some(writer);
                                    self.history.store = Some(store);
                                    self.history.status = HistoryStatus::Ready;
                                    if self.chat_ui.show_archived_conversations {
                                        self.request_conversation_list_page(true);
                                    }
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
                Delivery::Retried(ready) => {
                    self.history.retry.take();
                    self.history.status = if ready {
                        HistoryStatus::Ready
                    } else {
                        HistoryStatus::Failed
                    };
                }
                #[cfg(not(target_family = "wasm"))]
                Delivery::WriteState(state) => {
                    self.history.status = if state {
                        HistoryStatus::Ready
                    } else {
                        HistoryStatus::Failed
                    }
                }
                _ => {}
            }
        }
        if matches!(self.history.status, HistoryStatus::Ready)
            && self.history_writer().is_some_and(|writer| writer.ready())
        {
            let mut pending = Vec::new();
            for (version, sender) in self.history.waiters.drain(..) {
                if version <= self.history.committed {
                    sender.resolve(true);
                } else if sender.active() {
                    pending.push((version, sender));
                }
            }
            self.history.waiters = pending;
        }
        #[cfg(not(target_family = "wasm"))]
        self.history.answers.retain(|_, task| !task.is_finished());
        self.history
            .hosted_answers
            .retain(|_, answer| answer.lock().is_some());
        self.start_history_write(cx);
        cx.notify();
    }

    pub fn request_history_conversation(&mut self, id: String) {
        self.load_history_page(id, pages::PageDirection::Initial);
    }

    pub fn request_history_agents(&mut self, id: &str) {
        let Some(store) = self.history_reader() else {
            return;
        };
        let id = id.to_owned();
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        if let Some(task) = self.history.agents_load.take() {
            task.abort();
        }
        let generation = self.history.generation;
        self.history.agents_load = Some(store.agent_summaries(
            id.clone(),
            Box::new(move |result| {
                let _ = sender.send(Delivery::Agents(generation, id, result.map_err(|_| ())));
                wake.mark();
            }),
        ));
    }

    pub fn history_has_unsaved_messages(&self, conversation: &str) -> bool {
        self.history
            .changes
            .borrow()
            .messages
            .keys()
            .any(|(id, message)| {
                id == conversation && !self.agents.message_runs.contains_key(message)
            })
    }

    pub fn history_message_is_pending(&self, conversation: &str, message: &str) -> bool {
        self.history
            .changes
            .borrow()
            .messages
            .contains_key(&(conversation.into(), message.into()))
    }

    pub fn create_conversation(
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
    pub fn ensure_conversation(
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
    pub fn add_message(&mut self, conversation: &str, message: AiChatMessage) {
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

    pub fn history_message_deleted(&self, conversation: &str, id: &str) {
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
    pub fn history_message_changed(&self, conversation: &str, message: &str) {
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

    pub fn history_stream_text_changed(&self, conversation: &str, message: &str) {
        if self.history_message(conversation, message).is_none() {
            return;
        }
        let mut changes = self.history.changes.borrow_mut();
        let revision = changes.next();
        changes
            .messages
            .insert((conversation.into(), message.into()), revision);
    }
    pub fn history_metadata_changed(&self, conversation: &str) {
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

    pub fn history_title_changed(&self, conversation: &str) {
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
    pub fn history_created(&self, conversation: &str) {
        let mut changes = self.history.changes.borrow_mut();
        let version = changes.next();
        changes.created.insert(conversation.into(), version);
    }
    pub fn history_deleted(&self, conversation: &str) {
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
    pub fn history_load_failed(&mut self) {
        self.chat_initialization_error = Some(AiChatInitializationError {
            message_key: "ai.chat.load_failed_generic",
            can_retry: true,
        });
    }
    pub fn history_ready(&self) -> bool {
        !self.history.quitting
            && matches!(self.history.status, HistoryStatus::Ready)
            && self.chat_initialization_error.is_none()
            && self.history_writer().is_some_and(|writer| writer.ready())
            && self
                .conversation_state
                .active_conversation()
                .is_none_or(|conversation| conversation.messages_loaded)
    }
    pub fn retry_history_write(&mut self) {
        if self.history.retry.is_some() {
            return;
        }
        let (Some(writer), Some(ui)) = (self.history_writer(), self.history.foreground.as_ref())
        else {
            return;
        };
        let tx = self.history.sender.clone();
        let wake = self.history.wake.clone();
        self.history.retry = Some(ui.spawn(async move {
            let ok = writer.retry().await.is_ok();
            let _ = tx.send(Delivery::Retried(ok));
            wake.mark();
        }));
    }
    pub fn schedule_history_stream_save(&self) {
        if self.history.save_timer.borrow().is_some() {
            return;
        }
        let Some(background) = self.history.background.as_ref() else {
            return;
        };
        let timer = background.timer(Duration::from_millis(250));
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        *self.history.save_timer.borrow_mut() = Some(background.spawn(async move {
            timer.await;
            let _ = sender.send(Delivery::StreamSave);
            wake.mark();
        }));
    }
    pub fn request_history_save(&self) {
        let _ = self.history.sender.send(Delivery::Save);
        self.history.wake.mark();
    }
    pub(super) fn deliver_answer_after_commit(
        &mut self,
        key: (u64, String),
        deliver: impl FnOnce() + Send + 'static,
    ) {
        #[cfg(not(target_family = "wasm"))]
        if let Some(desktop) = &self.desktop {
            let runtime = desktop.task_runtime.clone();
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.after_history_commit(Box::new(tx));
            let task = runtime.spawn(async move {
                if rx.await == Ok(true) {
                    deliver();
                }
            });
            if let Some(old) = self.history.answers.insert(key, task) {
                old.abort();
            }
            return;
        }
        let action: PendingAnswer = Arc::new(parking_lot::Mutex::new(Some(Box::new(deliver))));
        let waiter = AnswerCommit(Arc::downgrade(&action));
        self.history.hosted_answers.insert(key, action);
        self.after_history_commit(Box::new(waiter));
    }
    pub(crate) fn after_history_commit(&mut self, sender: crate::stream::CommitReply) {
        let revision = self.history.changes.borrow().revision;
        if self.history.quitting && self.history_writer().is_none() && revision == 0 {
            // Initialization has admitted no work; cancellation preserves the source database.
            sender.resolve(true);
            return;
        }
        if matches!(self.history.status, HistoryStatus::Idle) {
            sender.resolve(true);
            return;
        }
        if self.history.committed >= revision
            && matches!(self.history.status, HistoryStatus::Ready)
            && self.history_writer().is_some_and(|writer| writer.ready())
        {
            sender.resolve(true);
        } else {
            self.history.waiters.push((revision, sender));
            self.request_history_save();
        }
    }

    pub fn history_event(
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

    pub fn capture_history_before_edit(&self, id: &str) {
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
    pub fn compact_history(&mut self, id: &str, through_message: String) {
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
        self.chat_ui.live_turn_replays.remove(id);
        if let Some(page) = self.history.pages.get_mut(id) {
            page.before = None;
            page.after = None;
        }
        drop(changes);
        self.history_metadata_changed(id);
        self.request_history_save();
    }
    pub fn replace_history_tail(&mut self, id: &str, first: usize, replaced_message: Option<&str>) {
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
        self.chat_ui.live_turn_replays.remove(id);
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

    fn start_history_write(&mut self, cx: &mut Context<Self>) {
        if self.history.write.is_some() || !matches!(self.history.status, HistoryStatus::Ready) {
            return;
        }
        let Some(writer) = self.history_writer() else {
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
        let background = cx.background_executor().clone();
        let hosted = self.history.session.is_some();
        self.history.write = Some(cx.spawn(async move |_, _| {
            let deltas = writer.accepts_stream_deltas();
            let batch = background
                .spawn(async move { batch.prepare(hosted, deltas) })
                .await;
            let result = if writer.submit(batch.operations).await.is_ok() {
                Delivery::Written(batch.revision, batch.streams)
            } else {
                Delivery::WriteFailed
            };
            let _ = sender.send(result);
            wake.mark();
        }));
    }
}
