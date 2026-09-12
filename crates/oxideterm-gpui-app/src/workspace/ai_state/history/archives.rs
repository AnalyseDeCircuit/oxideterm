use super::*;
use oxideterm_ai::{AiHistoryRange, MessagePage};

pub(in crate::workspace) struct ArchiveView {
    pub range: AiHistoryRange,
    pub messages: Vec<oxideterm_ai::MessageDescriptor>,
    pub list: gpui::ListState,
    pub before: Option<HistoryCursor>,
    pub expanded: bool,
    pub loading: bool,
    pub failed: bool,
    pub revision: u64,
    pub cursors: Vec<Option<HistoryCursor>>,
}

pub(super) struct ArchiveDelivery {
    generation: u64,
    revision: u64,
    key: (String, String),
    range: AiHistoryRange,
    result: Result<MessagePage, ()>,
}

impl AiWorkspaceEntity {
    pub(in crate::workspace) fn toggle_history_archive(
        &mut self,
        conversation: String,
        message: String,
        range: AiHistoryRange,
    ) {
        let key = (conversation.clone(), message.clone());
        if let Some(view) = self.history.archives.get_mut(&key) {
            if view.expanded && view.range == range {
                view.expanded = false;
                view.messages.clear();
                self.history
                    .auxiliary_pages
                    .remove(&HistoryViewOwner::Archive(
                        conversation.clone(),
                        message.clone(),
                    ));
                view.revision += 1;
                if let Some(task) = self.history.archive_tasks.remove(&key) {
                    task.abort();
                }
                return;
            }
        }
        self.history.archives.insert(
            key,
            ArchiveView {
                range: range.clone(),
                messages: Vec::new(),
                list: gpui::ListState::new(0, gpui::ListAlignment::Top, gpui::px(500.0)),
                before: None,
                expanded: true,
                loading: false,
                failed: false,
                revision: 1,
                cursors: vec![None],
            },
        );
        self.load_history_archive(conversation, message);
    }

    pub(in crate::workspace) fn page_history_archive(
        &mut self,
        conversation: String,
        message: String,
        older: bool,
    ) {
        let Some(view) = self
            .history
            .archives
            .get_mut(&(conversation.clone(), message.clone()))
        else {
            return;
        };
        if view.loading {
            return;
        }
        if older {
            let Some(before) = view.before.clone() else {
                return;
            };
            view.cursors.push(Some(before));
        } else if view.cursors.len() > 1 {
            view.cursors.pop();
        } else {
            return;
        }
        self.load_history_archive(conversation, message);
    }

    fn load_history_archive(&mut self, conversation: String, message: String) {
        let Some(store) = self.history.store.clone() else {
            return;
        };
        let key = (conversation, message);
        let Some(view) = self.history.archives.get_mut(&key) else {
            return;
        };
        view.loading = true;
        view.failed = false;
        view.revision += 1;
        let revision = view.revision;
        let before = view.cursors.last().cloned().flatten();
        let range = view.range.clone();
        let generation = self.history.generation;
        let sender = self.history.sender.clone();
        let wake = self.history.wake.clone();
        let owner = key.clone();
        let (commit, committed) = tokio::sync::oneshot::channel();
        self.history_barrier(commit);
        let task = self.task_runtime.spawn(async move {
            if committed.await != Ok(true) {
                return;
            }
            let target = key.0.clone();
            let query = range.clone();
            let result = tokio::task::spawn_blocking(move || {
                store.range_descriptions(&target, &query, before.as_ref(), 50)
            })
            .await
            .map_err(|_| ())
            .and_then(|result| result.map_err(|_| ()));
            let _ = sender.send(Delivery::Archive(ArchiveDelivery {
                generation,
                revision,
                key,
                range,
                result,
            }));
            wake.notify_one();
        });
        if let Some(previous) = self.history.archive_tasks.insert(owner, task) {
            previous.abort();
        }
    }

    pub(super) fn apply_history_archive(&mut self, delivery: ArchiveDelivery) {
        if delivery.generation != self.history.generation {
            return;
        }
        let Some(view) = self.history.archives.get_mut(&delivery.key) else {
            return;
        };
        if !view.expanded || view.range != delivery.range || view.revision != delivery.revision {
            return;
        }
        self.history.archive_tasks.remove(&delivery.key);
        view.loading = false;
        view.revision += 1;
        match delivery.result {
            Ok(page) => {
                view.list.reset(page.messages.len());
                let mut state = pages::HistoryPageState::default();
                state.branch = delivery.range.branch_id.clone();
                state.descriptions = page
                    .messages
                    .iter()
                    .map(|message| (message.id.clone(), message.clone()))
                    .collect();
                self.history.auxiliary_pages.insert(
                    HistoryViewOwner::Archive(delivery.key.0, delivery.key.1),
                    state,
                );
                view.messages = page.messages;
                view.before = page.before;
            }
            Err(()) => view.failed = true,
        }
    }
}
