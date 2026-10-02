use oxideterm_audit::*;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

struct Keys;
impl AuditKeyProvider for Keys {
    fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        Ok(Zeroizing::new(vec![41; 32]))
    }
    fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
        self.load(id)
    }
}

fn service(path: &Path) -> (AuditService, AuditClient, AuditContext) {
    {
        let mut store = AuditStore::open(path, &Keys).unwrap();
        let mut policy = store.policy().unwrap();
        policy.enabled = true;
        store.set_policy(policy).unwrap();
    }
    let service = AuditService::with_key_provider(path.to_owned(), Keys).unwrap();
    let client = service.client();
    let context = AuditContext::new(client.clone(), AuditSource::User)
        .session("local", "fixture-endpoint-secret");
    (service, client, context)
}

fn set_output(client: &AuditClient, enabled: bool) {
    let mut policy = futures::executor::block_on(client.policy()).unwrap();
    policy.record_output = enabled;
    futures::executor::block_on(client.set_policy(policy)).unwrap();
}

fn wait_for<T>(mut query: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(value) = query() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "recording writer did not reach expected state"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn policy_toggle_creates_new_segment_without_recording_disabled_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let (_service, client, context) = service(&path);
    let sink = context.recording_sink();
    sink.resize(91, 33);
    assert!(!sink.is_enabled());
    sink.record_output(b"disabled-before-secret");
    set_output(&client, true);
    assert!(sink.is_enabled());
    sink.record_output(b"first-segment");
    let first = wait_for(|| {
        let page = futures::executor::block_on(client.list_recordings(None, 10)).ok()?;
        page.recordings.first().map(|item| item.id.clone())
    });
    set_output(&client, false);
    assert!(!sink.is_enabled());
    sink.record_output(b"disabled-middle-secret");
    wait_for(|| {
        let list = futures::executor::block_on(client.list_recordings(None, 10)).ok()?;
        (list.recordings[0].state == RecordingState::Interrupted).then_some(())
    });
    set_output(&client, true);
    sink.record_output(b"second-segment");
    sink.close();
    let list = wait_for(|| {
        let list = futures::executor::block_on(client.list_recordings(None, 10)).ok()?;
        (list.recordings.len() == 2 && list.recordings[0].state == RecordingState::Finished)
            .then_some(list)
    });
    assert_ne!(list.recordings[0].id, first);
    assert_eq!(list.recordings[1].id, first);
    let filtered = futures::executor::block_on(client.list_recordings_for_session(
        context.session_id.clone().unwrap(),
        None,
        10,
    ))
    .unwrap();
    assert_eq!(
        filtered
            .recordings
            .iter()
            .map(|item| &item.id)
            .collect::<Vec<_>>(),
        list.recordings
            .iter()
            .map(|item| &item.id)
            .collect::<Vec<_>>()
    );
    let first_page =
        futures::executor::block_on(client.read_recording_page(first, None, None, 10)).unwrap();
    assert!(
        first_page
            .chunks
            .iter()
            .flat_map(|chunk| &chunk.frames)
            .any(|frame| matches!(
                frame.kind,
                StoredRecordingFrameKind::Resize {
                    columns: 91,
                    rows: 33
                }
            ))
    );
    let second_page = futures::executor::block_on(client.read_recording_page(
        list.recordings[0].id.clone(),
        None,
        None,
        10,
    ))
    .unwrap();
    assert!(second_page.chunks.iter().flat_map(|chunk| &chunk.frames).any(|frame|
        matches!(&frame.kind, StoredRecordingFrameKind::Output(data) if data.as_slice() == b"second-segment")));
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_file() {
            let data = std::fs::read(entry.path()).unwrap();
            for marker in [
                b"disabled-before-secret".as_slice(),
                b"disabled-middle-secret".as_slice(),
            ] {
                assert!(!data.windows(marker.len()).any(|window| window == marker));
            }
        }
    }
}

#[test]
fn close_flushes_and_drop_without_close_interrupts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let (service, client, context) = service(&path);
    set_output(&client, true);
    let normal = context.recording_sink();
    normal.record_output(b"flush-on-close");
    normal.close();
    drop(normal);
    let interrupted = context.recording_sink();
    interrupted.record_output(b"before-drop");
    drop(interrupted);
    wait_for(|| {
        let list = futures::executor::block_on(client.list_recordings(None, 10)).ok()?;
        (list.recordings.len() == 2
            && list
                .recordings
                .iter()
                .any(|r| r.state == RecordingState::Finished)
            && list
                .recordings
                .iter()
                .any(|r| r.state == RecordingState::Interrupted))
        .then_some(())
    });
    drop(service);
    let store = AuditStore::open(&path, &Keys).unwrap();
    let list = store.list_recordings(None, 10).unwrap();
    assert_eq!(list.recordings.len(), 2);
    let normal_id = list
        .recordings
        .iter()
        .find(|r| r.state == RecordingState::Finished)
        .unwrap()
        .id
        .clone();
    let page = store
        .read_recording_page(&normal_id, None, None, 10)
        .unwrap();
    assert!(page.chunks.iter().flat_map(|chunk| &chunk.frames).any(|frame|
        matches!(&frame.kind, StoredRecordingFrameKind::Output(data) if data.as_slice() == b"flush-on-close")));
}

#[test]
fn burst_over_queue_budget_records_a_gap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let (_service, client, context) = service(&path);
    set_output(&client, true);
    let sink = context.recording_sink();
    let burst = vec![b'x'; 32 * 1024 * 1024];
    sink.record_output(&burst);
    sink.close();
    let id = wait_for(|| {
        let list = futures::executor::block_on(client.list_recordings(None, 10)).ok()?;
        list.recordings
            .first()
            .and_then(|r| (r.state == RecordingState::Gaps).then_some(r.id.clone()))
    });
    let mut cursor = None;
    let mut gap = false;
    loop {
        let page =
            futures::executor::block_on(client.read_recording_page(id.clone(), cursor, None, 4))
                .unwrap();
        gap |= page.chunks.iter().flat_map(|chunk| &chunk.frames).any(|frame|
            matches!(frame.kind, StoredRecordingFrameKind::Gap { lost_bytes: Some(value) } if value > 0));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert!(gap);
}

#[test]
fn another_writer_policy_change_is_observed_by_existing_sink() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let (_first, first_client, context) = service(&path);
    let (_second, second_client, _) = service(&path);
    let sink = context.recording_sink();
    assert!(!sink.is_enabled());
    set_output(&second_client, true);
    wait_for(|| sink.is_enabled().then_some(()));
    sink.record_output(b"other-writer-enabled");
    wait_for(|| {
        let list = futures::executor::block_on(first_client.list_recordings(None, 10)).ok()?;
        (!list.recordings.is_empty()).then_some(())
    });
    set_output(&second_client, false);
    wait_for(|| (!sink.is_enabled()).then_some(()));
    sink.record_output(b"other-writer-disabled");
    wait_for(|| {
        let list = futures::executor::block_on(first_client.list_recordings(None, 10)).ok()?;
        (list.recordings[0].state == RecordingState::Interrupted).then_some(())
    });
}

#[test]
fn service_start_marks_unfinished_prior_instance_interrupted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let mut store = AuditStore::open(&path, &Keys).unwrap();
    let mut policy = store.policy().unwrap();
    policy.enabled = true;
    policy.record_output = true;
    store.set_policy(policy).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let id = store
        .create_recording(
            &uuid::Uuid::new_v4().to_string(),
            now,
            &RecordingDetails {
                session_id: "prior-session".into(),
                transport_id: None,
                consumer_id: None,
                operation_id: None,
                endpoint: None,
            },
        )
        .unwrap()
        .unwrap();
    store
        .append_recording_chunk(
            &id,
            &[RecordingFrame {
                occurred_at_ms: now + 1,
                kind: RecordingFrameKind::Output(b"prior-output"),
            }],
        )
        .unwrap();
    drop(store);
    let (_service, client, _) = service(&path);
    let list = wait_for(|| {
        let list = futures::executor::block_on(client.list_recordings(None, 10)).ok()?;
        if list
            .recordings
            .first()
            .is_some_and(|r| r.state == RecordingState::Interrupted)
        {
            Some(list)
        } else {
            None
        }
    });
    assert_eq!(list.recordings[0].id, id);
    let page = futures::executor::block_on(client.read_recording_page(id, None, None, 10)).unwrap();
    assert!(page.chunks.iter().flat_map(|chunk| &chunk.frames).any(|frame|
        matches!(&frame.kind, StoredRecordingFrameKind::Output(data) if data.as_slice() == b"prior-output")));
}

#[test]
fn orphan_sweep_keeps_files_owned_by_a_running_writer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let (service, client, context) = service(&path);
    set_output(&client, true);
    let sink = context.recording_sink();
    sink.record_output(b"active-content");
    let id = wait_for(|| {
        let list = futures::executor::block_on(client.list_recordings(None, 10)).ok()?;
        list.recordings.first().map(|item| item.id.clone())
    });
    let indexed = path
        .with_extension("recordings")
        .join(format!("{id}-{:016x}.chunk", 1));
    let orphan = path
        .with_extension("recordings")
        .join(format!("{id}-{:016x}.chunk", 2));
    std::fs::copy(&indexed, &orphan).unwrap();
    let observer = AuditStore::open(&path, &Keys).unwrap();
    assert_eq!(observer.recover_orphan_recording_files(&path).unwrap(), 0);
    assert!(orphan.exists());
    sink.close();
    drop(service);
    assert_eq!(observer.recover_orphan_recording_files(&path).unwrap(), 1);
    assert!(!orphan.exists());
    assert!(indexed.exists());
}

#[test]
fn batched_output_keeps_resize_between_original_output_frames() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let (_service, client, context) = service(&path);
    set_output(&client, true);
    let sink = context.recording_sink();
    sink.resize(80, 24);
    sink.record_output(b"before-resize");
    sink.resize(40, 12);
    sink.record_output(b"after-resize");
    sink.close();
    let id = wait_for(|| {
        let list = futures::executor::block_on(client.list_recordings(None, 10)).ok()?;
        list.recordings
            .first()
            .and_then(|item| (item.state == RecordingState::Finished).then_some(item.id.clone()))
    });
    let page = futures::executor::block_on(client.read_recording_page(id, None, None, 10)).unwrap();
    let frames = page
        .chunks
        .iter()
        .flat_map(|chunk| &chunk.frames)
        .map(|frame| match &frame.kind {
            StoredRecordingFrameKind::Resize { columns, rows } => {
                format!("resize:{columns}x{rows}")
            }
            StoredRecordingFrameKind::Output(bytes) => String::from_utf8(bytes.to_vec()).unwrap(),
            StoredRecordingFrameKind::Gap { .. } => "gap".into(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        frames,
        [
            "resize:80x24",
            "before-resize",
            "resize:40x12",
            "after-resize"
        ]
    );
}

#[test]
fn recording_burst_leaves_queue_capacity_for_structured_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let (_service, client, context) = service(&path);
    set_output(&client, true);
    let sink = context.recording_sink();
    sink.record_output(&vec![b'z'; 32 * 1024 * 1024]);
    context.observe(
        AuditCategory::System,
        "quota_probe",
        None,
        AuditOutcome::Succeeded,
        AuditEvidence::Lifecycle,
        AuditAuthorization::NotRequired,
    );
    let page = futures::executor::block_on(client.query(AuditQuery {
        category: Some(AuditCategory::System),
        limit: 100,
        ..Default::default()
    }))
    .unwrap();
    assert!(page.records.iter().any(|record| {
        record
            .details
            .operation
            .as_ref()
            .is_some_and(|operation| operation.action == "quota_probe")
    }));
    sink.close();
}
