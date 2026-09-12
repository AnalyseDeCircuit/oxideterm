use super::{content, records::*, *};

/// Mutations carry the version captured by the owning runtime, never a worker's wall clock.
#[derive(Clone, Serialize, Deserialize)]
pub enum HistoryMutation {
    Create {
        conversation: AiConversation,
        revision: u64,
    },
    CreateAgentHistory {
        conversation_id: String,
        run_id: crate::agent::AgentRunId,
        revision: u64,
    },
    Metadata {
        conversation: AiConversation,
        revision: u64,
    },
    Rename {
        conversation_id: String,
        title: String,
        updated_at: i64,
        revision: u64,
    },
    PutMessage {
        conversation_id: String,
        branch_id: String,
        message: AiChatMessage,
        revision: u64,
    },
    StreamText {
        conversation_id: String,
        branch_id: String,
        message_id: String,
        delta: HistoryStreamDelta,
    },
    DeleteMessage {
        conversation_id: String,
        branch_id: String,
        message_id: String,
        revision: u64,
    },
    DeleteConversation {
        conversation_id: String,
        revision: u64,
    },
    DeleteBranch {
        conversation_id: String,
        branch_id: String,
        revision: u64,
    },
    Fork {
        conversation_id: String,
        source_branch: String,
        branch_id: String,
        through_sequence: Option<u64>,
        revision: u64,
    },
    ReplaceTail {
        conversation_id: String,
        source_branch: String,
        branch_id: String,
        after_message: Option<String>,
        before_message: Option<String>,
        messages: Vec<AiChatMessage>,
        revision: u64,
    },
    Compact {
        conversation_id: String,
        source_branch: String,
        branch_id: String,
        through_message: String,
        anchor: AiChatMessage,
        revision: u64,
    },
    SelectBranch {
        conversation_id: String,
        branch_id: String,
        revision: u64,
    },
    PutEvent {
        conversation_id: String,
        family: String,
        id: String,
        value: Value,
        revision: u64,
    },
}

impl HistoryMutation {
    pub(super) fn collection_owner(&self) -> Option<&str> {
        match self {
            Self::PutMessage {
                conversation_id, ..
            }
            | Self::DeleteMessage {
                conversation_id, ..
            }
            | Self::DeleteBranch {
                conversation_id, ..
            }
            | Self::ReplaceTail {
                conversation_id, ..
            }
            | Self::Compact {
                conversation_id, ..
            } => Some(conversation_id),
            _ => None,
        }
    }
    pub(super) fn sanitize(&mut self) {
        match self {
            Self::Create { conversation, .. } | Self::Metadata { conversation, .. } => {
                conversation.title = crate::sanitize_for_persistence(&conversation.title);
                if let Some(metadata) = &mut conversation.session_metadata {
                    *metadata = crate::sanitize_tool_protocol_json_for_persistence(metadata);
                }
            }
            Self::Rename { title, .. } => *title = crate::sanitize_for_persistence(title),
            Self::PutMessage { message, .. }
            | Self::Compact {
                anchor: message, ..
            } => crate::context_sanitizer::sanitize_chat_message_for_persistence(message),
            Self::ReplaceTail { messages, .. } => {
                for message in messages {
                    crate::context_sanitizer::sanitize_chat_message_for_persistence(message);
                }
            }
            Self::PutEvent { family, value, .. } => {
                *value = if family == "diagnostic" {
                    crate::sanitize_json_for_ai(value)
                } else {
                    crate::sanitize_tool_protocol_json_for_persistence(value)
                }
            }
            // The delta factory diffs complete sanitized fields, including cross-token replacements.
            Self::StreamText { .. } => {}
            _ => {}
        }
    }
}

pub(super) fn write_head(tx: &redb::WriteTransaction, mut head: ConversationHead) -> Result<()> {
    head.conversation.messages.clear();
    head.conversation.messages_loaded = false;
    let id = head.conversation.id.as_str();
    let mut table = tx.open_table(HEADS)?;
    let previous = table
        .get(id)?
        .map(|row| rmp_serde::from_slice::<ConversationHead>(row.value()))
        .transpose()?;
    let mut updated = tx.open_table(UPDATED)?;
    if let Some(previous) = previous {
        updated.remove((previous.conversation.updated_at_ms, id))?;
    }
    let bytes = rmp_serde::to_vec_named(&head)?;
    table.insert(id, bytes.as_slice())?;
    updated.insert((head.conversation.updated_at_ms, id), ())?;
    Ok(())
}

fn writable_head(
    tx: &redb::WriteTransaction,
    id: &str,
    _revision: u64,
) -> Result<Option<ConversationHead>> {
    let head = tx
        .open_table(HEADS)?
        .get(id)?
        .map(|row| rmp_serde::from_slice::<ConversationHead>(row.value()))
        .transpose()?;
    Ok(head)
}

pub(super) fn apply_mutation(tx: &redb::WriteTransaction, mutation: HistoryMutation) -> Result<()> {
    match mutation {
        HistoryMutation::CreateAgentHistory {
            conversation_id,
            run_id,
            revision,
        } => {
            let Some(mut head) = writable_head(tx, &conversation_id, revision)? else {
                return Ok(());
            };
            let id = super::agent_queries::agent_history_branch(&run_id);
            let mut table = tx.open_table(BRANCHES)?;
            if table
                .get((conversation_id.as_str(), id.as_str()))?
                .is_none()
            {
                let branch = Branch {
                    parent: None,
                    owner: BranchOwner::Agent,
                    retained: true,
                    sealed: false,
                    next_sequence: 0,
                    message_count: 0,
                    turn_count: 0,
                    revision: 0,
                };
                table.insert(
                    (conversation_id.as_str(), id.as_str()),
                    rmp_serde::to_vec_named(&branch)?.as_slice(),
                )?;
            }
            head.revision = head.revision.max(revision);
            write_head(tx, head)?;
        }
        HistoryMutation::DeleteBranch {
            conversation_id,
            branch_id,
            revision,
        } => super::reachability::delete_branch(tx, &conversation_id, &branch_id, revision)?,
        HistoryMutation::StreamText {
            conversation_id,
            branch_id,
            message_id,
            delta,
        } => {
            super::stream_delta::apply(tx, &conversation_id, &branch_id, &message_id, delta)?;
        }
        HistoryMutation::Create {
            mut conversation,
            revision,
        } => {
            if tx
                .open_table(DELETED)?
                .get(conversation.id.as_str())?
                .is_some()
                || tx
                    .open_table(HEADS)?
                    .get(conversation.id.as_str())?
                    .is_some()
            {
                return Ok(());
            }
            if !conversation.messages.is_empty() {
                return Err(anyhow!("Create history metadata before inserting messages"));
            }
            conversation.message_count = 0;
            conversation.turn_count = 0;
            let branch = Branch {
                parent: None,
                owner: BranchOwner::Conversation,
                retained: true,
                sealed: false,
                next_sequence: 0,
                message_count: 0,
                turn_count: 0,
                revision: 0,
            };
            tx.open_table(BRANCHES)?.insert(
                (conversation.id.as_str(), "main"),
                rmp_serde::to_vec_named(&branch)?.as_slice(),
            )?;
            write_head(
                tx,
                ConversationHead {
                    conversation,
                    revision,
                    metadata_revision: revision,
                    title_revision: revision,
                    structure_revision: revision,
                    active_branch: "main".into(),
                },
            )?;
        }
        HistoryMutation::Metadata {
            mut conversation,
            revision,
        } => {
            if !conversation.messages.is_empty() {
                return Err(anyhow!("Metadata updates cannot contain message bodies"));
            }
            let Some(mut head) = writable_head(tx, &conversation.id, revision)? else {
                return Ok(());
            };
            if revision < head.metadata_revision {
                return Ok(());
            }
            conversation.message_count = head.conversation.message_count;
            conversation.turn_count = head.conversation.turn_count;
            conversation.title = head.conversation.title.clone();
            conversation.updated_at_ms = conversation
                .updated_at_ms
                .max(head.conversation.updated_at_ms);
            head.conversation = conversation;
            head.metadata_revision = revision;
            head.revision = head.revision.max(revision);
            write_head(tx, head)?;
        }
        HistoryMutation::Rename {
            conversation_id,
            title,
            updated_at,
            revision,
        } => {
            let Some(mut head) = writable_head(tx, &conversation_id, revision)? else {
                return Ok(());
            };
            if revision < head.title_revision {
                return Ok(());
            }
            head.title_revision = revision;
            head.conversation.title = crate::sanitize_for_persistence(&title);
            head.conversation.updated_at_ms = head.conversation.updated_at_ms.max(updated_at);
            head.revision = head.revision.max(revision);
            write_head(tx, head)?;
        }
        HistoryMutation::PutMessage {
            conversation_id,
            branch_id,
            mut message,
            revision,
        } => {
            if tx
                .open_table(BRANCH_DELETED)?
                .get((conversation_id.as_str(), branch_id.as_str()))?
                .is_some()
            {
                return Ok(());
            }
            let Some(mut head) = writable_head(tx, &conversation_id, revision)? else {
                return Ok(());
            };
            if tx
                .open_table(MESSAGE_DELETED)?
                .get((
                    conversation_id.as_str(),
                    branch_id.as_str(),
                    message.id.as_str(),
                ))?
                .is_some()
            {
                return Ok(());
            }
            // Replayed versions must not create new archive records, including after a parent was sealed.
            if let Some(sequence) = tx
                .open_table(IDS)?
                .get((
                    conversation_id.as_str(),
                    branch_id.as_str(),
                    message.id.as_str(),
                ))?
                .map(|row| row.value())
            {
                if let Some(key) = tx
                    .open_table(ORDER)?
                    .get((conversation_id.as_str(), branch_id.as_str(), sequence))?
                    .map(|row| row.value().to_owned())
                {
                    let previous: MessageDescriptor = tx
                        .open_table(DESCRIPTORS)?
                        .get((conversation_id.as_str(), key.as_str()))?
                        .map(|row| rmp_serde::from_slice(row.value()))
                        .transpose()?
                        .ok_or_else(|| anyhow!("History description is missing"))?;
                    if previous.revision >= revision {
                        return Ok(());
                    }
                }
            }
            super::archives::normalize_message(tx, &conversation_id, &mut message, revision)?;
            let references = super::reachability::references(tx, &conversation_id, &message)?;
            let inherited = write_message_key(tx, &conversation_id, &branch_id, &message.id)?
                .filter(|(owner, _, _)| owner != &branch_id);
            let mut branches = tx.open_table(BRANCHES)?;
            let mut branch: Branch = branches
                .get((conversation_id.as_str(), branch_id.as_str()))?
                .map(|row| rmp_serde::from_slice(row.value()))
                .transpose()?
                .ok_or_else(|| anyhow!("History branch is missing"))?;
            let mut ids = tx.open_table(IDS)?;
            let sequence = ids
                .get((
                    conversation_id.as_str(),
                    branch_id.as_str(),
                    message.id.as_str(),
                ))?
                .map(|row| row.value());
            let mut order = tx.open_table(ORDER)?;
            let turns = crate::ai_conversation_turn_count(std::slice::from_ref(&message));
            let (sequence, key) = if let Some(sequence) = sequence {
                let key = order
                    .get((conversation_id.as_str(), branch_id.as_str(), sequence))?
                    .ok_or_else(|| anyhow!("History index is missing"))?
                    .value()
                    .to_owned();
                let previous: MessageDescriptor = tx
                    .open_table(DESCRIPTORS)?
                    .get((conversation_id.as_str(), key.as_str()))?
                    .map(|row| rmp_serde::from_slice(row.value()))
                    .transpose()?
                    .ok_or_else(|| anyhow!("History description is missing"))?;
                if revision < previous.revision || (branch.sealed && revision == previous.revision)
                {
                    return Ok(());
                }
                if branch.sealed {
                    return Err(anyhow!("Fork sealed history before modifying it"));
                }
                if previous.role != message.role {
                    return Err(anyhow!("A message identity cannot change its role"));
                }
                branch.turn_count = branch
                    .turn_count
                    .saturating_sub(previous.turn_count)
                    .saturating_add(turns);
                branch.revision += 1;
                (sequence, key)
            } else {
                if branch.sealed {
                    return Err(anyhow!("Fork sealed history before modifying it"));
                }
                if let Some((_, sequence, source)) = inherited {
                    let previous: MessageDescriptor = rmp_serde::from_slice(
                        tx.open_table(DESCRIPTORS)?
                            .get((conversation_id.as_str(), source.as_str()))?
                            .ok_or_else(|| anyhow!("History description is missing"))?
                            .value(),
                    )?;
                    if previous.role != message.role {
                        return Err(anyhow!("A message identity cannot change its role"));
                    }
                    branch.turn_count = branch
                        .turn_count
                        .saturating_sub(previous.turn_count)
                        .saturating_add(turns);
                    branch.revision += 1;
                    (sequence, uuid::Uuid::new_v4().to_string())
                } else {
                    let sequence = branch.next_sequence;
                    branch.next_sequence = branch
                        .next_sequence
                        .checked_add(1)
                        .ok_or_else(|| anyhow!("History sequence exhausted"))?;
                    branch.message_count += 1;
                    branch.turn_count = branch.turn_count.saturating_add(turns);
                    (sequence, uuid::Uuid::new_v4().to_string())
                }
            };
            let description = MessageDescriptor {
                id: message.id.clone(),
                storage_id: key.clone(),
                sequence,
                role: message.role,
                timestamp_ms: message.timestamp_ms,
                preview: message.content.chars().take(256).collect(),
                turn_count: turns,
                history_count: message
                    .metadata
                    .as_ref()
                    .filter(|metadata| metadata.kind == "compaction-anchor")
                    .and_then(|metadata| metadata.original_count)
                    .unwrap_or(1),
                revision,
            };
            tx.open_table(DESCRIPTORS)?.insert(
                (conversation_id.as_str(), key.as_str()),
                rmp_serde::to_vec_named(&description)?.as_slice(),
            )?;
            let mut records = tx.open_table(MESSAGES)?;
            super::reachability::index_references(
                tx,
                &conversation_id,
                &branch_id,
                sequence,
                &references,
            )?;
            super::windows::index_tool_parts(tx, &conversation_id, &key, &message)?;
            let old = records
                .get((conversation_id.as_str(), key.as_str()))?
                .map(|row| rmp_serde::from_slice::<StoredValue>(row.value()))
                .transpose()?;
            let value = serde_json::to_value(&message)?;
            let value = match old {
                Some(old) => content::update_value(tx, &conversation_id, old, value)?,
                None => content::store_value(tx, &conversation_id, value)?,
            };
            records.insert(
                (conversation_id.as_str(), key.as_str()),
                rmp_serde::to_vec(&value)?.as_slice(),
            )?;
            order.insert(
                (conversation_id.as_str(), branch_id.as_str(), sequence),
                key.as_str(),
            )?;
            ids.insert(
                (
                    conversation_id.as_str(),
                    branch_id.as_str(),
                    message.id.as_str(),
                ),
                sequence,
            )?;
            branches.insert(
                (conversation_id.as_str(), branch_id.as_str()),
                rmp_serde::to_vec_named(&branch)?.as_slice(),
            )?;
            head.revision = head.revision.max(revision);
            if head.active_branch == branch_id {
                head.conversation.message_count = branch.message_count;
                head.conversation.turn_count = branch.turn_count;
            }
            head.conversation.updated_at_ms =
                head.conversation.updated_at_ms.max(message.timestamp_ms);
            write_head(tx, head)?;
        }
        HistoryMutation::DeleteMessage {
            conversation_id,
            branch_id,
            message_id,
            revision,
        } => {
            let Some(mut head) = writable_head(tx, &conversation_id, revision)? else {
                return Ok(());
            };
            let branch: Branch = tx
                .open_table(BRANCHES)?
                .get((conversation_id.as_str(), branch_id.as_str()))?
                .map(|row| rmp_serde::from_slice(row.value()))
                .transpose()?
                .ok_or_else(|| anyhow!("History branch is missing"))?;
            if branch.sealed {
                return Err(anyhow!("Fork sealed history before modifying it"));
            }
            let located = write_message_key(tx, &conversation_id, &branch_id, &message_id)?;
            tx.open_table(MESSAGE_DELETED)?.insert(
                (
                    conversation_id.as_str(),
                    branch_id.as_str(),
                    message_id.as_str(),
                ),
                revision,
            )?;
            if let Some((owner, sequence, key)) = located {
                let description: MessageDescriptor = tx
                    .open_table(DESCRIPTORS)?
                    .get((conversation_id.as_str(), key.as_str()))?
                    .map(|row| rmp_serde::from_slice(row.value()))
                    .transpose()?
                    .ok_or_else(|| anyhow!("History description is missing"))?;
                // Inherited bodies remain owned by the sealed source branch.
                if owner == branch_id {
                    tx.open_table(IDS)?.remove((
                        conversation_id.as_str(),
                        branch_id.as_str(),
                        message_id.as_str(),
                    ))?;
                    tx.open_table(ORDER)?.remove((
                        conversation_id.as_str(),
                        branch_id.as_str(),
                        sequence,
                    ))?;
                    tx.open_table(HISTORY_REFS)?.remove((
                        conversation_id.as_str(),
                        branch_id.as_str(),
                        sequence,
                    ))?;
                    tx.open_table(DESCRIPTORS)?
                        .remove((conversation_id.as_str(), key.as_str()))?;
                    super::windows::delete_tool_parts(tx, &conversation_id, &key)?;
                    let value = tx
                        .open_table(MESSAGES)?
                        .remove((conversation_id.as_str(), key.as_str()))?
                        .map(|row| rmp_serde::from_slice(row.value()))
                        .transpose()?;
                    if let Some(value) = value {
                        content::release_value(tx, &conversation_id, value)?;
                    }
                }
                let mut branch = branch;
                branch.revision += 1;
                branch.message_count = branch.message_count.saturating_sub(1);
                branch.turn_count = branch.turn_count.saturating_sub(description.turn_count);
                if head.active_branch == branch_id {
                    head.conversation.message_count = branch.message_count;
                    head.conversation.turn_count = branch.turn_count;
                }
                tx.open_table(BRANCHES)?.insert(
                    (conversation_id.as_str(), branch_id.as_str()),
                    rmp_serde::to_vec_named(&branch)?.as_slice(),
                )?;
            }
            head.revision = head.revision.max(revision);
            write_head(tx, head)?;
        }
        HistoryMutation::Fork {
            conversation_id,
            source_branch,
            branch_id,
            through_sequence,
            revision,
        } => {
            if tx
                .open_table(BRANCH_DELETED)?
                .get((conversation_id.as_str(), branch_id.as_str()))?
                .is_some()
            {
                return Ok(());
            }
            let Some(mut head) = writable_head(tx, &conversation_id, revision)? else {
                return Ok(());
            };
            if revision < head.structure_revision {
                return Ok(());
            }
            if branch_id == source_branch {
                return Err(anyhow!("A history branch cannot parent itself"));
            }
            let mut table = tx.open_table(BRANCHES)?;
            if table
                .get((conversation_id.as_str(), branch_id.as_str()))?
                .is_some()
            {
                if head.structure_revision == revision && head.active_branch == branch_id {
                    return Ok(());
                }
                return Err(anyhow!("History branch already exists"));
            }
            head.structure_revision = revision;
            let mut parent: Branch = table
                .get((conversation_id.as_str(), source_branch.as_str()))?
                .map(|row| rmp_serde::from_slice(row.value()))
                .transpose()?
                .ok_or_else(|| anyhow!("Source branch is missing"))?;
            if through_sequence.is_some_and(|sequence| sequence >= parent.next_sequence) {
                return Err(anyhow!("History branch boundary is unavailable"));
            }
            parent.sealed = true;
            table.insert(
                (conversation_id.as_str(), source_branch.as_str()),
                rmp_serde::to_vec_named(&parent)?.as_slice(),
            )?;
            drop(table);
            let (message_count, turn_count) = match through_sequence {
                Some(sequence) => prefix_counts(tx, &conversation_id, &source_branch, sequence)?,
                None => (0, 0),
            };
            let mut table = tx.open_table(BRANCHES)?;
            let branch = Branch {
                parent: through_sequence.map(|sequence| BranchParent {
                    branch_id: source_branch,
                    first_sequence: 0,
                    last_sequence: sequence,
                }),
                owner: BranchOwner::Conversation,
                retained: true,
                sealed: false,
                next_sequence: through_sequence.map(|sequence| sequence + 1).unwrap_or(0),
                message_count,
                turn_count,
                revision: 0,
            };
            head.conversation.message_count = message_count;
            head.conversation.turn_count = turn_count;
            table.insert(
                (conversation_id.as_str(), branch_id.as_str()),
                rmp_serde::to_vec_named(&branch)?.as_slice(),
            )?;
            head.active_branch = branch_id;
            head.revision = head.revision.max(revision);
            write_head(tx, head)?;
        }
        HistoryMutation::ReplaceTail {
            conversation_id,
            source_branch,
            branch_id,
            after_message,
            before_message,
            messages,
            revision,
        } => {
            let Some(head) = writable_head(tx, &conversation_id, revision)? else {
                return Ok(());
            };
            if revision < head.structure_revision {
                return Ok(());
            }
            // An acknowledged structural operation may be retried after the caller lost its reply.
            if tx
                .open_table(BRANCHES)?
                .get((conversation_id.as_str(), branch_id.as_str()))?
                .is_some()
            {
                if head.structure_revision == revision && head.active_branch == branch_id {
                    return Ok(());
                }
                return Err(anyhow!("History branch already exists"));
            }
            let boundary = match (after_message.as_deref(), before_message.as_deref()) {
                (Some(id), None) => Some(sequence_for_message(
                    tx,
                    &conversation_id,
                    &source_branch,
                    id,
                )?),
                (None, Some(id)) => {
                    sequence_for_message(tx, &conversation_id, &source_branch, id)?.checked_sub(1)
                }
                (None, None) => None,
                _ => return Err(anyhow!("History replacement has conflicting boundaries")),
            };
            apply_mutation(
                tx,
                HistoryMutation::Fork {
                    conversation_id: conversation_id.clone(),
                    source_branch,
                    branch_id: branch_id.clone(),
                    through_sequence: boundary,
                    revision,
                },
            )?;
            for message in messages {
                apply_mutation(
                    tx,
                    HistoryMutation::PutMessage {
                        conversation_id: conversation_id.clone(),
                        branch_id: branch_id.clone(),
                        message,
                        revision,
                    },
                )?;
            }
        }
        HistoryMutation::Compact {
            conversation_id,
            source_branch,
            branch_id,
            through_message,
            anchor,
            revision,
        } => {
            super::compaction::apply(
                tx,
                conversation_id,
                source_branch,
                branch_id,
                through_message,
                anchor,
                revision,
            )?;
        }
        HistoryMutation::SelectBranch {
            conversation_id,
            branch_id,
            revision,
        } => {
            if tx
                .open_table(BRANCH_DELETED)?
                .get((conversation_id.as_str(), branch_id.as_str()))?
                .is_some()
            {
                return Err(anyhow!("History branch was deleted"));
            }
            let Some(mut head) = writable_head(tx, &conversation_id, revision)? else {
                return Ok(());
            };
            if revision < head.structure_revision {
                return Ok(());
            }
            head.structure_revision = revision;
            let branch: Branch = tx
                .open_table(BRANCHES)?
                .get((conversation_id.as_str(), branch_id.as_str()))?
                .map(|row| rmp_serde::from_slice(row.value()))
                .transpose()?
                .ok_or_else(|| anyhow!("History branch is unavailable"))?;
            if !matches!(branch.owner, BranchOwner::Conversation) {
                return Err(anyhow!("History branch is not selectable"));
            }
            head.conversation.message_count = branch.message_count;
            head.conversation.turn_count = branch.turn_count;
            head.active_branch = branch_id;
            head.revision = head.revision.max(revision);
            write_head(tx, head)?;
        }
        HistoryMutation::PutEvent {
            conversation_id,
            family,
            id,
            value,
            revision,
        } => {
            let Some(mut head) = writable_head(tx, &conversation_id, revision)? else {
                return Ok(());
            };
            let mut versions = tx.open_table(EVENT_REVISIONS)?;
            if versions
                .get((conversation_id.as_str(), family.as_str(), id.as_str()))?
                .is_some_and(|row| row.value() > revision)
            {
                return Ok(());
            }
            versions.insert(
                (conversation_id.as_str(), family.as_str(), id.as_str()),
                revision,
            )?;
            let value = if family == "diagnostic" {
                crate::sanitize_json_for_ai(&value)
            } else {
                crate::sanitize_tool_protocol_json_for_persistence(&value)
            };
            let mut ids = tx.open_table(EVENT_IDS)?;
            let existing = ids
                .get((conversation_id.as_str(), family.as_str(), id.as_str()))?
                .map(|row| row.value());
            let sequence = if let Some(sequence) = existing {
                sequence
            } else {
                let mut next = tx.open_table(EVENT_NEXT)?;
                let sequence = next
                    .get((conversation_id.as_str(), family.as_str()))?
                    .map(|row| row.value())
                    .unwrap_or(0);
                next.insert(
                    (conversation_id.as_str(), family.as_str()),
                    sequence
                        .checked_add(1)
                        .ok_or_else(|| anyhow!("History sequence exhausted"))?,
                )?;
                sequence
            };
            let mut events = tx.open_table(EVENTS)?;
            let old = events
                .get((conversation_id.as_str(), family.as_str(), sequence))?
                .map(|row| rmp_serde::from_slice(row.value()))
                .transpose()?;
            let value = match old {
                Some(old) => content::update_value(tx, &conversation_id, old, value)?,
                None => content::store_value(tx, &conversation_id, value)?,
            };
            events.insert(
                (conversation_id.as_str(), family.as_str(), sequence),
                rmp_serde::to_vec(&value)?.as_slice(),
            )?;
            ids.insert(
                (conversation_id.as_str(), family.as_str(), id.as_str()),
                sequence,
            )?;
            head.revision = head.revision.max(revision);
            write_head(tx, head)?;
        }
        HistoryMutation::DeleteConversation {
            conversation_id,
            revision,
        } => {
            let Some(head) = writable_head(tx, &conversation_id, revision)? else {
                return Ok(());
            };
            tx.open_table(DELETED)?
                .insert(conversation_id.as_str(), revision)?;
            tx.open_table(HEADS)?.remove(conversation_id.as_str())?;
            tx.open_table(UPDATED)?
                .remove((head.conversation.updated_at_ms, conversation_id.as_str()))?;
            delete_rows(tx, &conversation_id)?;
        }
    }
    Ok(())
}

fn delete_rows(tx: &redb::WriteTransaction, conversation: &str) -> Result<()> {
    for definition in [BRANCHES, MESSAGES, BLOBS, DESCRIPTORS, PAYLOADS] {
        let mut table = tx.open_table(definition)?;
        let keys = table
            .range((conversation, "")..=(conversation, "\u{10ffff}"))?
            .map(|row| row.map(|(key, _)| key.value().1.to_owned()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for key in keys {
            table.remove((conversation, key.as_str()))?;
        }
    }
    for definition in [REFS, PAYLOAD_REFS] {
        let mut table = tx.open_table(definition)?;
        let keys = table
            .range((conversation, "")..=(conversation, "\u{10ffff}"))?
            .map(|row| row.map(|(key, _)| key.value().1.to_owned()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for key in keys {
            table.remove((conversation, key.as_str()))?;
        }
    }
    for definition in [ORDER, TEXT_CHUNKS] {
        let mut order = tx.open_table(definition)?;
        let keys = order
            .range((conversation, "", 0)..=(conversation, "\u{10ffff}", u64::MAX))?
            .map(|row| row.map(|(key, _)| (key.value().1.to_owned(), key.value().2)))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (branch, sequence) in keys {
            order.remove((conversation, branch.as_str(), sequence))?;
        }
    }
    let mut ids = tx.open_table(IDS)?;
    let keys = ids
        .range((conversation, "", "")..=(conversation, "\u{10ffff}", "\u{10ffff}"))?
        .map(|row| row.map(|(key, _)| (key.value().1.to_owned(), key.value().2.to_owned())))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (branch, id) in keys {
        ids.remove((conversation, branch.as_str(), id.as_str()))?;
    }
    for definition in [EVENTS, ARRAY_ITEMS, HISTORY_REFS] {
        let mut events = tx.open_table(definition)?;
        let keys = events
            .range((conversation, "", 0)..=(conversation, "\u{10ffff}", u64::MAX))?
            .map(|row| row.map(|(key, _)| (key.value().1.to_owned(), key.value().2)))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (family, sequence) in keys {
            events.remove((conversation, family.as_str(), sequence))?;
        }
    }
    let mut ids = tx.open_table(EVENT_IDS)?;
    let keys = ids
        .range((conversation, "", "")..=(conversation, "\u{10ffff}", "\u{10ffff}"))?
        .map(|row| row.map(|(key, _)| (key.value().1.to_owned(), key.value().2.to_owned())))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (family, id) in keys {
        ids.remove((conversation, family.as_str(), id.as_str()))?;
    }
    let mut next = tx.open_table(EVENT_NEXT)?;
    let keys = next
        .range((conversation, "")..=(conversation, "\u{10ffff}"))?
        .map(|row| row.map(|(key, _)| key.value().1.to_owned()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for family in keys {
        next.remove((conversation, family.as_str()))?;
    }
    for definition in [MESSAGE_DELETED, EVENT_REVISIONS] {
        let mut table = tx.open_table(definition)?;
        let keys = table
            .range((conversation, "", "")..=(conversation, "\u{10ffff}", "\u{10ffff}"))?
            .map(|row| row.map(|(key, _)| (key.value().1.to_owned(), key.value().2.to_owned())))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (scope, id) in keys {
            table.remove((conversation, scope.as_str(), id.as_str()))?;
        }
    }
    for definition in [OBJECT_FIELDS, TOOL_PARTS] {
        let mut fields = tx.open_table(definition)?;
        let keys = fields
            .range((conversation, "", "")..=(conversation, "\u{10ffff}", "\u{10ffff}"))?
            .map(|row| row.map(|(key, _)| (key.value().1.to_owned(), key.value().2.to_owned())))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (object, key) in keys {
            fields.remove((conversation, object.as_str(), key.as_str()))?;
        }
    }
    let mut deleted = tx.open_table(BRANCH_DELETED)?;
    let keys = deleted
        .range((conversation, "")..=(conversation, "\u{10ffff}"))?
        .map(|row| row.map(|(key, _)| key.value().1.to_owned()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for key in keys {
        deleted.remove((conversation, key.as_str()))?;
    }
    Ok(())
}

fn prefix_counts(
    tx: &redb::WriteTransaction,
    conversation: &str,
    branch_id: &str,
    end: u64,
) -> Result<(usize, usize)> {
    let (messages, turns, _) = super::compaction::prefix_counts(tx, conversation, branch_id, end)?;
    Ok((messages, turns))
}

pub(super) fn sequence_for_message(
    tx: &redb::WriteTransaction,
    conversation: &str,
    branch: &str,
    id: &str,
) -> Result<u64> {
    write_message_key(tx, conversation, branch, id)?
        .map(|(_, sequence, _)| sequence)
        .ok_or_else(|| anyhow!("History boundary is missing"))
}

pub(super) fn write_message_key(
    tx: &redb::WriteTransaction,
    conversation: &str,
    branch: &str,
    id: &str,
) -> Result<Option<(String, u64, String)>> {
    let mut current = branch.to_owned();
    let mut end = u64::MAX;
    let mut start = 0;
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current.clone()) {
            return Err(anyhow!("History branch cycle detected"));
        }
        if tx
            .open_table(MESSAGE_DELETED)?
            .get((conversation, current.as_str(), id))?
            .is_some()
        {
            return Ok(None);
        }
        if let Some(sequence) = tx
            .open_table(IDS)?
            .get((conversation, current.as_str(), id))?
            .map(|row| row.value())
        {
            if sequence >= start && sequence <= end {
                let key = tx
                    .open_table(ORDER)?
                    .get((conversation, current.as_str(), sequence))?
                    .ok_or_else(|| anyhow!("History index is missing"))?
                    .value()
                    .to_owned();
                return Ok(Some((current, sequence, key)));
            }
        }
        let branch: Branch = tx
            .open_table(BRANCHES)?
            .get((conversation, current.as_str()))?
            .map(|row| rmp_serde::from_slice(row.value()))
            .transpose()?
            .ok_or_else(|| anyhow!("History branch is missing"))?;
        let Some(parent) = branch.parent else {
            return Ok(None);
        };
        current = parent.branch_id;
        start = start.max(parent.first_sequence);
        end = end.min(parent.last_sequence);
    }
}
