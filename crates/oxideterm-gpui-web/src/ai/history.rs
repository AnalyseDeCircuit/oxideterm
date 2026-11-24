use oxideterm_ai::*;
use oxideterm_gpui_ai::bridge::{HostFuture, HostTask, history::*};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
#[derive(Clone, Default)]
pub struct History(Rc<RefCell<HashMap<String, ConversationHead>>>);
fn done() -> HostTask {
    HostTask::new(|| {})
}
impl AiHistoryHost for History {
    fn open(
        &self,
        limit: usize,
    ) -> HostFuture<Result<(AiHistorySession, Vec<ConversationHead>, u64), HistoryReadFailed>> {
        let mut heads: Vec<_> = self.0.borrow().values().cloned().collect();
        heads.sort_by_key(|v| std::cmp::Reverse(v.conversation.updated_at_ms));
        let rev = heads.iter().map(|v| v.revision).max().unwrap_or(0);
        heads.truncate(limit);
        let session = AiHistorySession {
            reader: Rc::new(self.clone()),
            writer: Rc::new(self.clone()),
        };
        Box::pin(async move { Ok((session, heads, rev)) })
    }
}
impl AiHistoryWriter for History {
    fn accepts_stream_deltas(&self) -> bool {
        false
    }
    fn submit(&self, ops: Vec<HistoryMutation>) -> HostFuture<Result<(), HistoryWriteFailed>> {
        let result = (|| {
            let mut all = self.0.borrow().clone();
            for op in ops {
                use HistoryMutation::*;
                match op {
                    Create {
                        conversation,
                        revision,
                    } => {
                        all.insert(
                            conversation.id.clone(),
                            ConversationHead {
                                conversation,
                                revision,
                                metadata_revision: revision,
                                title_revision: revision,
                                structure_revision: revision,
                                active_branch: "main".into(),
                            },
                        );
                    }
                    Metadata {
                        mut conversation,
                        revision,
                    } => {
                        let h = all.get_mut(&conversation.id).ok_or(HistoryWriteFailed)?;
                        conversation.messages = std::mem::take(&mut h.conversation.messages);
                        h.conversation = conversation;
                        h.metadata_revision = revision;
                        h.revision = revision;
                    }
                    Rename {
                        conversation_id,
                        title,
                        updated_at,
                        revision,
                    } => {
                        let h = all.get_mut(&conversation_id).ok_or(HistoryWriteFailed)?;
                        h.conversation.title = title;
                        h.conversation.updated_at_ms = updated_at;
                        h.title_revision = revision;
                        h.revision = revision;
                    }
                    PutMessage {
                        conversation_id,
                        branch_id,
                        message,
                        revision,
                    } => {
                        let h = all
                            .get_mut(&conversation_id)
                            .filter(|h| h.active_branch == branch_id)
                            .ok_or(HistoryWriteFailed)?;
                        if let Some(m) = h
                            .conversation
                            .messages
                            .iter_mut()
                            .find(|m| m.id == message.id)
                        {
                            *m = message
                        } else {
                            h.conversation.messages.push(message);
                        }
                        h.revision = revision;
                        h.conversation.message_count = h.conversation.messages.len();
                    }
                    DeleteMessage {
                        conversation_id,
                        message_id,
                        revision,
                        ..
                    } => {
                        let h = all.get_mut(&conversation_id).ok_or(HistoryWriteFailed)?;
                        h.conversation.messages.retain(|m| m.id != message_id);
                        h.revision = revision;
                        h.conversation.message_count = h.conversation.messages.len();
                    }
                    DeleteConversation {
                        conversation_id, ..
                    } => {
                        all.remove(&conversation_id);
                    }
                    PutEvent { .. } => {}
                    _ => return Err(HistoryWriteFailed),
                }
            }
            self.0.replace(all);
            Ok(())
        })();
        Box::pin(std::future::ready(result))
    }
    fn flush(&self) -> HostFuture<Result<(), HistoryWriteFailed>> {
        Box::pin(async { Ok(()) })
    }
    fn retry(&self) -> HostFuture<Result<(), HistoryWriteFailed>> {
        self.flush()
    }
    fn ready(&self) -> bool {
        true
    }
}
impl History {
    fn chat(&self, c: &str) -> Result<AiConversation, HistoryReadFailed> {
        self.0
            .borrow()
            .get(c)
            .map(|h| h.conversation.clone())
            .ok_or(HistoryReadFailed)
    }
    fn msg(&self, c: &str, id: &str) -> Result<AiChatMessage, HistoryReadFailed> {
        self.chat(c)?
            .messages
            .into_iter()
            .find(|m| m.id == id)
            .ok_or(HistoryReadFailed)
    }
    fn descriptors(&self, c: &str) -> Result<MessagePage, HistoryReadFailed> {
        let store = self.0.borrow();
        let h = store.get(c).ok_or(HistoryReadFailed)?;
        Ok(MessagePage {
            revision: h.revision,
            before: None,
            after: None,
            messages: h
                .conversation
                .messages
                .iter()
                .enumerate()
                .map(|(i, m)| MessageDescriptor {
                    id: m.id.clone(),
                    storage_id: m.id.clone(),
                    sequence: i as u64 + 1,
                    role: m.role.clone(),
                    timestamp_ms: m.timestamp_ms,
                    preview: m.content.chars().take(160).collect(),
                    turn_count: 0,
                    history_count: 0,
                    revision: h.revision,
                })
                .collect(),
        })
    }
}
impl AiHistoryReader for History {
    fn conversation_head(&self, id: &str) -> Result<Option<ConversationHead>, HistoryReadFailed> {
        Ok(self.0.borrow().get(id).cloned())
    }
    fn archive(
        &self,
        conversation: String,
        _range: oxideterm_ai::AiHistoryRange,
        _before: Option<HistoryCursor>,
        reply: HistoryReply<MessagePage>,
    ) -> HostTask {
        reply(self.descriptors(&conversation));
        done()
    }
    fn communication(
        &self,
        _conversation: String,
        _run: oxideterm_ai::agent::AgentRunId,
        _before: Option<u64>,
        reply: HistoryReply<Option<oxideterm_ai::AgentCommunicationPage>>,
    ) -> HostTask {
        reply(Ok(None));
        done()
    }
    fn branch_descriptions(
        &self,
        conversation: String,
        _branch: String,
        _cursor: Option<HistoryCursor>,
        _newer: bool,
        reply: HistoryReply<MessagePage>,
    ) -> HostTask {
        reply(self.descriptors(&conversation));
        done()
    }
    fn model_context(
        &self,
        conversation: String,
        _branch: String,
        _budget: usize,
        _provider: String,
        _model: String,
        reply: HistoryReply<oxideterm_ai::AiConversation>,
    ) -> HostTask {
        reply(self.chat(&conversation));
        done()
    }
    fn message_text(
        &self,
        conversation: String,
        storage: String,
        _revision: u64,
        reply: HistoryReply<String>,
    ) -> HostTask {
        reply(self.msg(&conversation, &storage).map(|m| m.content));
        done()
    }
    fn message_by_id(
        &self,
        conversation: String,
        _branch: String,
        id: String,
        reply: HistoryReply<oxideterm_ai::AiChatMessage>,
    ) -> HostTask {
        reply(self.msg(&conversation, &id));
        done()
    }
    fn retry_messages(
        &self,
        conversation: String,
        _branch: String,
        reply: HistoryReply<Vec<oxideterm_ai::AiChatMessage>>,
    ) -> HostTask {
        reply(self.chat(&conversation).map(|c| c.messages));
        done()
    }
    fn branch_tail(
        &self,
        conversation: String,
        _range: oxideterm_ai::AiHistoryRange,
        reply: HistoryReply<Vec<oxideterm_ai::AiChatMessage>>,
    ) -> HostTask {
        reply(self.chat(&conversation).map(|c| c.messages));
        done()
    }
    fn agent_summaries(
        &self,
        _conversation: String,
        reply: HistoryReply<Vec<oxideterm_ai::agent::AgentRecord>>,
    ) -> HostTask {
        reply(Ok(vec![]));
        done()
    }
    fn conversations(
        &self,
        archived: bool,
        _cursor: Option<(i64, String)>,
        limit: usize,
        reply: HistoryReply<Vec<ConversationHead>>,
    ) -> HostTask {
        let mut rows = self
            .0
            .borrow()
            .values()
            .filter(|h| h.conversation.archived == archived)
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by_key(|h| std::cmp::Reverse(h.conversation.updated_at_ms));
        rows.truncate(limit);
        reply(Ok(rows));
        done()
    }
    fn messages(
        &self,
        conversation: String,
        expected_branch: String,
        _window: MessageWindow,
        reply: HistoryReply<(String, MessagePage)>,
    ) -> HostTask {
        reply(
            self.descriptors(&conversation)
                .map(|page| (expected_branch, page)),
        );
        done()
    }
    fn body(
        &self,
        conversation: String,
        storage_id: String,
        _revision: u64,
        _section: Option<u64>,
        reply: HistoryReply<Arc<HistoryMessageView>>,
    ) -> HostTask {
        reply(self.msg(&conversation, &storage_id).map(|message| {
            Arc::new(HistoryMessageView {
                message,
                section: 0,
                first_section: 0,
                sections: 1,
                more: vec![],
            })
        }));
        done()
    }
    fn content(
        &self,
        _cursor: HistoryContentCursor,
        reply: HistoryReply<HistoryContentPage>,
    ) -> HostTask {
        reply(Err(HistoryReadFailed));
        done()
    }
    fn page(
        &self,
        conversation: String,
        _branch: String,
        _before: Option<HistoryCursor>,
        _limit: usize,
        reply: HistoryReply<HistoryPage>,
    ) -> HostTask {
        reply(self.chat(&conversation).map(|c| HistoryPage {
            messages: c.messages,
            before: None,
            revision: self.0.borrow()[&conversation].revision,
        }));
        done()
    }
}
