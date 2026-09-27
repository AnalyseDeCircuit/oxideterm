use oxideterm_audit::*;
use std::path::Path;
use zeroize::Zeroizing;

struct Keys(u8);
impl AuditKeyProvider for Keys {
    fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        Ok(Zeroizing::new(vec![self.0; 32]))
    }
    fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        self.load(id)
    }
}

fn enabled(store: &mut AuditStore, days: u32, bytes: u64) {
    let mut policy = store.policy().unwrap();
    policy.enabled = true;
    policy.record_output = true;
    policy.output_retention_days = days;
    policy.output_max_bytes = bytes;
    store.set_policy(policy).unwrap();
}

fn details() -> RecordingDetails {
    RecordingDetails {
        session_id: "session-private-marker".into(),
        transport_id: Some("transport-private-marker".into()),
        consumer_id: Some("consumer-private-marker".into()),
        operation_id: Some("operation-private-marker".into()),
        endpoint: Some(Zeroizing::new("endpoint-private-marker".into())),
    }
}

fn recording(store: &mut AuditStore, at: i64) -> String {
    store
        .create_recording(&uuid::Uuid::new_v4().to_string(), at, &details())
        .unwrap()
        .unwrap()
}

fn chunk_path(database: &Path, id: &str, sequence: i64) -> std::path::PathBuf {
    database
        .with_extension("recordings")
        .join(format!("{id}-{sequence:016x}.chunk"))
}

#[test]
fn encrypted_multiframe_recording_reopens_in_order_and_has_no_plaintext_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(4)).unwrap();
    enabled(&mut store, 7, 2 * 1024 * 1024);
    let id = recording(&mut store, 100);
    let first = [
        RecordingFrame {
            occurred_at_ms: 101,
            kind: RecordingFrameKind::Output(b"output-private-marker"),
        },
        RecordingFrame {
            occurred_at_ms: 102,
            kind: RecordingFrameKind::Resize {
                columns: 132,
                rows: 41,
            },
        },
    ];
    assert_eq!(store.append_recording_chunk(&id, &first).unwrap(), 1);
    assert_eq!(
        store
            .append_recording_chunk(
                &id,
                &[RecordingFrame {
                    occurred_at_ms: 103,
                    kind: RecordingFrameKind::Gap {
                        lost_bytes: Some(17)
                    },
                }]
            )
            .unwrap(),
        2
    );
    store.finish_recording(&id, 104, false).unwrap();
    assert_eq!(
        store.list_recordings(None, 10).unwrap().recordings[0].state,
        RecordingState::Gaps
    );
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_file() {
            let bytes = std::fs::read(entry.path()).unwrap();
            for marker in [
                "output-private-marker",
                "session-private-marker",
                "endpoint-private-marker",
            ] {
                assert!(
                    !bytes
                        .windows(marker.len())
                        .any(|window| window == marker.as_bytes())
                );
            }
        }
    }
    let bytes = std::fs::read(chunk_path(&path, &id, 1)).unwrap();
    assert!(
        !bytes
            .windows(b"output-private-marker".len())
            .any(|w| w == b"output-private-marker")
    );
    drop(store);
    let store = AuditStore::open(&path, &Keys(4)).unwrap();
    let summary = &store.list_recordings(None, 10).unwrap().recordings[0];
    assert_eq!(summary.details.session_id, "session-private-marker");
    let page = store.read_recording_page(&id, None, None, 1).unwrap();
    assert_eq!(page.next_cursor, Some(1));
    assert_eq!(page.chunks[0].sequence, 1);
    assert!(
        matches!(&page.chunks[0].frames[0].kind, StoredRecordingFrameKind::Output(v) if v.as_slice() == b"output-private-marker")
    );
    assert!(matches!(
        page.chunks[0].frames[1].kind,
        StoredRecordingFrameKind::Resize {
            columns: 132,
            rows: 41
        }
    ));
    let next = store
        .read_recording_page(&id, page.next_cursor, None, 1)
        .unwrap();
    assert!(matches!(
        next.chunks[0].frames[0].kind,
        StoredRecordingFrameKind::Gap {
            lost_bytes: Some(17)
        }
    ));
    assert_eq!(next.next_cursor, None);
}

#[test]
fn large_chunks_page_without_skipping_output_at_the_byte_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(4)).unwrap();
    enabled(&mut store, 7, 16 * 1024 * 1024);
    let id = recording(&mut store, 100);
    for (time, byte) in [(101, b'A'), (102, b'B')] {
        let output = vec![byte; 3 * 1024 * 1024];
        store
            .append_recording_chunk(
                &id,
                &[RecordingFrame {
                    occurred_at_ms: time,
                    kind: RecordingFrameKind::Output(&output),
                }],
            )
            .unwrap();
    }
    store.finish_recording(&id, 103, false).unwrap();
    drop(store);
    let store = AuditStore::open(&path, &Keys(4)).unwrap();
    let first = store.read_recording_page(&id, None, None, 16).unwrap();
    assert_eq!(
        first
            .chunks
            .iter()
            .map(|chunk| chunk.sequence)
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(first.next_cursor, Some(1));
    assert!(matches!(&first.chunks[0].frames[0].kind,
        StoredRecordingFrameKind::Output(bytes) if bytes.as_slice() == vec![b'A'; 3 * 1024 * 1024]));
    let second = store
        .read_recording_page(&id, first.next_cursor, None, 16)
        .unwrap();
    assert_eq!(
        second
            .chunks
            .iter()
            .map(|chunk| chunk.sequence)
            .collect::<Vec<_>>(),
        [2]
    );
    assert_eq!(second.next_cursor, None);
    assert!(matches!(&second.chunks[0].frames[0].kind,
        StoredRecordingFrameKind::Output(bytes) if bytes.as_slice() == vec![b'B'; 3 * 1024 * 1024]));
}

#[test]
fn tampering_and_swapping_chunks_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(8)).unwrap();
    enabled(&mut store, 7, 2 * 1024 * 1024);
    let id = recording(&mut store, 100);
    for (time, data) in [(101, b"one".as_slice()), (102, b"two".as_slice())] {
        store
            .append_recording_chunk(
                &id,
                &[RecordingFrame {
                    occurred_at_ms: time,
                    kind: RecordingFrameKind::Output(data),
                }],
            )
            .unwrap();
    }
    let one = chunk_path(&path, &id, 1);
    let two = chunk_path(&path, &id, 2);
    let original = std::fs::read(&one).unwrap();
    let mut tampered = original.clone();
    *tampered.last_mut().unwrap() ^= 1;
    std::fs::write(&one, tampered).unwrap();
    assert!(matches!(
        store.read_recording_page(&id, None, None, 2),
        Err(AuditError::Integrity)
    ));
    std::fs::write(&one, std::fs::read(&two).unwrap()).unwrap();
    assert!(matches!(
        store.read_recording_page(&id, None, None, 2),
        Err(AuditError::Integrity)
    ));
    std::fs::write(&one, original).unwrap();
    drop(store);
    assert!(matches!(
        AuditStore::open(&path, &Keys(9)),
        Err(AuditError::Integrity)
    ));
}

#[test]
fn retention_rotates_old_chunks_of_a_live_recording_and_keeps_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(3)).unwrap();
    enabled(&mut store, 1, 1024 * 1024);
    let id = recording(&mut store, 0);
    for (time, data) in [(0, b"old".as_slice()), (86_400_001, b"new".as_slice())] {
        store
            .append_recording_chunk(
                &id,
                &[RecordingFrame {
                    occurred_at_ms: time,
                    kind: RecordingFrameKind::Output(data),
                }],
            )
            .unwrap();
    }
    assert_eq!(store.prune_recordings(86_400_002).unwrap(), 1);
    assert!(!chunk_path(&path, &id, 1).exists());
    assert!(chunk_path(&path, &id, 2).exists());
    let summary = &store.list_recordings(None, 10).unwrap().recordings[0];
    assert_eq!(summary.state, RecordingState::InProgress);
    assert!(summary.has_expired_content);
    let page = store.read_recording_page(&id, None, None, 10).unwrap();
    assert_eq!(page.expired_sequences, vec![1]);
    assert_eq!(page.chunks.len(), 1);
    assert_eq!(page.chunks[0].sequence, 2);
    store.finish_recording(&id, 86_400_003, false).unwrap();
    assert_eq!(
        store.list_recordings(None, 10).unwrap().recordings[0].state,
        RecordingState::Gaps
    );
}

#[test]
fn oversized_chunk_is_rejected_and_interrupted_state_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(6)).unwrap();
    enabled(&mut store, 7, 2 * 1024 * 1024);
    let id = recording(&mut store, 0);
    let large = vec![b'x'; 4 * 1024 * 1024];
    assert!(matches!(
        store.append_recording_chunk(
            &id,
            &[RecordingFrame {
                occurred_at_ms: 1,
                kind: RecordingFrameKind::Output(&large),
            }]
        ),
        Err(AuditError::TooLarge)
    ));
    assert!(!chunk_path(&path, &id, 1).exists());
    store
        .append_recording_chunk(
            &id,
            &[RecordingFrame {
                occurred_at_ms: 2,
                kind: RecordingFrameKind::Output(b"partial"),
            }],
        )
        .unwrap();
    store.finish_recording(&id, 3, true).unwrap();
    assert_eq!(
        store.list_recordings(None, 10).unwrap().recordings[0].state,
        RecordingState::Interrupted
    );
    assert!(matches!(
        store.append_recording_chunk(
            &id,
            &[RecordingFrame {
                occurred_at_ms: 4,
                kind: RecordingFrameKind::Output(b"late"),
            }]
        ),
        Err(AuditError::Closed)
    ));
}

#[test]
fn abandoned_recording_is_interrupted_on_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(2)).unwrap();
    enabled(&mut store, 7, 2 * 1024 * 1024);
    let id = recording(&mut store, 10);
    store
        .append_recording_chunk(
            &id,
            &[RecordingFrame {
                occurred_at_ms: 11,
                kind: RecordingFrameKind::Output(b"survived"),
            }],
        )
        .unwrap();
    drop(store);
    let mut restarted = AuditStore::open(&path, &Keys(2)).unwrap();
    assert_eq!(restarted.recover_abandoned_recordings(&path).unwrap(), 1);
    assert_eq!(
        restarted.list_recordings(None, 10).unwrap().recordings[0].state,
        RecordingState::Interrupted
    );
    assert!(
        matches!(&restarted.read_recording_page(&id, None, None, 10).unwrap().chunks[0].frames[0].kind,
        StoredRecordingFrameKind::Output(data) if data.as_slice() == b"survived")
    );
}

#[test]
fn capacity_removes_only_oldest_chunks_even_before_time_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(1)).unwrap();
    enabled(&mut store, 7, 1024 * 1024);
    let id = recording(&mut store, 1);
    let output = vec![b'z'; 220 * 1024];
    for time in 1..=5 {
        store
            .append_recording_chunk(
                &id,
                &[RecordingFrame {
                    occurred_at_ms: time,
                    kind: RecordingFrameKind::Output(&output),
                }],
            )
            .unwrap();
    }
    assert_eq!(store.prune_recordings(6).unwrap(), 1);
    assert!(!chunk_path(&path, &id, 1).exists());
    for sequence in 2..=5 {
        assert!(chunk_path(&path, &id, sequence).exists());
    }
    let page = store.read_recording_page(&id, None, None, 10).unwrap();
    assert_eq!(page.expired_sequences, vec![1]);
    assert_eq!(
        page.chunks
            .iter()
            .map(|chunk| chunk.sequence)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5]
    );
}

#[test]
fn expired_page_advances_cursor_and_state_index_tampering_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(5)).unwrap();
    enabled(&mut store, 1, 1024 * 1024);
    let id = recording(&mut store, 0);
    for (time, value) in [(0, b"old".as_slice()), (86_400_001, b"live".as_slice())] {
        store
            .append_recording_chunk(
                &id,
                &[RecordingFrame {
                    occurred_at_ms: time,
                    kind: RecordingFrameKind::Output(value),
                }],
            )
            .unwrap();
    }
    store.prune_recordings(86_400_002).unwrap();
    let first = store.read_recording_page(&id, None, None, 1).unwrap();
    assert!(first.chunks.is_empty());
    assert_eq!(first.expired_sequences, vec![1]);
    assert_eq!(first.next_cursor, Some(1));
    let second = store
        .read_recording_page(&id, first.next_cursor, None, 1)
        .unwrap();
    assert_eq!(second.chunks[0].sequence, 2);
    assert_eq!(second.next_cursor, None);
    drop(store);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE audit_recordings SET state=1,ended_at=999 WHERE id=?",
        [&id],
    )
    .unwrap();
    drop(db);
    let store = AuditStore::open(&path, &Keys(5)).unwrap();
    assert!(matches!(
        store.list_recordings(None, 10),
        Err(AuditError::Integrity)
    ));
    assert!(matches!(
        store.read_recording_page(&id, None, None, 1),
        Err(AuditError::Integrity)
    ));
}

#[test]
fn policy_disable_stops_existing_recording_from_writing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(5)).unwrap();
    enabled(&mut store, 7, 1024 * 1024);
    let id = recording(&mut store, 0);
    let mut policy = store.policy().unwrap();
    policy.record_output = false;
    store.set_policy(policy).unwrap();
    assert!(matches!(
        store.append_recording_chunk(
            &id,
            &[RecordingFrame {
                occurred_at_ms: 1,
                kind: RecordingFrameKind::Output(b"disabled-content"),
            }]
        ),
        Err(AuditError::Closed)
    ));
    assert!(!chunk_path(&path, &id, 1).exists());
}

#[test]
fn session_filter_pages_exact_recordings_without_plaintext_index() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(7)).unwrap();
    enabled(&mut store, 7, 1024 * 1024);
    let instance = uuid::Uuid::new_v4().to_string();
    let create = |store: &mut AuditStore, session: &str, time| {
        store
            .create_recording(
                &instance,
                time,
                &RecordingDetails {
                    session_id: session.into(),
                    transport_id: None,
                    consumer_id: None,
                    operation_id: None,
                    endpoint: None,
                },
            )
            .unwrap()
            .unwrap()
    };
    let older = create(&mut store, "private-session-alpha", 1);
    let other = create(&mut store, "private-session-beta", 2);
    let newer = create(&mut store, "private-session-alpha", 3);
    let first = store
        .list_recordings_for_session("private-session-alpha", None, 1)
        .unwrap();
    assert_eq!(
        first.recordings.iter().map(|r| &r.id).collect::<Vec<_>>(),
        vec![&newer]
    );
    let second = store
        .list_recordings_for_session("private-session-alpha", first.next_cursor, 1)
        .unwrap();
    assert_eq!(
        second.recordings.iter().map(|r| &r.id).collect::<Vec<_>>(),
        vec![&older]
    );
    assert_eq!(second.next_cursor, None);
    assert_eq!(
        store
            .list_recordings_for_session("private-session-beta", None, 10)
            .unwrap()
            .recordings[0]
            .id,
        other
    );
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_file() {
            let bytes = std::fs::read(entry.path()).unwrap();
            for marker in ["private-session-alpha", "private-session-beta"] {
                assert!(
                    !bytes
                        .windows(marker.len())
                        .any(|window| window == marker.as_bytes())
                );
            }
        }
    }
}

#[test]
fn startup_removes_only_unindexed_files_from_inactive_recordings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(2)).unwrap();
    enabled(&mut store, 7, 1024 * 1024);
    let id = recording(&mut store, 10);
    store
        .append_recording_chunk(
            &id,
            &[RecordingFrame {
                occurred_at_ms: 11,
                kind: RecordingFrameKind::Output(b"indexed-content"),
            }],
        )
        .unwrap();
    let indexed = chunk_path(&path, &id, 1);
    let orphan = chunk_path(&path, &id, 2);
    std::fs::copy(&indexed, &orphan).unwrap();
    let temporary = path.with_extension("recordings").join(format!(
        "{id}-{:016x}-{}.tmp",
        3,
        uuid::Uuid::new_v4()
    ));
    std::fs::copy(&indexed, &temporary).unwrap();
    drop(store);
    let store = AuditStore::open(&path, &Keys(2)).unwrap();
    assert_eq!(store.recover_orphan_recording_files(&path).unwrap(), 2);
    assert!(indexed.exists());
    assert!(!orphan.exists());
    assert!(!temporary.exists());
    let page = store.read_recording_page(&id, None, None, 10).unwrap();
    assert_eq!(
        page.chunks
            .iter()
            .map(|chunk| chunk.sequence)
            .collect::<Vec<_>>(),
        vec![1]
    );
}

#[test]
fn retention_sweeps_many_chunks_in_bounded_batches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys(2)).unwrap();
    enabled(&mut store, 1, 1024 * 1024);
    let id = recording(&mut store, 0);
    for index in 0..140 {
        store
            .append_recording_chunk(
                &id,
                &[RecordingFrame {
                    occurred_at_ms: index,
                    kind: RecordingFrameKind::Output(b"x"),
                }],
            )
            .unwrap();
    }
    store.finish_recording(&id, 140, false).unwrap();
    assert_eq!(store.prune_recordings(2 * 86_400_000).unwrap(), 128);
    assert!(chunk_path(&path, &id, 129).exists());
    assert_eq!(store.prune_recordings(2 * 86_400_000).unwrap(), 12);
    assert!(!chunk_path(&path, &id, 129).exists());
    assert_eq!(
        store.list_recordings(None, 10).unwrap().recordings[0].state,
        RecordingState::Expired
    );
}
