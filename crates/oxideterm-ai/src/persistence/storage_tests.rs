use super::*;
use redb::ReadableTableMetadata;

fn empty_conversation() -> AiConversation {
    let mut state = AiChatState::default();
    state.create_conversation("history".into(), Some("Original".into()), 1, None);
    state.conversations.remove(0)
}
fn message(index: usize) -> AiChatMessage {
    serde_json::from_value(serde_json::json!({"id":format!("message-{index}"),"role":"user", "content":format!("text-{index}"),"timestamp_ms":1})).unwrap()
}
fn put(index: usize, revision: u64) -> HistoryMutation {
    HistoryMutation::PutMessage {
        conversation_id: "history".into(),
        branch_id: "main".into(),
        message: message(index),
        revision,
    }
}

#[test]
fn explicit_mutations_keep_newer_indices_and_deleted_histories_closed() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            put(0, 2),
            put(1, 3),
        ])
        .unwrap();
    store
        .apply(vec![HistoryMutation::Rename {
            conversation_id: "history".into(),
            title: "Latest".into(),
            updated_at: 3,
            revision: 4,
        }])
        .unwrap();
    store
        .apply(vec![
            put(0, 2),
            HistoryMutation::Rename {
                conversation_id: "history".into(),
                title: "Stale".into(),
                updated_at: 1,
                revision: 1,
            },
        ])
        .unwrap();
    let page = store.page("history", "main", None, 50).unwrap();
    assert_eq!(
        page.messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["text-0", "text-1"]
    );
    assert_eq!(
        store
            .conversation_head("history")
            .unwrap()
            .unwrap()
            .conversation
            .title,
        "Latest"
    );
    store
        .apply(vec![HistoryMutation::DeleteConversation {
            conversation_id: "history".into(),
            revision: 5,
        }])
        .unwrap();
    store
        .apply(vec![
            put(2, 6),
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
        ])
        .unwrap();
    assert!(store.conversation_head("history").unwrap().is_none());
    assert!(store.list_heads(None, 10).unwrap().is_empty());
}

#[test]
fn independent_late_message_updates_survive_newer_metadata_and_deleted_ids_stay_closed() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            put(0, 2),
            put(1, 3),
        ])
        .unwrap();
    store
        .apply(vec![HistoryMutation::Rename {
            conversation_id: "history".into(),
            title: "Renamed".into(),
            updated_at: 50,
            revision: 50,
        }])
        .unwrap();
    let mut metadata = empty_conversation();
    metadata.session_id = Some("acp-session".into());
    store
        .apply(vec![HistoryMutation::Metadata {
            conversation: metadata,
            revision: 20,
        }])
        .unwrap();
    let head = store.conversation_head("history").unwrap().unwrap();
    assert_eq!(
        (
            head.conversation.title.as_str(),
            head.conversation.session_id.as_deref()
        ),
        ("Renamed", Some("acp-session"))
    );
    let mut update = message(0);
    update.content = "late but current message revision".into();
    store
        .apply(vec![HistoryMutation::PutMessage {
            conversation_id: "history".into(),
            branch_id: "main".into(),
            message: update,
            revision: 4,
        }])
        .unwrap();
    assert_eq!(
        store.page("history", "main", None, 50).unwrap().messages[0].content,
        "late but current message revision"
    );
    store
        .apply(vec![
            HistoryMutation::DeleteMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message_id: "message-0".into(),
                revision: 51,
            },
            put(0, 52),
        ])
        .unwrap();
    assert_eq!(
        store
            .page("history", "main", None, 50)
            .unwrap()
            .messages
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        vec!["message-1"]
    );
    let head = store.conversation_head("history").unwrap().unwrap();
    assert_eq!(head.conversation.title, "Renamed");
    assert_eq!(head.conversation.turn_count, 1);
}

#[test]
fn range_pages_keep_equal_timestamps_in_insertion_order_and_reject_changed_structure() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    store
        .apply(vec![HistoryMutation::Create {
            conversation: empty_conversation(),
            revision: 1,
        }])
        .unwrap();
    store
        .apply((0..105).map(|index| put(index, index as u64 + 2)).collect())
        .unwrap();
    let last = store.page("history", "main", None, 50).unwrap();
    assert_eq!(
        last.messages
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>(),
        (55..105)
            .map(|i| format!("message-{i}"))
            .collect::<Vec<_>>()
    );
    store.apply(vec![put(105, 108)]).unwrap();
    let middle = store
        .page("history", "main", last.before.as_ref(), 50)
        .unwrap();
    assert_eq!(
        middle
            .messages
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>(),
        (5..55).map(|i| format!("message-{i}")).collect::<Vec<_>>()
    );
    store
        .apply(vec![HistoryMutation::DeleteMessage {
            conversation_id: "history".into(),
            branch_id: "main".into(),
            message_id: "message-1".into(),
            revision: 109,
        }])
        .unwrap();
    assert!(
        store
            .page("history", "main", middle.before.as_ref(), 50)
            .is_err()
    );
}

#[tokio::test]
async fn writer_acknowledges_only_committed_content_and_shutdown_drains() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v4.redb");
    let store = ConversationStore::open(&path).unwrap();
    let writer = HistoryWriter::new(store.clone()).unwrap();
    writer
        .submit(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            put(0, 2),
        ])
        .await
        .unwrap();
    assert_eq!(
        store.page("history", "main", None, 50).unwrap().messages[0].content,
        "text-0"
    );
    writer.shutdown().await.unwrap();
    assert_eq!(writer.pending_bytes(), 0);
    assert!(writer.submit(vec![put(1, 3)]).await.is_err());
}

#[test]
fn chunked_unicode_is_shared_and_replacing_content_reclaims_old_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let mut item = message(0);
    item.content = "中文abc".repeat(20_000);
    item.turn = Some(
        serde_json::json!({"parts":[{"type":"text","text":item.content}],"plainTextSummary":item.content}),
    );
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: item.clone(),
                revision: 2,
            },
        ])
        .unwrap();
    let loaded = store
        .page("history", "main", None, 50)
        .unwrap()
        .messages
        .remove(0);
    assert_eq!(loaded.content, "中文abc".repeat(20_000));
    assert_eq!(loaded.turn.unwrap()["parts"][0]["text"], loaded.content);
    let tx = store.db.read().as_ref().unwrap().begin_read().unwrap();
    assert_eq!(tx.open_table(records::BLOBS).unwrap().len().unwrap(), 3);
    drop(tx);
    store.apply(vec![put(0, 3)]).unwrap();
    assert_eq!(
        store
            .db
            .read()
            .as_ref()
            .unwrap()
            .begin_read()
            .unwrap()
            .open_table(records::BLOBS)
            .unwrap()
            .len()
            .unwrap(),
        0
    );
}

fn agent_record() -> crate::agent::AgentRecord {
    use crate::agent::*;
    let runtime = AgentRuntime::new(1);
    let parent = runtime.create_group(
        "history".into(),
        AgentModel {
            provider_id: "provider".into(),
            model: "model".into(),
        },
        AgentScope::default(),
        2,
    );
    let child = runtime
        .delegate(
            &parent,
            AgentText::new("Inspect"),
            AgentText::new("Inspect files"),
            AgentScope::default(),
            None,
        )
        .unwrap();
    runtime
        .send(
            &parent,
            &child,
            AgentMessageKind::UserSupplement,
            AgentText::new("Check the manifest"),
        )
        .unwrap();
    AgentRecord {
        parent_usage: AgentUsage::default(),
        created_at_ms: 2,
        snapshot: runtime.snapshot(&child).unwrap(),
        parent_message_id: "message-0".into(),
        target_labels: vec![AgentText::new("server")],
        messages: vec![message(200), message(201)],
        communication: runtime.communication(&child),
        revision: 1,
    }
}

#[test]
fn agent_messages_and_communication_use_independent_incremental_history() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let mut record = agent_record();
    let run = record.snapshot.run.run_id.clone();
    let branch = agent_history_branch(&run);
    let mut metadata = serde_json::to_value(&record).unwrap();
    metadata["messages"] = serde_json::json!([]);
    metadata["communication"] = serde_json::json!([]);
    metadata["messageBranch"] = branch.clone().into();
    let family = format!("agent-communication:{run}");
    let event = |record: &crate::agent::AgentRecord, revision| HistoryMutation::PutEvent {
        conversation_id: "history".into(),
        family: family.clone(),
        id: record.communication[0].sequence.to_string(),
        value: serde_json::to_value(&record.communication[0]).unwrap(),
        revision,
    };
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            put(0, 2),
            HistoryMutation::CreateAgentHistory {
                conversation_id: "history".into(),
                run_id: run.clone(),
                revision: 3,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: branch.clone(),
                message: record.messages[0].clone(),
                revision: 4,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: branch.clone(),
                message: record.messages[1].clone(),
                revision: 5,
            },
            HistoryMutation::PutEvent {
                conversation_id: "history".into(),
                family: "agent".into(),
                id: run.to_string(),
                value: metadata,
                revision: 6,
            },
            event(&record, 7),
        ])
        .unwrap();
    record.messages[1].content = "Updated child result".into();
    record.communication[0].consumed = true;
    let communication_text = "通信内容 🙂\n".repeat(20_000);
    record.communication[0].text = crate::agent::AgentText::new(&communication_text);
    store
        .apply(vec![
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: branch.clone(),
                message: record.messages[1].clone(),
                revision: 8,
            },
            event(&record, 9),
        ])
        .unwrap();
    let page = store
        .agent_communication_page("history", &run, None)
        .unwrap()
        .unwrap();
    let first_cursor = page.more.first().cloned().unwrap();
    let mut text = page.message.text.as_str().to_owned();
    let mut cursor = page.more.into_iter().next();
    while let Some(next) = cursor {
        let page = store.content_page(&next).unwrap();
        let part = page.value.as_str().unwrap();
        assert!(part.len() <= records::CONTENT_CHUNK_BYTES);
        text.push_str(part);
        cursor = page.more.into_iter().next();
    }
    assert_eq!(text, communication_text);
    store.apply(vec![event(&record, 10)]).unwrap();
    assert!(store.content_page(&first_cursor).is_err());
    let loaded = store.load_agent_record("history", &run).unwrap().unwrap();
    assert_eq!(
        loaded
            .messages
            .iter()
            .map(|message| (message.id.as_str(), message.content.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("message-200", "text-200"),
            ("message-201", "Updated child result")
        ]
    );
    assert_eq!(
        serde_json::to_value(&loaded.communication).unwrap(),
        serde_json::to_value(&record.communication).unwrap()
    );
    let head = store.conversation_head("history").unwrap().unwrap();
    assert_eq!(
        (head.active_branch.as_str(), head.conversation.message_count),
        ("main", 1)
    );
    assert_eq!(
        store.page("history", "main", None, 50).unwrap().messages[0].content,
        "text-0"
    );
    assert!(
        store.events("history", "agent", None, 10).unwrap()[0].1["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    store
        .apply(vec![HistoryMutation::DeleteConversation {
            conversation_id: "history".into(),
            revision: 11,
        }])
        .unwrap();
    assert!(store.load_agent_record("history", &run).unwrap().is_none());
}

#[test]
fn migration_preserves_legacy_file_and_validates_ordered_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let legacy = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    state.conversations.push(empty_conversation());
    state.active_conversation_id = Some("history".into());
    for index in 0..105 {
        state.add_message("history", message(index));
    }
    state.conversations[0].session_metadata = Some(serde_json::json!({
        "firstUserMessage":"original metadata prompt",
        "messageBackends":{"message-104":{"kind":"acp","backendId":"native","model":"test-model"}}
    }));
    legacy.save_state(state).unwrap();
    let agent = agent_record();
    legacy.save_agent_records(vec![agent.clone()]).unwrap();
    drop(legacy);
    let before = std::fs::read(&path).unwrap();
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    assert!(
        ConversationStore::open_or_migrate_cancellable(
            &path,
            |_, _| {
                cancelled.store(true, Ordering::Release);
            },
            &cancelled
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!path.with_file_name("chat_history.v4.redb").exists());
    assert!(
        path.with_file_name("chat_history.v4.redb.migrating")
            .exists()
    );
    let mut progress = Vec::new();
    let store =
        ConversationStore::open_or_migrate(&path, |done, total| progress.push((done, total)))
            .unwrap();
    assert_eq!(progress, vec![(1, 1)]);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let migrated_agent = store
        .load_agent_record("history", &agent.snapshot.run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        migrated_agent
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["text-200", "text-201"]
    );
    assert_eq!(
        serde_json::to_value(&migrated_agent.communication).unwrap(),
        serde_json::to_value(&agent.communication).unwrap()
    );
    let last = store.page("history", "main", None, 50).unwrap();
    let head = store.conversation_head("history").unwrap().unwrap();
    let metadata = head.conversation.session_metadata.as_ref().unwrap();
    assert!(metadata.get("messageBackends").is_none());
    assert!(metadata.get("firstUserMessage").is_none());
    assert_eq!(
        store
            .events("history", "legacy_metadata", None, 10)
            .unwrap()[0]
            .1["value"],
        "original metadata prompt"
    );
    let mut loaded = head.conversation;
    loaded.messages = last.messages.clone();
    let backend = crate::ai_message_backend_provenance(&loaded, "message-104").unwrap();
    assert_eq!(
        (backend.backend_id.as_str(), backend.model.as_str()),
        ("native", "test-model")
    );
    assert_eq!(
        last.messages
            .iter()
            .map(|m| m.content.clone())
            .collect::<Vec<_>>(),
        (55..105).map(|i| format!("text-{i}")).collect::<Vec<_>>()
    );
    let destination = store.path().to_owned();
    drop(store);
    std::fs::write(&destination, b"damaged history").unwrap();
    assert!(ConversationStore::open_or_migrate(&path, |_, _| {}).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read(&destination).unwrap(), b"damaged history");
}

#[test]
fn forks_share_frozen_prefixes_without_overwriting_old_branch_results() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            put(0, 2),
            put(1, 3),
            put(2, 4),
        ])
        .unwrap();
    store
        .apply(vec![HistoryMutation::Fork {
            conversation_id: "history".into(),
            source_branch: "main".into(),
            branch_id: "retry".into(),
            through_sequence: Some(0),
            revision: 5,
        }])
        .unwrap();
    let mut changed = message(1);
    changed.content = "different branch answer".into();
    store
        .apply(vec![HistoryMutation::PutMessage {
            conversation_id: "history".into(),
            branch_id: "retry".into(),
            message: changed,
            revision: 6,
        }])
        .unwrap();
    assert_eq!(
        store
            .page("history", "retry", None, 50)
            .unwrap()
            .messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>(),
        vec!["text-0", "different branch answer"]
    );
    assert_eq!(
        store
            .page("history", "main", None, 50)
            .unwrap()
            .messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>(),
        vec!["text-0", "text-1", "text-2"]
    );
    assert_eq!(
        store
            .conversation_head("history")
            .unwrap()
            .unwrap()
            .conversation
            .message_count,
        2
    );
    assert!(store.apply(vec![put(0, 7)]).is_err());
    store
        .apply(vec![HistoryMutation::DeleteMessage {
            conversation_id: "history".into(),
            branch_id: "retry".into(),
            message_id: "message-0".into(),
            revision: 7,
        }])
        .unwrap();
    assert_eq!(
        store.page("history", "retry", None, 1).unwrap().messages[0].content,
        "different branch answer"
    );
    store
        .apply(vec![HistoryMutation::Fork {
            conversation_id: "history".into(),
            source_branch: "retry".into(),
            branch_id: "after-delete".into(),
            through_sequence: Some(1),
            revision: 8,
        }])
        .unwrap();
    let inherited = store.page("history", "after-delete", None, 50).unwrap();
    assert_eq!(
        inherited
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["message-1"]
    );
    assert_eq!(
        store
            .conversation_head("history")
            .unwrap()
            .unwrap()
            .conversation
            .turn_count,
        1
    );
    store
        .apply(vec![HistoryMutation::SelectBranch {
            conversation_id: "history".into(),
            branch_id: "main".into(),
            revision: 8,
        }])
        .unwrap();
    assert_eq!(
        store
            .conversation_head("history")
            .unwrap()
            .unwrap()
            .conversation
            .message_count,
        3
    );
}

#[derive(Debug)]
struct FailingDisk {
    backend: redb::backends::FileBackend,
    fail: Arc<std::sync::atomic::AtomicBool>,
}
impl redb::StorageBackend for FailingDisk {
    fn len(&self) -> std::io::Result<u64> {
        self.backend.len()
    }
    fn read(&self, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
        self.backend.read(offset, len)
    }
    fn set_len(&self, len: u64) -> std::io::Result<()> {
        self.backend.set_len(len)
    }
    fn sync_data(&self, eventual: bool) -> std::io::Result<()> {
        self.backend.sync_data(eventual)
    }
    fn write(&self, offset: u64, data: &[u8]) -> std::io::Result<()> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("fixture disk full"));
        }
        self.backend.write(offset, data)
    }
}

#[tokio::test]
async fn failed_disk_write_retains_the_batch_until_storage_retry_commits_it() {
    for large in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v4.redb");
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let db = Database::builder()
            .create_with_backend(FailingDisk {
                backend: redb::backends::FileBackend::new(file).unwrap(),
                fail: fail.clone(),
            })
            .unwrap();
        records::initialize(&db).unwrap();
        let store = ConversationStore::from_database(path, db);
        let writer = HistoryWriter::new(store.clone()).unwrap();
        writer
            .submit(vec![HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            }])
            .await
            .unwrap();
        let mut status = writer.subscribe();
        fail.store(true, Ordering::SeqCst);
        let sender = writer.clone();
        let mut first = message(0);
        if large {
            first.content = "中文内容 abc\n".repeat(800_000);
        }
        let expected = first.content.clone();
        let pending = tokio::spawn(async move {
            sender
                .submit(vec![HistoryMutation::PutMessage {
                    conversation_id: "history".into(),
                    branch_id: "main".into(),
                    message: first,
                    revision: 2,
                }])
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            while *status.borrow_and_update() != HistoryWriteState::Failed {
                status.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        assert!(!pending.is_finished());
        assert!(writer.flush().await.is_err());
        // Losing a UI waiter must not discard an admitted transaction or its order.
        pending.abort();
        let later_writer = writer.clone();
        let later = tokio::spawn(async move { later_writer.submit(vec![put(1, 3)]).await });
        tokio::task::yield_now().await;
        assert!(!later.is_finished());
        let database = store.db.read();
        fail.store(false, Ordering::SeqCst);
        let retry_writer = writer.clone();
        let retry = tokio::spawn(async move { retry_writer.retry().await });
        let closing_writer = writer.clone();
        let closing = tokio::spawn(async move { closing_writer.shutdown().await });
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert_eq!(*status.borrow(), HistoryWriteState::Failed);
        assert!(!retry.is_finished());
        assert!(!closing.is_finished());
        drop(database);
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            retry.await.unwrap().unwrap();
            later.await.unwrap().unwrap();
            closing.await.unwrap().unwrap();
        })
        .await
        .expect("storage recovery must not deadlock behind later byte permits");
        assert_eq!(
            store
                .page("history", "main", None, 50)
                .unwrap()
                .messages
                .into_iter()
                .map(|message| (message.id, message.content))
                .collect::<Vec<_>>(),
            vec![
                ("message-0".into(), expected),
                ("message-1".into(), "text-1".into())
            ]
        );
        assert_eq!(writer.pending_bytes(), 0);
    }
}

#[test]
#[ignore = "child process used by the crash durability test"]
fn history_crash_child() {
    use std::io::Write;
    let path = std::env::var_os("OXIDETERM_HISTORY_CRASH_PATH").expect("child fixture path");
    let committed = std::env::var_os("OXIDETERM_HISTORY_CRASH_COMMIT").is_some();
    let store = ConversationStore::open(PathBuf::from(path)).unwrap();
    let guard = store.db.read();
    let tx = guard.as_ref().unwrap().begin_write().unwrap();
    super::mutations::apply_mutation(&tx, put(1, 3)).unwrap();
    if committed {
        tx.commit().unwrap();
    } else {
        println!("ready");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }
    println!("ready");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn process_crash_recovers_only_committed_transactions() {
    use std::io::BufRead;
    for committed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v4.redb");
        let store = ConversationStore::open(&path).unwrap();
        store
            .apply(vec![
                HistoryMutation::Create {
                    conversation: empty_conversation(),
                    revision: 1,
                },
                put(0, 2),
            ])
            .unwrap();
        drop(store);
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--ignored",
                "--exact",
                "persistence::storage_tests::history_crash_child",
                "--nocapture",
            ])
            .env("OXIDETERM_HISTORY_CRASH_PATH", &path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if committed {
            child.env("OXIDETERM_HISTORY_CRASH_COMMIT", "1");
        }
        let mut child = child.spawn().unwrap();
        let output = child.stdout.take().unwrap();
        let mut reached_boundary = false;
        for line in std::io::BufReader::new(output).lines() {
            if line.unwrap() == "ready" {
                reached_boundary = true;
                break;
            }
        }
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            reached_boundary,
            "child did not reach the selected transaction boundary"
        );
        let store = ConversationStore::open(&path).unwrap();
        let texts = store
            .page("history", "main", None, 50)
            .unwrap()
            .messages
            .into_iter()
            .map(|m| m.content)
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            if committed {
                vec!["text-0", "text-1"]
            } else {
                vec!["text-0"]
            }
        );
    }
}

#[test]
fn metadata_pages_do_not_decode_unopened_message_content() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let mut item = message(0);
    item.content = "body".repeat(40_000);
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: item,
                revision: 2,
            },
        ])
        .unwrap();
    {
        let guard = store.db.read();
        let tx = guard.as_ref().unwrap().begin_write().unwrap();
        let mut table = tx.open_table(records::BLOBS).unwrap();
        let key = table
            .iter()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .0
            .value()
            .1
            .to_owned();
        table
            .insert(("history", key.as_str()), b"\x01invalid zstd".as_slice())
            .unwrap();
        drop(table);
        tx.commit().unwrap();
    }
    let page = store.message_page("history", "main", None, 50).unwrap();
    assert_eq!(page.messages[0].id, "message-0");
    assert_eq!(page.messages[0].preview, "body".repeat(64));
    assert!(
        store
            .message("history", &page.messages[0].storage_id)
            .is_err()
    );
}

#[test]
fn tool_payload_references_restore_generated_output_without_storing_display_copies() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let result = serde_json::json!({"ok":true,"data":{"stdout":"short output","code":0}});
    let expected_output = "{\n  \"ok\": true,\n  \"data\": {\n    \"stdout\": \"short output\",\n    \"code\": 0\n  }\n}";
    let mut first = message(0);
    first.role = AiChatRole::Assistant;
    crate::stream_state::update_ai_tool_call_status(
        &mut first,
        "call",
        "run_command",
        "{}",
        "completed",
        Some(result.clone()),
        None,
        None,
        None,
        None,
    );
    let mut second = first.clone();
    second.id = "message-1".into();
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: first,
                revision: 2,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: second.clone(),
                revision: 3,
            },
        ])
        .unwrap();
    let actual = store.message_by_id("history", "main", "message-0").unwrap();
    assert_eq!(actual.tool_calls[0]["result"], result);
    assert_eq!(
        actual.turn.as_ref().unwrap()["parts"][1]["output"],
        expected_output
    );
    let guard = store.db.read();
    let tx = guard.as_ref().unwrap().begin_read().unwrap();
    let strings = tx
        .open_table(records::PAYLOADS)
        .unwrap()
        .iter()
        .unwrap()
        .filter_map(|row| {
            let (_, bytes) = row.unwrap();
            match rmp_serde::from_slice::<records::StoredValue>(bytes.value()).unwrap() {
                records::StoredValue::Scalar(Value::String(text)) => Some(text),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        strings
            .iter()
            .filter(|text| text.as_str() == "short output")
            .count(),
        1
    );
    assert!(!strings.iter().any(|text| text == expected_output));
    drop(tx);
    drop(guard);
    second.content = "updated response".into();
    store
        .apply(vec![
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: second,
                revision: 4,
            },
            HistoryMutation::DeleteMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message_id: "message-0".into(),
                revision: 5,
            },
        ])
        .unwrap();
    let remaining = store.message_by_id("history", "main", "message-1").unwrap();
    assert_eq!(remaining.content, "updated response");
    assert_eq!(
        remaining.turn.unwrap()["parts"][1]["output"],
        expected_output
    );
    store
        .apply(vec![HistoryMutation::DeleteMessage {
            conversation_id: "history".into(),
            branch_id: "main".into(),
            message_id: "message-1".into(),
            revision: 6,
        }])
        .unwrap();
    let guard = store.db.read();
    let tx = guard.as_ref().unwrap().begin_read().unwrap();
    assert_eq!(tx.open_table(records::PAYLOADS).unwrap().len().unwrap(), 0);
    assert_eq!(
        tx.open_table(records::PAYLOAD_REFS).unwrap().len().unwrap(),
        0
    );
}

#[test]
fn stream_deltas_keep_closed_unicode_chunks_and_redact_across_commit_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let prefix = "中文 safe text\n".repeat(40_000);
    let mut item = message(0);
    item.role = AiChatRole::Assistant;
    item.is_streaming = true;
    item.content = format!("{prefix}password=abcd");
    item.turn = Some(
        serde_json::json!({"id":"turn", "status":"streaming", "parts":[{"type":"text","text":item.content}],
        "providerParts":{"openai-responses":[{"type":"reasoning","encrypted_content":"opaque reasoning state"}]}}),
    );
    let base = HistoryStreamSnapshot::capture(&item, 2);
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: item.clone(),
                revision: 2,
            },
        ])
        .unwrap();
    let frozen = {
        let guard = store.db.read();
        let tx = guard.as_ref().unwrap().begin_read().unwrap();
        tx.open_table(records::TEXT_CHUNKS)
            .unwrap()
            .iter()
            .unwrap()
            .filter_map(|row| {
                let (key, value) = row.unwrap();
                (key.value().2 < 128 * 1024).then(|| {
                    (
                        key.value().1.to_owned(),
                        key.value().2,
                        value.value().to_owned(),
                    )
                })
            })
            .collect::<Vec<_>>()
    };
    item.content.push_str("efgh\nDone 中文");
    crate::stream_state::append_ai_turn_text_part(&mut item, "text", "efgh\nDone 中文", false);
    let (delta, next) = base.delta(&item, 3).unwrap();
    let operation = HistoryMutation::StreamText {
        conversation_id: "history".into(),
        branch_id: "main".into(),
        message_id: item.id.clone(),
        delta,
    };
    assert!(
        rmp_serde::to_vec(&operation).unwrap().len() < 2048,
        "a suffix commit must not encode the growing reply"
    );
    store.apply(vec![operation.clone(), operation]).unwrap();
    let actual = store
        .page("history", "main", None, 50)
        .unwrap()
        .messages
        .remove(0);
    let expected = format!("{prefix}password=[REDACTED]\nDone 中文");
    assert_eq!(actual.content, expected);
    assert_eq!(actual.turn.as_ref().unwrap()["parts"][0]["text"], expected);
    assert_eq!(
        actual.turn.as_ref().unwrap()["providerParts"]["openai-responses"][0]["encrypted_content"],
        "opaque reasoning state"
    );
    let guard = store.db.read();
    let tx = guard.as_ref().unwrap().begin_read().unwrap();
    for (text, offset, key) in frozen {
        assert_eq!(
            tx.open_table(records::TEXT_CHUNKS)
                .unwrap()
                .get(("history", text.as_str(), offset))
                .unwrap()
                .unwrap()
                .value(),
            key
        );
    }
    drop(tx);
    drop(guard);
    store
        .apply(vec![HistoryMutation::DeleteMessage {
            conversation_id: "history".into(),
            branch_id: "main".into(),
            message_id: item.id.clone(),
            revision: 4,
        }])
        .unwrap();
    item.content.push_str(" late");
    crate::stream_state::append_ai_turn_text_part(&mut item, "text", " late", false);
    let (delta, _) = next.delta(&item, 5).unwrap();
    store
        .apply(vec![HistoryMutation::StreamText {
            conversation_id: "history".into(),
            branch_id: "main".into(),
            message_id: item.id,
            delta,
        }])
        .unwrap();
    assert_eq!(
        store.page("history", "main", None, 50).unwrap().messages,
        Vec::<AiChatMessage>::new()
    );
}

#[test]
fn stream_deltas_initialize_turns_and_append_thinking_parts() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let mut item = message(0);
    item.role = AiChatRole::Assistant;
    item.content.clear();
    item.is_streaming = true;
    let mut base = HistoryStreamSnapshot::capture(&item, 2);
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: item.clone(),
                revision: 2,
            },
        ])
        .unwrap();
    for (revision, thinking, text) in [
        (3, false, "Hello"),
        (4, true, "Consider the constraints"),
        (5, false, " world"),
    ] {
        if thinking {
            item.thinking_content
                .get_or_insert_with(String::new)
                .push_str(text);
        } else {
            item.content.push_str(text);
        }
        crate::stream_state::append_ai_turn_text_part(
            &mut item,
            if thinking { "thinking" } else { "text" },
            text,
            thinking,
        );
        let (delta, next) = base.delta(&item, revision).unwrap();
        store
            .apply(vec![HistoryMutation::StreamText {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message_id: item.id.clone(),
                delta,
            }])
            .unwrap();
        base = next;
    }
    let actual = store
        .page("history", "main", None, 50)
        .unwrap()
        .messages
        .remove(0);
    assert_eq!(actual.content, "Hello world");
    assert_eq!(
        actual.thinking_content.as_deref(),
        Some("Consider the constraints")
    );
    let texts = actual.turn.as_ref().unwrap()["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| {
            (
                part["type"].as_str().unwrap(),
                part["text"].as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        texts,
        vec![
            ("text", "Hello"),
            ("thinking", "Consider the constraints"),
            ("text", " world")
        ]
    );
}

#[test]
fn content_windows_page_unicode_and_seek_tool_parts_without_decoding_other_entries() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let mut item = message(0);
    item.content = "中文abc\n".repeat(20_000);
    let expected = item.content.clone();
    item.turn = Some(serde_json::json!({
        "parts": (0..100).map(|index| serde_json::json!({"type":"text","text":format!("round {index}")})).collect::<Vec<_>>(),
        "payload": (0..1000).map(|index| (format!("field-{index:04}"), Value::String(format!("value {index}")))).collect::<serde_json::Map<_,_>>()
    }));
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: item,
                revision: 2,
            },
        ])
        .unwrap();
    let description = store
        .message_page("history", "main", None, 1)
        .unwrap()
        .messages
        .remove(0);
    let cursor = HistoryContentCursor {
        event: None,
        conversation_id: "history".into(),
        storage_id: description.storage_id.clone(),
        revision: 2,
        path: vec!["content".into()],
        offset: 0,
        byte_offset: 0,
        after_key: None,
    };
    let mut next = Some(cursor.clone());
    let mut text = String::new();
    while let Some(cursor) = next {
        let page = store.content_page(&cursor).unwrap();
        let fragment = page.value.as_str().unwrap();
        assert!(fragment.len() <= records::CONTENT_CHUNK_BYTES);
        text.push_str(fragment);
        next = page.more.into_iter().next();
    }
    assert_eq!(text, expected);
    {
        let database = store.db.read();
        let tx = database.as_ref().unwrap().begin_write().unwrap();
        let mut arrays = tx.open_table(records::ARRAY_ITEMS).unwrap();
        let (array, index) = arrays
            .iter()
            .unwrap()
            .next()
            .unwrap()
            .map(|(key, _)| (key.value().1.to_owned(), key.value().2))
            .unwrap();
        assert_eq!(index, 0);
        arrays
            .insert(
                ("history", array.as_str(), 0),
                b"invalid stored part".as_slice(),
            )
            .unwrap();
        drop(arrays);
        tx.commit().unwrap();
    }
    let mut part = cursor.clone();
    part.path = vec!["turn".into(), "parts".into(), "99".into()];
    assert_eq!(
        store.content_page(&part).unwrap().value,
        serde_json::json!({"type":"text","text":"round 99"})
    );
    let view = store
        .message_view("history", &description.storage_id, 2, None)
        .unwrap();
    assert_eq!((view.section, view.sections), (99, 100));
    assert_eq!(
        view.message.turn.as_ref().unwrap()["parts"],
        serde_json::json!([{"type":"text","text":"round 99"}])
    );
    part.path = vec!["turn".into(), "payload".into(), "field-0900".into()];
    assert_eq!(
        store.content_page(&part).unwrap().value,
        Value::String("value 900".into())
    );
    part.revision = 1;
    assert!(store.content_page(&part).is_err());
}

#[test]
fn history_cache_evicts_old_chunks_without_changing_live_readers() {
    let mut cache = super::cache::HistoryCache::default();
    let first: Arc<[u8]> = Arc::from(vec![1; HISTORY_CACHE_BYTES - 512]);
    cache.insert("history", "old", first.clone());
    assert!(Arc::ptr_eq(&cache.get("history", "old").unwrap(), &first));
    let next: Arc<[u8]> = Arc::from([2u8, 3, 4]);
    cache.insert("history", "new", next);
    assert!(cache.get("history", "old").is_none());
    assert_eq!(&*cache.get("history", "new").unwrap(), &[2, 3, 4]);
    assert_eq!(first[first.len() - 1], 1);
    let render = Arc::new(vec![9u8; HISTORY_CACHE_BYTES - 512]);
    cache.insert_render("history", "render", render.clone(), render.len());
    assert!(Arc::ptr_eq(
        &cache.render::<Vec<u8>>("history", "render").unwrap(),
        &render
    ));
    cache.insert("history", "replacement", Arc::from([5u8, 6, 7]));
    assert!(cache.render::<Vec<u8>>("history", "render").is_none());
    assert_eq!(&*cache.get("history", "replacement").unwrap(), &[5, 6, 7]);
}

#[tokio::test]
async fn large_messages_stream_through_the_writer_without_a_storage_size_cap() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let writer = HistoryWriter::new(store.clone()).unwrap();
    let mut item = message(0);
    item.content = "中文内容 abc\n".repeat(800_000);
    let expected = item.content.clone();
    writer
        .submit(vec![HistoryMutation::Create {
            conversation: empty_conversation(),
            revision: 1,
        }])
        .await
        .unwrap();
    let database = store.db.read();
    let held_transaction = database.as_ref().unwrap().begin_write().unwrap();
    let pending_writer = writer.clone();
    let pending = tokio::spawn(async move {
        pending_writer
            .submit(vec![HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: item,
                revision: 2,
            }])
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while writer.pending_bytes() != HISTORY_PENDING_BYTES {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !pending.is_finished(),
        "the producer must wait while the writer is blocked and its byte budget is full"
    );
    held_transaction.abort().unwrap();
    drop(database);
    pending.await.unwrap().unwrap();
    assert_eq!(
        store.page("history", "main", None, 50).unwrap().messages[0].content,
        expected
    );
    writer.shutdown().await.unwrap();
}

#[test]
fn deleting_a_branch_reclaims_exclusive_and_shadowed_bodies_without_losing_its_child() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    store
        .apply(vec![HistoryMutation::Create {
            conversation: empty_conversation(),
            revision: 1,
        }])
        .unwrap();
    store
        .apply(
            (0..4)
                .map(|index| {
                    let mut message = message(index);
                    message.content = format!("source {index}\n{}", "body ".repeat(2000));
                    HistoryMutation::PutMessage {
                        conversation_id: "history".into(),
                        branch_id: "main".into(),
                        message,
                        revision: index as u64 + 2,
                    }
                })
                .collect(),
        )
        .unwrap();
    store
        .apply(vec![HistoryMutation::Fork {
            conversation_id: "history".into(),
            source_branch: "main".into(),
            branch_id: "child".into(),
            through_sequence: Some(1),
            revision: 6,
        }])
        .unwrap();
    let mut changed = message(0);
    changed.content = "child content".into();
    store
        .apply(vec![HistoryMutation::PutMessage {
            conversation_id: "history".into(),
            branch_id: "child".into(),
            message: changed,
            revision: 7,
        }])
        .unwrap();
    assert_eq!(
        store
            .page("history", "child", None, 50)
            .unwrap()
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["message-0", "message-1"]
    );
    assert_eq!(
        store
            .conversation_head("history")
            .unwrap()
            .unwrap()
            .conversation
            .message_count,
        2
    );
    assert_eq!(
        store.page("history", "main", None, 1).unwrap().messages[0].content,
        format!("source 3\n{}", "body ".repeat(2000))
    );
    store
        .apply(vec![
            HistoryMutation::DeleteMessage {
                conversation_id: "history".into(),
                branch_id: "child".into(),
                message_id: "message-1".into(),
                revision: 8,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "child".into(),
                message: message(4),
                revision: 9,
            },
            HistoryMutation::DeleteBranch {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                revision: 10,
            },
        ])
        .unwrap();
    assert_eq!(
        store
            .page("history", "child", None, 50)
            .unwrap()
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["child content", "text-4"]
    );
    let guard = store.db.read();
    let tx = guard.as_ref().unwrap().begin_read().unwrap();
    assert_eq!(tx.open_table(records::MESSAGES).unwrap().len().unwrap(), 2);
    assert_eq!(tx.open_table(records::BLOBS).unwrap().len().unwrap(), 0);
    assert_eq!(
        tx.open_table(records::TEXT_CHUNKS).unwrap().len().unwrap(),
        0
    );
    drop(tx);
    drop(guard);
    store
        .apply(vec![
            HistoryMutation::Fork {
                conversation_id: "history".into(),
                source_branch: "child".into(),
                branch_id: "main".into(),
                through_sequence: Some(0),
                revision: 11,
            },
            put(100, 12),
        ])
        .unwrap();
    assert_eq!(
        store
            .conversation_head("history")
            .unwrap()
            .unwrap()
            .active_branch,
        "child"
    );
    assert!(
        store
            .apply(vec![HistoryMutation::SelectBranch {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                revision: 13
            }])
            .is_err()
    );
}

#[test]
fn edited_tail_reuses_prefix_and_replay_does_not_replace_newer_branch() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            put(0, 2),
            put(1, 3),
            put(2, 4),
        ])
        .unwrap();
    let prefix = store
        .message_page("history", "main", None, 10)
        .unwrap()
        .messages[0]
        .storage_id
        .clone();
    let mut edited = message(1);
    edited.content = "edited on a new branch".into();
    let edit = HistoryMutation::ReplaceTail {
        conversation_id: "history".into(),
        source_branch: "main".into(),
        branch_id: "edited".into(),
        after_message: None,
        before_message: Some("message-1".into()),
        messages: vec![edited],
        revision: 5,
    };
    store.apply(vec![edit.clone()]).unwrap();
    store.apply(vec![edit.clone()]).unwrap();
    let changed = store.page("history", "edited", None, 10).unwrap();
    assert_eq!(
        changed
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["text-0", "edited on a new branch"]
    );
    assert_eq!(
        store
            .page("history", "main", None, 10)
            .unwrap()
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["text-0", "text-1", "text-2"]
    );
    assert_eq!(
        store
            .message_page("history", "edited", None, 10)
            .unwrap()
            .messages[0]
            .storage_id,
        prefix
    );
    store
        .apply(vec![
            HistoryMutation::SelectBranch {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                revision: 6,
            },
            edit,
        ])
        .unwrap();
    assert_eq!(
        store
            .conversation_head("history")
            .unwrap()
            .unwrap()
            .active_branch,
        "main"
    );
}

#[test]
fn model_context_reads_beyond_the_ui_page_and_keeps_complete_tool_groups() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    store
        .apply(vec![HistoryMutation::Create {
            conversation: empty_conversation(),
            revision: 1,
        }])
        .unwrap();
    let mut messages: Vec<_> = (0..60).map(message).collect();
    let mut assistant = message(60);
    assistant.role = AiChatRole::Assistant;
    assistant.content.clear();
    assistant.tool_calls = vec![
        serde_json::json!({"id":"call-original","type":"function","function":{"name":"read_file","arguments":"{}"}}),
    ];
    let mut tool = message(61);
    tool.role = AiChatRole::Tool;
    tool.tool_call_id = Some("call-original".into());
    tool.content = "File content".into();
    messages.extend([assistant, tool]);
    store
        .apply(
            messages
                .into_iter()
                .enumerate()
                .map(|(index, message)| HistoryMutation::PutMessage {
                    conversation_id: "history".into(),
                    branch_id: "main".into(),
                    message,
                    revision: index as u64 + 2,
                })
                .collect(),
        )
        .unwrap();
    assert_eq!(
        store
            .message_page("history", "main", None, 50)
            .unwrap()
            .messages[0]
            .id,
        "message-12"
    );
    let full = store
        .model_context("history", "main", usize::MAX, "openai")
        .unwrap();
    assert_eq!(full.messages[0].id, "message-0");
    let recent = store.model_context("history", "main", 1, "openai").unwrap();
    assert_eq!(
        recent
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["message-59", "message-60", "message-61"]
    );
    assert_eq!(recent.messages[1].tool_calls[0]["id"], "call-original");
    assert_eq!(
        recent.messages[2].tool_call_id.as_deref(),
        Some("call-original")
    );
}

#[tokio::test]
async fn opening_the_same_database_shares_writer_order_and_failure_state() {
    let dir = tempfile::tempdir().unwrap();
    let first = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let second = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let writer = HistoryWriter::new(first.clone()).unwrap();
    let other = HistoryWriter::new(second.clone()).unwrap();
    writer
        .submit(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            put(0, 2),
        ])
        .await
        .unwrap();
    other
        .submit(vec![HistoryMutation::Rename {
            conversation_id: "history".into(),
            title: "Shared writer".into(),
            updated_at: 4,
            revision: 3,
        }])
        .await
        .unwrap();
    writer.shutdown().await.unwrap();
    assert!(other.submit(vec![put(1, 4)]).await.is_err());
    assert_eq!(
        second
            .conversation_head("history")
            .unwrap()
            .unwrap()
            .conversation
            .title,
        "Shared writer"
    );
    assert_eq!(
        first.page("history", "main", None, 10).unwrap().messages[0].content,
        "text-0"
    );
}

#[test]
fn archives_keep_all_originals_and_branch_text_out_of_the_loaded_message() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let mut anchor = message(5000);
    anchor.role = AiChatRole::System;
    anchor.content = "Compacted context".into();
    anchor.metadata = Some(AiChatMessageMetadata {
        kind: "compaction-anchor".into(),
        original_count: Some(2105),
        compacted_at_ms: Some(2),
        original_user_count: None,
        original_ref: None,
        original_messages: Some((0..2105).map(message).collect()),
    });
    let mut edited = message(6000);
    edited.branches = Some(crate::AiMessageBranches {
        total: 2,
        active_index: 1,
        tails: std::collections::HashMap::from([(0, vec![message(7), message(8)])]),
        refs: Default::default(),
    });
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: anchor,
                revision: 2,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: edited,
                revision: 3,
            },
        ])
        .unwrap();
    let page = store.page("history", "main", None, 50).unwrap();
    let metadata = page.messages[0].metadata.as_ref().unwrap();
    assert!(
        metadata.original_messages.is_none(),
        "opening the summary must not materialize the archived bodies"
    );
    let range = metadata.original_ref.as_ref().unwrap();
    let mut cursor = None;
    let mut pages = Vec::new();
    loop {
        let page = store
            .range_messages("history", range, cursor.as_ref(), 50)
            .unwrap();
        pages.push(page.messages);
        cursor = page.before;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        pages
            .into_iter()
            .rev()
            .flatten()
            .map(|message| message.content)
            .collect::<Vec<_>>(),
        (0..2105)
            .map(|index| format!("text-{index}"))
            .collect::<Vec<_>>()
    );
    let branches = page.messages[1].branches.as_ref().unwrap();
    assert!(branches.tails.is_empty());
    assert_eq!(
        store
            .range_messages("history", &branches.refs[&0], None, 50)
            .unwrap()
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["text-7", "text-8"]
    );
    assert_eq!(
        store
            .conversation_head("history")
            .unwrap()
            .unwrap()
            .conversation
            .turn_count,
        2106
    );
    store
        .apply(vec![HistoryMutation::DeleteMessage {
            conversation_id: "history".into(),
            branch_id: "main".into(),
            message_id: "message-5000".into(),
            revision: 4,
        }])
        .unwrap();
    assert_eq!(
        store
            .range_messages("history", &branches.refs[&0], None, 50)
            .unwrap()
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["text-7", "text-8"]
    );
    let guard = store.db.read();
    let tx = guard.as_ref().unwrap().begin_read().unwrap();
    assert_eq!(tx.open_table(records::MESSAGES).unwrap().len().unwrap(), 3);
    assert_eq!(tx.open_table(records::BRANCHES).unwrap().len().unwrap(), 2);
}

#[test]
fn compaction_references_the_retained_suffix_and_preserves_appends_and_later_forks() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    store
        .apply(vec![HistoryMutation::Create {
            conversation: empty_conversation(),
            revision: 1,
        }])
        .unwrap();
    store
        .apply((0..6).map(|index| put(index, index as u64 + 2)).collect())
        .unwrap();
    let kept = store
        .message_page("history", "main", None, 10)
        .unwrap()
        .messages[3]
        .storage_id
        .clone();
    let mut anchor = message(100);
    anchor.role = AiChatRole::System;
    anchor.content = "Summary".into();
    anchor.metadata = Some(AiChatMessageMetadata {
        kind: "compaction-anchor".into(),
        original_count: None,
        original_user_count: None,
        compacted_at_ms: Some(8),
        original_messages: None,
        original_ref: None,
    });
    store
        .apply(vec![
            HistoryMutation::Compact {
                conversation_id: "history".into(),
                source_branch: "main".into(),
                branch_id: "compacted".into(),
                through_message: "message-2".into(),
                anchor,
                revision: 8,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "compacted".into(),
                message: message(6),
                revision: 9,
            },
        ])
        .unwrap();
    let compacted = store.page("history", "compacted", None, 50).unwrap();
    assert_eq!(
        compacted
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["Summary", "text-3", "text-4", "text-5", "text-6"]
    );
    assert_eq!(
        store
            .message_page("history", "compacted", None, 50)
            .unwrap()
            .messages[1]
            .storage_id,
        kept
    );
    let metadata = compacted.messages[0].metadata.as_ref().unwrap();
    assert_eq!(metadata.original_count, Some(3));
    assert_eq!(
        store
            .range_messages("history", metadata.original_ref.as_ref().unwrap(), None, 50)
            .unwrap()
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["text-0", "text-1", "text-2"]
    );
    assert_eq!(
        store
            .conversation_head("history")
            .unwrap()
            .unwrap()
            .conversation
            .turn_count,
        7
    );
    store
        .apply(vec![HistoryMutation::ReplaceTail {
            conversation_id: "history".into(),
            source_branch: "compacted".into(),
            branch_id: "after-compaction".into(),
            after_message: Some("message-3".into()),
            before_message: None,
            messages: vec![message(10)],
            revision: 10,
        }])
        .unwrap();
    assert_eq!(
        store
            .page("history", "after-compaction", None, 50)
            .unwrap()
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["Summary", "text-3", "text-10"]
    );
    let head = store.conversation_head("history").unwrap().unwrap();
    assert_eq!(
        (
            head.conversation.message_count,
            head.conversation.turn_count
        ),
        (3, 5)
    );
}

#[test]
fn message_deltas_change_tool_status_without_reencoding_unchanged_output() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    let content = "Large reply 中文\n".repeat(100_000);
    let output = "Tool output line\n".repeat(100_000);
    let mut item = message(0);
    item.role = AiChatRole::Assistant;
    item.is_streaming = true;
    item.content = content.clone();
    item.tool_calls = vec![
        serde_json::json!({"id":"call", "name":"inspect", "status":"running", "result":{"data":output}}),
    ];
    item.turn = Some(
        serde_json::json!({"status":"running", "parts":[{"type":"text", "text":content}], "providerParts":{"openai-responses":[{"encrypted_content":"opaque reasoning state"}]}}),
    );
    let base = HistoryStreamSnapshot::capture(&item, 2);
    store
        .apply(vec![
            HistoryMutation::Create {
                conversation: empty_conversation(),
                revision: 1,
            },
            HistoryMutation::PutMessage {
                conversation_id: "history".into(),
                branch_id: "main".into(),
                message: item.clone(),
                revision: 2,
            },
        ])
        .unwrap();
    item.tool_calls[0]["status"] = "success".into();
    item.turn.as_mut().unwrap()["status"] = "complete".into();
    item.is_streaming = false;
    let (delta, _) = base.message_delta(&item, 3).unwrap();
    let operation = HistoryMutation::StreamText {
        conversation_id: "history".into(),
        branch_id: "main".into(),
        message_id: item.id.clone(),
        delta,
    };
    assert!(rmp_serde::to_vec(&operation).unwrap().len() < 2048);
    store.apply(vec![operation]).unwrap();
    let actual = store.message_by_id("history", "main", &item.id).unwrap();
    assert_eq!(actual.content, content);
    assert_eq!(actual.tool_calls[0]["status"], "success");
    assert_eq!(actual.tool_calls[0]["result"]["data"], output);
    assert_eq!(actual.turn.as_ref().unwrap()["status"], "complete");
    assert_eq!(
        actual.turn.as_ref().unwrap()["providerParts"]["openai-responses"][0]["encrypted_content"],
        "opaque reasoning state"
    );
}

#[test]
fn live_windows_preserve_every_unicode_byte_without_copying_unselected_tool_output() {
    let mut item = message(0);
    item.role = AiChatRole::Assistant;
    item.is_streaming = true;
    let text = "界🙂 line\n".repeat(30_000);
    item.content = text.clone();
    item.turn = Some(
        serde_json::json!({"parts":[{"type":"tool_result", "toolCallId":"huge", "output":"old output ".repeat(100_000)}, {"type":"text", "text":text}]}),
    );
    let view = live_message_view(&item, "history", 1, None).unwrap();
    assert_eq!(view.section, 1);
    assert!(view.message.tool_calls.is_empty());
    let mut restored = view.message.turn.as_ref().unwrap()["parts"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(restored.len() <= records::CONTENT_CHUNK_BYTES);
    let mut next = view.more.into_iter().next();
    while let Some(cursor) = next {
        let page = live_content_page(&item, &cursor).unwrap();
        let part = page.value.as_str().unwrap();
        assert!(part.len() <= records::CONTENT_CHUNK_BYTES);
        restored.push_str(part);
        next = page.more.into_iter().next();
    }
    assert_eq!(restored, text);
}

#[test]
fn import_uses_bounded_byte_batches_without_reordering_equal_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConversationStore::open(dir.path().join("v4.redb")).unwrap();
    store
        .apply(vec![HistoryMutation::Create {
            conversation: empty_conversation(),
            revision: 1,
        }])
        .unwrap();
    let mut revision = 1;
    let commits = store
        .import_messages(
            "history",
            "main",
            [4, 1, 3, 0, 2].into_iter().map(|index| {
                let mut item = message(index);
                item.content = "payload\n".repeat(256 * 1024);
                Ok(item)
            }),
            &mut revision,
        )
        .unwrap();
    assert_eq!(commits, 2);
    let page = store.message_page("history", "main", None, 50).unwrap();
    assert_eq!(
        page.messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "message-4",
            "message-1",
            "message-3",
            "message-0",
            "message-2"
        ]
    );
}
