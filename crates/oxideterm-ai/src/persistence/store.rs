use super::{content, records::*, *};
use std::collections::HashMap;

#[derive(Clone)]
pub struct ConversationStore {
    pub(super) db: Arc<parking_lot::RwLock<Option<Database>>>,
    path: PathBuf,
    pub(super) cache: Arc<parking_lot::Mutex<super::cache::HistoryCache>>,
    pub(super) writer: Arc<parking_lot::Mutex<std::sync::Weak<super::writer::Owner>>>,
}

struct OpenStore {
    db: std::sync::Weak<parking_lot::RwLock<Option<Database>>>,
    cache: std::sync::Weak<parking_lot::Mutex<super::cache::HistoryCache>>,
    writer: std::sync::Weak<parking_lot::Mutex<std::sync::Weak<super::writer::Owner>>>,
}
static OPEN_STORES: std::sync::OnceLock<parking_lot::Mutex<HashMap<PathBuf, OpenStore>>> =
    std::sync::OnceLock::new();

impl fmt::Debug for ConversationStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConversationStore").finish_non_exhaustive()
    }
}

impl ConversationStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let path = if path.exists() {
            std::fs::canonicalize(&path)?
        } else {
            std::fs::canonicalize(
                path.parent()
                    .ok_or_else(|| anyhow!("History directory is missing"))?,
            )?
            .join(
                path.file_name()
                    .ok_or_else(|| anyhow!("History filename is missing"))?,
            )
        };
        let mut stores = OPEN_STORES.get_or_init(Default::default).lock();
        stores.retain(|_, store| store.db.strong_count() > 0);
        if let Some(store) = stores.get(&path) {
            if let (Some(db), Some(cache), Some(writer)) = (
                store.db.upgrade(),
                store.cache.upgrade(),
                store.writer.upgrade(),
            ) {
                return Ok(Self {
                    db,
                    path,
                    cache,
                    writer,
                });
            }
        }
        // Opening a damaged history must never replace it with an empty database.
        let existed = path.exists();
        let db = if existed {
            Database::open(&path)?
        } else {
            Database::create(&path)?
        };
        set_owner_only_permissions(&path);
        if existed {
            let tx = db.begin_read()?;
            let version = tx.open_table(STATE)?.get("version")?.map(|row| row.value());
            if version != Some(4) {
                return Err(anyhow!("Unsupported history format"));
            }
        } else {
            super::records::initialize(&db)?;
        }
        let store = Self::from_database(path.clone(), db);
        stores.insert(
            path,
            OpenStore {
                db: Arc::downgrade(&store.db),
                cache: Arc::downgrade(&store.cache),
                writer: Arc::downgrade(&store.writer),
            },
        );
        Ok(store)
    }

    pub(super) fn from_database(path: PathBuf, db: Database) -> Self {
        Self {
            db: Arc::new(parking_lot::RwLock::new(Some(db))),
            path,
            cache: Arc::new(parking_lot::Mutex::new(
                super::cache::HistoryCache::default(),
            )),
            writer: Arc::new(parking_lot::Mutex::new(std::sync::Weak::new())),
        }
    }

    pub(super) fn reopen(&self) -> Result<()> {
        // Wait for active reads to finish before releasing the file lock and recovering a failed transaction.
        let mut database = self.db.write();
        database.take();
        *database = Some(Database::open(&self.path)?);
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cached_bytes(&self) -> usize {
        self.cache.lock().bytes()
    }

    /// Offline import batches by retained bytes rather than the unrelated UI page size.
    pub fn import_messages(
        &self,
        conversation: &str,
        branch: &str,
        messages: impl IntoIterator<Item = Result<AiChatMessage>>,
        revision: &mut u64,
    ) -> Result<usize> {
        let mut batch = Vec::new();
        let mut bytes = 0usize;
        let mut commits = 0;
        for message in messages {
            let message = message?;
            let retained =
                std::mem::size_of::<AiChatMessage>() + super::cache::message_heap(&message);
            if !batch.is_empty() && bytes.saturating_add(retained) > super::HISTORY_PENDING_BYTES {
                self.apply(std::mem::take(&mut batch))?;
                commits += 1;
                bytes = 0;
            }
            *revision = revision
                .checked_add(1)
                .ok_or_else(|| anyhow!("History revision exhausted"))?;
            bytes = bytes.saturating_add(retained);
            batch.push(HistoryMutation::PutMessage {
                conversation_id: conversation.into(),
                branch_id: branch.into(),
                message,
                revision: *revision,
            });
        }
        if !batch.is_empty() {
            self.apply(batch)?;
            commits += 1;
        }
        Ok(commits)
    }

    pub fn apply(&self, mutations: Vec<HistoryMutation>) -> Result<()> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        let tx = db.begin_write()?;
        let mut collect = HashSet::new();
        for mut mutation in mutations {
            if let Some(conversation) = mutation.collection_owner() {
                collect.insert(conversation.to_owned());
            }
            mutation.sanitize();
            if let HistoryMutation::DeleteConversation {
                conversation_id, ..
            } = &mutation
            {
                self.cache.lock().clear_conversation(conversation_id);
            }
            super::mutations::apply_mutation(&tx, mutation)?;
        }
        for conversation in collect {
            super::reachability::collect(&tx, &conversation)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn conversation_head(&self, id: &str) -> Result<Option<ConversationHead>> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        super::records::head(&db.begin_read()?, id)
    }

    pub fn list_heads(
        &self,
        before: Option<(i64, String)>,
        limit: usize,
    ) -> Result<Vec<ConversationHead>> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        let tx = db.begin_read()?;
        let index = tx.open_table(UPDATED)?;
        let mut output = Vec::new();
        for row in index
            .range(
                ..before
                    .as_ref()
                    .map(|(time, id)| (*time, id.as_str()))
                    .unwrap_or((i64::MAX, "\u{10ffff}")),
            )?
            .rev()
            .take(limit)
        {
            let (key, _) = row?;
            if let Some(head) = super::records::head(&tx, key.value().1)? {
                output.push(head);
            }
        }
        Ok(output)
    }

    pub fn events(
        &self,
        conversation: &str,
        family: &str,
        before: Option<u64>,
        limit: usize,
    ) -> Result<Vec<(u64, Value)>> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        let tx = db.begin_read()?;
        let table = tx.open_table(EVENTS)?;
        let mut result = Vec::new();
        for row in table
            .range((conversation, family, 0)..(conversation, family, before.unwrap_or(u64::MAX)))?
            .rev()
            .take(limit)
        {
            let (key, value) = row?;
            result.push((
                key.value().2,
                content::load_value(
                    &tx,
                    conversation,
                    rmp_serde::from_slice(value.value())?,
                    &self.cache,
                )?,
            ));
        }
        result.reverse();
        Ok(result)
    }

    pub fn message_page(
        &self,
        conversation: &str,
        branch_id: &str,
        before: Option<&HistoryCursor>,
        limit: usize,
    ) -> Result<MessagePage> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        let tx = db.begin_read()?;
        let state = super::records::branch(&tx, conversation, branch_id)?;
        if before.is_some_and(|cursor| {
            cursor.conversation_id != conversation
                || cursor.branch_id != branch_id
                || cursor.revision != state.revision
        }) {
            return Err(anyhow!("History cursor expired"));
        }
        let end = before
            .map(|cursor| cursor.before_sequence)
            .unwrap_or(u64::MAX);
        let descriptions = tx.open_table(DESCRIPTORS)?;
        let mut messages = Vec::new();
        let mut more = false;
        for (_, id) in page_keys(&tx, conversation, branch_id, end, limit.saturating_add(1))? {
            if messages.len() == limit {
                more = true;
                break;
            }
            let row = descriptions
                .get((conversation, id.as_str()))?
                .ok_or_else(|| anyhow!("History description is missing"))?;
            messages.push(rmp_serde::from_slice::<MessageDescriptor>(row.value())?);
        }
        messages.reverse();
        let after = before
            .and_then(|_| messages.last())
            .map(|message| HistoryCursor {
                conversation_id: conversation.into(),
                branch_id: branch_id.into(),
                revision: state.revision,
                before_sequence: message.sequence,
            });
        let before = messages
            .first()
            .filter(|_| more)
            .map(|message| HistoryCursor {
                conversation_id: conversation.into(),
                branch_id: branch_id.into(),
                revision: state.revision,
                before_sequence: message.sequence,
            });
        Ok(MessagePage {
            messages,
            before,
            after,
            revision: state.revision,
        })
    }

    pub fn message_page_after(
        &self,
        conversation: &str,
        branch_id: &str,
        after: &HistoryCursor,
        limit: usize,
    ) -> Result<MessagePage> {
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        let state = super::records::branch(&tx, conversation, branch_id)?;
        if after.conversation_id != conversation
            || after.branch_id != branch_id
            || after.revision != state.revision
        {
            return Err(anyhow!("History cursor expired"));
        }
        let keys = range_keys(
            &tx,
            conversation,
            branch_id,
            after.before_sequence.saturating_add(1),
            u64::MAX,
            limit.saturating_add(1),
            false,
        )?;
        let more = keys.len() > limit;
        let table = tx.open_table(DESCRIPTORS)?;
        let mut messages = Vec::new();
        for (_, id) in keys.into_iter().take(limit) {
            messages.push(rmp_serde::from_slice::<MessageDescriptor>(
                table
                    .get((conversation, id.as_str()))?
                    .ok_or_else(|| anyhow!("History description is missing"))?
                    .value(),
            )?);
        }
        let cursor = |message: &MessageDescriptor| HistoryCursor {
            conversation_id: conversation.into(),
            branch_id: branch_id.into(),
            revision: state.revision,
            before_sequence: message.sequence,
        };
        Ok(MessagePage {
            before: messages.first().map(cursor),
            after: messages.last().filter(|_| more).map(cursor),
            messages,
            revision: state.revision,
        })
    }

    pub fn message_page_at(
        &self,
        conversation: &str,
        branch_id: &str,
        before_sequence: u64,
        limit: usize,
    ) -> Result<MessagePage> {
        let revision = {
            let guard = self.db.read();
            let tx = guard
                .as_ref()
                .ok_or_else(|| anyhow!("History database is unavailable"))?
                .begin_read()?;
            super::records::branch(&tx, conversation, branch_id)?.revision
        };
        self.message_page(
            conversation,
            branch_id,
            Some(&HistoryCursor {
                conversation_id: conversation.into(),
                branch_id: branch_id.into(),
                revision,
                before_sequence,
            }),
            limit,
        )
    }

    pub fn message(&self, conversation: &str, storage_id: &str) -> Result<Option<AiChatMessage>> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        let tx = db.begin_read()?;
        let table = tx.open_table(MESSAGES)?;
        let Some(row) = table.get((conversation, storage_id))? else {
            return Ok(None);
        };
        let value = content::load_value(
            &tx,
            conversation,
            rmp_serde::from_slice(row.value())?,
            &self.cache,
        )?;
        let mut message: AiChatMessage = serde_json::from_value(value)?;
        normalize_interrupted_assistant_projection(&mut message);
        message.is_streaming = false;
        Ok(Some(message))
    }

    pub fn page(
        &self,
        conversation: &str,
        branch_id: &str,
        before: Option<&HistoryCursor>,
        limit: usize,
    ) -> Result<HistoryPage> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        let tx = db.begin_read()?;
        let state = super::records::branch(&tx, conversation, branch_id)?;
        if before.is_some_and(|cursor| {
            cursor.conversation_id != conversation
                || cursor.branch_id != branch_id
                || cursor.revision != state.revision
        }) {
            return Err(anyhow!("History cursor expired"));
        }
        let end = before
            .map(|cursor| cursor.before_sequence)
            .unwrap_or(u64::MAX);
        let messages = tx.open_table(MESSAGES)?;
        let mut page = Vec::new();
        let mut first_sequence = end;
        let mut more = false;
        for (sequence, id) in page_keys(&tx, conversation, branch_id, end, limit.saturating_add(1))?
        {
            if page.len() == limit {
                more = true;
                break;
            }
            first_sequence = sequence;
            let record = messages
                .get((conversation, id.as_str()))?
                .ok_or_else(|| anyhow!("History message is missing"))?;
            let value = content::load_value(
                &tx,
                conversation,
                rmp_serde::from_slice(record.value())?,
                &self.cache,
            )?;
            let mut message: AiChatMessage = serde_json::from_value(value)?;
            normalize_interrupted_assistant_projection(&mut message);
            message.is_streaming = false;
            page.push(message);
        }
        page.reverse();
        Ok(HistoryPage {
            messages: page,
            before: more.then(|| HistoryCursor {
                conversation_id: conversation.into(),
                branch_id: branch_id.into(),
                revision: state.revision,
                before_sequence: first_sequence,
            }),
            revision: state.revision,
        })
    }
}

pub(super) fn page_keys(
    tx: &redb::ReadTransaction,
    conversation: &str,
    branch_id: &str,
    end: u64,
    limit: usize,
) -> Result<Vec<(u64, String)>> {
    range_keys(tx, conversation, branch_id, 0, end, limit, true)
}

fn range_keys(
    tx: &redb::ReadTransaction,
    conversation: &str,
    branch_id: &str,
    mut start: u64,
    mut end: u64,
    limit: usize,
    newest: bool,
) -> Result<Vec<(u64, String)>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let order = tx.open_table(ORDER)?;
    let mut result = std::collections::BTreeMap::new();
    let mut current = branch_id.to_owned();
    let mut visited = HashSet::new();
    let deleted = tx.open_table(MESSAGE_DELETED)?;
    let descriptions = tx.open_table(DESCRIPTORS)?;
    while start < end {
        if !visited.insert(current.clone()) {
            return Err(anyhow!("History branch cycle detected"));
        }
        let state = super::records::branch(tx, conversation, &current)?;
        let mut accepted = 0;
        let mut rows = order.range(
            (conversation, current.as_str(), start)..(conversation, current.as_str(), end),
        )?;
        loop {
            let row = if newest {
                rows.next_back()
            } else {
                rows.next()
            };
            let Some(row) = row else {
                break;
            };
            let (key, id) = row?;
            let description: MessageDescriptor = rmp_serde::from_slice(
                descriptions
                    .get((conversation, id.value()))?
                    .ok_or_else(|| anyhow!("History description is missing"))?
                    .value(),
            )?;
            let mut excluded = false;
            for branch in &visited {
                if deleted
                    .get((conversation, branch.as_str(), description.id.as_str()))?
                    .is_some()
                {
                    excluded = true;
                    break;
                }
            }
            if excluded {
                continue;
            }
            result
                .entry(key.value().2)
                .or_insert_with(|| id.value().to_owned());
            accepted += 1;
            if accepted == limit {
                break;
            }
        }
        while result.len() > limit {
            if newest {
                result.pop_first();
            } else {
                result.pop_last();
            }
        }
        let Some(parent) = state.parent else {
            break;
        };
        current = parent.branch_id;
        start = start.max(parent.first_sequence);
        end = end.min(parent.last_sequence.saturating_add(1));
        if result.len() == limit {
            if newest {
                if let Some((minimum, _)) = result.first_key_value() {
                    start = start.max(*minimum);
                }
            } else if let Some((maximum, _)) = result.last_key_value() {
                end = end.min(maximum.saturating_add(1));
            }
        }
    }
    Ok(if newest {
        result.into_iter().rev().collect()
    } else {
        result.into_iter().collect()
    })
}
