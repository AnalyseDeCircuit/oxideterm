use super::{records::*, *};
use crate::history_journal::Change;
use zeroize::Zeroizing;

pub(super) fn apply(
    tx: &redb::WriteTransaction,
    conversation: &str,
    branch: &str,
    message_id: &str,
    delta: HistoryStreamDelta,
) -> Result<()> {
    let Some(mut head) = tx
        .open_table(HEADS)?
        .get(conversation)?
        .map(|row| rmp_serde::from_slice::<ConversationHead>(row.value()))
        .transpose()?
    else {
        return Ok(());
    };
    if tx
        .open_table(MESSAGE_DELETED)?
        .get((conversation, branch, message_id))?
        .is_some()
    {
        return Ok(());
    }
    let Some(sequence) = tx
        .open_table(IDS)?
        .get((conversation, branch, message_id))?
        .map(|row| row.value())
    else {
        return Ok(());
    };
    let key = tx
        .open_table(ORDER)?
        .get((conversation, branch, sequence))?
        .ok_or_else(|| anyhow!("History index is missing"))?
        .value()
        .to_owned();
    let mut description: MessageDescriptor = rmp_serde::from_slice(
        tx.open_table(DESCRIPTORS)?
            .get((conversation, key.as_str()))?
            .ok_or_else(|| anyhow!("History description is missing"))?
            .value(),
    )?;
    if description.revision >= delta.revision {
        return Ok(());
    }
    let branch: Branch = rmp_serde::from_slice(
        tx.open_table(BRANCHES)?
            .get((conversation, branch))?
            .ok_or_else(|| anyhow!("History branch is missing"))?
            .value(),
    )?;
    if branch.sealed || description.revision != delta.base_revision {
        return Err(anyhow!("History text base changed"));
    }
    let mut value: StoredValue = rmp_serde::from_slice(
        tx.open_table(MESSAGES)?
            .get((conversation, key.as_str()))?
            .ok_or_else(|| anyhow!("History message is missing"))?
            .value(),
    )?;
    for field in delta.fields {
        value = edit(tx, conversation, value, &field.path, field.change)?;
    }
    if !delta.tool_parts.is_empty() {
        let mut table = tx.open_table(TOOL_PARTS)?;
        for (id, link) in delta.tool_parts {
            if let Some(link) = link {
                table.insert(
                    (conversation, key.as_str(), id.as_str()),
                    rmp_serde::to_vec(&link)?.as_slice(),
                )?;
            } else {
                table.remove((conversation, key.as_str(), id.as_str()))?;
            }
        }
    }
    description.preview = preview(tx, conversation, &value)?;
    description.revision = delta.revision;
    tx.open_table(MESSAGES)?.insert(
        (conversation, key.as_str()),
        rmp_serde::to_vec(&value)?.as_slice(),
    )?;
    tx.open_table(DESCRIPTORS)?.insert(
        (conversation, key.as_str()),
        rmp_serde::to_vec_named(&description)?.as_slice(),
    )?;
    head.revision = head.revision.max(delta.revision);
    super::mutations::write_head(tx, head)
}

fn edit(
    tx: &redb::WriteTransaction,
    conversation: &str,
    value: StoredValue,
    path: &[String],
    change: Change,
) -> Result<StoredValue> {
    let Some((key, rest)) = path.split_first() else {
        return apply_field(tx, conversation, value, change);
    };
    Ok(match value {
        StoredValue::Object(mut fields) => {
            let previous = fields
                .remove(key)
                .unwrap_or(StoredValue::Scalar(Value::Null));
            if rest.is_empty() && matches!(change, Change::Remove) {
                super::content::release_value(tx, conversation, previous)?;
            } else {
                fields.insert(
                    key.clone(),
                    edit_object_field(tx, conversation, key, previous, rest, change)?,
                );
            }
            StoredValue::Object(fields)
        }
        StoredValue::ObjectRef { id } => {
            let previous = tx
                .open_table(OBJECT_FIELDS)?
                .get((conversation, id.as_str(), key.as_str()))?
                .map(|row| rmp_serde::from_slice(row.value()))
                .transpose()?
                .unwrap_or(StoredValue::Scalar(Value::Null));
            if rest.is_empty() && matches!(change, Change::Remove) {
                super::content::release_value(tx, conversation, previous)?;
                tx.open_table(OBJECT_FIELDS)?
                    .remove((conversation, id.as_str(), key.as_str()))?;
            } else {
                let next = edit_object_field(tx, conversation, key, previous, rest, change)?;
                tx.open_table(OBJECT_FIELDS)?.insert(
                    (conversation, id.as_str(), key.as_str()),
                    rmp_serde::to_vec(&next)?.as_slice(),
                )?;
            }
            StoredValue::ObjectRef { id }
        }
        StoredValue::Array { id, length } => {
            let index: u64 = key.parse()?;
            if index >= length {
                return Err(anyhow!("History array offset is invalid"));
            }
            let previous = rmp_serde::from_slice(
                tx.open_table(ARRAY_ITEMS)?
                    .get((conversation, id.as_str(), index))?
                    .ok_or_else(|| anyhow!("History part is missing"))?
                    .value(),
            )?;
            let next = edit(tx, conversation, previous, rest, change)?;
            tx.open_table(ARRAY_ITEMS)?.insert(
                (conversation, id.as_str(), index),
                rmp_serde::to_vec(&next)?.as_slice(),
            )?;
            StoredValue::Array { id, length }
        }
        _ => return Err(anyhow!("History field is unavailable")),
    })
}

fn edit_object_field(
    tx: &redb::WriteTransaction,
    conversation: &str,
    key: &str,
    previous: StoredValue,
    rest: &[String],
    change: Change,
) -> Result<StoredValue> {
    match (rest.is_empty(), change) {
        (true, Change::Set(value)) => {
            super::content::update_field(tx, conversation, key, Some(previous), value)
        }
        (_, change) => edit(tx, conversation, previous, rest, change),
    }
}

fn apply_field(
    tx: &redb::WriteTransaction,
    conversation: &str,
    value: StoredValue,
    change: Change,
) -> Result<StoredValue> {
    match change {
        Change::Text { keep, append } => match value {
            StoredValue::Text { id, bytes } => {
                super::text::replace_tail(tx, conversation, id, bytes, keep, &append)
            }
            value => {
                let mut text = match value {
                    StoredValue::Scalar(Value::String(text)) => Zeroizing::new(text),
                    StoredValue::Scalar(Value::Null) => Zeroizing::new(String::new()),
                    _ => return Err(anyhow!("History text is invalid")),
                };
                let keep = usize::try_from(keep)?;
                if keep > text.len() || !text.is_char_boundary(keep) {
                    return Err(anyhow!("History text boundary is invalid"));
                }
                text.truncate(keep);
                text.push_str(&append);
                super::content::store_value(
                    tx,
                    conversation,
                    Value::String(std::mem::take(&mut *text)),
                )
            }
        },
        Change::Append(values) => {
            let (id, mut length) = match value {
                StoredValue::Array { id, length } => (id, length),
                StoredValue::Scalar(Value::Array(values)) if values.is_empty() => {
                    (uuid::Uuid::new_v4().to_string(), 0)
                }
                _ => return Err(anyhow!("History array is invalid")),
            };
            for value in values {
                let value = super::content::store_value(tx, conversation, value)?;
                tx.open_table(ARRAY_ITEMS)?.insert(
                    (conversation, id.as_str(), length),
                    rmp_serde::to_vec(&value)?.as_slice(),
                )?;
                length += 1;
            }
            Ok(StoredValue::Array { id, length })
        }
        Change::Remove => Err(anyhow!("History removal requires an object field")),
        Change::Set(next) => super::content::update_value(tx, conversation, value, next),
    }
}

fn preview(tx: &redb::WriteTransaction, conversation: &str, value: &StoredValue) -> Result<String> {
    let content = match value {
        StoredValue::Object(fields) => fields.get("content").cloned(),
        StoredValue::ObjectRef { id } => tx
            .open_table(OBJECT_FIELDS)?
            .get((conversation, id.as_str(), "content"))?
            .map(|row| rmp_serde::from_slice(row.value()))
            .transpose()?,
        _ => None,
    }
    .ok_or_else(|| anyhow!("History text is missing"))?;
    match content {
        StoredValue::Scalar(Value::String(text)) => Ok(text.chars().take(256).collect()),
        StoredValue::Text { id, bytes: 0 } => {
            let _ = id;
            Ok(String::new())
        }
        StoredValue::Text { id, .. } => super::text::preview(tx, conversation, &id),
        _ => Err(anyhow!("History text is invalid")),
    }
}
