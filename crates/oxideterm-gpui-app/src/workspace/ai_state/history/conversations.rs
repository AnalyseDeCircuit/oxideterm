use super::*;

pub(in crate::workspace) const CONVERSATION_PAGE_SIZE: usize = 50;

#[derive(Default)]
pub(in crate::workspace) struct ConversationListPage {
    pub initialized: bool,
    pub has_more: bool,
    pub failed: bool,
    cursor: Option<(i64, String)>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl ConversationListPage {
    pub fn loading(&self) -> bool {
        self.task.is_some()
    }

    fn advance(&mut self, heads: &[oxideterm_ai::ConversationHead]) {
        self.initialized = true;
        self.failed = false;
        self.has_more = heads.len() > CONVERSATION_PAGE_SIZE;
        if let Some(head) = heads.iter().take(CONVERSATION_PAGE_SIZE).last() {
            self.cursor = Some((
                head.conversation.updated_at_ms,
                head.conversation.id.clone(),
            ));
        }
    }
}

impl Drop for ConversationListPage {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl AiWorkspaceEntity {
    pub(in crate::workspace) fn conversation_list_page(
        &self,
        archived: bool,
    ) -> &ConversationListPage {
        &self.history.conversation_lists[usize::from(archived)]
    }

    pub(in crate::workspace) fn request_conversation_list_page(&mut self, archived: bool) {
        let Some(store) = self.history.store.clone() else {
            return;
        };
        let page = &mut self.history.conversation_lists[usize::from(archived)];
        if page.loading() || (page.initialized && !page.has_more) || self.history.quitting {
            return;
        }
        let cursor = page.cursor.clone();
        let generation = self.history.generation;
        let epoch = self.history.conversation_list_epoch;
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        page.failed = false;
        page.task = Some(self.task_runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                store.list_conversation_heads(archived, cursor, CONVERSATION_PAGE_SIZE + 1)
            })
            .await
            .ok()
            .and_then(Result::ok)
            .ok_or(());
            let _ = sender.send(Delivery::Conversations(generation, epoch, archived, result));
            wake.notify_one();
        }));
    }

    pub(super) fn initialize_conversation_list(
        &mut self,
        heads: &mut Vec<oxideterm_ai::ConversationHead>,
    ) {
        self.history.conversation_lists = Default::default();
        self.history.conversation_lists[0].advance(heads);
        heads.truncate(CONVERSATION_PAGE_SIZE);
        self.history.deleted_conversations.clear();
    }

    pub(super) fn apply_conversation_list(
        &mut self,
        archived: bool,
        result: Result<Vec<oxideterm_ai::ConversationHead>, ()>,
    ) {
        let page = &mut self.history.conversation_lists[usize::from(archived)];
        page.task.take();
        let Ok(mut heads) = result else {
            page.failed = true;
            return;
        };
        page.advance(&heads);
        heads.truncate(CONVERSATION_PAGE_SIZE);
        for head in heads {
            let id = &head.conversation.id;
            // Local edits and deletions can precede this snapshot's delivery.
            if self.history.deleted_conversations.contains(id)
                || self
                    .conversation_state
                    .conversations
                    .iter()
                    .any(|item| &item.id == id)
            {
                continue;
            }
            self.history.branches.insert(id.clone(), head.active_branch);
            self.conversation_state
                .conversations
                .push(head.conversation);
        }
    }

    pub(in crate::workspace) fn reset_conversation_lists_after_clear(&mut self) {
        self.history.conversation_list_epoch += 1;
        self.history.conversation_lists = Default::default();
        for page in &mut self.history.conversation_lists {
            page.initialized = true;
        }
    }
}
