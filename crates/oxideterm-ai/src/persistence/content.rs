use super::{records::*, *};

pub(super) fn store_value(
    tx: &redb::WriteTransaction,
    conversation: &str,
    value: Value,
) -> Result<StoredValue> {
    store_inner(tx, conversation, value, false)
}

pub(super) fn store_payload_value(
    tx: &redb::WriteTransaction,
    conversation: &str,
    value: Value,
) -> Result<StoredValue> {
    store_inner(tx, conversation, value, true)
}

fn store_inner(
    tx: &redb::WriteTransaction,
    conversation: &str,
    value: Value,
    in_payload: bool,
) -> Result<StoredValue> {
    Ok(match value {
        Value::String(text) if text.len() >= 1024 => super::text::store(tx, conversation, text)?,
        Value::Array(values) if values.is_empty() => StoredValue::Scalar(Value::Array(values)),
        Value::Array(values) => {
            let id = uuid::Uuid::new_v4().to_string();
            let length = values.len() as u64;
            for (index, value) in values.into_iter().enumerate() {
                let value = store_inner(tx, conversation, value, in_payload)?;
                tx.open_table(ARRAY_ITEMS)?.insert(
                    (conversation, id.as_str(), index as u64),
                    rmp_serde::to_vec(&value)?.as_slice(),
                )?;
            }
            StoredValue::Array { id, length }
        }
        Value::Object(mut values) => {
            let mut result = derive_output(tx, conversation, &mut values, &mut Default::default())?;
            for (key, value) in values {
                let value =
                    if super::tool_payloads::shared_field(&key, in_payload) && !value.is_null() {
                        super::tool_payloads::store(tx, conversation, value)?
                    } else {
                        store_inner(tx, conversation, value, in_payload)?
                    };
                result.insert(key, value);
            }
            store_object(tx, conversation, result)?
        }
        scalar => StoredValue::Scalar(scalar),
    })
}

pub(super) fn load_value(
    tx: &redb::ReadTransaction,
    conversation: &str,
    value: StoredValue,
    cache: &parking_lot::Mutex<super::cache::HistoryCache>,
) -> Result<Value> {
    load_inner(tx, conversation, value, cache, &mut HashSet::new())
}

fn load_inner(
    tx: &redb::ReadTransaction,
    conversation: &str,
    value: StoredValue,
    cache: &parking_lot::Mutex<super::cache::HistoryCache>,
    visiting: &mut HashSet<String>,
) -> Result<Value> {
    Ok(match value {
        StoredValue::Scalar(value) => value,
        StoredValue::Text { id, bytes } => {
            Value::String(super::text::load(tx, conversation, &id, bytes, cache)?)
        }
        StoredValue::Array { id, length } => {
            let values = array_page(tx, conversation, &id, 0, length)?;
            Value::Array(
                values
                    .into_iter()
                    .map(|value| load_inner(tx, conversation, value, cache, visiting))
                    .collect::<Result<_>>()?,
            )
        }
        StoredValue::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    Ok((key, load_inner(tx, conversation, value, cache, visiting)?))
                })
                .collect::<Result<_>>()?,
        ),
        StoredValue::ObjectRef { id } => {
            let table = tx.open_table(OBJECT_FIELDS)?;
            let mut fields = serde_json::Map::new();
            for row in table.range(
                (conversation, id.as_str(), "")..=(conversation, id.as_str(), "\u{10ffff}"),
            )? {
                let (key, value) = row?;
                fields.insert(
                    key.value().2.to_owned(),
                    load_inner(
                        tx,
                        conversation,
                        rmp_serde::from_slice(value.value())?,
                        cache,
                        visiting,
                    )?,
                );
            }
            Value::Object(fields)
        }
        StoredValue::Shared { id } => {
            if !visiting.insert(id.clone()) {
                return Err(anyhow!("History tool payload cycle detected"));
            }
            let value = load_inner(
                tx,
                conversation,
                super::tool_payloads::value(tx, conversation, &id)?,
                cache,
                visiting,
            )?;
            visiting.remove(&id);
            value
        }
        StoredValue::JsonText { id, order } => {
            if !visiting.insert(id.clone()) {
                return Err(anyhow!("History tool payload cycle detected"));
            }
            let mut value = load_inner(
                tx,
                conversation,
                super::tool_payloads::value(tx, conversation, &id)?,
                cache,
                visiting,
            )?;
            visiting.remove(&id);
            if let Some(order) = order {
                let order = load_inner(
                    tx,
                    conversation,
                    StoredValue::Shared { id: order },
                    cache,
                    visiting,
                )?;
                super::tool_payloads::restore_order(&mut value, serde_json::from_value(order)?)?;
            }
            Value::String(serde_json::to_string_pretty(&value)?)
        }
    })
}

pub(super) fn release_value(
    tx: &redb::WriteTransaction,
    conversation: &str,
    value: StoredValue,
) -> Result<()> {
    match value {
        StoredValue::Text { id, .. } => super::text::remove_tail(tx, conversation, &id, 0)?,
        StoredValue::Array { id, length } => {
            for index in 0..length {
                let value: StoredValue = {
                    let mut table = tx.open_table(ARRAY_ITEMS)?;
                    let row = table
                        .remove((conversation, id.as_str(), index))?
                        .ok_or_else(|| anyhow!("History array item is missing"))?;
                    rmp_serde::from_slice(row.value())?
                };
                release_value(tx, conversation, value)?;
            }
        }
        StoredValue::Object(values) => {
            for value in values.into_values() {
                release_value(tx, conversation, value)?;
            }
        }
        StoredValue::ObjectRef { id } => loop {
            let entry = {
                let mut table = tx.open_table(OBJECT_FIELDS)?;
                let first = table
                    .range(
                        (conversation, id.as_str(), "")..=(conversation, id.as_str(), "\u{10ffff}"),
                    )?
                    .next()
                    .transpose()?
                    .map(|(key, value)| (key.value().2.to_owned(), value.value().to_vec()));
                if let Some((key, _)) = &first {
                    table.remove((conversation, id.as_str(), key.as_str()))?;
                }
                first
            };
            let Some((_, bytes)) = entry else {
                break;
            };
            release_value(tx, conversation, rmp_serde::from_slice(&bytes)?)?;
        },
        StoredValue::Scalar(_) => {}
        StoredValue::Shared { id } => super::tool_payloads::release(tx, conversation, &id)?,
        StoredValue::JsonText { id, order } => {
            super::tool_payloads::release(tx, conversation, &id)?;
            if let Some(order) = order {
                super::tool_payloads::release(tx, conversation, &order)?;
            }
        }
    }
    Ok(())
}

/// Stable fragments keep their ownership without being rewritten or refcounted on each token.
pub(super) fn update_value(
    tx: &redb::WriteTransaction,
    conversation: &str,
    old: StoredValue,
    value: Value,
) -> Result<StoredValue> {
    Ok(match (old, value) {
        (old @ (StoredValue::Shared { .. } | StoredValue::JsonText { .. }), value) => {
            super::tool_payloads::replace(tx, conversation, old, value)?
        }
        (StoredValue::Object(mut old), Value::Object(mut values)) => {
            let mut result = derive_output(tx, conversation, &mut values, &mut old)?;
            for (key, value) in values {
                let value = update_field(tx, conversation, &key, old.remove(&key), value)?;
                result.insert(key, value);
            }
            for old in old.into_values() {
                release_value(tx, conversation, old)?;
            }
            store_object(tx, conversation, result)?
        }
        (StoredValue::ObjectRef { id }, Value::Object(mut values)) => {
            let retained: HashSet<_> = values.keys().cloned().collect();
            let mut previous = std::collections::BTreeMap::new();
            for key in ["envelope", "output"] {
                if let Some(value) = tx
                    .open_table(OBJECT_FIELDS)?
                    .get((conversation, id.as_str(), key))?
                    .map(|row| rmp_serde::from_slice(row.value()))
                    .transpose()?
                {
                    previous.insert(key.to_owned(), value);
                }
            }
            for (key, value) in derive_output(tx, conversation, &mut values, &mut previous)? {
                tx.open_table(OBJECT_FIELDS)?.insert(
                    (conversation, id.as_str(), key.as_str()),
                    rmp_serde::to_vec(&value)?.as_slice(),
                )?;
            }
            for (key, value) in values {
                let old = tx
                    .open_table(OBJECT_FIELDS)?
                    .get((conversation, id.as_str(), key.as_str()))?
                    .map(|row| row.value().to_vec());
                let previous = old
                    .as_ref()
                    .map(|bytes| rmp_serde::from_slice(bytes))
                    .transpose()?;
                let value = update_field(tx, conversation, &key, previous, value)?;
                let bytes = rmp_serde::to_vec(&value)?;
                if old.as_deref() != Some(bytes.as_slice()) {
                    tx.open_table(OBJECT_FIELDS)?
                        .insert((conversation, id.as_str(), key.as_str()), bytes.as_slice())?;
                }
            }
            let removed = tx
                .open_table(OBJECT_FIELDS)?
                .range((conversation, id.as_str(), "")..=(conversation, id.as_str(), "\u{10ffff}"))?
                .filter_map(|row| match row {
                    Ok((key, value)) if !retained.contains(key.value().2) => {
                        Some(Ok((key.value().2.to_owned(), value.value().to_vec())))
                    }
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for (key, bytes) in removed {
                tx.open_table(OBJECT_FIELDS)?
                    .remove((conversation, id.as_str(), key.as_str()))?;
                release_value(tx, conversation, rmp_serde::from_slice(&bytes)?)?;
            }
            StoredValue::ObjectRef { id }
        }
        (
            StoredValue::Array {
                id,
                length: old_length,
            },
            Value::Array(values),
        ) => {
            let length = values.len() as u64;
            for (index, value) in values.into_iter().enumerate() {
                let previous = tx
                    .open_table(ARRAY_ITEMS)?
                    .get((conversation, id.as_str(), index as u64))?
                    .map(|row| row.value().to_vec());
                let value = match &previous {
                    Some(bytes) => {
                        update_value(tx, conversation, rmp_serde::from_slice(bytes)?, value)?
                    }
                    None => store_value(tx, conversation, value)?,
                };
                let encoded = rmp_serde::to_vec(&value)?;
                if previous.as_deref() != Some(encoded.as_slice()) {
                    tx.open_table(ARRAY_ITEMS)?.insert(
                        (conversation, id.as_str(), index as u64),
                        encoded.as_slice(),
                    )?;
                }
            }
            for index in length..old_length {
                let old: StoredValue = {
                    let mut table = tx.open_table(ARRAY_ITEMS)?;
                    let row = table
                        .remove((conversation, id.as_str(), index))?
                        .ok_or_else(|| anyhow!("History array item is missing"))?;
                    rmp_serde::from_slice(row.value())?
                };
                release_value(tx, conversation, old)?;
            }
            StoredValue::Array { id, length }
        }
        (StoredValue::Text { id, .. }, Value::String(text)) if text.len() >= 1024 => {
            super::text::update(tx, conversation, id, text)?
        }
        (old, value) => {
            let value = store_value(tx, conversation, value)?;
            release_value(tx, conversation, old)?;
            value
        }
    })
}

pub(super) fn array_page(
    tx: &redb::ReadTransaction,
    conversation: &str,
    id: &str,
    start: u64,
    end: u64,
) -> Result<Vec<StoredValue>> {
    let table = tx.open_table(ARRAY_ITEMS)?;
    (start..end)
        .map(|index| {
            let row = table
                .get((conversation, id, index))?
                .ok_or_else(|| anyhow!("History array item is missing"))?;
            Ok(rmp_serde::from_slice(row.value())?)
        })
        .collect()
}

fn store_object(
    tx: &redb::WriteTransaction,
    conversation: &str,
    fields: std::collections::BTreeMap<String, StoredValue>,
) -> Result<StoredValue> {
    let value = StoredValue::Object(fields);
    // Small message headers stay inline; large tool objects can be traversed without decoding every field.
    if rmp_serde::to_vec(&value)?.len() <= 16 * 1024 {
        return Ok(value);
    }
    let StoredValue::Object(fields) = value else {
        unreachable!()
    };
    let id = uuid::Uuid::new_v4().to_string();
    let mut table = tx.open_table(OBJECT_FIELDS)?;
    for (key, value) in fields {
        table.insert(
            (conversation, id.as_str(), key.as_str()),
            rmp_serde::to_vec(&value)?.as_slice(),
        )?;
    }
    Ok(StoredValue::ObjectRef { id })
}

pub(super) fn object_fields(
    tx: &redb::ReadTransaction,
    conversation: &str,
    value: StoredValue,
) -> Result<std::collections::BTreeMap<String, StoredValue>> {
    match super::tool_payloads::resolve(tx, conversation, value)? {
        StoredValue::Object(fields) => Ok(fields),
        StoredValue::ObjectRef { id } => tx
            .open_table(OBJECT_FIELDS)?
            .range((conversation, id.as_str(), "")..=(conversation, id.as_str(), "\u{10ffff}"))?
            .map(|row| {
                let (key, value) = row?;
                Ok((
                    key.value().2.to_owned(),
                    rmp_serde::from_slice(value.value())?,
                ))
            })
            .collect(),
        _ => Err(anyhow!("History object is missing")),
    }
}

pub(super) fn update_field(
    tx: &redb::WriteTransaction,
    conversation: &str,
    key: &str,
    previous: Option<StoredValue>,
    value: Value,
) -> Result<StoredValue> {
    if super::tool_payloads::shared_field(key, false) && !value.is_null() {
        match previous {
            Some(previous) => super::tool_payloads::replace(tx, conversation, previous, value),
            None => super::tool_payloads::store(tx, conversation, value),
        }
    } else {
        match previous {
            Some(previous) => update_value(tx, conversation, previous, value),
            None => store_value(tx, conversation, value),
        }
    }
}

fn derive_output(
    tx: &redb::WriteTransaction,
    conversation: &str,
    values: &mut serde_json::Map<String, Value>,
    previous: &mut std::collections::BTreeMap<String, StoredValue>,
) -> Result<std::collections::BTreeMap<String, StoredValue>> {
    let derived = values.get("type").and_then(Value::as_str) == Some("tool_result")
        && values
            .get("envelope")
            .zip(values.get("output").and_then(Value::as_str))
            .is_some_and(|(envelope, output)| {
                super::tool_payloads::is_pretty_output(envelope, output)
            });
    let mut result = std::collections::BTreeMap::new();
    if derived {
        let envelope = values.remove("envelope").unwrap();
        let order = super::tool_payloads::object_order(&envelope);
        if let Some(Value::String(output)) = values.remove("output") {
            drop(zeroize::Zeroizing::new(output));
        }
        let envelope = match previous.remove("envelope") {
            Some(previous) => super::tool_payloads::replace(tx, conversation, previous, envelope)?,
            None => super::tool_payloads::store(tx, conversation, envelope)?,
        };
        let StoredValue::Shared { id } = &envelope else {
            return Err(anyhow!("History tool payload is invalid"));
        };
        let output = super::tool_payloads::json_text(
            tx,
            conversation,
            id,
            order,
            previous.remove("output"),
        )?;
        result.insert("envelope".into(), envelope);
        result.insert("output".into(), output);
    }
    Ok(result)
}
