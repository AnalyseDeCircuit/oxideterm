use super::*;

impl ConversationStore {
    /// The source stays at its original path for an explicit downgrade; it is never initialized or rewritten.
    pub fn open_or_migrate(legacy_path: &Path, progress: impl FnMut(usize, usize)) -> Result<Self> {
        Self::open_or_migrate_cancellable(
            legacy_path,
            progress,
            &std::sync::atomic::AtomicBool::new(false),
        )
    }

    pub fn open_or_migrate_cancellable(
        legacy_path: &Path,
        mut progress: impl FnMut(usize, usize),
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Self> {
        check_cancelled(cancelled)?;
        let destination = legacy_path.with_file_name("chat_history.v4.redb");
        if destination.exists() {
            return Self::open(destination);
        }
        if !legacy_path.exists() {
            return Self::open(destination);
        }
        let temporary = destination.with_extension("redb.migrating");
        let legacy = AiChatPersistenceStore {
            path: legacy_path.to_owned(),
            db: Arc::new(Database::open(legacy_path)?),
        };
        // Hold the source lock before cleaning a previous attempt's temporary destination.
        if temporary.exists() {
            std::fs::remove_file(&temporary)?;
        }
        let metas = {
            let tx = legacy.db.begin_read()?;
            let table = tx.open_table(CONVERSATIONS_TABLE)?;
            table
                .iter()?
                .map(|row| {
                    let (_, bytes) = row?;
                    rmp_serde::from_slice::<ConversationMeta>(bytes.value())
                        .context("Migration conversation metadata is invalid")
                })
                .collect::<Result<Vec<_>>>()?
        };
        let store = Self::open(&temporary)?;
        let mut revision = 1;
        for (index, meta) in metas.iter().enumerate() {
            check_cancelled(cancelled)?;
            let source = legacy.db.begin_read()?;
            let ids: Vec<String> = source
                .open_table(CONV_MESSAGES_TABLE)?
                .get(meta.id.as_str())?
                .map(|row| rmp_serde::from_slice(row.value()))
                .transpose()?
                .unwrap_or_default();
            let mut seen = HashSet::new();
            let mut ordered = Vec::with_capacity(ids.len());
            for id in ids {
                check_cancelled(cancelled)?;
                if !seen.insert(id.clone()) {
                    return Err(anyhow!(
                        "Migration source contains duplicate message identities"
                    ));
                }
                // The legacy view sorted timestamps. Keep that order without retaining every body in memory.
                let message = source_message(&source, &id)?;
                ordered.push((message.timestamp_ms, id));
            }
            ordered.sort_by_key(|(timestamp, _)| *timestamp);
            let summaries = load_round_summaries_from_transcript(&source, &meta.id)?;
            let mut metadata = conversation_from_meta(meta.clone());
            let backends = metadata
                .session_metadata
                .as_mut()
                .and_then(Value::as_object_mut)
                .and_then(|metadata| metadata.remove("messageBackends"));
            if backends
                .as_ref()
                .is_some_and(|backends| !backends.is_object())
            {
                return Err(anyhow!("Migration backend provenance is invalid"));
            }
            let first_user = metadata
                .session_metadata
                .as_mut()
                .and_then(Value::as_object_mut)
                .and_then(|metadata| {
                    let value = metadata.remove("firstUserMessage")?;
                    metadata.insert(
                        "firstUserMessageRef".into(),
                        serde_json::json!({"family":"legacy_metadata","id":"first-user-message"}),
                    );
                    Some(value)
                });
            store.apply(vec![HistoryMutation::Create {
                conversation: metadata,
                revision,
            }])?;
            if let Some(value) = first_user {
                revision += 1;
                migrate_event_batch(
                    &store,
                    &meta.id,
                    "legacy_metadata",
                    vec![HistoryMutation::PutEvent {
                        conversation_id: meta.id.clone(),
                        family: "legacy_metadata".into(),
                        id: "first-user-message".into(),
                        value: serde_json::json!({"field":"firstUserMessage","value":value}),
                        revision,
                    }],
                )?;
            }
            store.import_messages(
                &meta.id,
                "main",
                ordered.iter().map(|(_, id)| {
                    check_cancelled(cancelled)?;
                    let mut message = source_message(&source, id)?;
                    if let Some(backends) = &backends {
                        crate::acp::migrate_message_backends(&mut message, backends)?;
                    }
                    apply_round_summaries_to_messages(
                        std::slice::from_mut(&mut message),
                        &summaries,
                    );
                    Ok(message)
                }),
                &mut revision,
            )?;
            migrate_transcript(&legacy, &store, &meta.id, &mut revision, cancelled)?;
            let mut cursor = None;
            let mut remaining = ordered.len();
            loop {
                check_cancelled(cancelled)?;
                let page = store.page(&meta.id, "main", cursor.as_ref(), HISTORY_PAGE_SIZE)?;
                let start = remaining
                    .checked_sub(page.messages.len())
                    .ok_or_else(|| anyhow!("Migration message count mismatch"))?;
                for (mut actual, (_, id)) in
                    page.messages.into_iter().zip(&ordered[start..remaining])
                {
                    let mut expected = source_message(&source, id)?;
                    if let Some(backends) = &backends {
                        crate::acp::migrate_message_backends(&mut expected, backends)?;
                    }
                    apply_round_summaries_to_messages(
                        std::slice::from_mut(&mut expected),
                        &summaries,
                    );
                    crate::context_sanitizer::sanitize_chat_message_for_persistence(&mut expected);
                    expected.is_streaming = false;
                    store.expand_message_archives(&meta.id, &mut actual)?;
                    if actual != expected {
                        return Err(anyhow!("Migration history validation failed"));
                    }
                }
                remaining = start;
                cursor = page.before;
                if cursor.is_none() {
                    break;
                }
            }
            if remaining != 0 {
                return Err(anyhow!("Migration history is incomplete"));
            }
            progress(index + 1, metas.len());
        }
        check_cancelled(cancelled)?;
        drop(store);
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temporary)?
            .sync_all()?;
        std::fs::rename(&temporary, &destination)?;
        #[cfg(unix)]
        if let Some(parent) = destination.parent() {
            std::fs::File::open(parent)?.sync_all()?;
        }
        drop(legacy);
        Self::open(destination)
    }
}

fn migrate_transcript(
    legacy: &AiChatPersistenceStore,
    store: &ConversationStore,
    id: &str,
    revision: &mut u64,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    let tx = legacy.db.begin_read()?;
    for (index_definition, table_definition, family) in [
        (CONV_TRANSCRIPT_TABLE, TRANSCRIPT_TABLE, "transcript"),
        (CONV_DIAGNOSTIC_TABLE, DIAGNOSTIC_TABLE, "diagnostic"),
    ] {
        let index = match tx.open_table(index_definition) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => continue,
            Err(error) => return Err(error.into()),
        };
        let ids: Vec<String> = index
            .get(id)?
            .map(|row| rmp_serde::from_slice(row.value()))
            .transpose()?
            .unwrap_or_default();
        let records = tx.open_table(table_definition)?;
        let mut seen = HashSet::new();
        let mut batch = Vec::new();
        for key in ids {
            check_cancelled(cancelled)?;
            if !seen.insert(key.clone()) {
                return Err(anyhow!("Migration event identities are duplicated"));
            }
            let row = records
                .get(key.as_str())?
                .ok_or_else(|| anyhow!("Migration event is missing"))?;
            let value = if family == "transcript" {
                serde_json::to_value(rmp_serde::from_slice::<PersistedTranscriptEntry>(
                    row.value(),
                )?)?
            } else {
                serde_json::to_value(rmp_serde::from_slice::<PersistedDiagnosticEvent>(
                    row.value(),
                )?)?
            };
            *revision += 1;
            batch.push(HistoryMutation::PutEvent {
                conversation_id: id.into(),
                family: family.into(),
                id: key,
                value,
                revision: *revision,
            });
            if batch.len() == HISTORY_PAGE_SIZE {
                migrate_event_batch(store, id, family, std::mem::take(&mut batch))?;
            }
        }
        migrate_event_batch(store, id, family, batch)?;
    }
    match tx.open_table(agents::AGENT_RECORDS) {
        Ok(records) => {
            let prefix = format!("{id}:");
            for row in records.range(prefix.as_str()..)? {
                check_cancelled(cancelled)?;
                let (key, row) = row?;
                if !key.value().starts_with(&prefix) {
                    break;
                }
                let mut record: crate::agent::AgentRecord = rmp_serde::from_slice(row.value())?;
                for message in &mut record.messages {
                    crate::context_sanitizer::sanitize_chat_message_for_persistence(message);
                }
                let expected = crate::sanitize_tool_protocol_json_for_persistence(
                    &serde_json::to_value(&record)?,
                );
                let run = record.snapshot.run.run_id.clone();
                let branch = super::agent_queries::agent_history_branch(&run);
                *revision += 1;
                store.apply(vec![HistoryMutation::CreateAgentHistory {
                    conversation_id: id.into(),
                    run_id: run.clone(),
                    revision: *revision,
                }])?;
                let messages = std::mem::take(&mut record.messages);
                for page in messages.chunks(HISTORY_PAGE_SIZE) {
                    check_cancelled(cancelled)?;
                    store.apply(
                        page.iter()
                            .map(|message| {
                                *revision += 1;
                                HistoryMutation::PutMessage {
                                    conversation_id: id.into(),
                                    branch_id: branch.clone(),
                                    message: message.clone(),
                                    revision: *revision,
                                }
                            })
                            .collect(),
                    )?;
                }
                let family = format!("agent-communication:{run}");
                let communication = std::mem::take(&mut record.communication);
                for page in communication.chunks(HISTORY_PAGE_SIZE) {
                    check_cancelled(cancelled)?;
                    let batch = page
                        .iter()
                        .map(|message| {
                            *revision += 1;
                            Ok(HistoryMutation::PutEvent {
                                conversation_id: id.into(),
                                family: family.clone(),
                                id: message.sequence.to_string(),
                                value: serde_json::to_value(message)?,
                                revision: *revision,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    migrate_event_batch(store, id, &family, batch)?;
                }
                let mut metadata = serde_json::to_value(record)?;
                metadata["messageBranch"] = branch.into();
                *revision += 1;
                migrate_event_batch(
                    store,
                    id,
                    "agent",
                    vec![HistoryMutation::PutEvent {
                        conversation_id: id.into(),
                        family: "agent".into(),
                        id: run.to_string(),
                        value: metadata,
                        revision: *revision,
                    }],
                )?;
                let actual = store
                    .load_agent_record(id, &run)?
                    .ok_or_else(|| anyhow!("Migrated agent record is missing"))?;
                let mut expected: crate::agent::AgentRecord = serde_json::from_value(expected)?;
                for message in &mut expected.messages {
                    super::agent_queries::normalize_agent_message(message);
                }
                if serde_json::to_value(actual)? != serde_json::to_value(expected)? {
                    return Err(anyhow!("Migrated agent history validation failed"));
                }
            }
        }
        Err(redb::TableError::TableDoesNotExist(_)) => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn migrate_event_batch(
    store: &ConversationStore,
    conversation: &str,
    family: &str,
    batch: Vec<HistoryMutation>,
) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let expected = batch
        .iter()
        .map(|mutation| match mutation {
            HistoryMutation::PutEvent { value, .. } if family == "diagnostic" => {
                crate::sanitize_json_for_ai(value)
            }
            HistoryMutation::PutEvent { value, .. } => {
                crate::sanitize_tool_protocol_json_for_persistence(value)
            }
            _ => unreachable!("migration event batches contain only events"),
        })
        .collect::<Vec<_>>();
    store.apply(batch)?;
    let actual = store
        .events(conversation, family, None, expected.len())?
        .into_iter()
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(anyhow!("Migration event validation failed"));
    }
    Ok(())
}

fn source_message(tx: &redb::ReadTransaction, id: &str) -> Result<AiChatMessage> {
    let messages = tx.open_table(MESSAGES_TABLE)?;
    let row = messages
        .get(id)?
        .ok_or_else(|| anyhow!("Migration source message is missing"))?;
    let mut persisted = decode_persisted_message(row.value())?;
    if let Some(context) = persisted.context_snapshot.as_mut() {
        if let Some(tail) = context.buffer_tail.as_ref() {
            context.buffer_tail = Some(decompress_buffer(tail, context.buffer_compressed)?);
            context.buffer_compressed = false;
        }
    }
    let mut message = message_from_persisted(persisted);
    super::archives::preserve_archive_counts(&mut message);
    Ok(message)
}

fn check_cancelled(cancelled: &std::sync::atomic::AtomicBool) -> Result<()> {
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        return Err(anyhow!("History migration was cancelled"));
    }
    Ok(())
}
