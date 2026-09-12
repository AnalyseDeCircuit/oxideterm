use super::{records::*, *};

pub(super) fn apply(
    tx: &redb::WriteTransaction,
    conversation: String,
    source: String,
    id: String,
    through: String,
    mut anchor: AiChatMessage,
    revision: u64,
) -> Result<()> {
    if tx
        .open_table(BRANCH_DELETED)?
        .get((conversation.as_str(), id.as_str()))?
        .is_some()
    {
        return Ok(());
    }
    let Some(mut head) = tx
        .open_table(HEADS)?
        .get(conversation.as_str())?
        .map(|row| rmp_serde::from_slice::<ConversationHead>(row.value()))
        .transpose()?
    else {
        return Ok(());
    };
    if revision < head.structure_revision {
        return Ok(());
    }
    if tx
        .open_table(BRANCHES)?
        .get((conversation.as_str(), id.as_str()))?
        .is_some()
    {
        if head.active_branch == id && head.structure_revision == revision {
            return Ok(());
        }
        return Err(anyhow!("History branch already exists"));
    }
    if head.active_branch != source {
        return Err(anyhow!("History changed during compaction"));
    }
    let boundary = super::mutations::sequence_for_message(tx, &conversation, &source, &through)?;
    let (messages, turns, originals) = prefix_counts(tx, &conversation, &source, boundary)?;
    let range = crate::AiHistoryRange {
        branch_id: source.clone(),
        first_message_id: None,
        last_message_id: Some(through),
    };
    if let Some(metadata) = &mut anchor.metadata {
        metadata.original_ref = Some(range.clone());
        metadata.original_messages = None;
        metadata.original_count = Some(originals);
        metadata.original_user_count = Some(turns);
    }
    if let Some(summary) = anchor.summary_ref.as_mut().and_then(Value::as_object_mut) {
        summary.insert("historyRange".into(), serde_json::to_value(&range)?);
        if summary.get("kind").and_then(Value::as_str) == Some("conversation") {
            summary.insert("originalUserCount".into(), serde_json::json!(turns));
        }
    }
    let mut table = tx.open_table(BRANCHES)?;
    let mut parent: Branch = rmp_serde::from_slice(
        table
            .get((conversation.as_str(), source.as_str()))?
            .ok_or_else(|| anyhow!("History source branch is missing"))?
            .value(),
    )?;
    parent.sealed = true;
    let next = parent.next_sequence;
    let branch = Branch {
        parent: (boundary.saturating_add(1) < next).then(|| BranchParent {
            branch_id: source.clone(),
            first_sequence: boundary + 1,
            last_sequence: next - 1,
        }),
        owner: BranchOwner::Conversation,
        retained: true,
        sealed: false,
        next_sequence: boundary,
        message_count: parent.message_count.saturating_sub(messages),
        turn_count: parent.turn_count.saturating_sub(turns),
        revision: 0,
    };
    table.insert(
        (conversation.as_str(), source.as_str()),
        rmp_serde::to_vec_named(&parent)?.as_slice(),
    )?;
    table.insert(
        (conversation.as_str(), id.as_str()),
        rmp_serde::to_vec_named(&branch)?.as_slice(),
    )?;
    drop(table);
    head.active_branch = id.clone();
    head.structure_revision = revision;
    head.revision = head.revision.max(revision);
    super::mutations::write_head(tx, head)?;
    super::mutations::apply_mutation(
        tx,
        HistoryMutation::PutMessage {
            conversation_id: conversation.clone(),
            branch_id: id.clone(),
            message: anchor,
            revision,
        },
    )?;
    let mut table = tx.open_table(BRANCHES)?;
    let mut branch: Branch = rmp_serde::from_slice(
        table
            .get((conversation.as_str(), id.as_str()))?
            .unwrap()
            .value(),
    )?;
    branch.next_sequence = next;
    table.insert(
        (conversation.as_str(), id.as_str()),
        rmp_serde::to_vec_named(&branch)?.as_slice(),
    )?;
    Ok(())
}

pub(super) fn prefix_counts(
    tx: &redb::WriteTransaction,
    conversation: &str,
    branch_id: &str,
    mut end: u64,
) -> Result<(usize, usize, usize)> {
    let order = tx.open_table(ORDER)?;
    let branches = tx.open_table(BRANCHES)?;
    let descriptions = tx.open_table(DESCRIPTORS)?;
    let deleted = tx.open_table(MESSAGE_DELETED)?;
    let mut current = branch_id.to_owned();
    let mut start = 0;
    let mut visited = HashSet::new();
    let mut sequences = HashSet::new();
    let (mut messages, mut turns, mut originals) = (0, 0, 0);
    while start <= end {
        if !visited.insert(current.clone()) {
            return Err(anyhow!("History branch cycle detected"));
        }
        for row in order.range(
            (conversation, current.as_str(), start)..=(conversation, current.as_str(), end),
        )? {
            let (sequence, key) = row?;
            if !sequences.insert(sequence.value().2) {
                continue;
            }
            let description: MessageDescriptor = rmp_serde::from_slice(
                descriptions
                    .get((conversation, key.value()))?
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
            messages += 1;
            turns += description.turn_count;
            originals += description.history_count;
        }
        let branch: Branch = rmp_serde::from_slice(
            branches
                .get((conversation, current.as_str()))?
                .ok_or_else(|| anyhow!("History branch is missing"))?
                .value(),
        )?;
        let Some(parent) = branch.parent else {
            break;
        };
        current = parent.branch_id;
        start = start.max(parent.first_sequence);
        end = end.min(parent.last_sequence);
    }
    Ok((messages, turns, originals))
}
