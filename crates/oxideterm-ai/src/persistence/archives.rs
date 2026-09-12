use super::{records::*, *};
use crate::AiHistoryRange;

pub(super) fn normalize_message(
    tx: &redb::WriteTransaction,
    conversation: &str,
    message: &mut AiChatMessage,
    revision: u64,
) -> Result<()> {
    preserve_archive_counts(message);
    if let Some(metadata) = &mut message.metadata {
        if let Some(originals) = metadata.original_messages.take() {
            metadata.original_ref = Some(archive(tx, conversation, originals, revision)?);
        }
    }
    if let Some(branches) = &mut message.branches {
        for (index, tail) in std::mem::take(&mut branches.tails) {
            branches
                .refs
                .insert(index, archive(tx, conversation, tail, revision)?);
        }
    }
    Ok(())
}

pub(super) fn preserve_archive_counts(message: &mut AiChatMessage) {
    if let Some(metadata) = &mut message.metadata {
        if let Some(originals) = &mut metadata.original_messages {
            for original in originals.iter_mut() {
                preserve_archive_counts(original);
            }
            if metadata.kind == "compaction-anchor"
                && metadata.original_user_count.is_none()
                && metadata
                    .original_count
                    .is_none_or(|count| count <= originals.len())
            {
                metadata.original_user_count = Some(crate::ai_conversation_turn_count(originals));
            }
        }
    }
    if let Some(branches) = &mut message.branches {
        for tail in branches.tails.values_mut() {
            for message in tail {
                preserve_archive_counts(message);
            }
        }
    }
}

fn archive(
    tx: &redb::WriteTransaction,
    conversation: &str,
    messages: Vec<AiChatMessage>,
    revision: u64,
) -> Result<AiHistoryRange> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut branch = Branch {
        parent: None,
        owner: BranchOwner::Archive,
        retained: false,
        sealed: false,
        next_sequence: 0,
        message_count: 0,
        turn_count: 0,
        revision: 0,
    };
    tx.open_table(BRANCHES)?.insert(
        (conversation, id.as_str()),
        rmp_serde::to_vec_named(&branch)?.as_slice(),
    )?;
    for message in messages {
        super::mutations::apply_mutation(
            tx,
            HistoryMutation::PutMessage {
                conversation_id: conversation.into(),
                branch_id: id.clone(),
                message,
                revision,
            },
        )?;
    }
    let mut table = tx.open_table(BRANCHES)?;
    branch = rmp_serde::from_slice(
        table
            .get((conversation, id.as_str()))?
            .ok_or_else(|| anyhow!("History archive is missing"))?
            .value(),
    )?;
    branch.sealed = true;
    table.insert(
        (conversation, id.as_str()),
        rmp_serde::to_vec_named(&branch)?.as_slice(),
    )?;
    Ok(AiHistoryRange {
        branch_id: id,
        first_message_id: None,
        last_message_id: None,
    })
}

impl ConversationStore {
    pub fn range_descriptions(
        &self,
        conversation: &str,
        range: &AiHistoryRange,
        before: Option<&HistoryCursor>,
        limit: usize,
    ) -> Result<MessagePage> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        let tx = db.begin_read()?;
        let branch = super::records::branch(&tx, conversation, &range.branch_id)?;
        if before.is_some_and(|cursor| {
            cursor.conversation_id != conversation
                || cursor.branch_id != range.branch_id
                || cursor.revision != branch.revision
        }) {
            return Err(anyhow!("History cursor expired"));
        }
        let first = range
            .first_message_id
            .as_deref()
            .map(|id| {
                super::queries::message_key(&tx, conversation, &range.branch_id, id)
                    .map(|(sequence, _)| sequence)
            })
            .transpose()?
            .unwrap_or(0);
        let end = range
            .last_message_id
            .as_deref()
            .map(|id| {
                super::queries::message_key(&tx, conversation, &range.branch_id, id)
                    .map(|(sequence, _)| sequence.saturating_add(1))
            })
            .transpose()?
            .unwrap_or(u64::MAX);
        let end = before
            .map(|cursor| cursor.before_sequence.min(end))
            .unwrap_or(end);
        let keys = super::store::page_keys(
            &tx,
            conversation,
            &range.branch_id,
            end,
            limit.saturating_add(1),
        )?;
        let keys: Vec<_> = keys
            .into_iter()
            .take_while(|(sequence, _)| *sequence >= first)
            .collect();
        let more = keys.len() > limit;
        let table = tx.open_table(DESCRIPTORS)?;
        let mut messages = Vec::new();
        let mut oldest = end;
        for (sequence, key) in keys.into_iter().take(limit) {
            oldest = sequence;
            let row = table
                .get((conversation, key.as_str()))?
                .ok_or_else(|| anyhow!("History message is missing"))?;
            let message: MessageDescriptor = rmp_serde::from_slice(row.value())?;
            messages.push(message);
        }
        messages.reverse();
        Ok(MessagePage {
            messages,
            after: None,
            before: more.then(|| HistoryCursor {
                conversation_id: conversation.into(),
                branch_id: range.branch_id.clone(),
                revision: branch.revision,
                before_sequence: oldest,
            }),
            revision: branch.revision,
        })
    }

    pub fn range_messages(
        &self,
        conversation: &str,
        range: &AiHistoryRange,
        before: Option<&HistoryCursor>,
        limit: usize,
    ) -> Result<HistoryPage> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        let tx = db.begin_read()?;
        let branch = super::records::branch(&tx, conversation, &range.branch_id)?;
        if before.is_some_and(|cursor| {
            cursor.conversation_id != conversation
                || cursor.branch_id != range.branch_id
                || cursor.revision != branch.revision
        }) {
            return Err(anyhow!("History cursor expired"));
        }
        let first = range
            .first_message_id
            .as_deref()
            .map(|id| {
                super::queries::message_key(&tx, conversation, &range.branch_id, id)
                    .map(|(sequence, _)| sequence)
            })
            .transpose()?
            .unwrap_or(0);
        let end = range
            .last_message_id
            .as_deref()
            .map(|id| {
                super::queries::message_key(&tx, conversation, &range.branch_id, id)
                    .map(|(sequence, _)| sequence.saturating_add(1))
            })
            .transpose()?
            .unwrap_or(u64::MAX);
        let end = before
            .map(|cursor| cursor.before_sequence.min(end))
            .unwrap_or(end);
        let keys = super::store::page_keys(
            &tx,
            conversation,
            &range.branch_id,
            end,
            limit.saturating_add(1),
        )?;
        let keys: Vec<_> = keys
            .into_iter()
            .take_while(|(sequence, _)| *sequence >= first)
            .collect();
        let more = keys.len() > limit;
        let table = tx.open_table(MESSAGES)?;
        let mut messages = Vec::new();
        let mut oldest = end;
        for (sequence, key) in keys.into_iter().take(limit) {
            oldest = sequence;
            let row = table
                .get((conversation, key.as_str()))?
                .ok_or_else(|| anyhow!("History message is missing"))?;
            let mut message: AiChatMessage = serde_json::from_value(super::content::load_value(
                &tx,
                conversation,
                rmp_serde::from_slice(row.value())?,
                &self.cache,
            )?)?;
            normalize_interrupted_assistant_projection(&mut message);
            message.is_streaming = false;
            messages.push(message);
        }
        messages.reverse();
        Ok(HistoryPage {
            messages,
            before: more.then(|| HistoryCursor {
                conversation_id: conversation.into(),
                branch_id: range.branch_id.clone(),
                revision: branch.revision,
                before_sequence: oldest,
            }),
            revision: branch.revision,
        })
    }

    /// Only explicit export and migration validation expand archived bodies recursively.
    pub fn expand_message_archives(
        &self,
        conversation: &str,
        message: &mut AiChatMessage,
    ) -> Result<()> {
        self.expand_archives(conversation, message, &mut HashSet::new())
    }
    fn expand_archives(
        &self,
        conversation: &str,
        message: &mut AiChatMessage,
        visited: &mut HashSet<String>,
    ) -> Result<()> {
        if let Some(metadata) = &mut message.metadata {
            if let Some(range) = metadata.original_ref.take() {
                metadata.original_messages =
                    Some(self.expand_range(conversation, &range, visited)?);
            }
        }
        if let Some(branches) = &mut message.branches {
            for (index, range) in std::mem::take(&mut branches.refs) {
                branches
                    .tails
                    .insert(index, self.expand_range(conversation, &range, visited)?);
            }
        }
        Ok(())
    }
    fn expand_range(
        &self,
        conversation: &str,
        range: &AiHistoryRange,
        visited: &mut HashSet<String>,
    ) -> Result<Vec<AiChatMessage>> {
        if !visited.insert(range.branch_id.clone()) {
            return Err(anyhow!("History archive cycle detected"));
        }
        let mut pages = Vec::new();
        let mut cursor = None;
        loop {
            let mut page =
                self.range_messages(conversation, range, cursor.as_ref(), HISTORY_PAGE_SIZE)?;
            for message in &mut page.messages {
                self.expand_archives(conversation, message, visited)?;
            }
            pages.push(page.messages);
            cursor = page.before;
            if cursor.is_none() {
                break;
            }
        }
        visited.remove(&range.branch_id);
        Ok(pages.into_iter().rev().flatten().collect())
    }
}
