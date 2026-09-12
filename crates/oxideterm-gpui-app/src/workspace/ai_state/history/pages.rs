use super::*;
use oxideterm_ai::{
    HistoryContentCursor, HistoryContentPage, HistoryMessageView, MessageDescriptor, MessagePage,
};

const WINDOW_MESSAGES: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::workspace) enum HistoryViewOwner {
    Main(String),
    Archive(String, String),
    Agent(String, oxideterm_ai::agent::AgentRunId),
}
impl HistoryViewOwner {
    pub fn conversation(&self) -> &str {
        match self {
            Self::Main(id) | Self::Archive(id, _) | Self::Agent(id, _) => id,
        }
    }
}

#[derive(Default)]
pub(in crate::workspace) struct HistoryPageState {
    pub branch: String,
    pub descriptions: HashMap<String, MessageDescriptor>,
    pub before: Option<HistoryCursor>,
    pub after: Option<HistoryCursor>,
    pub loading: bool,
    pub failed: bool,
    pub bodies: HashMap<String, std::sync::Weak<HistoryMessageView>>,
    pub body_errors: HashSet<String>,
    pub body_revisions: HashMap<String, u64>,
    body_tokens: HashMap<String, u64>,
    sections: HashMap<String, u64>,
    body_tasks: HashMap<String, tokio::task::JoinHandle<()>>,
    anchor: Option<(String, gpui::Pixels)>,
    pub content: Option<(String, HistoryContentPage)>,
    pub content_loading: Option<String>,
    pub content_error: Option<String>,
    content_cursors: Vec<HistoryContentCursor>,
    content_token: u64,
    content_task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for HistoryPageState {
    fn drop(&mut self) {
        for (_, task) in self.body_tasks.drain() {
            task.abort();
        }
        if let Some(task) = self.content_task.take() {
            task.abort();
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum PageDirection {
    Initial,
    Around(u64),
    Older,
    Newer,
}

pub(super) struct PageDelivery {
    generation: u64,
    id: String,
    branch: String,
    direction: PageDirection,
    result: Result<MessagePage, ()>,
}

pub(super) struct BodyDelivery {
    generation: u64,
    owner: HistoryViewOwner,
    storage_id: String,
    message: String,
    revision: u64,
    token: u64,
    result: Result<Arc<HistoryMessageView>, ()>,
}

pub(super) struct ContentDelivery {
    generation: u64,
    owner: HistoryViewOwner,
    message: String,
    token: u64,
    result: Result<HistoryContentPage, ()>,
}

impl AiWorkspaceEntity {
    pub(in crate::workspace) fn history_view(
        &self,
        owner: &HistoryViewOwner,
    ) -> Option<&HistoryPageState> {
        match owner {
            HistoryViewOwner::Main(id) => self.history.pages.get(id),
            _ => self.history.auxiliary_pages.get(owner),
        }
    }
    fn history_view_mut(&mut self, owner: &HistoryViewOwner) -> Option<&mut HistoryPageState> {
        match owner {
            HistoryViewOwner::Main(id) => self.history.pages.get_mut(id),
            _ => self.history.auxiliary_pages.get_mut(owner),
        }
    }

    pub(super) fn refresh_history_page(&mut self, id: String) {
        let top = self.chat_ui.message_list_state.logical_scroll_top();
        let sequence = self.history.pages.get(&id).and_then(|page| {
            let headers = usize::from(page.before.is_some())
                + usize::from(self.chat_ui.context_trim_notice_count.is_some());
            self.conversation_state
                .active_conversation()
                .and_then(|conversation| {
                    conversation
                        .messages
                        .get(top.item_ix.saturating_sub(headers))
                })
                .and_then(|message| page.descriptions.get(&message.id))
                .map(|description| description.sequence)
        });
        let direction = if self.chat_ui.message_list_state.is_following_tail() {
            PageDirection::Initial
        } else {
            sequence
                .map(PageDirection::Around)
                .unwrap_or(PageDirection::Initial)
        };
        self.load_history_page(id, direction);
    }

    pub(in crate::workspace) fn request_history_page(&mut self, id: String, older: bool) {
        self.load_history_page(
            id,
            if older {
                PageDirection::Older
            } else {
                PageDirection::Newer
            },
        );
    }

    pub(super) fn load_history_page(&mut self, id: String, direction: PageDirection) {
        if self.history.quitting {
            return;
        }
        let Some(store) = self.history.store.clone() else {
            return;
        };
        let initial = matches!(direction, PageDirection::Initial | PageDirection::Around(_));
        if initial && self.history_has_unsaved_messages(&id) {
            return;
        }
        let page = self.history.pages.entry(id.clone()).or_default();
        if page.loading && !initial {
            return;
        }
        if initial {
            for (_, task) in page.body_tasks.drain() {
                task.abort();
            }
            page.body_errors.clear();
        }
        let cursor = match direction {
            PageDirection::Initial | PageDirection::Around(_) => None,
            PageDirection::Older => page.before.clone(),
            PageDirection::Newer => page.after.clone(),
        };
        if !initial && cursor.is_none() {
            return;
        }
        page.loading = true;
        page.failed = false;
        if let Some(task) = self.history.load.take() {
            task.abort();
        }
        self.history.page_generation += 1;
        let generation = self.history.page_generation;
        let expected_branch = self
            .history
            .branches
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "main".into());
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        self.history.load = Some(self.task_runtime.spawn(async move {
            let target = id.clone();
            let result = tokio::task::spawn_blocking(move || {
                let head = store
                    .conversation_head(&target)?
                    .ok_or_else(|| anyhow::anyhow!("History conversation is missing"))?;
                let branch = head.active_branch;
                if !initial && branch != expected_branch {
                    return Err(anyhow::anyhow!("History cursor expired"));
                }
                let page = match direction {
                    PageDirection::Initial | PageDirection::Older => store.message_page(
                        &target,
                        &branch,
                        cursor.as_ref(),
                        oxideterm_ai::HISTORY_PAGE_SIZE,
                    )?,
                    PageDirection::Around(sequence) => store.message_page_at(
                        &target,
                        &branch,
                        sequence.saturating_add(1),
                        oxideterm_ai::HISTORY_PAGE_SIZE,
                    )?,
                    PageDirection::Newer => store.message_page_after(
                        &target,
                        &branch,
                        cursor.as_ref().unwrap(),
                        oxideterm_ai::HISTORY_PAGE_SIZE,
                    )?,
                };
                Ok::<_, anyhow::Error>((branch, page))
            })
            .await;
            let (branch, result) = match result {
                Ok(Ok((branch, page))) => (branch, Ok(page)),
                _ => (String::new(), Err(())),
            };
            let _ = sender.send(Delivery::Page(PageDelivery {
                generation,
                id,
                branch,
                direction,
                result,
            }));
            wake.notify_one();
        }));
    }

    pub(super) fn apply_history_page(&mut self, delivery: PageDelivery) {
        if delivery.generation != self.history.page_generation
            || self.conversation_state.active_conversation_id.as_deref() != Some(&delivery.id)
        {
            return;
        }
        self.history.load.take();
        let page = self.history.pages.entry(delivery.id.clone()).or_default();
        page.loading = false;
        let Ok(incoming) = delivery.result else {
            page.failed = true;
            if matches!(
                delivery.direction,
                PageDirection::Older | PageDirection::Newer
            ) {
                let top = self.chat_ui.message_list_state.logical_scroll_top();
                let headers = usize::from(page.before.is_some())
                    + usize::from(self.chat_ui.context_trim_notice_count.is_some());
                let sequence = self
                    .conversation_state
                    .active_conversation()
                    .and_then(|conversation| {
                        conversation
                            .messages
                            .get(top.item_ix.saturating_sub(headers))
                    })
                    .and_then(|message| page.descriptions.get(&message.id))
                    .map(|description| description.sequence);
                if let Some(sequence) = sequence {
                    self.load_history_page(delivery.id, PageDirection::Around(sequence));
                }
            }
            return;
        };
        page.branch = delivery.branch.clone();
        let initial = matches!(delivery.direction, PageDirection::Initial);
        let fresh = matches!(
            delivery.direction,
            PageDirection::Initial | PageDirection::Around(_)
        );
        let Some(conversation) = self
            .conversation_state
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == delivery.id)
        else {
            return;
        };
        if !initial {
            let top = self.chat_ui.message_list_state.logical_scroll_top();
            let headers = usize::from(page.before.is_some())
                + usize::from(self.chat_ui.context_trim_notice_count.is_some());
            if let Some(message) = conversation
                .messages
                .get(top.item_ix.saturating_sub(headers))
            {
                page.anchor = Some((message.id.clone(), top.offset_in_item));
            }
        }
        if fresh {
            page.descriptions.clear();
            page.bodies.clear();
            page.sections.clear();
            page.body_revisions.clear();
            page.body_tokens.clear();
            page.content.take();
            page.content_cursors.clear();
        }
        let old_ids: HashSet<_> = page.descriptions.keys().cloned().collect();
        for description in incoming.messages {
            page.descriptions
                .insert(description.id.clone(), description);
        }
        match delivery.direction {
            PageDirection::Initial | PageDirection::Around(_) => {
                page.before = incoming.before;
                page.after = incoming.after;
            }
            PageDirection::Older => page.before = incoming.before,
            PageDirection::Newer => page.after = incoming.after,
        }
        let mut descriptions: Vec<_> = page.descriptions.values().cloned().collect();
        descriptions.sort_unstable_by_key(|message| message.sequence);
        if descriptions.len() > WINDOW_MESSAGES {
            match delivery.direction {
                PageDirection::Older => {
                    descriptions.truncate(WINDOW_MESSAGES);
                    page.after = descriptions.last().map(|message| HistoryCursor {
                        conversation_id: delivery.id.clone(),
                        branch_id: delivery.branch.clone(),
                        revision: incoming.revision,
                        before_sequence: message.sequence,
                    });
                }
                _ => {
                    descriptions.drain(..descriptions.len() - WINDOW_MESSAGES);
                    page.before = descriptions.first().map(|message| HistoryCursor {
                        conversation_id: delivery.id.clone(),
                        branch_id: delivery.branch.clone(),
                        revision: incoming.revision,
                        before_sequence: message.sequence,
                    });
                }
            }
        }
        let retained: HashSet<_> = descriptions
            .iter()
            .map(|message| message.id.clone())
            .collect();
        page.descriptions.retain(|id, _| retained.contains(id));
        page.bodies.retain(|id, _| retained.contains(id));
        page.sections.retain(|id, _| retained.contains(id));
        page.body_errors.retain(|id| retained.contains(id));
        page.body_revisions.retain(|id, _| retained.contains(id));
        page.body_tokens.retain(|id, _| retained.contains(id));
        page.body_tasks.retain(|id, task| {
            if retained.contains(id) {
                true
            } else {
                task.abort();
                false
            }
        });
        let changes = self.history.changes.borrow();
        let pending = &changes.messages;
        let mut live = HashMap::new();
        let mut tail = Vec::new();
        for message in std::mem::take(&mut conversation.messages) {
            if pending.contains_key(&(delivery.id.clone(), message.id.clone()))
                || message.is_streaming
            {
                if retained.contains(&message.id) {
                    live.insert(message.id.clone(), message);
                } else {
                    tail.push(message);
                }
            } else if !fresh && !old_ids.contains(&message.id) && !retained.contains(&message.id) {
                tail.push(message);
            }
        }
        conversation.messages = descriptions
            .into_iter()
            .map(|description| {
                live.remove(&description.id)
                    .unwrap_or_else(|| preview_message(&description))
            })
            .chain(tail)
            .collect();
        conversation.messages_loaded = true;
        drop(changes);
        self.history
            .branches
            .insert(delivery.id.clone(), delivery.branch);
        if initial {
            self.reset_chat_message_list();
            self.load_agent_summaries(&delivery.id);
        }
    }

    pub(in crate::workspace) fn restore_history_scroll_anchor(&mut self, id: &str) {
        let Some(page) = self.history.pages.get_mut(id) else {
            return;
        };
        let Some((anchor, offset)) = page.anchor.take() else {
            return;
        };
        let Some(conversation) = self.conversation_state.active_conversation() else {
            return;
        };
        let index = conversation
            .messages
            .iter()
            .position(|message| message.id == anchor)
            .unwrap_or_else(|| conversation.messages.len().saturating_sub(1));
        let headers = usize::from(page.before.is_some())
            + usize::from(self.chat_ui.context_trim_notice_count.is_some());
        self.chat_ui.message_list_state.scroll_to(gpui::ListOffset {
            item_ix: index + headers,
            offset_in_item: offset,
        });
    }

    pub(in crate::workspace) fn live_history_view(
        &self,
        owner: &HistoryViewOwner,
        id: &str,
    ) -> Option<Arc<HistoryMessageView>> {
        if matches!(owner, HistoryViewOwner::Archive(_, _)) {
            return None;
        }
        let message = self.history_message(owner.conversation(), id)?;
        if !message.is_streaming
            && !self.history_message_is_pending(owner.conversation(), id)
            && self
                .history_view(owner)
                .is_some_and(|page| page.descriptions.contains_key(id))
        {
            return None;
        }
        let section = self
            .history_view(owner)
            .and_then(|page| page.sections.get(id).copied());
        let revision = self
            .history
            .changes
            .borrow()
            .messages
            .get(&(owner.conversation().into(), id.into()))
            .copied()
            .unwrap_or(self.history.committed);
        oxideterm_ai::live_message_view(message, owner.conversation(), revision, section)
            .map(Arc::new)
            .ok()
    }

    pub(in crate::workspace) fn request_history_body(
        &mut self,
        conversation: String,
        message: String,
        section: Option<u64>,
        retry: bool,
    ) {
        self.request_history_owned_body(
            HistoryViewOwner::Main(conversation),
            message,
            section,
            retry,
        );
    }

    pub(in crate::workspace) fn request_history_owned_body(
        &mut self,
        owner: HistoryViewOwner,
        message: String,
        section: Option<u64>,
        retry: bool,
    ) {
        if self.live_history_view(&owner, &message).is_some() {
            let page = match &owner {
                HistoryViewOwner::Main(id) => self.history.pages.entry(id.clone()).or_default(),
                _ => self
                    .history
                    .auxiliary_pages
                    .entry(owner.clone())
                    .or_default(),
            };
            if let Some(section) = section {
                page.sections.insert(message.clone(), section);
            }
            page.content.take();
            page.content_cursors.clear();
            *page.body_revisions.entry(message.clone()).or_default() += 1;
            self.chat_ui
                .message_signature_cache
                .borrow_mut()
                .invalidate_message(&message);
            return;
        }
        let Some(store) = self.history.store.clone() else {
            return;
        };
        let generation = self.history.generation;
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        let runtime = self.task_runtime.clone();
        let Some(page) = self.history_view_mut(&owner) else {
            return;
        };
        let Some(description) = page.descriptions.get(&message).cloned() else {
            return;
        };
        if !retry && (page.body_tasks.contains_key(&message) || page.body_errors.contains(&message))
        {
            return;
        }
        if let Some(task) = page.body_tasks.remove(&message) {
            task.abort();
        }
        page.body_errors.remove(&message);
        page.bodies.remove(&message);
        if retry && section.is_some() {
            page.anchor = Some((message.clone(), gpui::px(0.0)));
            if page.content.as_ref().is_some_and(|(id, _)| id == &message)
                || page.content_loading.as_ref() == Some(&message)
            {
                if let Some(task) = page.content_task.take() {
                    task.abort();
                }
                page.content_token += 1;
                page.content.take();
                page.content_cursors.clear();
                page.content_loading = None;
                page.content_error = None;
            }
        }
        let section = section.or_else(|| page.sections.get(&message).copied());
        let token = page.body_tokens.entry(message.clone()).or_default();
        *token += 1;
        let token = *token;
        let key = message.clone();
        let task = runtime.spawn(async move {
            let target = owner.conversation().to_owned();
            let storage_id = description.storage_id.clone();
            let revision = description.revision;
            let result = tokio::task::spawn_blocking(move || {
                store.message_view(&target, &description.storage_id, revision, section)
            })
            .await
            .map_err(|_| ())
            .and_then(|result| result.map_err(|_| ()));
            let _ = sender.send(Delivery::Body(BodyDelivery {
                generation,
                owner,
                storage_id,
                message,
                revision,
                token,
                result,
            }));
            wake.notify_one();
        });
        page.body_tasks.insert(key, task);
    }

    pub(super) fn apply_history_body(&mut self, delivery: BodyDelivery) {
        if delivery.generation != self.history.generation {
            return;
        }
        let Some(page) = self.history_view_mut(&delivery.owner) else {
            return;
        };
        if page.body_tokens.get(&delivery.message) != Some(&delivery.token) {
            return;
        }
        if page
            .descriptions
            .get(&delivery.message)
            .is_none_or(|description| {
                description.revision != delivery.revision
                    || description.storage_id != delivery.storage_id
            })
        {
            return;
        }
        page.body_tasks.remove(&delivery.message);
        let header = delivery.result.as_ref().ok().cloned();
        match delivery.result {
            Ok(body) => {
                page.sections.insert(delivery.message.clone(), body.section);
                page.bodies
                    .insert(delivery.message.clone(), Arc::downgrade(&body));
            }
            Err(()) => {
                page.body_errors.insert(delivery.message.clone());
            }
        }
        *page
            .body_revisions
            .entry(delivery.message.clone())
            .or_default() += 1;
        if let (HistoryViewOwner::Main(conversation), Some(body)) = (&delivery.owner, header) {
            if let Some(message) = self
                .conversation_state
                .conversations
                .iter_mut()
                .find(|item| &item.id == conversation)
                .and_then(|item| {
                    item.messages
                        .iter_mut()
                        .find(|message| message.id == delivery.message)
                })
            {
                message.metadata = body.message.metadata.clone();
                message.branches = body.message.branches.clone();
                message.summary_ref = body.message.summary_ref.clone();
                message.transcript_ref = body.message.transcript_ref.clone();
                message.model = body.message.model.clone();
            }
        }
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_message(&delivery.message);
    }

    pub(in crate::workspace) fn request_history_owned_content(
        &mut self,
        owner: HistoryViewOwner,
        message: String,
        cursor: HistoryContentCursor,
    ) {
        if cursor.conversation_id != owner.conversation() {
            return;
        }
        if self.live_history_view(&owner, &message).is_some() {
            let result = self
                .history_message(owner.conversation(), &message)
                .ok_or_else(|| anyhow::anyhow!("Live message is missing"))
                .and_then(|message| oxideterm_ai::live_content_page(message, &cursor));
            let page = match &owner {
                HistoryViewOwner::Main(id) => self.history.pages.entry(id.clone()).or_default(),
                _ => self
                    .history
                    .auxiliary_pages
                    .entry(owner.clone())
                    .or_default(),
            };
            if let Some(index) = page
                .content_cursors
                .iter()
                .position(|previous| previous == &cursor)
            {
                page.content_cursors.truncate(index + 1);
            } else {
                page.content_cursors.push(cursor);
            }
            match result {
                Ok(content) => {
                    page.content = Some((message.clone(), content));
                    page.content_error = None;
                }
                Err(_) => {
                    page.content_error = Some(message.clone());
                }
            }
            *page.body_revisions.entry(message).or_default() += 1;
            return;
        }
        let Some(store) = self.history.store.clone() else {
            return;
        };
        let generation = self.history.generation;
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        let runtime = self.task_runtime.clone();
        let Some(page) = self.history_view_mut(&owner) else {
            return;
        };
        if let Some(task) = page.content_task.take() {
            task.abort();
        }
        if let Some((old, _)) = page.content.take() {
            *page.body_revisions.entry(old).or_default() += 1;
        }
        if page.content_cursors.last().is_some_and(|previous| {
            previous.storage_id != cursor.storage_id || previous.revision != cursor.revision
        }) {
            page.content_cursors.clear();
        }
        if let Some(index) = page
            .content_cursors
            .iter()
            .position(|previous| previous == &cursor)
        {
            page.content_cursors.truncate(index + 1);
        } else {
            page.content_cursors.push(cursor.clone());
        }
        page.content_loading = Some(message.clone());
        page.content_error = None;
        page.content_token += 1;
        let token = page.content_token;
        page.content_task = Some(runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(move || store.content_page(&cursor))
                .await
                .map_err(|_| ())
                .and_then(|result| result.map_err(|_| ()));
            let _ = sender.send(Delivery::Content(ContentDelivery {
                generation,
                owner,
                message,
                token,
                result,
            }));
            wake.notify_one();
        }));
    }

    pub(super) fn apply_history_content(&mut self, delivery: ContentDelivery) {
        if delivery.generation != self.history.generation {
            return;
        }
        let Some(page) = self.history_view_mut(&delivery.owner) else {
            return;
        };
        if page.content_token != delivery.token {
            return;
        }
        page.content_task.take();
        page.content_loading = None;
        match delivery.result {
            Ok(content) => page.content = Some((delivery.message.clone(), content)),
            Err(()) => {
                page.content_error = Some(delivery.message.clone());
            }
        }
        *page
            .body_revisions
            .entry(delivery.message.clone())
            .or_default() += 1;
        self.chat_ui
            .message_signature_cache
            .borrow_mut()
            .invalidate_message(&delivery.message);
    }

    pub(in crate::workspace) fn previous_history_content(
        &mut self,
        owner: HistoryViewOwner,
        message: String,
    ) {
        let Some(page) = self.history_view_mut(&owner) else {
            return;
        };
        page.content_cursors.pop();
        let previous = page.content_cursors.last().cloned();
        if let Some(cursor) = previous {
            self.request_history_owned_content(owner, message, cursor);
        } else {
            page.content.take();
            *page.body_revisions.entry(message).or_default() += 1;
        }
    }

    pub(in crate::workspace) fn cancel_history_page_loads(&mut self) {
        self.history.generation += 1;
        self.history.page_generation += 1;
        for (_, task) in self.history.archive_tasks.drain() {
            task.abort();
        }
        self.history.archives.clear();
        self.history.auxiliary_pages.clear();
        if let Some(task) = self.history.agents_load.take() {
            task.abort();
        }
        if let Some(task) = self.history.load.take() {
            task.abort();
        }
        for page in self
            .history
            .pages
            .values_mut()
            .chain(self.history.auxiliary_pages.values_mut())
        {
            page.loading = false;
            for (_, task) in page.body_tasks.drain() {
                task.abort();
            }
            if let Some(task) = page.content_task.take() {
                task.abort();
            }
            page.content_loading = None;
            page.content_token += 1;
        }
    }
}

pub(super) fn preview_message(description: &MessageDescriptor) -> AiChatMessage {
    AiChatMessage {
        id: description.id.clone(),
        role: description.role,
        content: description.preview.clone(),
        timestamp_ms: description.timestamp_ms,
        model: None,
        context: None,
        thinking_content: None,
        is_streaming: false,
        metadata: None,
        tool_call_id: None,
        tool_calls: Vec::new(),
        turn: None,
        transcript_ref: None,
        summary_ref: None,
        branches: None,
        suggestions: Vec::new(),
    }
}
