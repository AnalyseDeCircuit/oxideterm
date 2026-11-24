use super::{HostReply, HostTask};
use oxideterm_ai::{
    ConversationHead, HistoryContentCursor, HistoryContentPage, HistoryCursor, HistoryMessageView,
    HistoryPage, MessagePage,
};
use std::sync::Arc;

pub enum MessageWindow {
    Latest,
    Around(u64),
    Older(HistoryCursor),
    Newer(HistoryCursor),
}

pub struct HistoryReadFailed;

/// Submit acknowledges the transaction only after it is durable. A rejected write can be retried.
pub trait AiHistoryWriter {
    /// Memory-backed writers can replace a message directly instead of storing journal deltas.
    fn accepts_stream_deltas(&self) -> bool {
        true
    }
    fn submit(
        &self,
        operations: Vec<oxideterm_ai::HistoryMutation>,
    ) -> super::HostFuture<Result<(), HistoryWriteFailed>>;
    fn flush(&self) -> super::HostFuture<Result<(), HistoryWriteFailed>>;
    fn retry(&self) -> super::HostFuture<Result<(), HistoryWriteFailed>>;
    fn ready(&self) -> bool;
}
pub struct HistoryWriteFailed;

#[cfg(not(target_family = "wasm"))]
pub(crate) struct DesktopHistoryWriter {
    pub runtime: Arc<tokio::runtime::Runtime>,
    pub writer: oxideterm_ai::HistoryWriter,
}

#[cfg(not(target_family = "wasm"))]
impl DesktopHistoryWriter {
    fn run(
        &self,
        future: impl std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
    ) -> super::HostFuture<Result<(), HistoryWriteFailed>> {
        let task = tokio_util::task::AbortOnDropHandle::new(self.runtime.spawn(future));
        Box::pin(async move {
            task.await
                .map_err(|_| HistoryWriteFailed)?
                .map_err(|_| HistoryWriteFailed)
        })
    }
}
#[cfg(not(target_family = "wasm"))]
impl AiHistoryWriter for DesktopHistoryWriter {
    fn submit(
        &self,
        operations: Vec<oxideterm_ai::HistoryMutation>,
    ) -> super::HostFuture<Result<(), HistoryWriteFailed>> {
        let writer = self.writer.clone();
        self.run(async move { writer.submit(operations).await })
    }
    fn flush(&self) -> super::HostFuture<Result<(), HistoryWriteFailed>> {
        let writer = self.writer.clone();
        self.run(async move { writer.flush().await })
    }
    fn retry(&self) -> super::HostFuture<Result<(), HistoryWriteFailed>> {
        let writer = self.writer.clone();
        self.run(async move { writer.retry().await })
    }
    fn ready(&self) -> bool {
        *self.writer.subscribe().borrow() == oxideterm_ai::HistoryWriteState::Ready
    }
}

#[derive(Clone)]
pub struct AiHistorySession {
    pub reader: std::rc::Rc<dyn AiHistoryReader>,
    pub writer: std::rc::Rc<dyn AiHistoryWriter>,
}

/// The host opens storage only when the owning view requests history.
pub trait AiHistoryHost {
    fn open(
        &self,
        head_limit: usize,
    ) -> super::HostFuture<Result<(AiHistorySession, Vec<ConversationHead>, u64), HistoryReadFailed>>;
}
pub type HistoryReply<T> = HostReply<Result<T, HistoryReadFailed>>;

pub(crate) async fn read_history<T: Send + 'static>(
    start: impl FnOnce(HistoryReply<T>) -> HostTask,
) -> Result<T, HistoryReadFailed> {
    let (sender, receiver) = futures::channel::oneshot::channel();
    let _request = start(Box::new(move |result| {
        let _ = sender.send(result);
    }));
    receiver.await.map_err(|_| HistoryReadFailed)?
}

/// Readers belong to an open history session. Cursors retain their branch and revision.
pub trait AiHistoryReader {
    fn conversation_head(&self, id: &str) -> Result<Option<ConversationHead>, HistoryReadFailed>;
    fn archive(
        &self,
        conversation: String,
        range: oxideterm_ai::AiHistoryRange,
        before: Option<HistoryCursor>,
        reply: HistoryReply<MessagePage>,
    ) -> HostTask;
    fn communication(
        &self,
        conversation: String,
        run: oxideterm_ai::agent::AgentRunId,
        before: Option<u64>,
        reply: HistoryReply<Option<oxideterm_ai::AgentCommunicationPage>>,
    ) -> HostTask;
    fn branch_descriptions(
        &self,
        conversation: String,
        branch: String,
        cursor: Option<HistoryCursor>,
        newer: bool,
        reply: HistoryReply<MessagePage>,
    ) -> HostTask;
    fn model_context(
        &self,
        conversation: String,
        branch: String,
        budget: usize,
        provider: String,
        model: String,
        reply: HistoryReply<oxideterm_ai::AiConversation>,
    ) -> HostTask;
    fn message_text(
        &self,
        conversation: String,
        storage: String,
        revision: u64,
        reply: HistoryReply<String>,
    ) -> HostTask;
    fn message_by_id(
        &self,
        conversation: String,
        branch: String,
        id: String,
        reply: HistoryReply<oxideterm_ai::AiChatMessage>,
    ) -> HostTask;
    fn retry_messages(
        &self,
        conversation: String,
        branch: String,
        reply: HistoryReply<Vec<oxideterm_ai::AiChatMessage>>,
    ) -> HostTask;
    fn branch_tail(
        &self,
        conversation: String,
        range: oxideterm_ai::AiHistoryRange,
        reply: HistoryReply<Vec<oxideterm_ai::AiChatMessage>>,
    ) -> HostTask;
    fn agent_summaries(
        &self,
        conversation: String,
        reply: HistoryReply<Vec<oxideterm_ai::agent::AgentRecord>>,
    ) -> HostTask;
    fn conversations(
        &self,
        archived: bool,
        cursor: Option<(i64, String)>,
        limit: usize,
        reply: HistoryReply<Vec<ConversationHead>>,
    ) -> HostTask;
    fn messages(
        &self,
        conversation: String,
        expected_branch: String,
        window: MessageWindow,
        reply: HistoryReply<(String, MessagePage)>,
    ) -> HostTask;
    fn body(
        &self,
        conversation: String,
        storage_id: String,
        revision: u64,
        section: Option<u64>,
        reply: HistoryReply<Arc<HistoryMessageView>>,
    ) -> HostTask;
    fn content(
        &self,
        cursor: HistoryContentCursor,
        reply: HistoryReply<HistoryContentPage>,
    ) -> HostTask;
    fn page(
        &self,
        conversation: String,
        branch: String,
        before: Option<HistoryCursor>,
        limit: usize,
        reply: HistoryReply<HistoryPage>,
    ) -> HostTask;
}

#[cfg(not(target_family = "wasm"))]
pub(crate) struct DesktopHistoryReader {
    pub runtime: Arc<tokio::runtime::Runtime>,
    pub store: oxideterm_ai::ConversationStore,
}

#[cfg(not(target_family = "wasm"))]
impl DesktopHistoryReader {
    fn read<T: Send + 'static>(
        &self,
        read: impl FnOnce(oxideterm_ai::ConversationStore) -> anyhow::Result<T> + Send + 'static,
        reply: HistoryReply<T>,
    ) -> HostTask {
        let store = self.store.clone();
        self.runtime
            .spawn(async move {
                let result = tokio::task::spawn_blocking(move || read(store))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .ok_or(HistoryReadFailed);
                reply(result);
            })
            .into()
    }
}

#[cfg(not(target_family = "wasm"))]
impl AiHistoryReader for DesktopHistoryReader {
    fn conversation_head(&self, id: &str) -> Result<Option<ConversationHead>, HistoryReadFailed> {
        self.store
            .conversation_head(id)
            .map_err(|_| HistoryReadFailed)
    }
    fn archive(
        &self,
        conversation: String,
        range: oxideterm_ai::AiHistoryRange,
        before: Option<HistoryCursor>,
        reply: HistoryReply<MessagePage>,
    ) -> HostTask {
        self.read(
            move |s| s.range_descriptions(&conversation, &range, before.as_ref(), 50),
            reply,
        )
    }
    fn communication(
        &self,
        conversation: String,
        run: oxideterm_ai::agent::AgentRunId,
        before: Option<u64>,
        reply: HistoryReply<Option<oxideterm_ai::AgentCommunicationPage>>,
    ) -> HostTask {
        self.read(
            move |store| store.agent_communication_page(&conversation, &run, before),
            reply,
        )
    }
    fn branch_descriptions(
        &self,
        conversation: String,
        branch: String,
        cursor: Option<HistoryCursor>,
        newer: bool,
        reply: HistoryReply<MessagePage>,
    ) -> HostTask {
        self.read(
            move |store| match (newer, cursor.as_ref()) {
                (true, Some(cursor)) => store.message_page_after(
                    &conversation,
                    &branch,
                    cursor,
                    oxideterm_ai::HISTORY_PAGE_SIZE,
                ),
                (true, None) => anyhow::bail!("History cursor is missing"),
                (false, cursor) => store.message_page(
                    &conversation,
                    &branch,
                    cursor,
                    oxideterm_ai::HISTORY_PAGE_SIZE,
                ),
            },
            reply,
        )
    }
    fn model_context(
        &self,
        conversation: String,
        branch: String,
        budget: usize,
        provider: String,
        model: String,
        reply: HistoryReply<oxideterm_ai::AiConversation>,
    ) -> HostTask {
        self.read(
            move |s| s.model_context(&conversation, &branch, budget, &provider, &model),
            reply,
        )
    }
    fn message_text(
        &self,
        conversation: String,
        storage: String,
        revision: u64,
        reply: HistoryReply<String>,
    ) -> HostTask {
        self.read(
            move |s| s.message_content(&conversation, &storage, revision),
            reply,
        )
    }
    fn message_by_id(
        &self,
        conversation: String,
        branch: String,
        id: String,
        reply: HistoryReply<oxideterm_ai::AiChatMessage>,
    ) -> HostTask {
        self.read(move |s| s.message_by_id(&conversation, &branch, &id), reply)
    }
    fn retry_messages(
        &self,
        conversation: String,
        branch: String,
        reply: HistoryReply<Vec<oxideterm_ai::AiChatMessage>>,
    ) -> HostTask {
        self.read(move |s| s.retry_messages(&conversation, &branch), reply)
    }
    fn agent_summaries(
        &self,
        conversation: String,
        reply: HistoryReply<Vec<oxideterm_ai::agent::AgentRecord>>,
    ) -> HostTask {
        self.read(move |s| s.load_agent_summaries(&conversation), reply)
    }
    fn branch_tail(
        &self,
        conversation: String,
        range: oxideterm_ai::AiHistoryRange,
        reply: HistoryReply<Vec<oxideterm_ai::AiChatMessage>>,
    ) -> HostTask {
        self.read(
            move |s| {
                let mut cursor = None;
                let mut pages = Vec::new();
                loop {
                    let page = s.range_messages(&conversation, &range, cursor.as_ref(), 50)?;
                    pages.push(page.messages);
                    cursor = page.before;
                    if cursor.is_none() {
                        break;
                    }
                }
                Ok(pages.into_iter().rev().flatten().collect())
            },
            reply,
        )
    }
    fn conversations(
        &self,
        archived: bool,
        cursor: Option<(i64, String)>,
        limit: usize,
        reply: HistoryReply<Vec<ConversationHead>>,
    ) -> HostTask {
        self.read(
            move |store| store.list_conversation_heads(archived, cursor, limit),
            reply,
        )
    }
    fn messages(
        &self,
        conversation: String,
        expected_branch: String,
        window: MessageWindow,
        reply: HistoryReply<(String, MessagePage)>,
    ) -> HostTask {
        self.read(
            move |store| {
                let branch = store
                    .conversation_head(&conversation)?
                    .ok_or_else(|| anyhow::anyhow!("History conversation is missing"))?
                    .active_branch;
                let limit = oxideterm_ai::HISTORY_PAGE_SIZE;
                let page = match window {
                    MessageWindow::Latest => {
                        store.message_page(&conversation, &branch, None, limit)?
                    }
                    MessageWindow::Around(sequence) => store.message_page_at(
                        &conversation,
                        &branch,
                        sequence.saturating_add(1),
                        limit,
                    )?,
                    MessageWindow::Older(cursor) if branch == expected_branch => {
                        store.message_page(&conversation, &branch, Some(&cursor), limit)?
                    }
                    MessageWindow::Newer(cursor) if branch == expected_branch => {
                        store.message_page_after(&conversation, &branch, &cursor, limit)?
                    }
                    _ => anyhow::bail!("History cursor expired"),
                };
                Ok((branch, page))
            },
            reply,
        )
    }
    fn body(
        &self,
        conversation: String,
        storage_id: String,
        revision: u64,
        section: Option<u64>,
        reply: HistoryReply<Arc<HistoryMessageView>>,
    ) -> HostTask {
        self.read(
            move |store| store.message_view(&conversation, &storage_id, revision, section),
            reply,
        )
    }
    fn content(
        &self,
        cursor: HistoryContentCursor,
        reply: HistoryReply<HistoryContentPage>,
    ) -> HostTask {
        self.read(move |store| store.content_page(&cursor), reply)
    }
    fn page(
        &self,
        conversation: String,
        branch: String,
        before: Option<HistoryCursor>,
        limit: usize,
        reply: HistoryReply<HistoryPage>,
    ) -> HostTask {
        self.read(
            move |store| store.page(&conversation, &branch, before.as_ref(), limit),
            reply,
        )
    }
}
