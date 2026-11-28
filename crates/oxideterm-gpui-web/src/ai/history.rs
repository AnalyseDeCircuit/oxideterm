use oxideterm_ai::*;
use oxideterm_gpui_ai::bridge::{HostFuture, HostTask, history::*};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
#[derive(Clone, Default)]
pub struct History(Rc<RefCell<Store>>);

#[derive(Clone, Default)]
struct Store {
    heads: HashMap<String, ConversationHead>,
    branches: HashMap<(String, String), Rc<Vec<AiChatMessage>>>,
}
fn done() -> HostTask {
    HostTask::new(|| {})
}
impl AiHistoryHost for History {
    fn open(
        &self,
        limit: usize,
    ) -> HostFuture<Result<(AiHistorySession, Vec<ConversationHead>, u64), HistoryReadFailed>> {
        let mut heads: Vec<_> = self.0.borrow().heads.values().cloned().collect();
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
                        all.heads.insert(
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
                        let h = all
                            .heads
                            .get_mut(&conversation.id)
                            .ok_or(HistoryWriteFailed)?;
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
                        let h = all
                            .heads
                            .get_mut(&conversation_id)
                            .ok_or(HistoryWriteFailed)?;
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
                            .heads
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
                        branch_id,
                        message_id,
                        revision,
                    } => {
                        let h = all
                            .heads
                            .get_mut(&conversation_id)
                            .filter(|h| h.active_branch == branch_id)
                            .ok_or(HistoryWriteFailed)?;
                        h.conversation.messages.retain(|m| m.id != message_id);
                        h.revision = revision;
                        h.conversation.message_count = h.conversation.messages.len();
                    }
                    DeleteConversation {
                        conversation_id, ..
                    } => {
                        all.heads.remove(&conversation_id);
                        all.branches.retain(|(c, _), _| c != &conversation_id);
                    }
                    ReplaceTail {
                        conversation_id,
                        source_branch,
                        branch_id,
                        after_message,
                        before_message,
                        mut messages,
                        revision,
                    } => {
                        let head = all
                            .heads
                            .get_mut(&conversation_id)
                            .ok_or(HistoryWriteFailed)?;
                        if head.active_branch != source_branch
                            || source_branch == branch_id
                            || all
                                .branches
                                .contains_key(&(conversation_id.clone(), branch_id.clone()))
                        {
                            return Err(HistoryWriteFailed);
                        }
                        let old = &head.conversation.messages;
                        let index = |id: &str| {
                            old.iter()
                                .position(|m| m.id == id)
                                .ok_or(HistoryWriteFailed)
                        };
                        let prefix = match (after_message, before_message) {
                            (Some(id), None) => index(&id)? + 1,
                            (None, Some(id)) => index(&id)?,
                            (None, None) => 0,
                            _ => return Err(HistoryWriteFailed),
                        };
                        let mut next = old[..prefix].to_vec();
                        next.append(&mut messages);
                        let previous = std::mem::replace(&mut head.conversation.messages, next);
                        all.branches
                            .insert((conversation_id, source_branch), Rc::new(previous));
                        head.active_branch = branch_id;
                        head.conversation.message_count = head.conversation.messages.len();
                        head.conversation.turn_count =
                            ai_conversation_turn_count(&head.conversation.messages);
                        head.structure_revision = revision;
                        head.revision = revision;
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
    pub(super) fn ids(&self) -> Vec<String> {
        self.0.borrow().heads.keys().cloned().collect()
    }
    fn branch(
        &self,
        conversation: &str,
        branch: &str,
    ) -> Result<AiConversation, HistoryReadFailed> {
        let store = self.0.borrow();
        let h = store.heads.get(conversation).ok_or(HistoryReadFailed)?;
        let mut view = h.conversation.clone();
        if h.active_branch != branch {
            view.messages = store
                .branches
                .get(&(conversation.into(), branch.into()))
                .ok_or(HistoryReadFailed)?
                .as_ref()
                .clone();
            view.message_count = view.messages.len();
            view.turn_count = ai_conversation_turn_count(&view.messages);
        }
        Ok(view)
    }
    pub(super) fn request_messages(
        &self,
        c: &str,
        reply: &str,
    ) -> Result<Vec<(String, String)>, HistoryReadFailed> {
        let store = self.0.borrow();
        let messages = &store
            .heads
            .get(c)
            .ok_or(HistoryReadFailed)?
            .conversation
            .messages;
        let end = messages
            .iter()
            .position(|m| m.id == reply && m.role == AiChatRole::Assistant)
            .ok_or(HistoryReadFailed)?;
        Ok(messages[..end]
            .iter()
            .filter(|m| !m.content.trim().is_empty())
            .map(|m| {
                let role = match m.role {
                    AiChatRole::User => "user",
                    AiChatRole::Assistant => "assistant",
                    AiChatRole::Tool => "tool",
                    AiChatRole::System => "system",
                };
                (role.into(), m.content.clone())
            })
            .collect())
    }

    fn msg(&self, c: &str, storage: &str) -> Result<AiChatMessage, HistoryReadFailed> {
        let (branch, id) = storage.split_once(':').ok_or(HistoryReadFailed)?;
        let store = self.0.borrow();
        store
            .messages(c, branch)?
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or(HistoryReadFailed)
    }

    fn descriptors(
        &self,
        c: &str,
        branch: &str,
        range: Option<&AiHistoryRange>,
        window: MessageWindow,
    ) -> Result<MessagePage, HistoryReadFailed> {
        let store = self.0.borrow();
        let messages = store.messages(c, branch)?;
        let revision = store.heads[c].revision;
        let bounds = range
            .map(|r| message_range(messages, r))
            .transpose()?
            .unwrap_or(0..messages.len());
        let (page, before, after) =
            message_window(c, branch, revision, bounds, window, HISTORY_PAGE_SIZE)?;
        Ok(MessagePage {
            revision,
            before,
            after,
            messages: page
                .map(|i| {
                    let m = &messages[i];
                    MessageDescriptor {
                        id: m.id.clone(),
                        storage_id: format!("{branch}:{}", m.id),
                        sequence: i as u64 + 1,
                        role: m.role,
                        timestamp_ms: m.timestamp_ms,
                        preview: m.content.chars().take(160).collect(),
                        turn_count: ai_conversation_turn_count(std::slice::from_ref(m)),
                        history_count: m
                            .metadata
                            .as_ref()
                            .filter(|m| m.kind == "compaction-anchor")
                            .and_then(|m| m.original_count)
                            .unwrap_or(1),
                        revision,
                    }
                })
                .collect(),
        })
    }
}

impl Store {
    fn messages(&self, c: &str, branch: &str) -> Result<&[AiChatMessage], HistoryReadFailed> {
        let head = self.heads.get(c).ok_or(HistoryReadFailed)?;
        if head.active_branch == branch {
            Ok(&head.conversation.messages)
        } else {
            self.branches
                .get(&(c.into(), branch.into()))
                .map(|m| m.as_slice())
                .ok_or(HistoryReadFailed)
        }
    }
}

fn message_range(
    messages: &[AiChatMessage],
    range: &AiHistoryRange,
) -> Result<std::ops::Range<usize>, HistoryReadFailed> {
    let index = |id: &str| {
        messages
            .iter()
            .position(|m| m.id == id)
            .ok_or(HistoryReadFailed)
    };
    let start = range
        .first_message_id
        .as_deref()
        .map(index)
        .transpose()?
        .unwrap_or(0);
    let end = range
        .last_message_id
        .as_deref()
        .map(index)
        .transpose()?
        .map_or(messages.len(), |i| i + 1);
    (start <= end)
        .then_some(start..end)
        .ok_or(HistoryReadFailed)
}

fn message_window(
    c: &str,
    branch: &str,
    revision: u64,
    bounds: std::ops::Range<usize>,
    window: MessageWindow,
    limit: usize,
) -> Result<
    (
        std::ops::Range<usize>,
        Option<HistoryCursor>,
        Option<HistoryCursor>,
    ),
    HistoryReadFailed,
> {
    let clamp = |n: u64| {
        usize::try_from(n)
            .unwrap_or(usize::MAX)
            .clamp(bounds.start, bounds.end)
    };
    let cursor_seq = |cursor: HistoryCursor| {
        (cursor.conversation_id == c && cursor.branch_id == branch && cursor.revision == revision)
            .then_some(cursor.before_sequence)
            .ok_or(HistoryReadFailed)
    };
    let older = |end: usize| end.saturating_sub(limit).max(bounds.start)..end;
    let page = match window {
        MessageWindow::Latest => older(bounds.end),
        MessageWindow::Around(seq) => older(clamp(seq)),
        MessageWindow::Older(cur) => older(clamp(cursor_seq(cur)?.saturating_sub(1))),
        MessageWindow::Newer(cur) => {
            let start = clamp(cursor_seq(cur)?);
            start..start.saturating_add(limit).min(bounds.end)
        }
    };
    let cursor = |before_sequence| HistoryCursor {
        conversation_id: c.into(),
        branch_id: branch.into(),
        revision,
        before_sequence,
    };
    let before =
        (!page.is_empty() && page.start > bounds.start).then(|| cursor(page.start as u64 + 1));
    let after = (!page.is_empty() && page.end < bounds.end).then(|| cursor(page.end as u64));
    Ok((page, before, after))
}
impl AiHistoryReader for History {
    fn conversation_head(&self, id: &str) -> Result<Option<ConversationHead>, HistoryReadFailed> {
        Ok(self.0.borrow().heads.get(id).cloned())
    }
    fn archive(
        &self,
        conversation: String,
        range: AiHistoryRange,
        before: Option<HistoryCursor>,
        reply: HistoryReply<MessagePage>,
    ) -> HostTask {
        reply(self.descriptors(
            &conversation,
            &range.branch_id,
            Some(&range),
            before.map_or(MessageWindow::Latest, MessageWindow::Older),
        ));
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
        branch: String,
        cursor: Option<HistoryCursor>,
        newer: bool,
        reply: HistoryReply<MessagePage>,
    ) -> HostTask {
        let window = match cursor {
            None => MessageWindow::Latest,
            Some(c) if newer => MessageWindow::Newer(c),
            Some(c) => MessageWindow::Older(c),
        };
        reply(self.descriptors(&conversation, &branch, None, window));
        done()
    }
    fn model_context(
        &self,
        conversation: String,
        branch: String,
        _budget: usize,
        _provider: String,
        _model: String,
        reply: HistoryReply<oxideterm_ai::AiConversation>,
    ) -> HostTask {
        reply(self.branch(&conversation, &branch));
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
        branch: String,
        id: String,
        reply: HistoryReply<oxideterm_ai::AiChatMessage>,
    ) -> HostTask {
        reply(self.branch(&conversation, &branch).and_then(|c| {
            c.messages
                .into_iter()
                .find(|m| m.id == id)
                .ok_or(HistoryReadFailed)
        }));
        done()
    }
    fn retry_messages(
        &self,
        conversation: String,
        branch: String,
        reply: HistoryReply<Vec<oxideterm_ai::AiChatMessage>>,
    ) -> HostTask {
        reply(self.branch(&conversation, &branch).map(|c| c.messages));
        done()
    }
    fn branch_tail(
        &self,
        conversation: String,
        range: oxideterm_ai::AiHistoryRange,
        reply: HistoryReply<Vec<oxideterm_ai::AiChatMessage>>,
    ) -> HostTask {
        let result = (|| {
            let store = self.0.borrow();
            let messages = store.messages(&conversation, &range.branch_id)?;
            Ok(messages[message_range(messages, &range)?].to_vec())
        })();
        reply(result);
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
        cursor: Option<(i64, String)>,
        limit: usize,
        reply: HistoryReply<Vec<ConversationHead>>,
    ) -> HostTask {
        let mut rows = self
            .0
            .borrow()
            .heads
            .values()
            .filter(|h| {
                h.conversation.archived == archived
                    && cursor.as_ref().is_none_or(|(time, id)| {
                        (h.conversation.updated_at_ms, &h.conversation.id) < (*time, id)
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            (b.conversation.updated_at_ms, &b.conversation.id)
                .cmp(&(a.conversation.updated_at_ms, &a.conversation.id))
        });
        rows.truncate(limit);
        reply(Ok(rows));
        done()
    }
    fn messages(
        &self,
        conversation: String,
        expected_branch: String,
        window: MessageWindow,
        reply: HistoryReply<(String, MessagePage)>,
    ) -> HostTask {
        let branch = self
            .0
            .borrow()
            .heads
            .get(&conversation)
            .map(|h| h.active_branch.clone());
        reply(branch.ok_or(HistoryReadFailed).and_then(|branch| {
            if matches!(window, MessageWindow::Older(_) | MessageWindow::Newer(_))
                && branch != expected_branch
            {
                return Err(HistoryReadFailed);
            }
            self.descriptors(&conversation, &branch, None, window)
                .map(|page| (branch, page))
        }));
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
        branch: String,
        before: Option<HistoryCursor>,
        limit: usize,
        reply: HistoryReply<HistoryPage>,
    ) -> HostTask {
        let result = (|| {
            let store = self.0.borrow();
            let messages = store.messages(&conversation, &branch)?;
            let revision = store.heads[&conversation].revision;
            let (range, before, _) = message_window(
                &conversation,
                &branch,
                revision,
                0..messages.len(),
                before.map_or(MessageWindow::Latest, MessageWindow::Older),
                limit,
            )?;
            Ok(HistoryPage {
                messages: messages[range].to_vec(),
                before,
                revision,
            })
        })();
        reply(result);
        done()
    }
}
