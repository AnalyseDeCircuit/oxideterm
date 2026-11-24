use super::{records::*, *};
use crate::history_journal::identity;

pub(super) fn shared_field(key: &str, in_payload: bool) -> bool {
    matches!(
        key,
        "result" | "output" | "envelope" | "arguments" | "argumentsText"
    ) || (in_payload
        && matches!(
            key,
            "data" | "stdout" | "stderr" | "text" | "content" | "message"
        ))
}

pub(super) fn store(
    tx: &redb::WriteTransaction,
    conversation: &str,
    value: Value,
) -> Result<StoredValue> {
    let id = identity(&value)?;
    retain(tx, conversation, &id, value)?;
    Ok(StoredValue::Shared { id })
}

pub(super) fn replace(
    tx: &redb::WriteTransaction,
    conversation: &str,
    previous: StoredValue,
    value: Value,
) -> Result<StoredValue> {
    let id = identity(&value)?;
    if matches!(&previous, StoredValue::Shared { id: old } if old == &id) {
        return Ok(previous);
    }
    retain(tx, conversation, &id, value)?;
    super::content::release_value(tx, conversation, previous)?;
    Ok(StoredValue::Shared { id })
}

#[derive(Serialize, Deserialize)]
pub(super) struct ObjectOrder {
    path: Vec<String>,
    keys: Vec<String>,
}

pub(super) fn object_order(value: &Value) -> Vec<ObjectOrder> {
    fn visit(value: &Value, path: &mut Vec<String>, result: &mut Vec<ObjectOrder>) {
        match value {
            Value::Object(fields) => {
                let keys: Vec<_> = fields.keys().cloned().collect();
                if keys.windows(2).any(|pair| pair[0] > pair[1]) {
                    result.push(ObjectOrder {
                        path: path.clone(),
                        keys,
                    });
                }
                for (key, value) in fields {
                    path.push(key.clone());
                    visit(value, path, result);
                    path.pop();
                }
            }
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    path.push(index.to_string());
                    visit(value, path, result);
                    path.pop();
                }
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    visit(value, &mut Vec::new(), &mut result);
    result
}

pub(super) fn restore_order(value: &mut Value, orders: Vec<ObjectOrder>) -> Result<()> {
    for order in orders {
        let mut target = &mut *value;
        for component in order.path {
            target = match target {
                Value::Object(fields) => fields.get_mut(&component),
                Value::Array(values) => component
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| values.get_mut(index)),
                _ => None,
            }
            .ok_or_else(|| anyhow!("History display order is invalid"))?;
        }
        let Value::Object(fields) = target else {
            return Err(anyhow!("History display order is invalid"));
        };
        let mut previous = std::mem::take(fields);
        for key in order.keys {
            let value = previous
                .remove(&key)
                .ok_or_else(|| anyhow!("History display field is missing"))?;
            fields.insert(key, value);
        }
        if !previous.is_empty() {
            return Err(anyhow!("History display order is incomplete"));
        }
    }
    Ok(())
}

pub(super) fn json_text(
    tx: &redb::WriteTransaction,
    conversation: &str,
    id: &str,
    orders: Vec<ObjectOrder>,
    previous: Option<StoredValue>,
) -> Result<StoredValue> {
    let format = (!orders.is_empty())
        .then(|| serde_json::to_value(orders))
        .transpose()?;
    let order = format.as_ref().map(identity).transpose()?;
    if matches!(&previous, Some(StoredValue::JsonText { id: old, order: old_order }) if old == id && old_order == &order)
    {
        return Ok(previous.unwrap());
    }
    let mut refs = tx.open_table(PAYLOAD_REFS)?;
    let count = refs
        .get((conversation, id))?
        .map(|row| row.value())
        .ok_or_else(|| anyhow!("History tool payload is missing"))?;
    refs.insert((conversation, id), count + 1)?;
    drop(refs);
    if let (Some(order), Some(format)) = (&order, format) {
        retain(tx, conversation, order, format)?;
    }
    if let Some(previous) = previous {
        super::content::release_value(tx, conversation, previous)?;
    }
    Ok(StoredValue::JsonText {
        id: id.into(),
        order,
    })
}

pub(super) fn is_pretty_output(value: &Value, output: &str) -> bool {
    struct Compare<'a> {
        expected: &'a [u8],
        offset: usize,
    }
    impl std::io::Write for Compare<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self
                .expected
                .get(self.offset..self.offset.saturating_add(bytes.len()))
                != Some(bytes)
            {
                return Err(std::io::Error::other("Different tool projection"));
            }
            self.offset += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Compare {
        expected: output.as_bytes(),
        offset: 0,
    };
    serde_json::to_writer_pretty(&mut writer, value).is_ok() && writer.offset == output.len()
}

fn retain(tx: &redb::WriteTransaction, conversation: &str, id: &str, value: Value) -> Result<()> {
    let exists = tx.open_table(PAYLOADS)?.get((conversation, id))?.is_some();
    if !exists {
        // Nested display fields refer to the same canonical payload rather than storing envelope copies.
        let value = super::content::store_payload_value(tx, conversation, value)?;
        tx.open_table(PAYLOADS)?
            .insert((conversation, id), rmp_serde::to_vec(&value)?.as_slice())?;
    }
    let mut refs = tx.open_table(PAYLOAD_REFS)?;
    let count = refs
        .get((conversation, id))?
        .map(|row| row.value())
        .unwrap_or(0);
    refs.insert((conversation, id), count + 1)?;
    Ok(())
}

pub(super) fn value(
    tx: &redb::ReadTransaction,
    conversation: &str,
    id: &str,
) -> Result<StoredValue> {
    let table = tx.open_table(PAYLOADS)?;
    let row = table
        .get((conversation, id))?
        .ok_or_else(|| anyhow!("History tool payload is missing"))?;
    Ok(rmp_serde::from_slice(row.value())?)
}

pub(super) fn resolve(
    tx: &redb::ReadTransaction,
    conversation: &str,
    mut node: StoredValue,
) -> Result<StoredValue> {
    let mut seen = HashSet::new();
    while let StoredValue::Shared { id } | StoredValue::JsonText { id, .. } = node {
        if !seen.insert(id.clone()) {
            return Err(anyhow!("History tool payload cycle detected"));
        }
        node = value(tx, conversation, &id)?;
    }
    Ok(node)
}

pub(super) fn release(tx: &redb::WriteTransaction, conversation: &str, id: &str) -> Result<()> {
    let count = tx
        .open_table(PAYLOAD_REFS)?
        .get((conversation, id))?
        .map(|row| row.value())
        .ok_or_else(|| anyhow!("History payload reference is missing"))?;
    if count > 1 {
        tx.open_table(PAYLOAD_REFS)?
            .insert((conversation, id), count - 1)?;
        return Ok(());
    }
    tx.open_table(PAYLOAD_REFS)?.remove((conversation, id))?;
    let node = tx
        .open_table(PAYLOADS)?
        .remove((conversation, id))?
        .map(|row| rmp_serde::from_slice(row.value()))
        .transpose()?
        .ok_or_else(|| anyhow!("History tool payload is missing"))?;
    super::content::release_value(tx, conversation, node)
}
