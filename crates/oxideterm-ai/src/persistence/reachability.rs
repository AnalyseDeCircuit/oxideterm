use super::{records::*, *};
use crate::AiHistoryRange;
use std::collections::{BTreeMap, VecDeque};

pub(super) fn references(
    tx: &redb::WriteTransaction,
    conversation: &str,
    message: &AiChatMessage,
) -> Result<Vec<AiHistoryRange>> {
    let mut ranges = Vec::new();
    if let Some(range) = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.original_ref.as_ref())
    {
        ranges.push(range.clone());
    }
    if let Some(branches) = &message.branches {
        ranges.extend(branches.refs.values().cloned());
    }
    if let Some(range) = message
        .summary_ref
        .as_ref()
        .and_then(|summary| summary.get("historyRange"))
    {
        ranges.push(serde_json::from_value(range.clone())?);
    }
    for range in &ranges {
        let source: Branch = tx
            .open_table(BRANCHES)?
            .get((conversation, range.branch_id.as_str()))?
            .map(|row| rmp_serde::from_slice(row.value()))
            .transpose()?
            .ok_or_else(|| anyhow!("History reference is missing"))?;
        if !source.sealed {
            return Err(anyhow!("History references require an immutable source"));
        }
        bounds(tx, conversation, range, &source)?;
    }
    Ok(ranges)
}

pub(super) fn index_references(
    tx: &redb::WriteTransaction,
    conversation: &str,
    branch: &str,
    sequence: u64,
    ranges: &[AiHistoryRange],
) -> Result<()> {
    let mut table = tx.open_table(HISTORY_REFS)?;
    if ranges.is_empty() {
        table.remove((conversation, branch, sequence))?;
    } else {
        table.insert(
            (conversation, branch, sequence),
            rmp_serde::to_vec(&ranges)?.as_slice(),
        )?;
    }
    Ok(())
}

fn bounds(
    tx: &redb::WriteTransaction,
    conversation: &str,
    range: &AiHistoryRange,
    branch: &Branch,
) -> Result<Option<(u64, u64)>> {
    let Some(last) = branch.next_sequence.checked_sub(1) else {
        if range.first_message_id.is_some() || range.last_message_id.is_some() {
            return Err(anyhow!("History range is invalid"));
        }
        return Ok(None);
    };
    let first = range
        .first_message_id
        .as_deref()
        .map(|id| super::mutations::sequence_for_message(tx, conversation, &range.branch_id, id))
        .transpose()?
        .unwrap_or(0);
    let last = range
        .last_message_id
        .as_deref()
        .map(|id| super::mutations::sequence_for_message(tx, conversation, &range.branch_id, id))
        .transpose()?
        .unwrap_or(last);
    if first > last {
        return Err(anyhow!("History range is reversed"));
    }
    Ok(Some((first, last)))
}

fn enqueue(
    coverage: &mut BTreeMap<String, Vec<(u64, u64)>>,
    queue: &mut VecDeque<(String, u64, u64)>,
    branch: &str,
    first: u64,
    last: u64,
) {
    if first > last {
        return;
    }
    let covered = coverage.entry(branch.into()).or_default();
    let mut next = first;
    for &(start, end) in covered.iter() {
        if end < next {
            continue;
        }
        if start > last {
            break;
        }
        if start > next {
            queue.push_back((branch.into(), next, start - 1));
        }
        next = end.saturating_add(1);
        if next > last {
            break;
        }
    }
    if next <= last {
        queue.push_back((branch.into(), next, last));
    }
    covered.push((first, last));
    covered.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (start, end) in std::mem::take(covered) {
        if let Some(previous) = merged
            .last_mut()
            .filter(|previous| start <= previous.1.saturating_add(1))
        {
            previous.1 = previous.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    *covered = merged;
}

/// Walk sparse reference edges, then remove only ranges that lost their final owner.
pub(super) fn collect(tx: &redb::WriteTransaction, conversation: &str) -> Result<()> {
    let branches = tx
        .open_table(BRANCHES)?
        .range((conversation, "")..=(conversation, "\u{10ffff}"))?
        .map(|row| {
            let (id, bytes) = row?;
            Ok((
                id.value().1.to_owned(),
                rmp_serde::from_slice::<Branch>(bytes.value())?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut coverage = BTreeMap::new();
    let mut queue = VecDeque::new();
    for (id, branch) in &branches {
        if branch.retained {
            if let Some(last) = branch.next_sequence.checked_sub(1) {
                enqueue(&mut coverage, &mut queue, id, 0, last);
            }
        }
    }
    while let Some((id, first, last)) = queue.pop_front() {
        let branch = branches
            .get(&id)
            .ok_or_else(|| anyhow!("History reference is missing"))?;
        if let Some(parent) = &branch.parent {
            let first = first.max(parent.first_sequence);
            let last = last.min(parent.last_sequence);
            if first <= last {
                let mut excluded = tx
                    .open_table(ORDER)?
                    .range((conversation, id.as_str(), first)..=(conversation, id.as_str(), last))?
                    .map(|row| row.map(|(key, _)| key.value().2))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let deleted = tx
                    .open_table(MESSAGE_DELETED)?
                    .range(
                        (conversation, id.as_str(), "")..=(conversation, id.as_str(), "\u{10ffff}"),
                    )?
                    .map(|row| row.map(|(key, _)| key.value().2.to_owned()))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                for key in deleted {
                    if let Some((_, sequence, _)) = super::mutations::write_message_key(
                        tx,
                        conversation,
                        &parent.branch_id,
                        &key,
                    )? {
                        if sequence >= first && sequence <= last {
                            excluded.push(sequence);
                        }
                    }
                }
                excluded.sort_unstable();
                excluded.dedup();
                let mut next = first;
                for sequence in excluded {
                    if next < sequence {
                        enqueue(
                            &mut coverage,
                            &mut queue,
                            &parent.branch_id,
                            next,
                            sequence - 1,
                        );
                    }
                    next = sequence + 1;
                }
                if next <= last {
                    enqueue(&mut coverage, &mut queue, &parent.branch_id, next, last);
                }
            }
        }
        let table = tx.open_table(HISTORY_REFS)?;
        for row in
            table.range((conversation, id.as_str(), first)..=(conversation, id.as_str(), last))?
        {
            let (_, bytes) = row?;
            for range in rmp_serde::from_slice::<Vec<AiHistoryRange>>(bytes.value())? {
                let source = branches
                    .get(&range.branch_id)
                    .ok_or_else(|| anyhow!("History reference is missing"))?;
                if let Some((first, last)) = bounds(tx, conversation, &range, source)? {
                    enqueue(&mut coverage, &mut queue, &range.branch_id, first, last);
                } else {
                    coverage.entry(range.branch_id).or_default();
                }
            }
        }
    }
    let mut metadata = coverage.keys().cloned().collect::<HashSet<_>>();
    let mut parents: Vec<_> = branches
        .iter()
        .filter(|(id, branch)| branch.retained || metadata.contains(*id))
        .map(|(id, _)| id.clone())
        .collect();
    while let Some(id) = parents.pop() {
        if let Some(parent) = branches.get(&id).and_then(|branch| branch.parent.as_ref()) {
            if metadata.insert(parent.branch_id.clone()) {
                parents.push(parent.branch_id.clone());
            }
        }
    }
    for (id, branch) in branches {
        if branch.retained {
            continue;
        }
        let ranges = coverage.remove(&id).unwrap_or_default();
        let mut start = 0;
        for (first, last) in &ranges {
            if start < *first {
                remove_range(tx, conversation, &id, start, first - 1)?;
            }
            start = last.saturating_add(1);
        }
        if start < branch.next_sequence {
            remove_range(tx, conversation, &id, start, branch.next_sequence - 1)?;
        }
        if ranges.is_empty() && !metadata.contains(&id) {
            tx.open_table(BRANCHES)?
                .remove((conversation, id.as_str()))?;
            let keys = tx
                .open_table(MESSAGE_DELETED)?
                .range((conversation, id.as_str(), "")..=(conversation, id.as_str(), "\u{10ffff}"))?
                .map(|row| row.map(|(key, _)| key.value().2.to_owned()))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for key in keys {
                tx.open_table(MESSAGE_DELETED)?.remove((
                    conversation,
                    id.as_str(),
                    key.as_str(),
                ))?;
            }
        }
    }
    Ok(())
}

fn remove_range(
    tx: &redb::WriteTransaction,
    conversation: &str,
    branch: &str,
    first: u64,
    last: u64,
) -> Result<()> {
    let entries = tx
        .open_table(ORDER)?
        .range((conversation, branch, first)..=(conversation, branch, last))?
        .map(|row| row.map(|(sequence, key)| (sequence.value().2, key.value().to_owned())))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (sequence, key) in entries {
        let description: MessageDescriptor = rmp_serde::from_slice(
            tx.open_table(DESCRIPTORS)?
                .get((conversation, key.as_str()))?
                .ok_or_else(|| anyhow!("History description is missing"))?
                .value(),
        )?;
        let value: StoredValue = rmp_serde::from_slice(
            tx.open_table(MESSAGES)?
                .get((conversation, key.as_str()))?
                .ok_or_else(|| anyhow!("History message is missing"))?
                .value(),
        )?;
        super::content::release_value(tx, conversation, value)?;
        tx.open_table(MESSAGES)?
            .remove((conversation, key.as_str()))?;
        tx.open_table(DESCRIPTORS)?
            .remove((conversation, key.as_str()))?;
        tx.open_table(ORDER)?
            .remove((conversation, branch, sequence))?;
        tx.open_table(IDS)?
            .remove((conversation, branch, description.id.as_str()))?;
        tx.open_table(HISTORY_REFS)?
            .remove((conversation, branch, sequence))?;
        super::windows::delete_tool_parts(tx, conversation, &key)?;
    }
    Ok(())
}

pub(super) fn delete_branch(
    tx: &redb::WriteTransaction,
    conversation: &str,
    id: &str,
    revision: u64,
) -> Result<()> {
    let Some(mut head) = tx
        .open_table(HEADS)?
        .get(conversation)?
        .map(|row| rmp_serde::from_slice::<ConversationHead>(row.value()))
        .transpose()?
    else {
        return Ok(());
    };
    if revision < head.structure_revision {
        return Ok(());
    }
    if head.active_branch == id {
        return Err(anyhow!(
            "Select another branch before deleting the active branch"
        ));
    }
    let branch = tx
        .open_table(BRANCHES)?
        .get((conversation, id))?
        .map(|row| rmp_serde::from_slice::<Branch>(row.value()))
        .transpose()?;
    tx.open_table(BRANCH_DELETED)?
        .insert((conversation, id), revision)?;
    if let Some(mut branch) = branch {
        if !matches!(branch.owner, BranchOwner::Conversation) {
            return Err(anyhow!("History branch has a separate owner"));
        }
        branch.retained = false;
        branch.revision += 1;
        tx.open_table(BRANCHES)?.insert(
            (conversation, id),
            rmp_serde::to_vec_named(&branch)?.as_slice(),
        )?;
    }
    head.structure_revision = revision;
    head.revision = head.revision.max(revision);
    super::mutations::write_head(tx, head)
}
