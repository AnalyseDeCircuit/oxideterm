use super::{windows::HistoryMessageView, *};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

#[derive(Clone)]
enum Cached {
    Chunk(Arc<[u8]>),
    Message(Arc<HistoryMessageView>),
    Render(Arc<dyn std::any::Any + Send + Sync>),
}

/// Committed UI projections and decoded chunks share one eviction budget.
#[derive(Default)]
pub(super) struct HistoryCache {
    values: HashMap<(String, String), (Cached, u64, usize)>,
    order: BTreeMap<u64, (String, String)>,
    bytes: usize,
    clock: u64,
}
impl HistoryCache {
    fn touch(&mut self, conversation: &str, id: &str) -> Option<Cached> {
        let entry = self
            .values
            .get_mut(&(conversation.to_owned(), id.to_owned()))?;
        self.order.remove(&entry.1);
        self.clock = self.clock.wrapping_add(1);
        entry.1 = self.clock;
        self.order
            .insert(self.clock, (conversation.to_owned(), id.to_owned()));
        Some(entry.0.clone())
    }
    pub fn get(&mut self, conversation: &str, id: &str) -> Option<Arc<[u8]>> {
        match self.touch(conversation, id)? {
            Cached::Chunk(bytes) => Some(bytes),
            _ => None,
        }
    }
    pub fn get_message(&mut self, conversation: &str, id: &str) -> Option<Arc<HistoryMessageView>> {
        match self.touch(conversation, id)? {
            Cached::Message(message) => Some(message),
            _ => None,
        }
    }
    pub fn insert(&mut self, conversation: &str, id: &str, bytes: Arc<[u8]>) {
        let size = bytes.len();
        self.insert_value(conversation, id, Cached::Chunk(bytes), size);
    }
    pub fn insert_message(&mut self, conversation: &str, id: &str, view: Arc<HistoryMessageView>) {
        let size = std::mem::size_of::<HistoryMessageView>()
            + message_heap(&view.message)
            + view.more.capacity() * std::mem::size_of::<super::windows::HistoryContentCursor>()
            + view
                .more
                .iter()
                .map(|cursor| {
                    cursor.conversation_id.capacity()
                        + cursor.storage_id.capacity()
                        + cursor.path.capacity() * std::mem::size_of::<String>()
                        + cursor.path.iter().map(String::capacity).sum::<usize>()
                        + cursor.after_key.as_ref().map_or(0, String::capacity)
                        + cursor
                            .event
                            .as_ref()
                            .map_or(0, |event| event.family.capacity() + event.id.capacity())
                })
                .sum::<usize>();
        self.insert_value(conversation, id, Cached::Message(view), size);
    }
    fn insert_value(&mut self, conversation: &str, id: &str, value: Cached, payload: usize) {
        self.remove(conversation, id);
        // Include duplicated keys, map nodes and allocation bookkeeping, not just text lengths.
        let size = payload.saturating_add(256 + 2 * (conversation.len() + id.len()));
        if size > super::HISTORY_CACHE_BYTES {
            return;
        }
        while self.bytes + size > super::HISTORY_CACHE_BYTES {
            let Some((_, key)) = self.order.first_key_value() else {
                break;
            };
            let key = key.clone();
            self.remove(&key.0, &key.1);
        }
        self.clock = self.clock.wrapping_add(1);
        self.bytes += size;
        let key = (conversation.to_owned(), id.to_owned());
        self.order.insert(self.clock, key.clone());
        self.values.insert(key, (value, self.clock, size));
    }
    pub fn render<T: Send + Sync + 'static>(
        &mut self,
        conversation: &str,
        key: &str,
    ) -> Option<Arc<T>> {
        match self.touch(conversation, key)? {
            Cached::Render(value) => value.downcast().ok(),
            _ => None,
        }
    }
    pub fn insert_render<T: Send + Sync + 'static>(
        &mut self,
        conversation: &str,
        key: &str,
        value: Arc<T>,
        bytes: usize,
    ) {
        self.insert_value(conversation, key, Cached::Render(value), bytes);
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn remove(&mut self, conversation: &str, id: &str) {
        if let Some((_, tick, bytes)) = self.values.remove(&(conversation.into(), id.into())) {
            self.bytes -= bytes;
            self.order.remove(&tick);
        }
    }
    pub fn clear_conversation(&mut self, conversation: &str) {
        self.values.retain(|(id, _), (_, tick, bytes)| {
            if id != conversation {
                true
            } else {
                self.bytes -= *bytes;
                self.order.remove(tick);
                false
            }
        });
    }
}

fn json_heap(value: &Value) -> usize {
    match value {
        Value::String(text) => text.capacity() + 32,
        Value::Array(values) => {
            values.capacity() * std::mem::size_of::<Value>()
                + values.iter().map(json_heap).sum::<usize>()
                + 32
        }
        Value::Object(values) => {
            values.len() * 128
                + values
                    .iter()
                    .map(|(key, value)| key.capacity() + 32 + json_heap(value))
                    .sum::<usize>()
        }
        _ => 0,
    }
}

fn range_heap(range: &crate::AiHistoryRange) -> usize {
    range.branch_id.capacity()
        + range.first_message_id.as_ref().map_or(0, String::capacity)
        + range.last_message_id.as_ref().map_or(0, String::capacity)
        + 96
}

pub(super) fn message_heap(message: &AiChatMessage) -> usize {
    let mut bytes = message.id.capacity() + message.content.capacity() + 64;
    for text in [
        &message.model,
        &message.context,
        &message.thinking_content,
        &message.tool_call_id,
    ]
    .into_iter()
    .flatten()
    {
        bytes += text.capacity() + 32;
    }
    bytes += message.tool_calls.capacity() * std::mem::size_of::<Value>()
        + message.tool_calls.iter().map(json_heap).sum::<usize>();
    for value in [&message.turn, &message.transcript_ref, &message.summary_ref]
        .into_iter()
        .flatten()
    {
        bytes += json_heap(value);
    }
    bytes += message.suggestions.capacity() * std::mem::size_of::<crate::AiFollowUpSuggestion>()
        + message
            .suggestions
            .iter()
            .map(|suggestion| suggestion.icon.capacity() + suggestion.text.capacity() + 64)
            .sum::<usize>();
    if let Some(metadata) = &message.metadata {
        bytes += metadata.kind.capacity() + 32;
        if let Some(range) = &metadata.original_ref {
            bytes += range_heap(range);
        }
        if let Some(messages) = &metadata.original_messages {
            bytes += messages.capacity() * std::mem::size_of::<AiChatMessage>()
                + messages.iter().map(message_heap).sum::<usize>();
        }
    }
    if let Some(branches) = &message.branches {
        bytes +=
            branches.refs.capacity() * 192 + branches.refs.values().map(range_heap).sum::<usize>();
        bytes += branches.tails.capacity() * 128
            + branches
                .tails
                .values()
                .map(|messages| {
                    messages.capacity() * std::mem::size_of::<AiChatMessage>()
                        + messages.iter().map(message_heap).sum::<usize>()
                })
                .sum::<usize>();
    }
    bytes
}

impl ConversationStore {
    /// Renderers share the store's eviction owner without a dependency on the UI crate.
    pub fn cached_render<T: Send + Sync + 'static>(
        &self,
        conversation: &str,
        key: &str,
    ) -> Option<Arc<T>> {
        self.cache
            .lock()
            .render(conversation, &format!("render:{key}"))
    }
    pub fn cache_render<T: Send + Sync + 'static>(
        &self,
        conversation: &str,
        key: &str,
        value: Arc<T>,
        retained_bytes: usize,
    ) {
        self.cache.lock().insert_render(
            conversation,
            &format!("render:{key}"),
            value,
            retained_bytes,
        );
    }
}
